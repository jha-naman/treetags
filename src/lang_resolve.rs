//! Pure helpers for language resolution.
//!
//! Currently hosts the filename-pattern (glob) matcher used to map names like
//! `Makefile`, `Rakefile`, or `*.gemspec` to a language. Kept free of registry
//! state so it can be unit-tested in isolation.

/// Matches a filename against an `fnmatch`-style glob pattern.
///
/// Supported syntax (case-sensitive, matched against the whole basename):
/// - `*` — any run of characters (including empty)
/// - `?` — exactly one character
/// - `[...]` — a character class, with ranges (`a-z`) and negation (`[!...]`
///   or `[^...]`); a literal `]` is allowed as the first class member
///
/// There is no path-separator special-casing: patterns are only ever matched
/// against a basename, so `*` spans the entire name.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    // Position to resume from on mismatch: (index of '*', name index it covers).
    let mut backtrack: Option<(usize, usize)> = None;

    while ni < n.len() {
        let mut advanced = false;
        if pi < p.len() {
            if p[pi] == '*' {
                backtrack = Some((pi, ni));
                pi += 1;
                continue;
            }
            let (matched, next_pi) = match_single(&p, pi, n[ni]);
            if matched {
                pi = next_pi;
                ni += 1;
                advanced = true;
            }
        }
        if !advanced {
            match backtrack {
                Some((star_pi, star_ni)) => {
                    pi = star_pi + 1;
                    ni = star_ni + 1;
                    backtrack = Some((star_pi, star_ni + 1));
                }
                None => return false,
            }
        }
    }

    // Any remaining pattern must be all '*' to match the empty tail.
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Matches the single pattern token at `p[pi]` against char `c`.
/// Returns `(matched, index just past the token)`.
fn match_single(p: &[char], pi: usize, c: char) -> (bool, usize) {
    match p[pi] {
        '?' => (true, pi + 1),
        '[' => match_class(p, pi, c),
        lit => (lit == c, pi + 1),
    }
}

/// Matches a `[...]` character class starting at `p[start]` (`'['`).
/// Falls back to treating `[` as a literal if the class is unterminated.
fn match_class(p: &[char], start: usize, c: char) -> (bool, usize) {
    let mut i = start + 1;
    let mut negate = false;
    if i < p.len() && (p[i] == '!' || p[i] == '^') {
        negate = true;
        i += 1;
    }
    let mut matched = false;
    let mut first = true;
    while i < p.len() && (p[i] != ']' || first) {
        first = false;
        if i + 2 < p.len() && p[i + 1] == '-' && p[i + 2] != ']' {
            if p[i] <= c && c <= p[i + 2] {
                matched = true;
            }
            i += 3;
        } else {
            if p[i] == c {
                matched = true;
            }
            i += 1;
        }
    }
    if i < p.len() && p[i] == ']' {
        (matched ^ negate, i + 1)
    } else {
        // Unterminated class: treat the opening '[' as a literal character.
        ('[' == c, start + 1)
    }
}

/// Extracts the interpreter name from a `#!` shebang at the very start of
/// `content`. Only the first line is inspected.
///
/// Handles absolute interpreters (`#!/bin/sh` → `sh`, `#!/usr/bin/python3` →
/// `python3`) and the `env` form (`#!/usr/bin/env python3` → `python3`,
/// including `env -S`/`env VAR=val` prefixes). Returns the interpreter's
/// basename, or `None` when there is no shebang or no interpreter follows.
pub fn parse_shebang(content: &[u8]) -> Option<String> {
    let line_end = content
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(content.len());
    let rest = content[..line_end].strip_prefix(b"#!")?;
    let text = std::str::from_utf8(rest).ok()?;

    let mut tokens = text.split_whitespace();
    let first = basename(tokens.next()?);
    if first == "env" {
        // Skip `env` options (`-S`, `-i`, …) and `VAR=value` assignments.
        for tok in tokens {
            if tok.starts_with('-') || tok.contains('=') {
                continue;
            }
            return Some(basename(tok).to_string());
        }
        return None;
    }
    Some(first.to_string())
}

/// Returns the final path component of `s`, splitting on `/` or `\`.
fn basename(s: &str) -> &str {
    s.rsplit(['/', '\\']).next().unwrap_or(s)
}

