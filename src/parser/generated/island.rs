//! Single-pass island parser engine for the C family.
//!
//! This is the runtime counterpart to the grammar-derived tables in
//! [`super::cfamily`]. Source is tokenized exactly once, then a cursor walks the
//! token stream, recognizing the constructs of interest and skipping ("water")
//! everything that can never carry a tag. It replaces the previous multi-pass
//! regex scanner while producing byte-identical output.
#![allow(dead_code)]

use super::cfamily::{
    ACCESS_SPECIFIERS, ANON_PREFIX, ANON_SEED, CONTROL_KEYWORDS, CTYPE_STRIP, DECL_CV_PREFIXES,
    DECL_DESTRUCTOR, DECL_OPERATOR_KW, DECL_POINTER_PREFIXES, DECL_SCOPE_OP, EXPR_SKIP_OPS,
    EXPR_SKIP_SCOPE_OP, FIELD_FUNCTION_QUALIFIER, FIELD_PARAM_FUNCTION, FIELD_TYPEREF_DEFAULT,
    FIELD_TYPEREF_KEY, KEYWORDS, PREPROC_DEFINE, PREPROC_INCLUDE, PREPROC_MACRO_PARAM_FIELD,
    PUNCTUATORS, STRING_PREFIXES, TEMPLATE_KW, TEMPLATE_PARAM_KEYWORDS, TYPEREF_PREFIXES,
};

/// Lexical class of a token. The scanner keys off these plus the token text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tok {
    /// Identifier that is not a reserved word.
    Ident,
    /// Reserved word (a `STRING` grammar terminal that lexes as an identifier).
    Keyword,
    /// Operator or punctuator.
    Punct,
    /// Numeric literal.
    Num,
    /// String or character literal.
    Str,
    /// A whole preprocessor directive line (`#...`).
    Preproc,
}

/// A lexed token: its class, source slice, byte offset, and 0-based line.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Token<'a> {
    pub kind: Tok,
    pub text: &'a str,
    pub start: usize,
    pub row: usize,
}

fn is_ident_start(b: u8) -> bool {
    b == b'_' || b == b'$' || b.is_ascii_alphabetic() || b >= 0x80
}
fn is_ident_continue(b: u8) -> bool {
    b == b'_' || b == b'$' || b.is_ascii_alphanumeric() || b >= 0x80
}

