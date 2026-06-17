# Contributing

Thanks for your interest in `mdbook-treesitter`. The crate is deliberately
small and grammar-agnostic — it ships no grammar of its own and highlights
whatever you configure in `book.toml`.

## Building

```sh
cargo build
cargo clippy --all
cargo fmt
```

Clippy should be clean and `cargo fmt` applied before a change is considered
done. Public items carry doc comments.

## Running the example

The book under [`examples/languages`](examples/languages) doubles as the
documentation site and as an integration test across several grammars. Grammars
are external (compiled parsers + third-party queries), so they are staged
locally rather than committed:

```sh
cd examples/languages
./setup.sh            # stage parsers/ and queries/ (gitignored)
mdbook build          # needs the mdbook-treesitter binary on PATH
```

`setup.sh` copies parsers and queries from a local nvim-treesitter install by
default; override the source paths with the environment variables documented at
the top of the script.

## Scope

The package stays language-agnostic: please do not add grammars or
language-specific behaviour to the crate itself. New languages belong in a
book's configuration (and, for the example, in `setup.sh`).

## Pull requests

- Keep changes focused and the commit history readable.
- Match the surrounding code's style and comment density.
- Update the README and the example when behaviour or configuration changes.