/// Heuristically decides whether a header's content is C++ rather than C, for
/// disambiguating ambiguous extensions like `.h`.
///
/// Looks for high-precision C++-only signals in a bounded content prefix.
/// Returns `false` (i.e. "assume C") when no signal is present — matching
/// ctags, which defaults `.h` to C. The signals are chosen to rarely appear in
/// plain C or in prose comments; this is a heuristic, not a parser.
pub fn looks_like_cpp(prefix: &[u8]) -> bool {
    const SIGNALS: &[&str] = &[
        "::",
        "namespace",
        "template<",
        "template <",
        "class ",
        "public:",
        "private:",
        "protected:",
        "virtual ",
        "nullptr",
        "using namespace",
        "extern \"C\"",
        "std::",
    ];
    let text = String::from_utf8_lossy(prefix);
    SIGNALS.iter().any(|sig| text.contains(sig))
}

#[cfg(test)]
mod tests {
    use super::{glob_match, looks_like_cpp, parse_shebang};

    #[test]
    fn exact_and_literal() {
        assert!(glob_match("Makefile", "Makefile"));
        assert!(!glob_match("Makefile", "makefile"));
        assert!(!glob_match("Makefile", "Makefile.in"));
        assert!(glob_match(".bashrc", ".bashrc"));
    }

    #[test]
    fn star() {
        assert!(glob_match("*.gemspec", "foo.gemspec"));
        assert!(glob_match("*.gemspec", ".gemspec"));
        assert!(!glob_match("*.gemspec", "foo.gemspecs"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
        assert!(glob_match("Makefile.*", "Makefile.am"));
        assert!(glob_match("a*b*c", "axxbyyc"));
        assert!(!glob_match("a*b*c", "axxbyy"));
    }

    #[test]
    fn question() {
        assert!(glob_match("?.c", "a.c"));
        assert!(!glob_match("?.c", "ab.c"));
        assert!(!glob_match("?", ""));
    }

    #[test]
    fn char_class() {
        assert!(glob_match("[Mm]akefile", "Makefile"));
        assert!(glob_match("[Mm]akefile", "makefile"));
        assert!(!glob_match("[Mm]akefile", "Xakefile"));
        assert!(glob_match("file[0-9]", "file7"));
        assert!(!glob_match("file[0-9]", "filea"));
        assert!(glob_match("file[!0-9]", "filea"));
        assert!(!glob_match("file[!0-9]", "file7"));
    }

    #[test]
    fn unterminated_class_is_literal() {
        assert!(glob_match("[abc", "[abc"));
        assert!(!glob_match("[abc", "a"));
    }

    #[test]
    fn shebang_absolute() {
        assert_eq!(parse_shebang(b"#!/bin/sh\n").as_deref(), Some("sh"));
        assert_eq!(
            parse_shebang(b"#!/usr/bin/python3\nprint(1)").as_deref(),
            Some("python3")
        );
        assert_eq!(parse_shebang(b"#!/bin/bash").as_deref(), Some("bash"));
    }

    #[test]
    fn shebang_env_form() {
        assert_eq!(
            parse_shebang(b"#!/usr/bin/env python3\n").as_deref(),
            Some("python3")
        );
        assert_eq!(
            parse_shebang(b"#!/usr/bin/env -S python3 -u\n").as_deref(),
            Some("python3")
        );
        assert_eq!(
            parse_shebang(b"#!/usr/bin/env FOO=bar ruby\n").as_deref(),
            Some("ruby")
        );
    }

    #[test]
    fn cpp_detection() {
        assert!(looks_like_cpp(b"class Widget {\npublic:\n  void run();\n};"));
        assert!(looks_like_cpp(b"namespace app {\n}"));
        assert!(looks_like_cpp(b"template <typename T>\nT id(T x);"));
        assert!(looks_like_cpp(b"int n = std::max(a, b);"));
        assert!(looks_like_cpp(b"extern \"C\" {\n#include <foo.h>\n}"));
        // Plain C headers have no C++ signal -> assume C.
        assert!(!looks_like_cpp(
            b"#ifndef FOO_H\n#define FOO_H\nint add(int a, int b);\n#endif\n"
        ));
        assert!(!looks_like_cpp(b"typedef struct { int x; } Point;"));
    }

    #[test]
    fn shebang_edge_cases() {
        assert_eq!(parse_shebang(b"#! /bin/sh\n").as_deref(), Some("sh"));
        assert_eq!(parse_shebang(b"#!/bin/sh\r\n").as_deref(), Some("sh"));
        assert_eq!(parse_shebang(b"no shebang here").as_deref(), None);
        assert_eq!(parse_shebang(b"").as_deref(), None);
        assert_eq!(parse_shebang(b"#!").as_deref(), None);
        assert_eq!(parse_shebang(b"#!/usr/bin/env\n").as_deref(), None);
    }
}
