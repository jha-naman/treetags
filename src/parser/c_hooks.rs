#![allow(dead_code)]
//! Forward-only, token-stream tag hook for C, built on the shared linear
//! primitives (BlockMap auto-`end:`, scope stack, generated aliases). Validated
//! construct-by-construct against the tree-sitter C oracle (`cpp::generate` with
//! the C kind set), including its anonymous-struct naming and `typeref` quirks.

use super::{
    generated::c,
    linear::{BlockMap, HookInput, NoExternalLexer, TagHooks, Tok, TokenCursor, TokenRange},
    tag_emitter::{TagEmitter, TextValue},
};

pub(crate) struct CHooks {
    /// Anonymous aggregate counter, mirroring cpp.rs: starts at 1, read then
    /// incremented per generated name.
    sequence: u16,
    /// djb2 hash of the file name, as cpp.rs computes it.
    hash: String,
    /// End of the last inspected reference, preventing duplicates when a
    /// declaration is consumed by more than one handler.
    references_end: u32,
}

impl Default for CHooks {
    fn default() -> Self {
        Self {
            sequence: 1,
            hash: String::new(),
            references_end: 0,
        }
    }
}

impl CHooks {
    /// `__anon<filehash><seq><kind_id>`, kind_id 8 for struct/union, matching
    /// cpp.rs's `generate_anonymous_name`.
    fn anon(&mut self) -> String {
        let name = format!("__anon{}{:02x}{:02x}", self.hash, self.sequence, 8u8);
        self.sequence += 1;
        name
    }
}

impl TagHooks for CHooks {
    fn generate(
        &mut self,
        input: HookInput<'_>,
        mut cursor: TokenCursor<'_>,
        output: &mut TagEmitter<'_>,
    ) {
        // The walk always sits at file scope: function bodies are skipped whole,
        // and aggregate bodies are consumed by their handlers.
        while let Some(token) = cursor.peek(0) {
            if let Some(range) = standalone_macro_call(&cursor) {
                // A macro invocation need not end in `;`. Bound both reference
                // scanning and consumption to the call so the next item cannot
                // be mistaken for its return type, body, or declaration tail.
                let call = cursor.view(range).expect("range from this cursor");
                scan_type_references(&call, output, &mut self.references_end);
                while cursor.mark() < range.end {
                    cursor.next();
                }
                cursor.consume_if(c::SEMI);
                continue;
            }
            scan_type_references(&cursor, output, &mut self.references_end);
            cursor.next();
            match token.kind {
                c::LITERAL => self.directive(input.source, &mut cursor, output, token),
                c::KW_ENUM => self.enum_decl(&mut cursor, output, token),
                c::KW_STRUCT | c::KW_UNION => self.aggregate(&mut cursor, output, token),
                c::KW_TYPEDEF => self.typedef(&mut cursor, output, token),
                c::LBRACE => skip_block(&mut cursor),
                c::IDENTIFIER
                | c::KW_STATIC
                | c::KW_EXTERN
                | c::KW_CONST
                | c::KW_SIGNED
                | c::KW_UNSIGNED => self.declaration(&mut cursor, output, token),
                _ => {}
            }
        }
    }
}

impl CHooks {
    /// `#define NAME …` → macro `d`; `#include <h>`/`"h"` → header `h`. Every
    /// directive consumes its whole logical line, including backslash-newline
    /// continuations, so macro bodies and guard operands stay opaque.
    fn directive(
        &self,
        source: &str,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        token: Tok,
    ) {
        let text = cursor.text(token);
        let end = directive_end(source, token.start as usize);
        if text.starts_with("#define") {
            if cursor
                .peek(0)
                .is_some_and(|t| t.kind == c::IDENTIFIER && (t.start as usize) < end)
            {
                let name = cursor.next().expect("peeked macro name");
                out.tag("d", name, (token, name)).emit();
            }
        }
        let (mut first, mut last) = (None, None);
        while let Some(next) = cursor.peek(0) {
            if next.start as usize >= end {
                break;
            }
            cursor.next();
            first.get_or_insert(next);
            last = Some(next);
        }
        if text.starts_with("#include") {
            if let (Some(first), Some(last)) = (first, last) {
                let raw = cursor.span_text(first, last);
                let delim = |c| matches!(c, '<' | '>' | '"');
                let lead = (raw.len() - raw.trim_start_matches(delim).len()) as u32;
                let trail = (raw.len() - raw.trim_end_matches(delim).len()) as u32;
                out.tag(
                    "h",
                    TextValue::Span(first.start + lead, last.end - trail),
                    (token, first),
                )
                .emit();
            }
        }
    }

