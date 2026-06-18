//! Normalising Neovim-specific query predicates into the standard tree-sitter
//! ones that [`tree_sitter_highlight`] actually evaluates.
//!
//! nvim-treesitter queries use `#lua-match?` / `#not-lua-match?`, whose argument
//! is a [Lua pattern] rather than a regex. `tree-sitter-highlight` does not know
//! these predicates, and — crucially — it does not drop the patterns that carry
//! them: it simply ignores the predicate, so the capture applies
//! *unconditionally*. A rule meant to fire only on, say, all-caps identifiers
//! (`((identifier) @constant (#lua-match? @constant "^[A-Z][A-Z%d_]*$"))`) then
//! colours *every* identifier as a constant.
//!
//! [`normalize`] rewrites each `#lua-match?` into the standard `#match?` (and
//! `#not-lua-match?` into `#not-match?`), translating the Lua pattern to the
//! equivalent regex, so the predicate is evaluated as the grammar author meant.
//!
//! [Lua pattern]: https://www.lua.org/manual/5.4/manual.html#6.4.1

/// The Neovim predicate names and the standard predicates they map to.
const PREDICATE_REWRITES: [(&str, &str); 2] = [
    ("#not-lua-match?", "#not-match?"),
    ("#lua-match?", "#match?"),
];

/// Rewrite every Neovim `#lua-match?` / `#not-lua-match?` predicate in `query`
/// into the standard `#match?` / `#not-match?`, translating its Lua-pattern
/// argument to a regex. Queries without these predicates are returned unchanged.
pub fn normalize(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    let mut rest = query;

    while let Some((standard, after_name, consumed)) = next_rewrite(rest) {
        out.push_str(&rest[..consumed]);
        out.push_str(standard);
        // The pattern is the next string literal after the capture name(s).
        match split_pattern_literal(after_name) {
            Some((before, lua_pattern, tail)) => {
                out.push_str(before);
                out.push('"');
                out.push_str(&escape_string_literal(&lua_pattern_to_regex(&lua_pattern)));
                out.push('"');
                rest = tail;
            }
            // No string literal follows (malformed predicate): leave the rest as
            // is rather than guessing.
            None => rest = after_name,
        }
    }
    out.push_str(rest);
    out
}

/// Find the next Neovim predicate to rewrite. Returns the standard replacement
/// name, the slice immediately after the original name, and the byte offset of
/// the original name within `query`.
fn next_rewrite(query: &str) -> Option<(&'static str, &str, usize)> {
    PREDICATE_REWRITES
        .iter()
        .filter_map(|&(nvim, standard)| {
            query
                .find(nvim)
                .map(|at| (at, standard, &query[at + nvim.len()..]))
        })
        .min_by_key(|&(at, _, _)| at)
        .map(|(at, standard, after)| (standard, after, at))
}

/// Split `after_name` at the first string literal, returning the text before the
/// opening quote, the (unescaped) literal contents, and the text after the
/// closing quote.
fn split_pattern_literal(after_name: &str) -> Option<(&str, String, &str)> {
    let open = after_name.find('"')?;
    let mut contents = String::new();
    let mut chars = after_name[open + 1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    contents.push(escaped);
                }
            }
            '"' => {
                let close = open + 1 + i;
                return Some((&after_name[..open], contents, &after_name[close + 1..]));
            }
            _ => contents.push(c),
        }
    }
    None
}