/// Tokenize C-family source in a single forward pass. Whitespace, line
/// continuations, and comments are consumed but never emitted.
pub(crate) fn lex(src: &str) -> Vec<Token<'_>> {
    let bytes = src.as_bytes();
    let n = bytes.len();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut row = 0usize;
    let mut at_line_start = true;
    while i < n {
        let b = bytes[i];
        // Whitespace and backslash-newline continuations (grammar `extras`).
        if b == b'\n' {
            i += 1;
            row += 1;
            at_line_start = true;
            continue;
        }
        if b == b' ' || b == b'\t' || b == b'\r' || b == 0x0c || b == 0x0b {
            i += 1;
            continue;
        }
        if b == b'\\' && i + 1 < n && (bytes[i + 1] == b'\n' || bytes[i + 1] == b'\r') {
            i += 1;
            continue;
        }
        // Comments.
        if b == b'/' && i + 1 < n && bytes[i + 1] == b'/' {
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && i + 1 < n && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < n && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    row += 1;
                }
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }
        // Preprocessor directive: `#` as the first token on a line. Consumes the
        // physical line only (matching the previous line-oriented behavior).
        if b == b'#' && at_line_start {
            let start = i;
            let line_start_row = row;
            while i < n && bytes[i] != b'\n' {
                i += 1;
            }
            tokens.push(Token {
                kind: Tok::Preproc,
                text: &src[start..i],
                start,
                row: line_start_row,
            });
            at_line_start = false;
            continue;
        }
        at_line_start = false;
        // String / character literals, with optional encoding prefix.
        if b == b'"' || b == b'\'' {
            let start = i;
            let quote = b;
            i += 1;
            while i < n {
                if bytes[i] == b'\\' {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        row += 1;
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\n' {
                    row += 1;
                    i += 1;
                    continue;
                }
                if bytes[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            tokens.push(Token {
                kind: Tok::Str,
                text: &src[start..i],
                start,
                row,
            });
            continue;
        }
        // Identifiers and reserved words (with string-prefix lookahead).
        if is_ident_start(b) {
            let start = i;
            i += 1;
            while i < n && is_ident_continue(bytes[i]) {
                i += 1;
            }
            let text = &src[start..i];
            // `L"..."`, `u8'c'`, raw-string prefixes, etc.
            if i < n
                && (bytes[i] == b'"' || bytes[i] == b'\'')
                && STRING_PREFIXES.contains(&text)
            {
                let quote = bytes[i];
                i += 1;
                while i < n {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'\n' {
                        row += 1;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                tokens.push(Token {
                    kind: Tok::Str,
                    text: &src[start..i],
                    start,
                    row,
                });
                continue;
            }
            let kind = if KEYWORDS.binary_search(&text).is_ok() {
                Tok::Keyword
            } else {
                Tok::Ident
            };
            tokens.push(Token {
                kind,
                text,
                start,
                row,
            });
            continue;
        }
        // Numeric literals.
        if b.is_ascii_digit() || (b == b'.' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let start = i;
            i += 1;
            while i < n {
                let c = bytes[i];
                if c.is_ascii_alphanumeric() || c == b'.' || c == b'\'' {
                    i += 1;
                } else if (c == b'+' || c == b'-')
                    && matches!(bytes[i - 1], b'e' | b'E' | b'p' | b'P')
                {
                    i += 1;
                } else {
                    break;
                }
            }
            tokens.push(Token {
                kind: Tok::Num,
                text: &src[start..i],
                start,
                row,
            });
            continue;
        }
        // Operators / punctuators, longest match first.
        if let Some(op) = PUNCTUATORS
            .iter()
            .find(|op| src[i..].as_bytes().starts_with(op.as_bytes()))
        {
            tokens.push(Token {
                kind: Tok::Punct,
                text: op,
                start: i,
                row,
            });
            i += op.len();
            continue;
        }
        // Unknown byte: skip it so lexing always terminates.
        i += 1;
    }
    tokens
}

/// Result of walking a C-family declarator: the innermost name plus the
/// structural facts the tag rules need.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declarator {
    /// Innermost identifier, normalized (`A::B::f` → `f`, `operator+` →
    /// `operator +`, `~T` kept as-is).
    pub name: String,
    /// Qualifier before the name (`A::B` in `A::B::f`), if any.
    pub qualifier: Option<String>,
    /// A top-level function parameter list `(...)` is attached.
    pub is_function: bool,
    /// Parenthesized `(*name)` grouping — i.e. a function/array pointer.
    pub grouped_pointer: bool,
    /// A leading `*` pointer operator applies to this declarator.
    pub leading_pointer: bool,
}

/// True for tokens that prefix a declarator core without changing its name.
fn is_declarator_prefix(t: &Token) -> bool {
    DECL_POINTER_PREFIXES.contains(&t.text) || DECL_CV_PREFIXES.contains(&t.text)
}

/// Index just past the balanced bracket that opens at `open` (which must be one
/// of `(`, `[`, `<`). Returns `toks.len()` if unbalanced.
fn skip_balanced(toks: &[Token], open: usize) -> usize {
    let (o, c) = match toks[open].text {
        "(" => ("(", ")"),
        "[" => ("[", "]"),
        "<" => ("<", ">"),
        "{" => ("{", "}"),
        _ => return open + 1,
    };
    let template = o == "<";
    let mut depth = 0i32;
    let mut i = open;
    while i < toks.len() {
        let t = toks[i].text;
        // In template context the lexer munches `>>`/`<<` into single tokens;
        // treat them as two angle brackets so `vector<vector<int>>` balances.
        if template && t == ">>" {
            depth -= 2;
        } else if template && t == "<<" {
            depth += 2;
        } else if t == o {
            depth += 1;
        } else if t == c {
            depth -= 1;
        }
        if depth <= 0 && i >= open && matches!(t, ">" | ">>" | ")" | "]" | "}") {
            return i + 1;
        }
        i += 1;
    }
    toks.len()
}

/// Normalize an innermost declarator id: keep the last scope segment, and put a
/// space after the `operator` keyword.
fn normalize_name(raw: &str) -> String {
    let last = raw.rsplit(DECL_SCOPE_OP).next().unwrap_or(raw).trim();
    match last.strip_prefix(DECL_OPERATOR_KW) {
        Some(rest)
            if rest.is_empty() || !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') =>
        {
            format!("{DECL_OPERATOR_KW} {}", rest.trim())
        }
        _ => last.to_owned(),
    }
}

/// Parse a declarator token slice, locating the innermost name and recording
/// whether a function signature or pointer grouping is attached.
pub(crate) fn parse_declarator(toks: &[Token]) -> Option<Declarator> {
    let mut decl = Declarator::default();
    let mut i = 0;
    // Leading pointer/reference/cv prefixes.
    while i < toks.len() && is_declarator_prefix(&toks[i]) {
        if toks[i].text == "*" {
            decl.leading_pointer = true;
        }
        i += 1;
    }
    if i >= toks.len() {
        return None;
    }
    // Parenthesized declarator: `(*name)(...)` or grouping.
    if toks[i].text == "(" {
        let close = skip_balanced(toks, i);
        // `close` is `toks.len()` when the `(` is unbalanced; clamp so the inner
        // range never inverts (an unbalanced/empty group yields no name).
        let inner_end = close.saturating_sub(1).max(i + 1);
        let inner = parse_declarator(&toks[i + 1..inner_end])?;
        decl.name = inner.name;
        decl.qualifier = inner.qualifier;
        decl.grouped_pointer = inner.leading_pointer;
        // A following `(` is the function signature of the grouped pointer.
        if close < toks.len() && toks[close].text == "(" {
            decl.is_function = true;
        }
        return Some(decl);
    }
    // Declarator id: a `::`-separated path whose final segment is the name.
    // Each segment is an identifier (optionally templated), an `operator` name,
    // or a `~destructor`.
    let mut segments: Vec<String> = Vec::new();
    loop {
        let Some(t) = toks.get(i) else { break };
        let mut seg = String::new();
        let mut terminal = false;
        if t.text == DECL_DESTRUCTOR {
            seg.push_str(DECL_DESTRUCTOR);
            i += 1;
            if let Some(id) = toks.get(i).filter(|t| t.kind == Tok::Ident || t.kind == Tok::Keyword)
            {
                seg.push_str(id.text);
                i += 1;
            }
        } else if t.text == DECL_OPERATOR_KW {
            seg.push_str(DECL_OPERATOR_KW);
            i += 1;
            while let Some(o) = toks.get(i) {
                if o.text == "(" || o.text == "[" {
                    break;
                }
                seg.push_str(o.text);
                i += 1;
                if seg.ends_with("[]") || seg.ends_with("()") {
                    break;
                }
            }
            terminal = true;
        } else if t.kind == Tok::Ident || t.kind == Tok::Keyword {
            seg.push_str(t.text);
            i += 1;
            if let Some(lt) = toks.get(i).filter(|t| t.text == "<") {
                let _ = lt;
                let close = skip_balanced(toks, i);
                for tok in &toks[i..close] {
                    seg.push_str(tok.text);
                }
                i = close;
            }
        } else {
            break;
        }
        if !terminal && toks.get(i).is_some_and(|t| t.text == DECL_SCOPE_OP) {
            segments.push(seg);
            i += 1;
            continue;
        }
        segments.push(seg);
        break;
    }
    if segments.is_empty() {
        return None;
    }
    let raw_name = segments.pop().unwrap();
    if !segments.is_empty() {
        decl.qualifier = Some(segments.join(DECL_SCOPE_OP));
    }
    decl.name = normalize_name(&raw_name);
    if decl.name.is_empty() {
        return None;
    }
    // Suffixes: function parameter list and/or array dimensions.
    while i < toks.len() {
        match toks[i].text {
            "(" => {
                decl.is_function = true;
                i = skip_balanced(toks, i);
            }
            "[" => {
                i = skip_balanced(toks, i);
            }
            _ => break,
        }
    }
    Some(decl)
}

// ---------------------------------------------------------------------------
// Statement scanner
// ---------------------------------------------------------------------------

use super::runtime::{Candidate, LanguageTables, SpecifierSpec};
use crate::parser::TagKindConfig;

/// Normalize a type substring the way ctags does: split on whitespace, drop
/// storage/cv keywords (from `CTYPE_STRIP`) and pointer/reference sigils.
fn ctype(s: &str) -> String {
    s.split_whitespace()
        .map(|w| w.trim_matches(|c| c == '*' || c == '&'))
        .filter(|w| !w.is_empty() && !CTYPE_STRIP.contains(w))
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c| c == '*' || c == '&')
        .trim()
        .to_owned()
}