    /// `enum NAME { A, B = 2, C }` → enum `g` plus enumerators `e` scoped
    /// `enum:NAME`. Bare type references are emitted by the signature scan.
    fn enum_decl(&self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, kw: Tok) {
        let name = cursor.consume_if(c::IDENTIFIER);
        if cursor.peek(0).map(|t| t.kind) != Some(c::LBRACE) {
            return;
        }
        if let Some(name) = name {
            out.tag("g", name, (kw, name)).emit();
            out.enter_scope("enum", cursor.text(name).to_string());
        }
        cursor.next(); // consume `{`
        loop {
            while matches!(
                cursor.peek(0).map(|t| t.kind),
                Some(c::COMMA) | Some(c::SEMI)
            ) {
                cursor.next();
            }
            match cursor.peek(0).map(|t| t.kind) {
                None => break,
                Some(c::RBRACE) => {
                    cursor.next();
                    break;
                }
                _ => {}
            }
            if let Some(member) = cursor.consume_if(c::IDENTIFIER) {
                out.tag("e", member, (member, member)).emit();
            }
            skip_to_enum_separator(cursor);
        }
        if name.is_some() {
            out.leave_scope();
        }
    }

    /// A `struct`/`union` statement: a definition (`s`/`u` + members), or a
    /// type reference in a variable declaration (`struct NAME var;` → `s` ref +
    /// `v` with a `struct:` typeref).
    fn aggregate(&mut self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, akw: Tok) {
        // Aggregate return types use the same function/prototype parser.
        let mut i = 0;
        while let Some(t) = cursor.peek(i) {
            if t.kind == c::LPAREN {
                self.declaration(cursor, out, akw);
                return;
            }
            if matches!(t.kind, c::LBRACE | c::SEMI | c::EQ | c::COMMA) {
                break;
            }
            i += 1;
        }
        let (kind, scope_key) = agg_kind(akw);
        let name = cursor.consume_if(c::IDENTIFIER);
        if cursor.peek(0).map(|t| t.kind) == Some(c::LBRACE) {
            let (struct_name, addr, typeref) = match name {
                Some(n) => (
                    cursor.text(n).to_string(),
                    n,
                    Some(cursor.text(n).to_string()),
                ),
                None => (self.anon(), akw, None),
            };
            self.struct_body(cursor, out, kind, scope_key, struct_name, addr);
            // A trailing declarator, e.g. `struct { … } var;` (anonymous → no
            // typeref) or `struct Foo { … } var;`.
            let (var, star) = read_declarator(cursor);
            if let Some(var) = var {
                let mut builder = out.tag("v", var, (var, var));
                if let Some(name) = typeref {
                    builder = builder.typeref_as("struct", struct_ref_value(name, star));
                }
                builder.emit();
            }
        } else if let Some(name) = name {
            // `struct NAME <declarator>;` — reference tag plus the declared var.
            let struct_name = cursor.text(name).to_string();
            let (var, star) = read_declarator(cursor);
            if let Some(var) = var {
                out.tag("v", var, (var, var))
                    .typeref_as("struct", struct_ref_value(struct_name, star))
                    .emit();
            }
        } else {
            skip_to_semicolon(cursor);
        }
    }

    /// Emits a named/anonymous aggregate (`s`/`u`) and its members, with the
    /// cursor at the opening `{` (consumed here through the matching `}`).
    fn struct_body(
        &mut self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        kind: &'static str,
        scope_key: &'static str,
        name: String,
        addr: Tok,
    ) {
        out.tag(kind, TextValue::Owned(name.clone()), (addr, addr))
            .emit();
        cursor.next(); // consume `{`
        out.enter_scope(scope_key, name);
        self.members(cursor, out);
        out.leave_scope();
    }

