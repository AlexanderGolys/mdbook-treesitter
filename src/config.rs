//! The `[preprocessor.tree-sitter]` section of `book.toml`.
//!
//! ```toml
//! [preprocessor.tree-sitter]
//!
//! # Add a language by pointing at a compiled parser and a highlights query.
//! [preprocessor.tree-sitter.languages.nix]
//! library = "parsers/libtree-sitter-nix.so"   # compiled grammar (relative to book root)
//! highlights = "queries/nix/highlights.scm"    # tree-sitter highlights query
//! # symbol = "tree_sitter_nix"                 # optional; defaults to tree_sitter_<name>
//! # injections = "queries/nix/injections.scm"  # optional
//! # locals = "queries/nix/locals.scm"          # optional
//! # aliases = ["nix"]                           # fence tags; defaults to the table key
//! ```

use std::collections::HashMap;

use anyhow::{Context, Result};
use mdbook_preprocessor::PreprocessorContext;
use serde::Deserialize;

/// Parsed preprocessor configuration. Unknown keys (e.g. mdBook's own
/// `command`, `renderers`) are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// User-added languages, keyed by language name.
    pub languages: HashMap<String, LanguageConfig>,
    /// Whether grammars compiled into the binary (e.g. the bundled Macaulay2)
    /// are offered. Set `bundled = false` in `book.toml` to ignore them and
    /// highlight only the languages configured here. A binary built with
    /// `--no-default-features` carries no bundled grammars regardless.
    pub bundled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            languages: HashMap::new(),
            bundled: true,
        }
    }
}

/// One dynamically loaded grammar.
#[derive(Debug, Deserialize)]
pub struct LanguageConfig {
    /// Path to the compiled parser shared object. Required for languages not
    /// bundled into the binary.
    pub library: Option<String>,
    /// The parser's exported constructor symbol; defaults to
    /// `tree_sitter_<name>` (with `-` mapped to `_`).
    pub symbol: Option<String>,
    /// Path to the highlights query (`highlights.scm`).
    pub highlights: String,
    /// Optional injections query for embedded languages.
    pub injections: Option<String>,
    /// Optional locals query for scope-aware highlighting.
    pub locals: Option<String>,
    /// Code-fence tags this grammar handles; defaults to the table key.
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl Config {
    /// Read the preprocessor's table out of the book context, falling back to
    /// an empty configuration when the section is absent.
    pub fn from_context(ctx: &PreprocessorContext) -> Result<Self> {
        ctx.config
            .get::<Self>("preprocessor.tree-sitter")
            .context("invalid [preprocessor.tree-sitter] configuration")
            .map(Option::unwrap_or_default)
    }
}
