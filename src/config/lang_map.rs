//! Parsing and representation of user langmap edits (`--map-<LANG>` and
//! `--langmap`), applied over the built-in extension/pattern defaults when the
//! `LanguageParserRegistry` is built.
//!
//! Syntax mirrors Universal Ctags:
//! - `--map-<LANG>=[+|-]<item>` — add (`+`, the default) or remove (`-`) one
//!   `.ext` extension, `(pattern)` glob, or `%rexpr%` relative-path regular
//!   expression. A regex may take a trailing `i`/`{icase}` case-insensitive
//!   flag, and a literal `%` inside it is escaped as `\%`. Repeatable.
//! - `--langmap=<LANG>:<spec>[,<LANG>:<spec>...]` — bulk. A leading `+` on the
//!   spec appends; otherwise it replaces the language's name mappings. `<spec>`
//!   is a run of `.ext` and `(pattern)` tokens, e.g. `.c.h` or `(Makefile).mak`.
//!   (Regexes are only supported via `--map-<LANG>`, matching ctags.)

use regex::Regex;
use std::collections::HashMap;

/// The ordered set of langmap edits gathered from the command line.
#[derive(Clone, Debug, Default)]
pub struct LangMapEdits {
    pub edits: Vec<LangMapEdit>,
}

/// A single edit to a language's name-based mappings.
#[derive(Clone, Debug, PartialEq)]
pub enum LangMapEdit {
    AddExt {
        lang: String,
        ext: String,
    },
    RemoveExt {
        lang: String,
        ext: String,
    },
    AddPattern {
        lang: String,
        pattern: String,
    },
    RemovePattern {
        lang: String,
        pattern: String,
    },
    /// A relative-path regular expression, with an optional case-insensitive
    /// flag. `regex` is the raw pattern (with `\%` already unescaped to `%`).
    AddRexpr {
        lang: String,
        regex: String,
        icase: bool,
    },
    RemoveRexpr {
        lang: String,
        regex: String,
        icase: bool,
    },
    /// Replace all of a language's extensions and patterns.
    Replace {
        lang: String,
        exts: Vec<String>,
        patterns: Vec<String>,
    },
}

impl LangMapEdit {
    /// The language name this edit targets.
    pub fn lang(&self) -> &str {
        match self {
            LangMapEdit::AddExt { lang, .. }
            | LangMapEdit::RemoveExt { lang, .. }
            | LangMapEdit::AddPattern { lang, .. }
            | LangMapEdit::RemovePattern { lang, .. }
            | LangMapEdit::AddRexpr { lang, .. }
            | LangMapEdit::RemoveRexpr { lang, .. }
            | LangMapEdit::Replace { lang, .. } => lang,
        }
    }

    /// Applies this edit to the registry's `by_extension` / `by_pattern` /
    /// `by_rexpr` tables, mapping to the resolved language id. New mappings take
    /// precedence (are inserted at the front) over existing candidates.
    pub fn apply(
        &self,
        id: usize,
        by_ext: &mut HashMap<String, Vec<usize>>,
        by_pat: &mut Vec<(String, usize)>,
        by_rexpr: &mut Vec<(Regex, usize)>,
    ) {
        match self {
            LangMapEdit::AddExt { ext, .. } => {
                let v = by_ext.entry(ext.clone()).or_default();
                if !v.contains(&id) {
                    v.insert(0, id);
                }
            }
            LangMapEdit::RemoveExt { ext, .. } => {
                if let Some(v) = by_ext.get_mut(ext) {
                    v.retain(|&x| x != id);
                }
                if by_ext.get(ext).is_some_and(Vec::is_empty) {
                    by_ext.remove(ext);
                }
            }
            LangMapEdit::AddPattern { pattern, .. } => {
                if !by_pat.iter().any(|(p, i)| p == pattern && *i == id) {
                    by_pat.insert(0, (pattern.clone(), id));
                }
            }
            LangMapEdit::RemovePattern { pattern, .. } => {
                by_pat.retain(|(p, i)| !(p == pattern && *i == id));
            }
            LangMapEdit::AddRexpr { regex, icase, .. } => match compile_rexpr(regex, *icase) {
                Ok(re) => {
                    if !by_rexpr
                        .iter()
                        .any(|(r, i)| r.as_str() == re.as_str() && *i == id)
                    {
                        by_rexpr.insert(0, (re, id));
                    }
                }
                Err(e) => eprintln!("treetags: invalid --map regex %{regex}%: {e}"),
            },
            LangMapEdit::RemoveRexpr { regex, icase, .. } => {
                if let Ok(re) = compile_rexpr(regex, *icase) {
                    let target = re.as_str().to_string();
                    by_rexpr.retain(|(r, i)| !(r.as_str() == target && *i == id));
                }
            }
            LangMapEdit::Replace { exts, patterns, .. } => {
                by_ext.retain(|_, v| {
                    v.retain(|&x| x != id);
                    !v.is_empty()
                });
                by_pat.retain(|(_, i)| *i != id);
                by_rexpr.retain(|(_, i)| *i != id);
                for e in exts {
                    by_ext.entry(e.clone()).or_default().insert(0, id);
                }
                // Insert at the front (like `AddPattern`) so replaced patterns
                // take precedence over built-in ones, keeping declared order.
                let new = patterns.iter().map(|p| (p.clone(), id));
                by_pat.splice(0..0, new);
            }
        }
    }
}