    /// Aggregate members up to and including the closing `}`.
    fn members(&mut self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>) {
        loop {
            while cursor.consume_if(c::SEMI).is_some() {}
            match cursor.peek(0).map(|t| t.kind) {
                None => return,
                Some(c::RBRACE) => {
                    cursor.next();
                    return;
                }
                _ => {}
            }
            scan_type_references(cursor, out, &mut self.references_end);
            let Some(first) = cursor.next() else {
                return;
            };
            if matches!(first.kind, c::KW_STRUCT | c::KW_UNION) {
                self.struct_typed_member(cursor, out, first);
                continue;
            }
            let head = scan_decl_head(cursor, first);
            if let Some((type_start, type_end, name)) = head.named() {
                out.tag("m", name, (type_start, name))
                    .typeref(TextValue::Span(type_start.start, type_end.end))
                    .emit();
            }
            skip_to_semicolon(cursor);
        }
    }

    /// `struct NAME *field;` inside an aggregate body: a `s` reference tag plus
    /// the member `m` with a `struct:` typeref.
    fn struct_typed_member(
        &mut self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        _akw: Tok,
    ) {
        let Some(name) = cursor.consume_if(c::IDENTIFIER) else {
            skip_to_semicolon(cursor);
            return;
        };
        if cursor.peek(0).map(|t| t.kind) == Some(c::LBRACE) {
            // A nested aggregate member definition; still to come.
            cursor.next();
            skip_block(cursor);
            skip_to_semicolon(cursor);
            return;
        }
        let struct_name = cursor.text(name).to_string();
        let (field, star) = read_declarator(cursor);
        if let Some(field) = field {
            let builder = out.tag("m", field, (name, field));
            if star {
                builder
                    .typeref_as("struct", TextValue::Owned(format!("{struct_name} *")))
                    .emit();
            } else {
                builder
                    .typeref_as(
                        "typename",
                        TextValue::Owned(format!("struct:{struct_name}")),
                    )
                    .emit();
            }
        }
    }

    /// A type-led top-level declaration: a function definition (`f`, body
    /// skipped), a prototype (untagged), or a file-scope variable (`v`).
    fn declaration(&self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, first: Tok) {
        let head = scan_decl_head(cursor, first);
        if cursor.peek(0).map(|t| t.kind) == Some(c::LPAREN) {
            let Some(name) = head.name else {
                skip_to_semicolon(cursor);
                return;
            };
            cursor.consume_balanced_pair(c::LPAREN, c::RPAREN);
            if cursor.consume_if(c::LBRACE).is_some() {
                let ret_start = head.type_start.unwrap_or(name);
                let mut builder = out.tag("f", name, (ret_start, name));
                if let (Some(type_start), Some(type_end)) = (head.type_start, head.type_end) {
                    builder = builder.typeref(TextValue::Span(type_start.start, type_end.end));
                }
                builder.emit();
                skip_block(cursor);
            } else {
                skip_to_semicolon(cursor); // prototype (untagged)
            }
            return;
        }
        if let Some((type_start, type_end, name)) = head.named() {
            out.tag("v", name, (type_start, name))
                .typeref(TextValue::Span(type_start.start, type_end.end))
                .emit();
        }
        skip_to_semicolon(cursor);
    }

    /// `typedef …` in all its C shapes: simple named types, aggregate
    /// definitions/references, and function pointers.
    fn typedef(&mut self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, kw: Tok) {
        match cursor.peek(0).map(|t| t.kind) {
            Some(c::KW_STRUCT) | Some(c::KW_UNION) => self.typedef_aggregate(cursor, out, kw),
            _ => self.typedef_plain(cursor, out, kw),
        }
    }

