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

#[cfg(test)]
mod tests {
    use super::glob_match;

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
}