/// Compiles a relative-path regex, baking in the case-insensitive flag so the
/// compiled form (`Regex::as_str`) is enough to identify it for removal.
fn compile_rexpr(regex: &str, icase: bool) -> Result<Regex, regex::Error> {
    if icase {
        Regex::new(&format!("(?i){regex}"))
    } else {
        Regex::new(regex)
    }
}

/// Parses a single `--map-<LANG>=<item>` item into an edit, or `None` if it is
/// malformed. `item` may be prefixed with `+` (add, default) or `-` (remove),
/// then either `.ext` or `(pattern)`.
fn parse_map_item(lang: &str, item: &str) -> Option<LangMapEdit> {
    let (remove, body) = match item.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, item.strip_prefix('+').unwrap_or(item)),
    };
    let lang = lang.to_string();
    if let Some(inner) = body.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        if inner.is_empty() {
            return None;
        }
        return Some(if remove {
            LangMapEdit::RemovePattern {
                lang,
                pattern: inner.to_string(),
            }
        } else {
            LangMapEdit::AddPattern {
                lang,
                pattern: inner.to_string(),
            }
        });
    }
    if let Some(after_pct) = body.strip_prefix('%') {
        let (regex, flags) = split_rexpr(after_pct)?;
        if regex.is_empty() {
            return None;
        }
        let icase = flags == "i" || flags == "{icase}";
        return Some(if remove {
            LangMapEdit::RemoveRexpr { lang, regex, icase }
        } else {
            LangMapEdit::AddRexpr { lang, regex, icase }
        });
    }
    if let Some(ext) = body.strip_prefix('.') {
        if ext.is_empty() {
            return None;
        }
        return Some(if remove {
            LangMapEdit::RemoveExt {
                lang,
                ext: ext.to_string(),
            }
        } else {
            LangMapEdit::AddExt {
                lang,
                ext: ext.to_string(),
            }
        });
    }
    None
}

/// Splits `%<regex>%<flags>` content (the part after the opening `%`) into the
/// regex (with `\%` unescaped to a literal `%`) and the trailing flag string.
/// Returns `None` if there is no closing `%`.
fn split_rexpr(s: &str) -> Option<(String, String)> {
    let mut regex = String::new();
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' if chars.peek().map(|&(_, n)| n) == Some('%') => {
                regex.push('%');
                chars.next();
            }
            '%' => return Some((regex, s[i + 1..].to_string())),
            _ => regex.push(c),
        }
    }
    None
}

/// Parses a langmap `<spec>` (a run of `.ext` and `(pattern)` tokens) into
/// (extensions, patterns).
fn parse_spec(spec: &str) -> (Vec<String>, Vec<String>) {
    let chars: Vec<char> = spec.chars().collect();
    let mut exts = Vec::new();
    let mut patterns = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '.' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != '.' && chars[i] != '(' {
                    i += 1;
                }
                if i > start {
                    exts.push(chars[start..i].iter().collect());
                }
            }
            '(' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != ')' {
                    i += 1;
                }
                if i > start {
                    patterns.push(chars[start..i].iter().collect());
                }
                if i < chars.len() {
                    i += 1; // skip ')'
                }
            }
            _ => i += 1, // ignore stray characters
        }
    }
    (exts, patterns)
}

