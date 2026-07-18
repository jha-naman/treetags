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

/// Extracts a language/mode name from an editor modeline in the file's head or
/// tail. Recognizes Vim modelines (`vim: set ft=…:`, `vim: ft=…`) and Emacs
/// modelines (first-line `-*- mode: … -*-` / `-*- … -*-`, and a trailing
/// `Local Variables:` … `mode: …` … `End:` block).
///
/// Returns the raw mode name (e.g. `cpp`, `python-mode`); mapping it to a
/// treetags language is the caller's job. Only the first match wins.
pub fn parse_modeline(head: &[u8], tail: &[u8]) -> Option<String> {
    const SCAN_LINES: usize = 5;
    let head = String::from_utf8_lossy(head);
    let tail = String::from_utf8_lossy(tail);
    // For small files the whole content is in `head` and `tail` is empty, so
    // tail-based checks fall back to the head region.
    let tail_region: &str = if tail.is_empty() {
        head.as_ref()
    } else {
        tail.as_ref()
    };

    // Emacs first-line form.
    if let Some(first) = head.lines().next() {
        if let Some(mode) = parse_emacs_first_line(first) {
            return Some(mode);
        }
    }
    // Vim modelines: first N lines of the head, then last N lines of the tail.
    for line in head.lines().take(SCAN_LINES) {
        if let Some(ft) = parse_vim_modeline(line) {
            return Some(ft);
        }
    }
    let tail_lines: Vec<&str> = tail_region.lines().collect();
    for line in tail_lines.iter().rev().take(SCAN_LINES) {
        if let Some(ft) = parse_vim_modeline(line) {
            return Some(ft);
        }
    }
    // Emacs `Local Variables:` block, near the end of the file.
    parse_emacs_local_vars(tail_region)
}

/// Byte offset just past a Vim modeline marker (`vim:`/`vi:`/`ex:`) that starts
/// the line or follows whitespace, or `None`.
fn find_vim_marker(line: &str) -> Option<usize> {
    const MARKERS: &[&[u8]] = &[b"vim:", b"Vim:", b"vi:", b"ex:"];
    let b = line.as_bytes();
    for i in 0..b.len() {
        for m in MARKERS {
            if b[i..].starts_with(m) && (i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t') {
                return Some(i + m.len());
            }
        }
    }
    None
}

/// Returns the prefix of `s` up to the first unescaped `:` (the terminator of a
/// Vim `set` modeline), or all of `s`.
fn take_until_unescaped_colon(s: &str) -> &str {
    let b = s.as_bytes();
    for i in 0..b.len() {
        if b[i] == b':' && (i == 0 || b[i - 1] != b'\\') {
            return &s[..i];
        }
    }
    s
}

fn parse_vim_modeline(line: &str) -> Option<String> {
    let rest = line[find_vim_marker(line)?..].trim_start();
    // `set` form: space-separated options terminated by `:`. Otherwise the
    // options are `:`-separated.
    let (opts_str, space_separated) = match rest
        .strip_prefix("set ")
        .or_else(|| rest.strip_prefix("se "))
    {
        Some(after) => (take_until_unescaped_colon(after), true),
        None => (rest, false),
    };
    let opts: Vec<&str> = if space_separated {
        opts_str.split_whitespace().collect()
    } else {
        opts_str.split(':').collect()
    };
    for opt in opts {
        for key in ["ft=", "filetype=", "syntax="] {
            if let Some(val) = opt.trim().strip_prefix(key) {
                let val = val.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

fn parse_emacs_first_line(line: &str) -> Option<String> {
    let start = line.find("-*-")?;
    let after = &line[start + 3..];
    let inner = after[..after.find("-*-")?].trim();
    if inner.contains(':') {
        // e.g. `coding: utf-8; mode: c++`
        for part in inner.split(';') {
            let part = part.trim();
            if let Some(val) = part
                .strip_prefix("mode:")
                .or_else(|| part.strip_prefix("Mode:"))
            {
                let val = val.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
        None
    } else if inner.is_empty() {
        None
    } else {
        // Bare `-*- ruby -*-`.
        Some(inner.to_string())
    }
}

fn parse_emacs_local_vars(tail: &str) -> Option<String> {
    let start = tail.find("Local Variables:")?;
    let block = &tail[start..];
    let block = block.find("End:").map_or(block, |e| &block[..e]);
    for line in block.lines() {
        let Some(pos) = line.find("mode:") else {
            continue;
        };
        // Require a standalone `mode:` variable, not a suffix like `foo-mode:`.
        let standalone = line[..pos]
            .chars()
            .last()
            .map_or(true, |c| !c.is_alphanumeric() && c != '-');
        if !standalone {
            continue;
        }
        let val = line[pos + 5..]
            .trim()
            .split(|c: char| c.is_whitespace() || c == ';')
            .next()
            .unwrap_or("")
            .trim();
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{glob_match, looks_like_cpp, parse_modeline, parse_shebang};

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
    fn modeline_vim() {
        let m = |s: &[u8]| parse_modeline(s, b"");
        assert_eq!(m(b"/* vim: set ft=ruby: */").as_deref(), Some("ruby"));
        assert_eq!(m(b"// vim:ft=python").as_deref(), Some("python"));
        assert_eq!(m(b"# vim: ts=4:ft=sh:noexpandtab").as_deref(), Some("sh"));
        assert_eq!(m(b"-- vim: set filetype=lua :").as_deref(), Some("lua"));
        // Marker must follow whitespace/start; not part of a word.
        assert_eq!(m(b"servicevim:ft=c").as_deref(), None);
        assert_eq!(m(b"no modeline here").as_deref(), None);
    }

    #[test]
    fn modeline_vim_in_tail() {
        // Vim modelines are honored in the last lines too.
        let tail = b"int main() {}\n// vim: set ft=cpp:\n";
        assert_eq!(parse_modeline(b"", tail).as_deref(), Some("cpp"));
    }

    #[test]
    fn modeline_emacs() {
        let m = |s: &[u8]| parse_modeline(s, b"");
        assert_eq!(m(b"# -*- mode: python -*-").as_deref(), Some("python"));
        assert_eq!(m(b";; -*- ruby -*-").as_deref(), Some("ruby"));
        assert_eq!(
            m(b"/* -*- coding: utf-8; mode: c++ -*- */").as_deref(),
            Some("c++")
        );
    }

    #[test]
    fn modeline_emacs_local_variables() {
        let block = b"code\n# Local Variables:\n# mode: perl\n# tab-width: 4\n# End:\n";
        // In the tail (large file) ...
        assert_eq!(parse_modeline(b"", block).as_deref(), Some("perl"));
        // ... and in the head with an empty tail (small file).
        assert_eq!(parse_modeline(block, b"").as_deref(), Some("perl"));
        // `foo-mode:` must not be mistaken for the major-mode variable.
        let block2 = b"# Local Variables:\n# whitespace-mode: t\n# End:\n";
        assert_eq!(parse_modeline(block2, b""), None);
    }

    #[test]
    fn cpp_detection() {
        assert!(looks_like_cpp(
            b"class Widget {\npublic:\n  void run();\n};"
        ));
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
