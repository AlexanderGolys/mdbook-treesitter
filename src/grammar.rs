//! Grammar resolution: maps a code-fence language tag (e.g. `rust`) to a
//! configured tree-sitter [`HighlightConfiguration`]. Every grammar is loaded
//! at runtime from a shared library described in `book.toml` ([`load_dynamic`]).
//! Dynamic loading is what makes the preprocessor work for *any* language: point
//! it at a compiled parser and a highlights query and it highlights that
//! language.
//!
//! All grammars in a run share one capture-index space (see [`Registry::build`])
//! so that spans produced by an injected sub-language resolve to the same CSS
//! classes as the host language.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use libloading::{Library, Symbol};
use tree_sitter::Language;
use tree_sitter_highlight::{Highlight, HighlightConfiguration, Highlighter, HtmlRenderer};
use tree_sitter_language::LanguageFn;

use crate::config::{Config, LanguageConfig};
use crate::{predicates, render};

/// All grammars known to a single preprocessor run. Each grammar is stored once
/// in `configs` and reachable through every fence tag (alias) in `by_alias`.
/// `classes` is the shared class table, indexed by the global capture index
/// every config was configured against.
pub struct Registry {
    configs: Vec<HighlightConfiguration>,
    by_alias: HashMap<String, usize>,
    classes: Vec<Vec<u8>>,
    /// Whether embedded languages are resolved and highlighted. When `false`,
    /// the injection callback always declines, so blocks render with only their
    /// host grammar.
    inject: bool,
    /// Dynamically loaded parser libraries, kept alive for as long as the
    /// [`Language`]s derived from them are in use. Declared last so it drops
    /// after `configs`.
    _libraries: Vec<Library>,
}

impl Registry {
    /// Load and configure every grammar named in `book.toml`.
    pub fn build(root: &Path, config: &Config) -> Result<Self> {
        let mut configs: Vec<HighlightConfiguration> = Vec::new();
        let mut by_alias: HashMap<String, usize> = HashMap::new();
        let mut libraries: Vec<Library> = Vec::new();

        for (name, lang_cfg) in &config.languages {
            let (cfg, library) = load_dynamic(name, lang_cfg, root, config.inject)?;
            let index = configs.len();
            configs.push(cfg);
            libraries.push(library);
            for alias in aliases_of(name, lang_cfg) {
                if by_alias
                    .get(alias)
                    .is_some_and(|&existing| existing != index)
                {
                    return Err(anyhow!(
                        "fence tag `{alias}` is claimed by more than one language; \
                         give each grammar distinct aliases"
                    ));
                }
                by_alias.insert(alias.to_string(), index);
            }
        }

        // Give every grammar one shared list of recognised capture names, so a
        // capture maps to the same highlight index — and therefore the same CSS
        // class — regardless of which grammar (host or injected) produced it.
        let global_names = union_capture_names(&configs);
        for cfg in &mut configs {
            cfg.configure(&global_names);
        }
        let classes = render::class_attributes(&global_names);

        Ok(Self {
            configs,
            by_alias,
            classes,
            inject: config.inject,
            _libraries: libraries,
        })
    }

    /// Highlight `source` as `lang`, or `None` if no grammar handles that fence
    /// tag (the block is then left untouched). Embedded languages are resolved
    /// through the same alias table, so an injected block in an unregistered
    /// language is simply left unhighlighted rather than erroring.
    pub fn highlight(&self, lang: &str, source: &str) -> Option<Result<String>> {
        let config = self.config_for(lang)?;
        Some(self.render(config, source))
    }

    /// Render one block. Inlined here (rather than in `render`) so the injection
    /// callback and the host config share the `&self` lifetime — the borrow
    /// checker resolves the highlighter's internal lifetime locally, mirroring
    /// tree-sitter-highlight's own C binding.
    fn render(&self, config: &HighlightConfiguration, source: &str) -> Result<String> {
        let mut highlighter = Highlighter::new();
        let events = highlighter
            .highlight(config, source.as_bytes(), None, |name: &str| {
                // Resolve embedded languages only among grammars already built;
                // injection never loads a new grammar. Disabled when `inject` is
                // off, so a misbehaving sub-grammar can be switched out entirely.
                self.inject.then(|| self.config_for(name)).flatten()
            })
            .context("tree-sitter highlighting failed")?;

        let mut renderer = HtmlRenderer::new();
        renderer
            .render(
                events,
                source.as_bytes(),
                &|highlight: Highlight, out: &mut Vec<u8>| {
                    // An out-of-range index would mean a capture we never configured;
                    // fall back to an empty attribute rather than panic.
                    if let Some(attr) = self.classes.get(highlight.0) {
                        out.extend_from_slice(attr);
                    }
                },
            )
            .context("rendering highlighted HTML failed")?;

        Ok(renderer.lines().collect())
    }