    fn typedef_aggregate(
        &mut self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        kw: Tok,
    ) {
        let akw = cursor.next().expect("peeked aggregate keyword");
        let (kind, scope_key) = agg_kind(akw);
        let name = cursor.consume_if(c::IDENTIFIER);
        if cursor.peek(0).map(|t| t.kind) == Some(c::LBRACE) {
            // `typedef struct [NAME] { … } ALIAS;`. For an anonymous body cpp.rs
            // generates the typeref's anonymous name *before* the definition's,
            // so reserve it first to match the sequence numbers.
            let (typeref_name, struct_name, addr) = match name {
                Some(n) => (cursor.text(n).to_string(), cursor.text(n).to_string(), n),
                None => {
                    let typeref = self.anon();
                    let definition = self.anon();
                    (typeref, definition, akw)
                }
            };
            self.struct_body(cursor, out, kind, scope_key, struct_name, addr);
            if let Some(alias) = read_declarator(cursor).0 {
                out.tag("t", alias, (kw, alias))
                    .typeref_as("struct", TextValue::Owned(typeref_name))
                    .emit();
            }
        } else if let Some(name) = name {
            // `typedef struct NAME ALIAS;` — reference tag plus the alias.
            let struct_name = cursor.text(name).to_string();
            if let Some(alias) = read_declarator(cursor).0 {
                out.tag("t", alias, (kw, alias))
                    .typeref_as("struct", TextValue::Owned(struct_name))
                    .emit();
            }
        } else {
            skip_to_semicolon(cursor);
        }
    }

    fn typedef_plain(&mut self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, kw: Tok) {
        let Some(first) = cursor.next() else {
            return;
        };
        let head = scan_decl_head(cursor, first);
        if cursor.peek(0).map(|t| t.kind) == Some(c::LPAREN) {
            // Function pointer: `typedef <ret> (*NAME)(params);`. cpp.rs renders
            // the base as a literal `void (*)` regardless of the return type.
            cursor.next(); // `(`
            if cursor.consume_if(c::STAR).is_some() {
                if let Some(name) = cursor.consume_if(c::IDENTIFIER) {
                    cursor.consume_if(c::RPAREN);
                    if let Some(params) = cursor.consume_balanced_pair(c::LPAREN, c::RPAREN) {
                        let text = cursor.span_text(params.open, params.close);
                        out.tag("t", name, (kw, name))
                            .typeref_as("typename", TextValue::Owned(format!("void (*){text}")))
                            .emit();
                    }
                }
            }
            skip_to_semicolon(cursor);
            return;
        }
        if let Some((type_start, type_end, name)) = head.named() {
            out.tag("t", name, (kw, name))
                .typeref(TextValue::Span(type_start.start, type_end.end))
                .emit();
        }
        skip_to_semicolon(cursor);
    }
}

/// The first unspliced newline ends a preprocessor directive. Inspect source
/// bytes because the scanner discards backslash-newline pairs and whitespace,
/// including continuation lines with no tokens. CRLF is spliced as a unit.
fn directive_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    for i in start..bytes.len() {
        if bytes[i] == b'\n' {
            let before_newline = if i > start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            if before_newline == start || bytes[before_newline - 1] != b'\\' {
                return i;
            }
        }
    }
    bytes.len()
}