/// Parses `--langmap` values (`<LANG>:<spec>[,<LANG>:<spec>...]`) into edits.
pub fn parse_langmap_values(values: &[String]) -> Vec<LangMapEdit> {
    let mut edits = Vec::new();
    for value in values {
        for entry in value.split(',') {
            let Some((lang, spec)) = entry.split_once(':') else {
                continue;
            };
            let (append, spec) = match spec.strip_prefix('+') {
                Some(rest) => (true, rest),
                None => (false, spec),
            };
            let (exts, patterns) = parse_spec(spec);
            if append {
                for ext in exts {
                    edits.push(LangMapEdit::AddExt {
                        lang: lang.to_string(),
                        ext,
                    });
                }
                for pattern in patterns {
                    edits.push(LangMapEdit::AddPattern {
                        lang: lang.to_string(),
                        pattern,
                    });
                }
            } else {
                edits.push(LangMapEdit::Replace {
                    lang: lang.to_string(),
                    exts,
                    patterns,
                });
            }
        }
    }
    edits
}

/// Scans raw args for `--map-<LANG>[=<item>]` (with `<item>` optionally in the
/// following arg) and returns the parsed edits.
pub fn extract_map_edits(args: &[String]) -> Vec<LangMapEdit> {
    let mut edits = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if let Some(rest) = args[i].strip_prefix("--map-") {
            let (lang, item) = if let Some((l, v)) = rest.split_once('=') {
                (l.to_string(), v.to_string())
            } else {
                let v = if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    i += 1;
                    args[i].clone()
                } else {
                    String::new()
                };
                (rest.to_string(), v)
            };
            match parse_map_item(&lang, &item) {
                Some(edit) => edits.push(edit),
                None if !item.is_empty() => {
                    eprintln!("treetags: ignoring malformed --map-{lang}={item}");
                }
                None => {}
            }
        }
        i += 1;
    }
    edits
}