/// Re-escape a regex so it can be embedded back into a query string literal.
fn escape_string_literal(regex: &str) -> String {
    regex.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Translate a [Lua pattern] into an equivalent Rust `regex`.
///
/// Supports the constructs that appear in tree-sitter highlight queries: the
/// `%`-classes (`%d`, `%a`, `%s`, …, and their negations), `%`-escaped
/// literals, character sets `[...]` (including `%`-classes inside them), the
/// anchors `^`/`$`, the quantifiers `*`/`+`/`?`, and Lua's lazy `-` quantifier.
/// Characters that are literal in Lua but special in regex (`{`, `}`, `|`) are
/// escaped.
///
/// [Lua pattern]: https://www.lua.org/manual/5.4/manual.html#6.4.1
fn lua_pattern_to_regex(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    let mut in_set = false;

    while let Some(c) = chars.next() {
        match c {
            '%' => match chars.next() {
                Some(class) => out.push_str(&translate_percent(class, in_set)),
                None => out.push('%'),
            },
            '[' if !in_set => {
                in_set = true;
                out.push('[');
            }
            ']' => {
                in_set = false;
                out.push(']');
            }
            // Lua's `-` is a lazy zero-or-more quantifier; regex spells it `*?`.
            // At the very start there is nothing to quantify, so treat it as a
            // literal dash.
            '-' if !in_set => out.push_str(if out.is_empty() { "\\-" } else { "*?" }),
            // Literal in Lua, special in regex.
            '{' | '}' | '|' if !in_set => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Translate the character following a `%` in a Lua pattern. `in_set` selects
/// between a standalone class (`[0-9]`) and the bare contents used inside an
/// existing `[...]` set (`0-9`).
fn translate_percent(class: char, in_set: bool) -> String {
    let ranges = match class {
        'a' | 'A' => "a-zA-Z",
        'd' | 'D' => "0-9",
        'l' | 'L' => "a-z",
        'u' | 'U' => "A-Z",
        's' | 'S' => " \\t\\n\\r\\f\\x0b",
        'w' | 'W' => "0-9a-zA-Z",
        'x' | 'X' => "0-9a-fA-F",
        'p' | 'P' => "!-/:-@\\[-`{-~",
        // Not a class letter: a `%`-escaped literal (e.g. `%.`, `%%`, `%-`).
        // Escape it for regex when it is a metacharacter.
        other => return escape_regex_literal(other),
    };
    let negated = class.is_ascii_uppercase();
    match (in_set, negated) {
        // Negated classes have no in-set spelling; emit a standalone class even
        // inside a set is not valid, so callers only negate outside sets in
        // practice. Emit a standalone negated class.
        (_, true) => format!("[^{ranges}]"),
        (true, false) => ranges.to_string(),
        (false, false) => format!("[{ranges}]"),
    }
}

/// Escape a single character that was `%`-escaped in a Lua pattern so it is a
/// literal in regex.
fn escape_regex_literal(c: char) -> String {
    const REGEX_META: &str = r".^$*+?()[]{}|\";
    if REGEX_META.contains(c) {
        format!("\\{c}")
    } else {
        c.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_class_inside_set() {
        assert_eq!(
            lua_pattern_to_regex("^[A-Z][A-Z%d_]*$"),
            "^[A-Z][A-Z0-9_]*$"
        );
    }

    #[test]
    fn translates_standalone_class() {
        assert_eq!(lua_pattern_to_regex("%s?@"), "[ \\t\\n\\r\\f\\x0b]?@");
    }

    #[test]
    fn passes_through_plain_anchored_set() {
        assert_eq!(lua_pattern_to_regex("^[A-Z]"), "^[A-Z]");
        assert_eq!(
            lua_pattern_to_regex("^[A-Z][A-Z_0-9]*$"),
            "^[A-Z][A-Z_0-9]*$"
        );
    }

    #[test]
    fn literal_dash_in_set_is_preserved() {
        assert_eq!(lua_pattern_to_regex("^[-][-][-]"), "^[-][-][-]");
        assert_eq!(
            lua_pattern_to_regex("^[-][-](%s?)@"),
            "^[-][-]([ \\t\\n\\r\\f\\x0b]?)@"
        );
    }

    #[test]
    fn escapes_percent_literals() {
        assert_eq!(lua_pattern_to_regex("a%.b"), "a\\.b");
        assert_eq!(lua_pattern_to_regex("100%%"), "100%");
    }

    #[test]
    fn rewrites_lua_match_predicate() {
        let query = r#"((identifier) @constant (#lua-match? @constant "^[A-Z][A-Z%d_]*$"))"#;
        let expected = r#"((identifier) @constant (#match? @constant "^[A-Z][A-Z0-9_]*$"))"#;
        assert_eq!(normalize(query), expected);
    }

    #[test]
    fn rewrites_not_lua_match_predicate() {
        let query = r#"(#not-lua-match? @v "^[A-Z]")"#;
        let expected = r#"(#not-match? @v "^[A-Z]")"#;
        assert_eq!(normalize(query), expected);
    }

    #[test]
    fn leaves_queries_without_lua_match_untouched() {
        let query = "(identifier) @variable\n((symbol) @b (#eq? @b \"x\"))";
        assert_eq!(normalize(query), query);
    }
}