/// Recognize a bare `NAME(...)` (optionally storage-qualified), without relying
/// on macro names or line breaks. A body or declarator continuation belongs to
/// the ordinary declaration handler; only standalone calls stop at `)`.
fn standalone_macro_call(cursor: &TokenCursor<'_>) -> Option<TokenRange> {
    let mut i = 0;
    while cursor
        .peek(i)
        .is_some_and(|t| matches!(t.kind, c::KW_STATIC | c::KW_EXTERN))
    {
        i += 1;
    }
    if cursor.peek(i)?.kind != c::IDENTIFIER || cursor.peek(i + 1)?.kind != c::LPAREN {
        return None;
    }
    i += 2;
    let mut depth = 1u32;
    while let Some(token) = cursor.peek(i) {
        match token.kind {
            c::LPAREN => depth += 1,
            c::RPAREN => {
                depth -= 1;
                if depth == 0 {
                    let end = i + 1;
                    if cursor.peek(end).is_some_and(|t| {
                        matches!(
                            t.kind,
                            c::LBRACE | c::EQ | c::COMMA | c::LPAREN | c::LBRACKET
                        )
                    }) {
                        return None;
                    }
                    return Some(TokenRange {
                        start: cursor.mark(),
                        end: cursor.mark() + end,
                    });
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Inspect a declaration before its handler consumes the signature. References
/// keep the enclosing scope; named definitions are emitted by their handlers.
/// Stop at a body or statement boundary so skipped function bodies stay skipped.
fn scan_type_references(cursor: &TokenCursor<'_>, out: &mut TagEmitter<'_>, scanned_end: &mut u32) {
    // The oracle puts the entire definition signature in function scope and
    // suppresses struct references there (but retains enum/union references).
    let mut function = None;
    let mut depth = 0u32;
    let mut previous = None;
    let mut candidate = None;
    let mut i = 0;
    while let Some(token) = cursor.peek(i) {
        match token.kind {
            c::LITERAL | c::SEMI => break,
            c::LPAREN => {
                if depth == 0 && candidate.is_none() {
                    candidate = previous;
                }
                depth += 1;
            }
            c::RPAREN => depth = depth.saturating_sub(1),
            c::LBRACE => {
                if depth == 0 {
                    function = candidate;
                }
                break;
            }
            _ => {}
        }
        previous = Some(token);
        i += 1;
    }
    if let Some(name) = function {
        out.enter_scope("function", cursor.text(name).to_string());
    }

    let mut index = 0;
    while let Some(kw) = cursor.peek(index) {
        if matches!(kw.kind, c::LBRACE | c::SEMI | c::LITERAL) {
            break;
        }
        let kind = match kw.kind {
            c::KW_STRUCT => "s",
            c::KW_UNION => "u",
            c::KW_ENUM => "g",
            _ => {
                index += 1;
                continue;
            }
        };
        if let Some(name) = cursor.peek(index + 1).filter(|t| t.kind == c::IDENTIFIER) {
            if cursor.peek(index + 2).map(|t| t.kind) != Some(c::LBRACE) {
                if name.start >= *scanned_end {
                    if kind != "s" || function.is_none() {
                        out.tag(kind, name, (kw, kw)).emit();
                    }
                    *scanned_end = name.end;
                }
            }
        }
        index += 1;
    }
    if function.is_some() {
        out.leave_scope();
    }
}

/// Type specifier + declarator name gathered from a linear run of tokens.
#[derive(Clone, Copy)]
struct DeclHead {
    type_start: Option<Tok>,
    type_end: Option<Tok>,
    name: Option<Tok>,
}

impl DeclHead {
    /// A named declaration only when a type precedes the name.
    fn named(self) -> Option<(Tok, Tok, Tok)> {
        match (self.type_start, self.type_end, self.name) {
            (Some(type_start), Some(type_end), Some(name)) => Some((type_start, type_end, name)),
            _ => None,
        }
    }
}

fn agg_kind(kw: Tok) -> (&'static str, &'static str) {
    if kw.kind == c::KW_UNION {
        ("u", "union")
    } else {
        ("s", "struct")
    }
}

/// The `struct:` typeref value for a referenced aggregate: the tag name, plus a
/// ` *` suffix when the declarator is a pointer.
fn struct_ref_value(name: String, star: bool) -> TextValue<'static> {
    TextValue::Owned(if star { format!("{name} *") } else { name })
}

/// Gathers a declaration's type specifier and declarator name from `first` up to
/// the next top-level declarator boundary (`(`, `;`, `=`, `[`, `,`, `{`), which
/// is left unconsumed. The declarator name is the final identifier before that
/// boundary; storage specifiers (`static`/`extern`/`typedef`) are not part of
/// the type.
fn scan_decl_head(cursor: &mut TokenCursor<'_>, first: Tok) -> DeclHead {
    let mut head = DeclHead {
        type_start: None,
        type_end: None,
        name: None,
    };
    absorb(&mut head, first);
    while let Some(next) = cursor.peek(0) {
        if matches!(
            next.kind,
            c::LPAREN | c::SEMI | c::EQ | c::LBRACKET | c::COMMA | c::LBRACE
        ) {
            break;
        }
        cursor.next();
        absorb(&mut head, next);
    }
    head
}

fn absorb(head: &mut DeclHead, token: Tok) {
    if matches!(token.kind, c::KW_STATIC | c::KW_EXTERN | c::KW_TYPEDEF) {
        return;
    }
    head.type_start.get_or_insert(token);
    if token.kind == c::IDENTIFIER {
        if let Some(prev) = head.name {
            head.type_end = Some(prev);
        }
        head.name = Some(token);
    } else {
        head.type_end = Some(token);
    }
}

/// Consumes through the next top-level `;`, reporting the final identifier before
/// it (the declared name) and whether the declarator was a pointer.
fn read_declarator(cursor: &mut TokenCursor<'_>) -> (Option<Tok>, bool) {
    let (mut name, mut star, mut depth) = (None, false, 0u32);
    while let Some(token) = cursor.next() {
        match token.kind {
            c::LBRACE | c::LPAREN | c::LBRACKET => depth += 1,
            c::RBRACE | c::RPAREN | c::RBRACKET => depth = depth.saturating_sub(1),
            c::SEMI if depth == 0 => break,
            c::STAR if depth == 0 => star = true,
            c::IDENTIFIER if depth == 0 => name = Some(token),
            _ => {}
        }
    }
    (name, star)
}

/// Consumes to the `}` matching an already-consumed `{`.
fn skip_block(cursor: &mut TokenCursor<'_>) {
    let mut depth = 1u32;
    while let Some(token) = cursor.next() {
        match token.kind {
            c::LBRACE => depth += 1,
            c::RBRACE => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

/// Consumes through the next top-level `;`, skipping balanced groups so a `;`
/// inside an initializer does not end the statement early.
fn skip_to_semicolon(cursor: &mut TokenCursor<'_>) {
    let mut depth = 0u32;
    while let Some(token) = cursor.next() {
        match token.kind {
            c::LBRACE | c::LPAREN | c::LBRACKET => depth += 1,
            c::RBRACE | c::RPAREN | c::RBRACKET => depth = depth.saturating_sub(1),
            c::SEMI if depth == 0 => return,
            _ => {}
        }
    }
}

/// Advances to the next top-level `,` or `}` (both left unconsumed), skipping any
/// balanced initializer expression after `=`.
fn skip_to_enum_separator(cursor: &mut TokenCursor<'_>) {
    let mut depth = 0u32;
    while let Some(token) = cursor.peek(0) {
        match token.kind {
            c::LPAREN | c::LBRACKET => depth += 1,
            c::RPAREN | c::RBRACKET => depth = depth.saturating_sub(1),
            c::RBRACE if depth == 0 => return,
            c::RBRACE => depth -= 1,
            c::COMMA if depth == 0 => return,
            _ => {}
        }
        cursor.next();
    }
}

fn filename_hash(path: &str) -> String {
    let mut hash: u32 = 5381;
    for byte in path.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    format!("{hash:08x}")
}

/// Adapter matching `BuiltinGenerateFn` so the C row can select this backend
/// behind the `native-c` feature. Mirrors `go::generate`.
pub(crate) fn generate_builtin(
    _ts_parser: &mut tree_sitter::Parser,
    code: &[u8],
    path: &str,
    kinds: &super::TagKindConfig,
    config: &crate::config::Config,
) -> Option<Vec<crate::tag::Tag>> {
    let source = match std::str::from_utf8(code) {
        Ok(source) => source,
        Err(_) => {
            eprintln!("Warning: Input for {path} is not valid UTF-8, skipping.");
            return None;
        }
    };
    generate(
        source,
        path,
        super::linear::HookOptions::from_config(kinds, config),
    )
    .map_err(|error| eprintln!("Warning: Failed to scan {path}: {error}"))
    .ok()
}

pub(crate) fn generate(
    source: &str,
    path: &str,
    mut options: super::linear::HookOptions<'_>,
) -> Result<Vec<crate::tag::Tag>, String> {
    // The tree-sitter C backend keeps the shorthand kind column and never emits
    // `kind:`/`file:` as extension fields.
    options.kind = false;
    options.file = false;
    let stream = c::scan::<NoExternalLexer>(source)?;
    let input = HookInput {
        source,
        path,
        options,
        line_starts: &stream.line_starts,
    };
    let blocks = BlockMap::new(&stream.tokens, c::DELIMITERS);
    let mut tags = Vec::new();
    let mut emitter = TagEmitter::new(input, &mut tags, blocks);
    let mut hooks = CHooks {
        sequence: 1,
        hash: filename_hash(path),
        references_end: 0,
    };
    hooks.generate(
        input,
        TokenCursor::new(source, &stream.tokens),
        &mut emitter,
    );
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::cpp::{C_KIND_DEFAULTS, C_KIND_OPTIONALS};
    use crate::parser::TagKindConfig;
    use clap::Parser as _;

    fn oracle(source: &str) -> Vec<crate::tag::Tag> {
        let kinds = TagKindConfig::from_string("", C_KIND_DEFAULTS, C_KIND_OPTIONALS);
        let config = crate::config::Config::parse_from(["treetags"]);
        crate::parser::cpp::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "source.c",
            &kinds,
            &config,
        )
        .unwrap()
    }

    fn actual(source: &str) -> Vec<crate::tag::Tag> {
        let kinds = TagKindConfig::from_string("", C_KIND_DEFAULTS, C_KIND_OPTIONALS);
        let config = crate::config::Config::parse_from(["treetags"]);
        generate(
            source,
            "source.c",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap()
    }

    fn sorted(mut tags: Vec<crate::tag::Tag>) -> Vec<crate::tag::Tag> {
        tags.sort_by(|a, b| {
            (a.name.as_str(), a.kind.as_deref(), a.address.as_str()).cmp(&(
                b.name.as_str(),
                b.kind.as_deref(),
                b.address.as_str(),
            ))
        });
        tags
    }

    fn assert_matches_oracle(source: &str) {
        assert_eq!(sorted(actual(source)), sorted(oracle(source)));
    }

    #[test]
    fn continued_defines_do_not_swallow_following_items() {
        let source = concat!(
            "#define OBJECT_BODY \\\n",
            "    loose_identifier + \\\n",
            "    another_identifier\n",
            "#define AFTER_OBJECT 1\n",
            "int after_object(void) { return AFTER_OBJECT; }\n",
            "#define FUNCTION_BODY(value) \\\n",
            "    do { \\\n",
            "        struct Hidden *hidden = (value); \\\n",
            "        use(hidden); \\\n",
            "    } while (0)\n",
            "#define AFTER_FUNCTION 2\n",
            "int after_function(void) { return AFTER_FUNCTION; }\n",
            "void after_prototype(struct Context *, union Value *, enum Mode);\n",
            "int after_variable;\n",
        );
        assert_matches_oracle(source);
        assert_matches_oracle(&source.replace('\n', "\r\n"));
    }

    #[test]
    fn continued_directive_boundaries_match_oracle() {
        for source in [
            // Token-free continuation lines still belong to the directive.
            "#define EMPTY_LINES \\\n\\\n loose_token\n#define AFTER_EMPTY 1\nint after_empty;\n",
            // A blank, unescaped line ends the directive.
            "#define BLANK_END value \\\n\n#define AFTER_BLANK 1\nint after_blank;\n",
            // A backslash inside a comment is not a line splice.
            "#define COMMENT_END 1 /* \\ */\nint after_comment;\n",
            // Handle EOF both with and without a final newline.
            "#define AT_EOF \\\n loose_token",
            "#define AT_EOF \\\n loose_token\n",
        ] {
            assert_matches_oracle(source);
        }
    }

    #[test]
    fn standalone_macro_calls_do_not_swallow_following_items() {
        for invocation in [
            "DEFINE_FREE(cleanup, void *, release(_T))",
            "EXPORT_SYMBOL(exported)",
            "LIST_HEAD(entries)",
            "DEFINE_PER_CPU(int, counter)",
            "static DEFINE_PER_CPU(int, counter)",
            "CUSTOM_CALL(outer(inner(1)),\n second(2))",
        ] {
            for suffix in ["", ";"] {
                let following = "#define AFTER_CALL 1\n\
                    int after_call(void) { return AFTER_CALL; }\n\
                    int after_variable;\n";
                let source = format!("{invocation}{suffix}\n{following}");
                assert_matches_oracle(&source);
                assert_eq!(sorted(actual(&source)), sorted(actual(following)));
            }
        }
    }

    #[test]
    fn macro_boundary_preserves_following_signature_scope() {
        assert_matches_oracle(
            "LIST_HEAD(entries)\n\
             int after_macro(union Payload *p, enum State s) { return 0; }\n\
             EXPORT_SYMBOL(after_macro)\n\
             void next_prototype(struct Context *, union Value *, enum Mode);\n",
        );
    }

    #[test]
    fn macro_call_boundary_keeps_declaration_continuations() {
        assert_matches_oracle(
            "DEFINE_PER_CPU(int, counter) = initial_value;\n\
             int after_initializer;\n\
             int multiline(\n int value\n)\n{ return value; }\n\
             void prototype(\n struct Context *\n);\n\
             int (*callback)(int);\n\
             int after_callback;\n",
        );
    }

    #[test]
    fn signature_type_references_match_oracle() {
        let source = "void prototype(struct Param *, enum Mode, union Value *);\n\
            extern struct Result *factory(union Input *, enum State);\n\
            void multiline(struct\n MultiStruct *, enum\n MultiEnum, union\n MultiUnion *);\n\
            void repeated(struct Repeat *, struct Repeat *);\n\
            typedef void (*Callback)(struct Context *, enum Event, union Data *);\n\
            int defined(struct Argument *arg, union Payload *p, enum Flag f) { return 0; }\n\
            #define CAST(x) ((struct MacroType *)(x))\n\
            DECLARE(struct MacroArgument *, enum MacroEnum, union MacroUnion);\n";
        let refs = |tags: Vec<crate::tag::Tag>| {
            sorted(
                tags.into_iter()
                    .filter(|t| matches!(t.kind.as_deref(), Some("s" | "g" | "u")))
                    .collect(),
            )
        };
        assert_eq!(refs(actual(source)), refs(oracle(source)));
    }

    #[test]
    fn includes_macros_enums_match_oracle() {
        assert_matches_oracle(
            "#define DAYS_IN_YEAR 365\n\
             #define ADD(a, b) ((a) + (b))\n\
             #include <stdlib.h>\n\
             #include \"../my_lib/my_lib_header.h\"\n\
             enum days {SUN = 1, MON, TUE, WED = 99, THU, FRI, SAT};\n\
             enum traffic_light_state {GREEN, YELLOW, RED};\n",
        );
    }

    #[test]
    fn functions_vars_named_structs_match_oracle() {
        assert_matches_oracle(
            "int function_2(void);\n\
             int add_two_ints(int x1, int x2)\n{\n  return x1 + x2;\n}\n\
             void function_1()\n{\n  int local = 3;\n}\n\
             struct rectangle {\n  int width;\n  int height;\n};\n\
             int i = 0;\n\
             static int j = 0;\n",
        );
    }

    #[test]
    fn typedefs_and_struct_refs_match_oracle() {
        assert_matches_oracle(
            "typedef int my_type;\n\
             my_type my_type_var = 0;\n\
             struct rectangle {\n  int width;\n};\n\
             typedef struct rectangle rect;\n\
             struct rectangle r;\n\
             typedef void (*my_fnp_type)(char *);\n",
        );
    }

    #[test]
    fn anonymous_and_node_structs_match_oracle() {
        assert_matches_oracle(
            "struct {\n  int x;\n  int y;\n} anonymous_struct_var;\n\
             typedef struct {\n  int width;\n  int height;\n} rect;\n\
             typedef struct Node\n{\n    int val;\n    struct Node *next;\n} Node;\n",
        );
    }

    #[test]
    fn basic_fixture_matches_oracle() {
        let source = include_str!("../../tests/test_cases/c/basic/input/source.c");
        assert_eq!(sorted(actual(source)), sorted(oracle(source)));
    }

    #[test]
    fn header_and_guard_fixture_matches_oracle() {
        assert_matches_oracle(include_str!(
            "../../tests/test_cases/c/header_selector/input/api.h"
        ));
        assert_matches_oracle(include_str!(
            "../../tests/test_cases/c/langmap_custom_ext/input/widget.qc"
        ));
    }
}
