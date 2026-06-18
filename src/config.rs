//! The `[preprocessor.tsitter]` section of `book.toml`.
//!
//! ```toml
//! [preprocessor.tsitter]
//!
//! # Add a language by pointing at a compiled parser and a highlights query.
//! [preprocessor.tsitter.languages.nix]
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
    /// The languages to highlight, keyed by language name.
    pub languages: HashMap<String, LanguageConfig>,
    /// Whether languages embedded in a block (via a grammar's injections query)
    /// are highlighted with their own grammar. Only languages already
    /// configured here are ever used — injection never loads a new grammar — so
    /// an embedded language that is not configured is simply left as-is. Set
    /// `inject = false` to switch injection off entirely, e.g. if an injected
    /// grammar misbehaves.
    pub inject: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            languages: HashMap::new(),
            inject: true,
        }
    }
}

/// One dynamically loaded grammar.
#[derive(Debug, Deserialize)]
pub struct LanguageConfig {
    /// Path to the compiled parser shared object.
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
            .get::<Self>("preprocessor.tsitter")
            .context("invalid [preprocessor.tsitter] configuration")
            .map(Option::unwrap_or_default)
    }
}
