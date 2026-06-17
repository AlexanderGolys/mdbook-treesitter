# mdbook-tree-sitter

An [mdBook](https://rust-lang.github.io/mdBook/) preprocessor that highlights
fenced code blocks with [tree-sitter](https://tree-sitter.github.io/). It works
for **any** language that has a tree-sitter grammar: point it at a compiled
parser and a highlights query and it highlights that language. Macaulay2 is
bundled by default, so ```` ```m2 ```` blocks highlight with no extra setup.

Highlighting happens at build time. Each tree-sitter capture becomes a
`<span class="ts-…">`, so colours live in a stylesheet (like the rest of
mdBook's theming) rather than being baked into the HTML.

## How it works

A preprocessor receives each chapter as Markdown and returns modified Markdown.
This one parses the Markdown with the same parser mdBook uses
([pulldown-cmark](https://docs.rs/pulldown-cmark)), finds every fenced code
block whose info string names a known grammar, highlights its contents, and
splices in a ready-made HTML block:

```html
<pre class="tree-sitter"><code class="no-highlight language-m2">…spans…</code></pre>
```

The `no-highlight` class stops mdBook's default highlight.js from touching the
spans. Blocks in unknown languages (or with no language tag) are left exactly as
written, so mdBook's default highlighter still handles them.

## Install

```sh
cargo install --path .            # bundles the Macaulay2 grammar (default)
cargo install --path . --no-default-features   # language-agnostic; configure your own grammars
```

The `mdbook-tree-sitter` binary must be on your `PATH`.

## Set up a book

```toml
# book.toml
[preprocessor.tree-sitter]

[output.html]
additional-css = ["theme/tree-sitter.css"]
```

Copy [`assets/tree-sitter.css`](assets/tree-sitter.css) to your book's
`theme/tree-sitter.css` (or wherever `additional-css` points) and adjust the
colours to taste. See [`example/`](example/) for a complete, buildable book.

## Opting out of the bundled grammar

Macaulay2 is bundled only as a convenience. There are two ways to drop it:

- **Per book, no rebuild** — set `bundled = false` to ignore every grammar
  compiled into the binary and highlight only the languages you configure:

  ```toml
  [preprocessor.tree-sitter]
  bundled = false
  ```

- **Per binary** — `cargo install --no-default-features` builds a
  language-agnostic binary that carries no grammar at all.

A configured language always overrides a bundled grammar that shares an alias,
so you can also just replace `m2` rather than disabling it.

## Adding a language

Bundled grammars need no configuration. Any other language is added by pointing
the preprocessor at a compiled parser shared object and a highlights query:

```toml
[preprocessor.tree-sitter.languages.nix]
library = "parsers/libtree-sitter-nix.so"   # compiled grammar, relative to the book root
highlights = "queries/nix/highlights.scm"    # tree-sitter highlights query
# symbol = "tree_sitter_nix"                  # optional; defaults to tree_sitter_<name>
# injections = "queries/nix/injections.scm"   # optional, for embedded languages
# locals = "queries/nix/locals.scm"           # optional, for scope-aware highlighting
# aliases = ["nix"]                            # fence tags; defaults to the table key
```

The table key (`nix` here) is the default code-fence tag and the default symbol
suffix. A configured language overrides a bundled grammar that shares an alias.

### Getting a parser shared object and queries

Most grammars live in a `tree-sitter-<lang>` repository. With the
[tree-sitter CLI](https://github.com/tree-sitter/tree-sitter):

```sh
git clone https://github.com/<owner>/tree-sitter-nix
cd tree-sitter-nix
tree-sitter build --output libtree-sitter-nix.so
```

The highlights query is the grammar's `queries/highlights.scm`.

## Capture names and CSS classes

There is no fixed list of supported captures — the preprocessor reads the
capture names out of each grammar's own query, so it supports whatever that
query defines. A capture becomes a class with the `ts-` prefix and dots turned
into hyphens, and every prefix is emitted so broad rules cascade and specific
ones override:

| capture             | classes                              |
| ------------------- | ------------------------------------ |
| `keyword`           | `ts-keyword`                         |
| `keyword.operator`  | `ts-keyword ts-keyword-operator`     |
| `string.regexp`     | `ts-string ts-string-regexp`         |

The bundled [`assets/tree-sitter.css`](assets/tree-sitter.css) styles the
[standard tree-sitter / nvim-treesitter capture names](https://github.com/nvim-treesitter/nvim-treesitter/blob/main/CONTRIBUTING.md#highlights)
(`@comment`, `@keyword`, `@string`, `@function`, `@type`, `@variable`, …), so a
grammar whose query uses those names is styled out of the box. Add rules for any
extra captures your grammar defines.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
