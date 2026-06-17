//! Grammar resolution: maps a code-fence language tag (e.g. `m2`) to a
//! configured tree-sitter [`HighlightConfiguration`]. Grammars come from two
//! sources — ones compiled into the binary (feature-gated [`builtins`]) and
//! ones loaded at runtime from shared libraries described in `book.toml`
//! ([`Grammar::load_dynamic`]). Dynamic loading is what makes the preprocessor
//! work for *any* language: point it at a compiled parser and a highlights
//! query and it highlights that language.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use libloading::Library;
use tree_sitter::Language;
use tree_sitter_highlight::HighlightConfiguration;
use tree_sitter_language::LanguageFn;

use crate::config::{Config, LanguageConfig};
use crate::render;

/// A loaded, configured grammar ready to highlight source text.
pub struct Grammar {
    config: HighlightConfiguration,
    /// HTML `class="…"` attribute bytes for each highlight index, derived from
    /// the query's own capture names so the class set always matches the
    /// grammar (see [`render::class_attributes`]).
    classes: Vec<Vec<u8>>,
    /// Held only to keep a dynamically loaded parser alive for as long as its
    /// [`Language`] is used; `None` for grammars compiled into the binary.
    _library: Option<Library>,
}

impl Grammar {
    /// Build a grammar from an already-resolved [`Language`] and its queries.
    fn new(
        name: &str,
        language: Language,
        highlights: &str,
        injections: &str,
        locals: &str,
        library: Option<Library>,
    ) -> Result<Self> {
        let mut config =
            HighlightConfiguration::new(language, name, highlights, injections, locals)
                .with_context(|| format!("invalid tree-sitter queries for `{name}`"))?;

        // Recognise exactly the capture names the query defines, so every
        // capture maps to itself and we can derive matching CSS classes.
        let names: Vec<String> = config.names().iter().map(|s| s.to_string()).collect();
        config.configure(&names);
        let classes = render::class_attributes(&names);

        Ok(Self {
            config,
            classes,
            _library: library,
        })
    }

    /// Render `source` to inline HTML spans (no `<pre>`/`<code>` wrapper).
    pub fn highlight(&self, source: &str) -> Result<String> {
        render::to_html(&self.config, &self.classes, source)
    }

    /// Load a grammar from a compiled parser shared library, as configured in
    /// `book.toml`. `root` is the book project root that relative paths resolve
    /// against.
    fn load_dynamic(name: &str, cfg: &LanguageConfig, root: &Path) -> Result<Self> {
        let library_path = resolve(root, cfg.library.as_ref().ok_or_else(|| {
            anyhow!("language `{name}` has no built-in grammar; set `library` to a parser shared object")
        })?);
        let symbol = cfg
            .symbol
            .clone()
            .unwrap_or_else(|| format!("tree_sitter_{}", name.replace('-', "_")));

        // SAFETY: we load a tree-sitter parser whose exported symbol is the
        // standard `extern "C" fn() -> *const ()` language constructor. The
        // library is stored in the returned `Grammar` so it outlives the
        // `Language` derived from it.
        let library = unsafe { Library::new(&library_path) }
            .with_context(|| format!("loading parser `{}`", library_path.display()))?;
        let language: Language = unsafe {
            let constructor: libloading::Symbol<unsafe extern "C" fn() -> *const ()> =
                library.get(symbol.as_bytes()).with_context(|| {
                    format!("symbol `{symbol}` not found in {}", library_path.display())
                })?;
            LanguageFn::from_raw(*constructor).into()
        };

        let highlights = read_query(root, &cfg.highlights, name, "highlights")?;
        let injections = read_optional_query(root, cfg.injections.as_ref())?;
        let locals = read_optional_query(root, cfg.locals.as_ref())?;

        Self::new(
            name,
            language,
            &highlights,
            &injections,
            &locals,
            Some(library),
        )
    }
}

/// All grammars known to a single preprocessor run. A grammar is stored once in
/// `grammars` and reachable through every fence tag (alias) in `by_alias`.
pub struct Registry {
    grammars: Vec<Grammar>,
    by_alias: HashMap<String, usize>,
}

impl Registry {
    /// Resolve all configured and built-in grammars. User-configured languages
    /// take precedence over built-ins sharing the same alias, so a project can
    /// override a bundled grammar.
    pub fn build(root: &Path, config: &Config) -> Result<Self> {
        let mut registry = Self {
            grammars: Vec::new(),
            by_alias: HashMap::new(),
        };

        for (name, lang_cfg) in &config.languages {
            let grammar = Grammar::load_dynamic(name, lang_cfg, root)?;
            let aliases = if lang_cfg.aliases.is_empty() {
                std::slice::from_ref(name)
            } else {
                lang_cfg.aliases.as_slice()
            };
            registry.insert(grammar, aliases.iter().map(String::as_str));
        }

        let builtins = if config.bundled {
            builtins::all()
        } else {
            Vec::new()
        };
        for builtin in builtins {
            if builtin
                .aliases
                .iter()
                .all(|a| !registry.by_alias.contains_key(*a))
            {
                let grammar = Grammar::new(
                    builtin.name,
                    (builtin.language)(),
                    builtin.highlights,
                    "",
                    "",
                    None,
                )?;
                registry.insert(grammar, builtin.aliases.iter().copied());
            }
        }

        Ok(registry)
    }

    /// Store `grammar` once and map each alias to it.
    fn insert<'a>(&mut self, grammar: Grammar, aliases: impl Iterator<Item = &'a str>) {
        let index = self.grammars.len();
        self.grammars.push(grammar);
        for alias in aliases {
            self.by_alias.insert(alias.to_string(), index);
        }
    }

    /// Highlight `source` as `lang`, or `None` if no grammar handles that fence
    /// tag (the block is then left untouched).
    pub fn highlight(&self, lang: &str, source: &str) -> Option<Result<String>> {
        let index = *self.by_alias.get(lang)?;
        Some(self.grammars[index].highlight(source))
    }
}

/// Resolve a possibly-relative path against the book root.
fn resolve(root: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

fn read_query(root: &Path, path: &str, lang: &str, kind: &str) -> Result<String> {
    let full = resolve(root, path);
    std::fs::read_to_string(&full)
        .with_context(|| format!("reading {kind} query for `{lang}` from {}", full.display()))
}

fn read_optional_query(root: &Path, path: Option<&String>) -> Result<String> {
    match path {
        Some(p) => {
            let full = resolve(root, p);
            std::fs::read_to_string(&full)
                .with_context(|| format!("reading query from {}", full.display()))
        }
        None => Ok(String::new()),
    }
}

/// Grammars compiled into the binary. Each is feature-gated so a slim build can
/// drop the ones it does not need and rely on dynamic loading instead.
mod builtins {
    /// A grammar statically linked into this binary.
    pub struct Builtin {
        pub name: &'static str,
        pub aliases: &'static [&'static str],
        pub language: fn() -> tree_sitter::Language,
        pub highlights: &'static str,
    }

    pub fn all() -> Vec<Builtin> {
        vec![
            #[cfg(feature = "macaulay2")]
            Builtin {
                name: "macaulay2",
                aliases: &["macaulay2", "m2"],
                language: tree_sitter_macaulay2::language,
                highlights: include_str!("../queries/macaulay2/highlights.scm"),
            },
        ]
    }
}