/// Removes `--map-<LANG>[=<item>]` args (and their space-separated values) so
/// clap does not see the dynamic flags.
pub fn strip_map_args(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        if let Some(rest) = args[i].strip_prefix("--map-") {
            if !rest.contains('=') && i + 1 < args.len() && !args[i + 1].starts_with('-') {
                i += 1; // skip space-separated value
            }
            i += 1;
            continue;
        }
        out.push(args[i].clone());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_replace_patterns_take_front_precedence() {
        // Both `--map-<LANG>=(pat)` (AddPattern) and `--langmap` (Replace) must
        // place a language's patterns ahead of pre-existing (built-in) ones.
        let existing = vec![("Makefile".to_string(), 7usize)];

        let mut by_pat = existing.clone();
        LangMapEdit::AddPattern {
            lang: "ruby".into(),
            pattern: "Jarfile".into(),
        }
        .apply(3, &mut HashMap::new(), &mut by_pat, &mut Vec::new());
        assert_eq!(
            by_pat,
            vec![("Jarfile".to_string(), 3), ("Makefile".to_string(), 7)]
        );

        let mut by_pat = existing.clone();
        LangMapEdit::Replace {
            lang: "ruby".into(),
            exts: vec![],
            patterns: vec!["A".into(), "B".into()],
        }
        .apply(3, &mut HashMap::new(), &mut by_pat, &mut Vec::new());
        // Front precedence, and the declared order (A before B) is preserved.
        assert_eq!(
            by_pat,
            vec![
                ("A".to_string(), 3),
                ("B".to_string(), 3),
                ("Makefile".to_string(), 7),
            ]
        );
    }

    #[test]
    fn map_item_rexpr() {
        assert_eq!(
            parse_map_item("c++", r"%include/.*\.h%"),
            Some(LangMapEdit::AddRexpr {
                lang: "c++".into(),
                regex: r"include/.*\.h".into(),
                icase: false,
            })
        );
        assert_eq!(
            parse_map_item("c++", "%foo%i"),
            Some(LangMapEdit::AddRexpr {
                lang: "c++".into(),
                regex: "foo".into(),
                icase: true,
            })
        );
        assert_eq!(
            parse_map_item("c++", "%foo%{icase}"),
            Some(LangMapEdit::AddRexpr {
                lang: "c++".into(),
                regex: "foo".into(),
                icase: true,
            })
        );
        assert_eq!(
            parse_map_item("make", "-%Makefile.*%"),
            Some(LangMapEdit::RemoveRexpr {
                lang: "make".into(),
                regex: "Makefile.*".into(),
                icase: false,
            })
        );
        // `\%` is unescaped to a literal `%` inside the regex.
        assert_eq!(
            parse_map_item("x", r"%a\%b%"),
            Some(LangMapEdit::AddRexpr {
                lang: "x".into(),
                regex: "a%b".into(),
                icase: false,
            })
        );
        // Unterminated or empty regexes are rejected.
        assert_eq!(parse_map_item("x", "%foo"), None);
        assert_eq!(parse_map_item("x", "%%"), None);
    }

    #[test]
    fn map_item_add_remove() {
        assert_eq!(
            parse_map_item("c", ".foo"),
            Some(LangMapEdit::AddExt {
                lang: "c".into(),
                ext: "foo".into()
            })
        );
        assert_eq!(
            parse_map_item("c", "+.foo"),
            Some(LangMapEdit::AddExt {
                lang: "c".into(),
                ext: "foo".into()
            })
        );
        assert_eq!(
            parse_map_item("c", "-.h"),
            Some(LangMapEdit::RemoveExt {
                lang: "c".into(),
                ext: "h".into()
            })
        );
        assert_eq!(
            parse_map_item("make", "(Makefile)"),
            Some(LangMapEdit::AddPattern {
                lang: "make".into(),
                pattern: "Makefile".into()
            })
        );
        assert_eq!(
            parse_map_item("make", "-(Makefile)"),
            Some(LangMapEdit::RemovePattern {
                lang: "make".into(),
                pattern: "Makefile".into()
            })
        );
        assert_eq!(parse_map_item("c", "garbage"), None);
        assert_eq!(parse_map_item("c", "."), None);
    }

    #[test]
    fn spec_parsing() {
        assert_eq!(parse_spec(".c.h"), (vec!["c".into(), "h".into()], vec![]));
        assert_eq!(
            parse_spec("(Makefile).mak"),
            (vec!["mak".into()], vec!["Makefile".into()])
        );
        assert_eq!(
            parse_spec(".rb(Rakefile)(*.gemspec)"),
            (
                vec!["rb".into()],
                vec!["Rakefile".into(), "*.gemspec".into()]
            )
        );
    }

    #[test]
    fn langmap_replace_and_append() {
        assert_eq!(
            parse_langmap_values(&["c:.c.h".into()]),
            vec![LangMapEdit::Replace {
                lang: "c".into(),
                exts: vec!["c".into(), "h".into()],
                patterns: vec![]
            }]
        );
        assert_eq!(
            parse_langmap_values(&["c:+.x".into()]),
            vec![LangMapEdit::AddExt {
                lang: "c".into(),
                ext: "x".into()
            }]
        );
        // Multiple languages in one value.
        let edits = parse_langmap_values(&["ruby:+(Jarfile),c:+.inc".into()]);
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn extract_and_strip_map_args() {
        let args: Vec<String> = [
            "treetags",
            "--map-c=.foo",
            "-f",
            "tags",
            "--map-make",
            "(Makefile)",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let edits = extract_map_edits(&args);
        assert_eq!(
            edits,
            vec![
                LangMapEdit::AddExt {
                    lang: "c".into(),
                    ext: "foo".into()
                },
                LangMapEdit::AddPattern {
                    lang: "make".into(),
                    pattern: "Makefile".into()
                },
            ]
        );
        let stripped = strip_map_args(args);
        assert_eq!(stripped, vec!["treetags", "-f", "tags"]);
    }
}
