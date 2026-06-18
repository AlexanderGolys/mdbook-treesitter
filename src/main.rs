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

/// The default class-based stylesheet, embedded so it can be emitted without
/// the source checkout.
const DEFAULT_CSS: &str = include_str!("../assets/treesitter.css");

#[derive(Subcommand)]
enum Command {
    /// Answer mdBook's renderer-support query: exit 0 if supported, 1 if not.
    Supports {
        /// The renderer mdBook is about to run.
        renderer: String,
    },
    /// Print the default theme to stdout, e.g.
    /// `mdbook-tsitter css > theme/treesitter.css`, then reference it from
    /// `[output.html] additional-css`.
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

/// Read `[context, book]` from stdin, transform, and write the book to stdout.
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