    fn config_for(&self, lang: &str) -> Option<&HighlightConfiguration> {
        self.by_alias.get(lang).map(|&index| &self.configs[index])
    }
}

/// The fence tags a configured language answers to: its explicit `aliases`, or
/// its table key when none are given.
fn aliases_of<'a>(name: &'a str, cfg: &'a LanguageConfig) -> impl Iterator<Item = &'a str> {
    let configured = cfg.aliases.iter().map(String::as_str);
    let fallback = cfg.aliases.is_empty().then_some(name);
    configured.chain(fallback)
}

/// The ordered union of every config's capture names, first occurrence kept.
fn union_capture_names(configs: &[HighlightConfiguration]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for config in configs {
        for name in config.names() {
            if !names.iter().any(|existing| existing == name) {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Prefix of the exported constructor symbol that tree-sitter parsers use, e.g.
/// `tree_sitter_rust`. Overridable per language via the `symbol` config key.
const SYMBOL_PREFIX: &str = "tree_sitter_";

/// Load a grammar from a compiled parser shared library, as configured in
/// `book.toml`. `root` is the book project root that relative paths resolve
/// against. Returns the configuration and the library that backs it (which the
/// caller must keep alive).
fn load_dynamic(
    name: &str,
    cfg: &LanguageConfig,
    root: &Path,
    inject: bool,
) -> Result<(HighlightConfiguration, Library)> {
    let library_path = resolve(
        root,
        cfg.library.as_ref().ok_or_else(|| {
            anyhow!(
                "language `{name}` needs a `library` pointing at a compiled parser shared object"
            )
        })?,
    );
    let symbol = cfg
        .symbol
        .clone()
        .unwrap_or_else(|| format!("{SYMBOL_PREFIX}{}", name.replace('-', "_")));

    // SAFETY: we load a tree-sitter parser whose exported symbol is the standard
    // `extern "C" fn() -> *const ()` language constructor. The library is
    // returned to the caller so it outlives the `Language` derived from it.
    let library = unsafe { Library::new(&library_path) }
        .with_context(|| format!("loading parser `{}`", library_path.display()))?;
    let language: Language = unsafe {
        let constructor: Symbol<unsafe extern "C" fn() -> *const ()> =
            library.get(symbol.as_bytes()).with_context(|| {
                format!("symbol `{symbol}` not found in {}", library_path.display())
            })?;
        LanguageFn::from_raw(*constructor).into()
    };

    // Normalise nvim-specific predicates (`#lua-match?`) into standard ones that
    // tree-sitter-highlight evaluates; otherwise the predicate is ignored and
    // its capture fires unconditionally (e.g. every identifier becomes a
    // `@constant`). See [`crate::predicates`].
    let highlights = predicates::normalize(&read_query(root, &cfg.highlights, name, "highlights")?);
    // With injection off we never read the injections query, so a broken
    // `injections.scm` cannot fail compilation and take `highlights` down with it.
    let injections = if inject {
        predicates::normalize(&read_optional_query(root, cfg.injections.as_ref())?)
    } else {
        String::new()
    };
    let locals = predicates::normalize(&read_optional_query(root, cfg.locals.as_ref())?);

    let config = HighlightConfiguration::new(language, name, &highlights, &injections, &locals)
        .with_context(|| format!("invalid tree-sitter queries for `{name}`"))?;
    Ok((config, library))
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
    fs::read_to_string(&full)
        .with_context(|| format!("reading {kind} query for `{lang}` from {}", full.display()))
}

fn read_optional_query(root: &Path, path: Option<&String>) -> Result<String> {
    match path {
        Some(p) => {
            let full = resolve(root, p);
            fs::read_to_string(&full)
                .with_context(|| format!("reading query from {}", full.display()))
        }
        None => Ok(String::new()),
    }
}
