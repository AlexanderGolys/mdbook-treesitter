# Example book

A buildable book that doubles as the project's documentation site, a usage
example, and an integration test across several grammars. Its chapters are:

- **Introduction** — the root [`README.md`](../../README.md), pulled in with
  `{{#include}}` (the file stays at the repo root; nothing is copied).
- **Highlighting demos** — one chapter per feature, each explaining itself above
  the code:
  - *Macaulay2* — a well-formed block and one with a syntax error (which still
    highlights, since tree-sitter recovers).
  - *Five languages* — one block each in Macaulay2, Rust, Lua, Haskell and
    Markdown.
  - *Injection* — a Markdown block with embedded `lua` (configured, so it is
    sub-highlighted) and `c` (not configured, so it degrades to plain text).
- **Contributing** — the root [`CONTRIBUTING.md`](../../CONTRIBUTING.md), also
  via `{{#include}}`.

Grammars are external (compiled parsers + third-party queries), so they are not
committed. [`setup.sh`](setup.sh) stages them into `parsers/` and `queries/`
(both gitignored), copying from a local nvim-treesitter install by default.

```sh
./setup.sh          # stage parsers + queries (override sources via env vars)
mdbook build        # uses the mdbook-treesitter binary on your PATH
```