struct Frame {
    kind: &'static str,
    name: String,
    depth: usize,
    /// This frame is an anonymous aggregate; a trailing `} name;` after it is a
    /// variable (or, when `typedef`, already handled by look-ahead).
    anon_aggregate: bool,
    typedef: bool,
}

impl Frame {
    fn scope(kind: &'static str, name: String, depth: usize) -> Self {
        Frame {
            kind,
            name,
            depth,
            anon_aggregate: false,
            typedef: false,
        }
    }
}

struct Scanner<'a> {
    t: &'static LanguageTables,
    src: &'a str,
    toks: Vec<Token<'a>>,
    kinds: &'a TagKindConfig,
    out: Vec<Candidate>,
    scopes: Vec<Frame>,
    depth: usize,
    hash: u32,
    seq: u16,
    anon: u16,
}

/// Entry point used by [`super::runtime::generate`] under the island switch.
pub(crate) fn scan(
    t: &'static LanguageTables,
    src: &str,
    path: &str,
    kinds: &TagKindConfig,
) -> Vec<Candidate> {
    let mut hash = ANON_SEED;
    for b in path.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u32);
    }
    let mut scanner = Scanner {
        t,
        src,
        toks: lex(src),
        kinds,
        out: Vec::new(),
        scopes: Vec::new(),
        depth: 0,
        hash,
        seq: 1,
        anon: 1,
    };
    scanner.run();
    scanner.out
}

