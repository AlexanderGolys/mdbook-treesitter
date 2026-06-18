# Rust — a longer sample

This crate's own `src/main.rs` — attributes, `use` paths, derive macros, a
`match`, error handling, and doc comments — for eyeballing the palette and the
`language-rust` per-language overrides (macros tint as keywords, lifetimes get
the special hue) on real code.

```rust
//! CLI wrapper implementing the mdBook preprocessor protocol: a `supports`
//! subcommand for renderer negotiation, and the default stdin→stdout JSON pass
//! that transforms the book.

use std::io;
use std::process;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mdbook_preprocessor::{self, Preprocessor, MDBOOK_VERSION};
use mdbook_tsitter::TreeSitterPreprocessor;
use semver::{Version, VersionReq};

#[derive(Parser)]
#[command(about = "An mdBook preprocessor for tree-sitter syntax highlighting")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

const DEFAULT_CSS: &str = include_str!("../assets/treesitter.css");

#[derive(Subcommand)]
enum Command {
    Supports {
        renderer: String,
    },
    Css,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let preprocessor = TreeSitterPreprocessor;

    match cli.command {
        Some(Command::Supports { renderer }) => {
            let supported = preprocessor
                .supports_renderer(&renderer)
                .context("checking renderer support")?;
            process::exit(if supported { 0 } else { 1 });
        }
        Some(Command::Css) => {
            print!("{DEFAULT_CSS}");
            Ok(())
        }
        None => preprocess(&preprocessor),
    }
}

fn preprocess(preprocessor: &dyn Preprocessor) -> Result<()> {
    let (ctx, book) = mdbook_preprocessor::parse_input(io::stdin())
        .context("parsing preprocessor input from stdin")?;

    let book_version = Version::parse(&ctx.mdbook_version).context("parsing mdBook version")?;
    let supported =
        VersionReq::parse(MDBOOK_VERSION).context("parsing supported mdBook version")?;
    if !supported.matches(&book_version) {
        eprintln!(
            "mdbook-tsitter: built against mdBook {MDBOOK_VERSION}, running under {} — continuing",
            ctx.mdbook_version,
        );
    }

    let processed = preprocessor.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed).context("writing processed book to stdout")?;
    Ok(())
}
```


```rust
//! Finding fenced code blocks in a chapter and replacing the ones we can
//! highlight with pre-rendered HTML. We locate blocks by byte offset with
//! pulldown-cmark (the same parser mdBook uses) and splice raw HTML in place,
//! which mdBook then passes through untouched.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::grammar::Registry;

pub fn rewrite(content: &str, registry: &Registry) -> String {
    let mut replacements: Vec<(Range<usize>, String)> = Vec::new();
    let mut open: Option<OpenBlock> = None;

    let parser = Parser::new_ext(content, Options::all()).into_offset_iter();
    for (event, range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                open = Some(OpenBlock {
                    start: range.start,
                    top_level: at_line_start(content, range.start),
                    skipped: fence_tokens(&info).any(|token| token == SKIP_TAG),
                    lang: fence_language(&info).to_string(),
                    code: String::new(),
                });
            }
            Event::Text(text) => {
                if let Some(block) = open.as_mut() {
                    block.code.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(block) = open.take() {
                    if let Some(html) = render_block(&block, registry) {
                        replacements.push((block.start..range.end, html));
                    }
                }
            }
            _ => {}
        }
    }

    splice(content, replacements)
}

const SKIP_TAG: &str = "notreesitter";

struct OpenBlock {
    start: usize,
    top_level: bool,
    skipped: bool,
    lang: String,
    code: String,
}

/// The whitespace/comma-separated tokens of a fence info string, e.g.
/// `rust,no_run` -> `rust`, `no_run`.
fn fence_tokens(info: &str) -> impl Iterator<Item = &str> {
    info.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|token| !token.is_empty())
}

fn fence_language(info: &str) -> &str {
    fence_tokens(info).next().unwrap_or_default()
}

fn at_line_start(content: &str, offset: usize) -> bool {
    offset == 0 || content.as_bytes()[offset - 1] == b'\n'
}

fn render_block(block: &OpenBlock, registry: &Registry) -> Option<String> {
    if block.skipped || !block.top_level {
        return None;
    }
    let highlighted = match registry.highlight(&block.lang, &block.code)? {
        Ok(html) => html,
        Err(error) => {
            eprintln!(
                "mdbook-tsitter: skipping `{}` block: {error:#}",
                block.lang
            );
            return None;
        }
    };
    Some(format!(
        "\n<pre class=\"treesitter\"><code class=\"no-highlight language-{lang}\">{highlighted}</code></pre>\n",
        lang = block.lang,
    ))
}
fn splice(content: &str, mut replacements: Vec<(Range<usize>, String)>) -> String {
    if replacements.is_empty() {
        return content.to_string();
    }
    replacements.sort_by_key(|(range, _)| range.start);
    let mut out = content.to_string();
    for (range, html) in replacements.into_iter().rev() {
        out.replace_range(range, &html);
    }
    out
}
```