fn is_ident_word(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && c.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

impl<'a> Scanner<'a> {
    fn run(&mut self) {
        let mut i = 0;
        while i < self.toks.len() {
            let t = self.toks[i];
            match t.kind {
                Tok::Preproc => {
                    self.preproc(t);
                    i += 1;
                }
                _ if t.text == "}" => {
                    let closed = if self.scopes.last().is_some_and(|f| f.depth == self.depth) {
                        self.scopes.pop()
                    } else {
                        None
                    };
                    self.depth = self.depth.saturating_sub(1);
                    i += 1;
                    // A trailing `} name, name2;` after an anonymous aggregate
                    // declares variables; after a typedef aggregate the alias was
                    // already emitted, so it is skipped.
                    if let Some(frame) = closed {
                        if frame.typedef {
                            i = self.skip_to_semi(i);
                        } else if frame.anon_aggregate {
                            i = self.trailing_vars(i);
                        }
                    }
                }
                _ if t.text == "{" => {
                    // An unlabeled block (e.g. a function body we do not descend
                    // for default kinds): skip it balanced.
                    i = skip_balanced(&self.toks, i);
                }
                _ if t.text == ";" || t.text == ")" || t.text == "(" || t.text == "]" => {
                    i += 1;
                }
                _ => {
                    i = self.statement(i);
                }
            }
        }
    }

    /// A scope kind counts as a container (prepended as a scope field) iff some
    /// keyword-introduced construct enters it — i.e. it appears in `SPECIFIERS`.
    fn is_container_scope(&self, kind: &str) -> bool {
        self.t.specifiers.iter().any(|s| s.scope == Some(kind))
    }

    /// The innermost container scope field to prepend to a tag.
    fn scope_field(&self) -> Option<(&'static str, String)> {
        self.scopes
            .iter()
            .rev()
            .find(|f| self.is_container_scope(f.kind))
            .map(|f| (f.kind, f.name.clone()))
    }

    fn owner(&self, kinds: &[&str]) -> Option<String> {
        self.scopes
            .iter()
            .rev()
            .find(|f| kinds.contains(&f.kind))
            .map(|f| f.name.clone())
    }

    fn emit(&mut self, name: &str, kind: &'static str, row: usize, vals: Vec<(&'static str, String)>) {
        if !self.kinds.is_kind_enabled(kind) || name.is_empty() || name == "_" {
            return;
        }
        let mut fields = Vec::new();
        if let Some(scope) = self.scope_field() {
            fields.push(scope);
        }
        fields.extend(vals);
        self.out.push(Candidate {
            name: name.to_owned(),
            kind,
            row,
            fields,
        });
    }

    fn anon_name(&self, counter: u16, id: u8) -> String {
        format!("{ANON_PREFIX}{:08x}{:02x}{:02x}", self.hash, counter, id)
    }

    fn preproc(&mut self, t: Token) {
        let line = t.text.trim_start().trim_start_matches('#').trim_start();
        if let Some(rest) = line.strip_prefix(PREPROC_INCLUDE) {
            let path = rest.trim();
            let name = path.trim_matches(|c| c == '<' || c == '>' || c == '"');
            if let Some(letter) = self.role("header") {
                if !name.is_empty() {
                    self.emit(name, letter, t.row, vec![]);
                }
            }
        } else if let Some(rest) = line.strip_prefix(PREPROC_DEFINE) {
            let rest = rest.trim_start();
            let name: String = rest
                .chars()
                .take_while(|c| *c == '_' || c.is_ascii_alphanumeric())
                .collect();
            if name.is_empty() {
                return;
            }
            if let Some(letter) = self.role("macro") {
                self.emit(&name, letter, t.row, vec![]);
            }
            // Function-like macro parameters.
            let after = &rest[name.len()..];
            if after.starts_with('(') {
                if let (Some(close), Some(letter)) = (after.find(')'), self.role("macro_param")) {
                    for p in after[1..close].split(',') {
                        let p = p.trim();
                        if is_ident_word(p) {
                            self.emit(
                                p,
                                letter,
                                t.row,
                                vec![(PREPROC_MACRO_PARAM_FIELD, name.clone())],
                            );
                        }
                    }
                }
            }
        }
    }

    /// Collect a statement head up to the next `;`, `{`, or `}` at the current
    /// nesting (not descending into `()`/`[]`), dispatch it, and return the
    /// cursor position after the statement.
    fn statement(&mut self, start: usize) -> usize {
        let mut j = start;
        let mut end = self.toks.len();
        let mut terminator = "";
        while j < self.toks.len() {
            match self.toks[j].text {
                "(" | "[" | "<" => {
                    let next = skip_balanced(&self.toks, j);
                    // `<` is only a bracket in template/type context; if it did
                    // not balance, treat it as a comparison operator.
                    j = if next > j { next } else { j + 1 };
                    continue;
                }
                ";" | "{" | "}" => {
                    end = j;
                    terminator = self.toks[j].text;
                    break;
                }
                _ => j += 1,
            }
        }
        let head: Vec<Token<'a>> = self.toks[start..end].to_vec();
        let descend = self.dispatch(&head, terminator, end);
        match terminator {
            "{" => {
                if descend {
                    self.depth += 1;
                    if let Some(f) = self.scopes.last_mut() {
                        f.depth = self.depth;
                    }
                    end + 1
                } else {
                    skip_balanced(&self.toks, end)
                }
            }
            ";" => end + 1,
            _ => end, // `}` is handled by the main loop
        }
    }

    fn src_between(&self, a: usize, b: usize) -> &'a str {
        let src: &'a str = self.src;
        src[a..b].trim()
    }

    /// Find the specifier row for a leading keyword, optionally constrained to a
    /// category. Two rows can share a keyword (e.g. `namespace` def vs alias).
    fn spec(&self, keyword: &str, category: Option<&str>) -> Option<&'static SpecifierSpec> {
        self.t
            .specifiers
            .iter()
            .find(|s| s.keyword == keyword && category.is_none_or(|c| s.category == c))
    }

    /// Classify a statement head and emit its tags. Returns whether the trailing
    /// `{` (if any) opens a scope the walker should descend into.
    fn dispatch(&mut self, head: &[Token<'a>], terminator: &str, brace: usize) -> bool {
        if head.is_empty() {
            return false;
        }
        // Strip a leading `template <...>` prefix (its parameter list is a
        // separate construct; the trailing declaration is what we dispatch on).
        let mut p = 0;
        if head.get(p).map(|t| t.text) == Some(TEMPLATE_KW) {
            p += 1;
            if head.get(p).is_some_and(|t| t.text == "<") {
                let close = skip_balanced(head, p);
                self.template_params(&head[p..close]);
                p = close;
            }
        }
        // Strip a leading access specifier (`public:` / `private:` / …).
        if head.get(p).is_some_and(|t| ACCESS_SPECIFIERS.contains(&t.text)) {
            p += 1;
            if head.get(p).is_some_and(|t| t.text == ":") {
                p += 1;
            }
        }
        let head = &head[p..];
        let Some(lead) = head.first() else {
            return false;
        };

        // A leading `typedef` specifier prefixes another construct.
        let is_typedef = self.spec(lead.text, Some("typedef")).is_some();
        let ti = usize::from(is_typedef);
        let Some(kw) = head.get(ti).map(|t| t.text) else {
            return false;
        };

        // Keyword-introduced constructs dispatch off the SPECIFIERS table.
        if let Some(spec) = self.spec(kw, None) {
            match spec.category {
                "aggregate" | "enum" => {
                    let descend = self.type_specifier(head, ti, spec, is_typedef, terminator, brace);
                    if terminator != "{" {
                        self.declaration(head, is_typedef, terminator, brace);
                    }
                    return descend;
                }
                "namespace" | "alias" => return self.namespace(head, ti, terminator),
                "using" => {
                    self.using(head, ti);
                    return false;
                }
                "template" => return false,
                _ => {}
            }
        }

        // Fallthrough: declaration or function definition.
        self.declaration(head, is_typedef, terminator, brace);
        false
    }

    /// Emit template type-parameters (`template<class T, typename U>`) as their
    /// own kind. `list` spans the `<...>` inclusive.
    fn template_params(&mut self, list: &[Token<'a>]) {
        let Some(letter) = self.role("template_param") else {
            return;
        };
        if list.len() < 2 {
            return;
        }
        let inner = &list[1..list.len() - 1];
        let mut emits = Vec::new();
        for param in split_top_level(inner, ",") {
            // A type parameter is introduced by a template param keyword; its
            // name is the following identifier.
            if let Some(kw_pos) = param
                .iter()
                .position(|t| TEMPLATE_PARAM_KEYWORDS.contains(&t.text))
            {
                if let Some(name) = param.get(kw_pos + 1).filter(|t| t.kind == Tok::Ident) {
                    emits.push((name.text.to_owned(), name.row));
                }
            }
        }
        for (name, row) in emits {
            self.emit(&name, letter, row, vec![]);
        }
    }

    /// `namespace NAME { … }` (definition) or `namespace NAME = …;` (alias).
    fn namespace(&mut self, head: &[Token<'a>], ti: usize, terminator: &str) -> bool {
        let Some(name) = head.get(ti + 1).filter(|t| t.kind == Tok::Ident) else {
            return false;
        };
        if terminator == "{" {
            if let Some(spec) = self.spec(head[ti].text, Some("namespace")) {
                self.emit(name.text, spec.letter, name.row, vec![]);
                if let Some(scope) = spec.scope {
                    self.scopes
                        .push(Frame::scope(scope, name.text.to_owned(), self.depth));
                }
                return true;
            }
        } else if let Some(spec) = self.spec(head[ti].text, Some("alias")) {
            self.emit(name.text, spec.letter, name.row, vec![]);
        }
        false
    }

    /// `using [namespace] QUALIFIED-NAME;`.
    fn using(&mut self, head: &[Token<'a>], ti: usize) {
        let Some(spec) = self.spec(head[ti].text, Some("using")) else {
            return;
        };
        // Skip an optional `namespace` sub-keyword before the target name.
        let mut start = ti + 1;
        if self.spec(head.get(start).map(|t| t.text).unwrap_or(""), None).is_some() {
            start += 1;
        }
        if let (Some(first), Some(last)) = (head.get(start), head.last()) {
            let name = self.src_between(first.start, last.start + last.text.len());
            if !name.is_empty() {
                self.emit(name, spec.letter, first.row, vec![]);
            }
        }
    }

    /// Handle an aggregate/enum specifier: definition (`{`), typed reference, or
    /// `typedef`. All kind letters, scopes, and anon ids come from `spec`.
    fn type_specifier(
        &mut self,
        head: &[Token<'a>],
        ti: usize,
        spec: &'static SpecifierSpec,
        is_typedef: bool,
        terminator: &str,
        brace: usize,
    ) -> bool {
        let is_enum = spec.category == "enum";
        let letter = spec.letter;
        let id = spec.anon_id;
        let name_tok = head.get(ti + 1).filter(|t| t.kind == Tok::Ident);
        // `enum NAME : base` — the base type field, formatted per the spec.
        let mut base_field = Vec::new();
        if let Some(base_format) = spec.base_format {
            if let Some(colon) = head.iter().position(|t| t.text == ":") {
                let last = head.last().unwrap();
                let base = self.src_between(head[colon + 1].start, last.start + last.text.len());
                if !base.is_empty() {
                    base_field.push(("typeref", base_format.replace("{}", base)));
                }
            }
        }

        if let Some(name) = name_tok {
            self.emit(name.text, letter, name.row, base_field);
            // `typedef struct Name { … }` also introduces the alias `Name`.
            if is_typedef && !is_enum && terminator == "{" {
                if let (Some(t_letter), Some((_, label))) =
                    (self.role("typedef"), TYPEREF_PREFIXES.iter().find(|(k, _)| *k == spec.scope.unwrap_or("")))
                {
                    self.emit(
                        name.text,
                        t_letter,
                        name.row,
                        vec![("typeref", format!("{label}:{}", name.text))],
                    );
                }
            }
            if terminator == "{" {
                if is_enum {
                    self.enum_items(name.text, spec.scope, brace);
                    return false;
                }
                if let Some(scope) = spec.scope {
                    let mut frame = Frame::scope(scope, name.text.to_owned(), self.depth);
                    frame.typedef = is_typedef;
                    self.scopes.push(frame);
                    return true;
                }
            }
            return false;
        }

        // Anonymous aggregate definition.
        if terminator == "{" {
            if is_enum {
                self.enum_items("", spec.scope, brace);
                return false;
            }
            // The typedef alias's typeref uses the first anon counter; the tag
            // uses the next; the member scope name uses the `seq` counter.
            if is_typedef {
                let typeref_name = self.anon_name(self.anon, id);
                self.anon += 1;
                let close = skip_balanced(&self.toks, brace);
                if let (Some(alias), Some(t_letter), Some((_, label))) = (
                    self.toks.get(close).filter(|t| t.kind == Tok::Ident),
                    self.role("typedef"),
                    TYPEREF_PREFIXES.iter().find(|(k, _)| *k == spec.scope.unwrap_or("")),
                ) {
                    self.emit(
                        alias.text,
                        t_letter,
                        head[ti].row,
                        vec![("typeref", format!("{label}:{typeref_name}"))],
                    );
                }
            }
            let tag_name = self.anon_name(self.anon, id);
            self.anon += 1;
            self.emit(&tag_name, letter, head[ti].row, vec![]);
            if is_typedef {
                self.seq += 1;
            }
            let scope_name = self.anon_name(self.seq, id);
            self.seq += 1;
            if let Some(scope) = spec.scope {
                let mut frame = Frame::scope(scope, scope_name, self.depth);
                frame.anon_aggregate = true;
                frame.typedef = is_typedef;
                self.scopes.push(frame);
                return true;
            }
        }
        false
    }

    fn enum_items(&mut self, enum_name: &str, enum_scope: Option<&'static str>, brace: usize) {
        let Some(letter) = self.role("enumerator") else {
            return;
        };
        let end = skip_balanced(&self.toks, brace);
        let mut k = brace + 1;
        let mut expect_name = true;
        while k < end.saturating_sub(1) {
            let t = self.toks[k];
            if t.text == "," {
                expect_name = true;
                k += 1;
                continue;
            }
            if expect_name && t.kind == Tok::Ident {
                let fields = match enum_scope {
                    Some(scope) => vec![(scope, enum_name.to_owned())],
                    None => vec![],
                };
                self.emit(t.text, letter, t.row, fields);
                expect_name = false;
            }
            // Skip `= value` up to the next comma at depth 0.
            if matches!(t.text, "(" | "[" | "{" | "<") {
                k = skip_balanced(&self.toks, k);
                continue;
            }
            k += 1;
        }
    }

    /// Emit the variables in a trailing `} a, b;` after an anonymous aggregate.
    fn trailing_vars(&mut self, start: usize) -> usize {
        let mut end = start;
        while end < self.toks.len() && self.toks[end].text != ";" {
            end += 1;
        }
        let group: Vec<Token<'a>> = self.toks[start..end].to_vec();
        let Some(letter) = self.role("variable") else {
            return (end + 1).min(self.toks.len());
        };
        let mut emits: Vec<(String, usize)> = Vec::new();
        for part in split_top_level(&group, ",") {
            if let Some(decl) = parse_declarator(part) {
                if let Some(row) = part.first().map(|t| t.row) {
                    emits.push((decl.name, row));
                }
            }
        }
        for (name, row) in emits {
            self.emit(&name, letter, row, vec![]);
        }
        (end + 1).min(self.toks.len())
    }

    /// Byte offset just past a token.
    fn tok_end(&self, t: &Token) -> usize {
        t.start + t.text.len()
    }

    /// Render a type substring into a `typeref` value: a leading aggregate
    /// keyword (from `TYPEREF_PREFIXES`) becomes `struct:X`/`union:X`/`enum:X`,
    /// otherwise the default label (`typename:X`).
    fn typeref_of(&self, type_str: &str, pointer: bool) -> String {
        let ty = ctype(type_str);
        for (keyword, label) in TYPEREF_PREFIXES {
            let prefix = format!("{keyword} ");
            if let Some(rest) = ty.strip_prefix(&prefix) {
                return format!("{label}:{}{}", rest.trim(), if pointer { " *" } else { "" });
            }
        }
        format!("{FIELD_TYPEREF_DEFAULT}:{ty}")
    }

    /// The kind letter for a role in this language, or None if unavailable.
    fn role(&self, role: &str) -> Option<&'static str> {
        self.t
            .roles
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, letter)| *letter)
    }

    /// Advance past the next `;`.
    fn skip_to_semi(&self, start: usize) -> usize {
        let mut i = start;
        while i < self.toks.len() && self.toks[i].text != ";" {
            i += 1;
        }
        (i + 1).min(self.toks.len())
    }

    fn declaration(&mut self, head: &[Token<'a>], is_typedef: bool, terminator: &str, brace: usize) {
        if head.is_empty() {
            return;
        }
        if is_typedef {
            self.typedef(head);
            return;
        }
        // Function? The first top-level `(` preceded by a name (possibly with a
        // space before the `(`). The name is the contiguous declarator-id ending
        // at the token just before the `(`.
        if let Some(paren) = self.first_top_level(head, "(") {
            if paren > 0 && matches!(head[paren - 1].kind, Tok::Ident | Tok::Punct | Tok::Keyword) {
                let mut start = paren - 1;
                while start > 0 && self.contiguous(&head[start - 1], &head[start]) {
                    start -= 1;
                }
                if let Some(decl) = parse_declarator(&head[start..]) {
                    // Exclude control-flow keywords masquerading as calls.
                    if decl.is_function
                        && !decl.name.is_empty()
                        && !CONTROL_KEYWORDS.contains(&head[start].text)
                    {
                        return self.function(head, start, decl, terminator, paren, brace);
                    }
                }
            }
        }
        // Otherwise: one or more variable/member declarators.
        self.variables(head, terminator);
    }

    fn function(
        &mut self,
        head: &[Token<'a>],
        name_start: usize,
        decl: Declarator,
        terminator: &str,
        paren: usize,
        brace: usize,
    ) {
        // Definition (`{`) vs prototype (`;`), letters from the role table.
        let is_def = terminator == "{";
        let role = if is_def { "function" } else { "prototype" };
        let Some(letter) = self.role(role) else {
            return;
        };
        let row = head[name_start].row;
        // Return type is everything before the name.
        let mut fields = Vec::new();
        if name_start > 0 {
            let type_str = self.src[head[0].start..head[name_start].start].trim();
            let ty = ctype(type_str);
            if !ty.is_empty() {
                fields.push((FIELD_TYPEREF_KEY, format!("{FIELD_TYPEREF_DEFAULT}:{ty}")));
            }
        }
        // Class qualifier: explicit `A::B::` appends; a lexical aggregate scope
        // is prepended (both use the qualifier field key).
        let lexical_owner = self.aggregate_owner();
        if let Some(owner) = decl.qualifier.clone() {
            fields.push((FIELD_FUNCTION_QUALIFIER, owner));
        } else if let Some(owner) = lexical_owner.clone() {
            fields.insert(0, (FIELD_FUNCTION_QUALIFIER, owner));
        }
        self.emit(&decl.name, letter, row, fields);

        if is_def {
            // Function definition: emit its parameters and any body labels.
            let close = skip_balanced(head, paren);
            // Clamp so an unbalanced `(` cannot invert the parameter range.
            let params_end = close.saturating_sub(1).max(paren + 1);
            let params = &head[paren + 1..params_end];
            self.emit_params(params, &decl.name, lexical_owner.as_deref());
            self.emit_labels(brace, &decl.name, lexical_owner.as_deref());
        }
    }

    /// Emit function parameters (`z`) with type and enclosing-function fields.
    fn emit_params(&mut self, params: &[Token<'a>], fname: &str, lexical_owner: Option<&str>) {
        let Some(letter) = self.role("parameter") else {
            return;
        };
        let function = match lexical_owner {
            Some(owner) => format!("{owner}{DECL_SCOPE_OP}{fname}"),
            None => fname.to_owned(),
        };
        let mut emits: Vec<(String, usize, String)> = Vec::new();
        for param in split_top_level(params, ",") {
            let toks = strip_initializer(param);
            let ds = declarator_start(toks);
            if ds == 0 || ds >= toks.len() {
                continue; // unnamed / abstract parameter
            }
            let Some(decl) = parse_declarator(&toks[ds..]) else {
                continue;
            };
            if decl.name.is_empty() {
                continue;
            }
            let type_str = self
                .src
                .get(toks[0].start..toks[ds].start)
                .unwrap_or("")
                .trim();
            let ty = ctype(type_str);
            emits.push((decl.name, toks[ds].row, ty));
        }
        for (name, row, ty) in emits {
            self.emit(
                &name,
                letter,
                row,
                vec![
                    (FIELD_TYPEREF_KEY, format!("{FIELD_TYPEREF_DEFAULT}:{ty}")),
                    (FIELD_PARAM_FUNCTION, function.clone()),
                ],
            );
        }
    }

    /// Emit labels (`L`) declared alone on a line within a function body.
    fn emit_labels(&mut self, brace: usize, fname: &str, lexical_owner: Option<&str>) {
        let Some(letter) = self.role("label") else {
            return;
        };
        if self.toks.get(brace).map(|t| t.text) != Some("{") {
            return;
        }
        let function = match lexical_owner {
            Some(owner) => format!("{owner}{DECL_SCOPE_OP}{fname}"),
            None => fname.to_owned(),
        };
        let end = skip_balanced(&self.toks, brace);
        let mut emits: Vec<(String, usize)> = Vec::new();
        let mut i = brace + 1;
        while i + 1 < end {
            let (a, b) = (self.toks[i], self.toks[i + 1]);
            // `IDENT :` alone on its line (the colon ends the line).
            let colon_ends_line = a.kind == Tok::Ident
                && b.text == ":"
                && a.row == b.row
                && self.toks.get(i + 2).is_none_or(|n| n.row > b.row);
            if colon_ends_line {
                emits.push((a.text.to_owned(), a.row));
            }
            i += 1;
        }
        for (name, row) in emits {
            self.emit(&name, letter, row, vec![(FIELD_PARAM_FUNCTION, function.clone())]);
        }
    }

    /// The innermost enclosing aggregate scope name (class/struct/union — the
    /// scopes entered by an `aggregate`-category specifier).
    fn aggregate_owner(&self) -> Option<String> {
        self.scopes
            .iter()
            .rev()
            .find(|f| {
                self.t
                    .specifiers
                    .iter()
                    .any(|s| s.category == "aggregate" && s.scope == Some(f.kind))
            })
            .map(|f| f.name.clone())
    }

    fn variables(&mut self, head: &[Token<'a>], _terminator: &str) {
        // Stream/expression statements (`std::cin >> val;`, `a.b = c;`) are not
        // declarations: a scope-resolution op combined with an expression-only
        // operator (from parse.json) signals an expression, not a declaration.
        let has_scope = head.iter().any(|t| t.text == EXPR_SKIP_SCOPE_OP);
        if has_scope && head.iter().any(|t| EXPR_SKIP_OPS.contains(&t.text)) {
            return;
        }
        let groups = split_top_level(head, ",");
        let Some(first) = groups.first() else { return };
        let first_decl_toks = strip_initializer(first);
        let decl_start = declarator_start(first_decl_toks);
        if decl_start >= first_decl_toks.len() {
            return;
        }
        let type_str = self
            .src
            .get(first_decl_toks[0].start..first_decl_toks[decl_start].start)
            .unwrap_or("")
            .trim();
        // member (inside an aggregate) vs a storage-class role (e.g. extern) vs
        // plain variable — storage roles come from the data table.
        let member_scope = self.aggregate_owner();
        let storage_letter = head.iter().find_map(|t| {
            self.t
                .storage_roles
                .iter()
                .find(|(kw, _)| *kw == t.text)
                .map(|(_, letter)| *letter)
        });
        let letter = if member_scope.is_some() {
            match self.role("member") {
                Some(l) => l,
                None => return,
            }
        } else if let Some(l) = storage_letter {
            l
        } else {
            match self.role("variable") {
                Some(l) => l,
                None => return,
            }
        };
        for (idx, group) in groups.iter().enumerate() {
            let toks = strip_initializer(group);
            let ds = if idx == 0 {
                decl_start
            } else {
                declarator_start(toks)
            };
            if ds >= toks.len() {
                continue;
            }
            let Some(decl) = parse_declarator(&toks[ds..]) else {
                continue;
            };
            if decl.is_function {
                continue;
            }
            let typeref = self.typeref_of(type_str, decl.leading_pointer);
            let row = toks[ds].row;
            self.emit(&decl.name, letter, row, vec![(FIELD_TYPEREF_KEY, typeref)]);
        }
    }

    fn typedef(&mut self, head: &[Token<'a>]) {
        let Some(letter) = self.role("typedef") else {
            return;
        };
        // head[0] is the typedef specifier. Function pointer: `RET (*NAME)(params)`.
        let body = &head[1..];
        if let Some((name, row, typeref)) = self.function_pointer(body) {
            self.emit(&name, letter, row, vec![(FIELD_TYPEREF_KEY, typeref)]);
            return;
        }
        // An aggregate-body typedef is handled by `type_specifier`.
        let heads_aggregate = body
            .first()
            .is_some_and(|t| self.spec(t.text, None).is_some_and(|s| s.scope.is_some()));
        if heads_aggregate && body.iter().any(|t| t.text == "{") {
            return;
        }
        // General: `typedef <type> <name>`; the name is the last declarator.
        let toks = strip_initializer(body);
        let ds = declarator_start(toks);
        if ds >= toks.len() {
            return;
        }
        let Some(decl) = parse_declarator(&toks[ds..]) else {
            return;
        };
        let type_str = self
            .src
            .get(toks[0].start..toks[ds].start)
            .unwrap_or("")
            .trim();
        let typeref = self.typeref_of(type_str, false);
        self.emit(&decl.name, letter, toks[ds].row, vec![(FIELD_TYPEREF_KEY, typeref)]);
    }

    /// Recognize `<type> (*NAME)(params)` → (name, row, typeref value).
    fn function_pointer(&self, body: &[Token<'a>]) -> Option<(String, usize, String)> {
        let lp = self.first_top_level(body, "(")?;
        // Must be `( <ptr> name )` immediately.
        if !body.get(lp + 1).is_some_and(|t| DECL_POINTER_PREFIXES.contains(&t.text)) {
            return None;
        }
        let name = body.get(lp + 2).filter(|t| t.kind == Tok::Ident)?;
        if body.get(lp + 3).map(|t| t.text) != Some(")") {
            return None;
        }
        let ret = self.src[body[0].start..body[lp].start].trim();
        let params_open = lp + 4;
        if body.get(params_open).map(|t| t.text) != Some("(") {
            return None;
        }
        let params_close = skip_balanced(body, params_open);
        let params = self.src[body[params_open].start..self.tok_end(&body[params_close - 1])].trim();
        Some((
            name.text.to_owned(),
            name.row,
            format!("{FIELD_TYPEREF_DEFAULT}:{ret} (*){params}"),
        ))
    }

    fn first_top_level(&self, toks: &[Token<'a>], target: &str) -> Option<usize> {
        let mut i = 0;
        while i < toks.len() {
            match toks[i].text {
                t if t == target => return Some(i),
                "(" | "[" | "<" | "{" => {
                    let next = skip_balanced(toks, i);
                    i = if next > i { next } else { i + 1 };
                    continue;
                }
                _ => i += 1,
            }
        }
        None
    }

    fn contiguous(&self, a: &Token, b: &Token) -> bool {
        self.tok_end(a) == b.start
            && matches!(a.kind, Tok::Ident | Tok::Keyword | Tok::Punct)
            && matches!(b.kind, Tok::Ident | Tok::Keyword | Tok::Punct)
    }
}

/// Strip a trailing initializer (`= ...`) from a declarator group.
fn strip_initializer<'a, 'b>(toks: &'b [Token<'a>]) -> &'b [Token<'a>] {
    match toks.iter().position(|t| t.text == "=") {
        Some(eq) => &toks[..eq],
        None => toks,
    }
}

/// Index of the first declarator token in a `type declarator` group: the first
/// pointer/reference, else the last identifier (the declared name).
fn declarator_start(toks: &[Token]) -> usize {
    if let Some(p) = toks.iter().position(|t| matches!(t.text, "*" | "&" | "&&")) {
        return p;
    }
    // Last identifier that is not swallowed by array/template brackets.
    let mut i = 0;
    let mut last_ident = toks.len();
    while i < toks.len() {
        match toks[i].text {
            "[" | "<" | "(" => {
                let next = skip_balanced(toks, i);
                i = if next > i { next } else { i + 1 };
                continue;
            }
            _ => {
                if toks[i].kind == Tok::Ident {
                    last_ident = i;
                }
                i += 1;
            }
        }
    }
    last_ident
}

/// Split a token slice on a top-level delimiter (ignoring `()`/`[]`/`<>`/`{}`).
fn split_top_level<'a, 'b>(toks: &'b [Token<'a>], delim: &str) -> Vec<&'b [Token<'a>]> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < toks.len() {
        match toks[i].text {
            "(" | "[" | "<" | "{" => {
                let next = skip_balanced(toks, i);
                i = if next > i { next } else { i + 1 };
                continue;
            }
            t if t == delim => {
                parts.push(&toks[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(&toks[start..]);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(Tok, &str)> {
        lex(src).into_iter().map(|t| (t.kind, t.text)).collect()
    }

    fn decl(src: &str) -> Declarator {
        parse_declarator(&lex(src)).expect("declarator parses")
    }

    #[test]
    fn classifies_keywords_and_identifiers() {
        assert_eq!(
            kinds("int main"),
            vec![(Tok::Keyword, "int"), (Tok::Ident, "main")]
        );
    }

    #[test]
    fn maximal_munch_operators() {
        assert_eq!(
            kinds("a >>= b"),
            vec![
                (Tok::Ident, "a"),
                (Tok::Punct, ">>="),
                (Tok::Ident, "b")
            ]
        );
        assert_eq!(
            kinds("x->y"),
            vec![(Tok::Ident, "x"), (Tok::Punct, "->"), (Tok::Ident, "y")]
        );
        // `A::B` scope resolution, not two colons.
        assert_eq!(
            kinds("A::B"),
            vec![(Tok::Ident, "A"), (Tok::Punct, "::"), (Tok::Ident, "B")]
        );
    }

    #[test]
    fn brackets_are_single_tokens() {
        // `x[y[z]]` must not munch `]]` or `[[`.
        assert_eq!(
            kinds("x[y[z]]"),
            vec![
                (Tok::Ident, "x"),
                (Tok::Punct, "["),
                (Tok::Ident, "y"),
                (Tok::Punct, "["),
                (Tok::Ident, "z"),
                (Tok::Punct, "]"),
                (Tok::Punct, "]"),
            ]
        );
    }

    #[test]
    fn skips_comments_and_tracks_rows() {
        let toks = lex("int a; // note\n/* multi\nline */ int b;");
        let names: Vec<_> = toks.iter().map(|t| t.text).collect();
        assert_eq!(names, vec!["int", "a", ";", "int", "b", ";"]);
        // `b` sits on the third physical line (row 2).
        let b = toks.iter().find(|t| t.text == "b").unwrap();
        assert_eq!(b.row, 2);
    }

    #[test]
    fn strings_and_chars_do_not_leak_tokens() {
        assert_eq!(
            kinds(r#"s = "a;b\"c"; c = '\'';"#),
            vec![
                (Tok::Ident, "s"),
                (Tok::Punct, "="),
                (Tok::Str, r#""a;b\"c""#),
                (Tok::Punct, ";"),
                (Tok::Ident, "c"),
                (Tok::Punct, "="),
                (Tok::Str, r#"'\''"#),
                (Tok::Punct, ";"),
            ]
        );
    }

    #[test]
    fn preprocessor_lines_are_single_tokens() {
        let toks = lex("#include <stdio.h>\nint x;");
        assert_eq!(toks[0].kind, Tok::Preproc);
        assert_eq!(toks[0].text, "#include <stdio.h>");
        assert_eq!(toks[1].text, "int");
        // A `#` mid-line is not a directive.
        let mid = lex("a # b");
        assert!(mid.iter().all(|t| t.kind != Tok::Preproc));
    }

    #[test]
    fn numbers_with_suffixes_and_exponents() {
        assert_eq!(kinds("0x1F 3.14e-10 1'000"), vec![
            (Tok::Num, "0x1F"),
            (Tok::Num, "3.14e-10"),
            (Tok::Num, "1'000"),
        ]);
    }

    #[test]
    fn declarator_plain_function() {
        let d = decl("add_two_ints(int x1, int x2)");
        assert_eq!(d.name, "add_two_ints");
        assert!(d.is_function);
        assert!(d.qualifier.is_none());
    }

    #[test]
    fn declarator_pointer_variable() {
        let d = decl("*px");
        assert_eq!(d.name, "px");
        assert!(d.leading_pointer);
        assert!(!d.is_function);
        assert_eq!(decl("not_a_pointer").name, "not_a_pointer");
    }

    #[test]
    fn declarator_qualified_method() {
        let d = decl("Dog::print()");
        assert_eq!(d.name, "print");
        assert_eq!(d.qualifier.as_deref(), Some("Dog"));
        assert!(d.is_function);
    }

    #[test]
    fn declarator_operator() {
        let d = decl("Point::operator+(const Point& rhs)");
        assert_eq!(d.name, "operator +");
        assert_eq!(d.qualifier.as_deref(), Some("Point"));
        assert!(d.is_function);
        assert_eq!(decl("operator+=(const Point& rhs)").name, "operator +=");
    }

    #[test]
    fn declarator_destructor() {
        let d = decl("Dog::~Dog()");
        assert_eq!(d.name, "~Dog");
        assert_eq!(d.qualifier.as_deref(), Some("Dog"));
        assert!(d.is_function);
    }

    #[test]
    fn declarator_function_pointer() {
        // typedef target `(*my_fnp_type)(char *)`
        let d = decl("(*my_fnp_type)(char *)");
        assert_eq!(d.name, "my_fnp_type");
        assert!(d.grouped_pointer);
        assert!(d.is_function);
    }

    #[test]
    fn declarator_array_and_field() {
        assert_eq!(decl("my_char_array[20]").name, "my_char_array");
        let d = decl("*next");
        assert_eq!(d.name, "next");
        assert!(d.leading_pointer);
    }

    #[test]
    fn declarator_templated_type_name() {
        let d = decl("boxOfBox");
        assert_eq!(d.name, "boxOfBox");
    }

    #[test]
    fn declarator_unbalanced_paren_does_not_panic() {
        // A trailing, unbalanced `(` (e.g. from macro-mangled kernel code) must
        // not panic the parenthesized-declarator slice. Regression for the
        // "slice index starts at 1 but ends at 0" crash.
        assert!(parse_declarator(&lex("(")).is_none());
        assert!(parse_declarator(&lex("*(")).is_none());
        assert!(parse_declarator(&lex("(*")).is_none());
        // An unbalanced group inside an otherwise real statement must not panic.
        let _ = lex("int foo(");
        assert!(parse_declarator(&lex("foo(")).is_some());
    }
}
