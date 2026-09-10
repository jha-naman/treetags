#![allow(dead_code)]
//! Forward-only, token-stream tag hook for C, built on the shared linear
//! primitives (BlockMap auto-`end:`, scope stack, generated aliases). Validated
//! construct-by-construct against the tree-sitter C oracle (`cpp::generate` with
//! the C kind set), including its anonymous-struct naming and `typeref` quirks.

use super::{
    generated::c,
    linear::{
        BalancedUntil, BlockMap, ExternalLexInput, ExternalLexemeSink, ExternalLexer, ExternalScan,
        HookInput, MemberRule, SeparatedRange, TagHooks, Tok, TokenCursor, TokenFlags, TokenKind,
        TokenRange,
    },
    tag_emitter::{TagBuilder, TagEmitter, TextValue},
};
use std::num::NonZeroU32;

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
        _input: HookInput<'_>,
        mut cursor: TokenCursor<'_>,
        output: &mut TagEmitter<'_>,
    ) {
        output.inherit_all_scopes();
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
                c::LITERAL => self.directive(&mut cursor, output, token),
                c::KW_ENUM => self.enum_decl(&mut cursor, output, token),
                c::KW_STRUCT | c::KW_UNION => self.aggregate(&mut cursor, output, token),
                c::KW_TYPEDEF => self.typedef(&mut cursor, output, token),
                c::LBRACE => skip_block(&mut cursor),
                c::IDENTIFIER => self.declaration(&mut cursor, output, token),
                _ if storage_or_qualifier(token)
                    || size_specifier(token)
                    || attribute_keyword(token) =>
                {
                    self.declaration(&mut cursor, output, token)
                }
                _ => {}
            }
        }
    }
}

impl CHooks {
    /// A preprocessor directive, pre-tokenized by [`CPreprocLexer`] into the
    /// `#keyword` introducer, an optional name/path operand, and a virtual `;`
    /// terminator. `#define NAME …` → macro `d`; `#include <h>`/`"h"` → header `h`.
    fn directive(&self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, introducer: Tok) {
        let text = cursor.text(introducer);
        let keyword = text.strip_prefix('#').map(str::trim_start).unwrap_or(text);
        if keyword.starts_with("define") {
            if let Some(name) = cursor.consume_if(c::IDENTIFIER) {
                out.tag("d", name, (introducer, name)).emit();
            }
        } else if keyword.starts_with("include") {
            if let Some(path) = cursor.peek(0).filter(|t| t.kind != c::SEMI) {
                cursor.next();
                let raw = cursor.text(path);
                let delim = |c| matches!(c, '<' | '>' | '"');
                let lead = (raw.len() - raw.trim_start_matches(delim).len()) as u32;
                let trail = (raw.len() - raw.trim_end_matches(delim).len()) as u32;
                out.tag(
                    "h",
                    TextValue::Span(path.start + lead, path.end - trail),
                    (introducer, path),
                )
                .emit();
            }
        }
        // Consume the virtual terminator at the top level; inside a bounded member
        // fragment it was already consumed as the fragment separator.
        cursor.consume_if(c::SEMI);
    }

    /// `enum NAME { A, B = 2, C }` → enum `g` plus enumerators `e` scoped
    /// `enum:NAME`. Bare type references are emitted by the signature scan.
    fn enum_decl(&self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, kw: Tok) {
        while consume_attribute(cursor) {}
        let name = cursor.consume_if(c::IDENTIFIER);
        if cursor.peek(0).map(|t| t.kind) != Some(c::LBRACE) {
            return;
        }
        // Compute the scope name before the closure borrows the cursor.
        let scope_name = name.map(|name| {
            out.tag("g", name, (kw, name)).emit();
            cursor.text(name).to_string()
        });
        cursor.next(); // consume `{`
        let mut scan_members = |out: &mut TagEmitter<'_>| loop {
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
        };
        match scope_name {
            Some(name) => out.in_scope("enum", name, scan_members),
            None => scan_members(out),
        }
    }

    /// A `struct`/`union` statement: a definition (`s`/`u` + members), or a
    /// type reference in a variable declaration (`struct NAME var;` → `s` ref +
    /// `v` with a `struct:` typeref).
    fn aggregate(&mut self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, akw: Tok) {
        // Aggregate return types use the same function/prototype parser.
        let mut i = 0;
        while let Some(t) = cursor.peek(i) {
            if let Some(end) = attribute_end(cursor, i) {
                i = end;
                continue;
            }
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
        while consume_attribute(cursor) {}
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
            let (var, _star) = read_declarator(cursor);
            if let Some(var) = var {
                let mut builder = out.tag("v", var, (var, var));
                if let Some(name) = typeref {
                    // Variable typerefs omit the pointer star (oracle parity).
                    builder = builder.typeref_as("struct", TextValue::Owned(name));
                }
                builder.emit();
            }
        } else if let Some(name) = name {
            // `struct NAME <declarator>;` — reference tag plus the declared var.
            let struct_name = cursor.text(name).to_string();
            let (var, _star) = read_declarator(cursor);
            if let Some(var) = var {
                out.tag("v", var, (var, var))
                    // Variable typerefs omit the pointer star (oracle parity).
                    .typeref_as("struct", TextValue::Owned(struct_name))
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
        out.in_scope(scope_key, name, |out| self.members(cursor, out));
    }

    /// Aggregate members up to and including the closing `}`. Each member is a
    /// top-level-`;`-delimited fragment; because `CPreprocLexer` self-terminates
    /// directives with a virtual `;`, a `#define` inside a body is now one clean
    /// fragment too, so C can share the [`TokenCursor::members`] combinator.
    fn members(&mut self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>) {
        let rule = MemberRule {
            close: c::RBRACE,
            skip: &[c::SEMI],
            fragment: BalancedUntil {
                delimiters: c::DELIMITERS,
                owner_close: Some(c::RBRACE),
                logical_line: false,
                can_terminate_line: |_| false,
            },
        };
        cursor.members(rule, |member| self.member(member, out));
    }

    /// One aggregate member, bounded to its own fragment: a directive (`d`/`h`),
    /// a nested/typed aggregate field, or a plain typed field (`m`).
    fn member(&mut self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>) {
        scan_type_references(cursor, out, &mut self.references_end);
        let Some(first) = cursor.next() else {
            return;
        };
        if first.kind == c::LITERAL {
            self.directive(cursor, out, first);
        } else if matches!(first.kind, c::KW_STRUCT | c::KW_UNION) {
            self.struct_typed_member(cursor, out, first);
        } else {
            let head = scan_decl_head(cursor, first);
            if let Some((type_start, _type_end, name)) = head.named() {
                head.with_typeref(out.tag("m", name, (type_start, name)), cursor, "m")
                    .emit();
            }
        }
    }

    /// Aggregate-typed fields, recursively including named and anonymous bodies.
    /// Trailing declarators belong to the enclosing scope, not the nested body.
    fn struct_typed_member(
        &mut self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        akw: Tok,
    ) {
        while consume_attribute(cursor) {}
        let name = cursor.consume_if(c::IDENTIFIER);
        let nested = cursor.peek(0).map(|t| t.kind) == Some(c::LBRACE);
        if nested {
            let (kind, scope_key) = agg_kind(akw);
            if let Some(name) = name {
                self.struct_body(
                    cursor,
                    out,
                    kind,
                    scope_key,
                    cursor.text(name).to_string(),
                    akw,
                );
            } else if akw.kind == c::KW_STRUCT {
                let anonymous = self.anon();
                self.struct_body(cursor, out, kind, scope_key, anonymous, akw);
            } else {
                // The oracle gives anonymous unions neither a name nor a scope.
                cursor.next();
                self.members(cursor, out);
            }
        } else if name.is_none() {
            skip_to_semicolon(cursor);
            return;
        }
        let (field, star) = read_field_declarator(cursor);
        if let Some(field) = field {
            let mut builder = out.tag("m", field, (akw, field));
            // Only struct specifiers contribute a field typeref in cpp.rs.
            if akw.kind == c::KW_STRUCT {
                if let Some(name) = name {
                    let struct_name = cursor.text(name);
                    builder = if star {
                        builder.typeref_as("struct", TextValue::Owned(format!("{struct_name} *")))
                    } else {
                        builder.typeref(TextValue::Owned(format!("struct:{struct_name}")))
                    };
                }
            }
            builder.emit();
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
            while consume_attribute(cursor) {}
            if cursor.consume_if(c::LBRACE).is_some() {
                let mut builder = out.tag("f", name, (first, name));
                builder = head.with_typeref(builder, cursor, "f");
                builder.emit();
                skip_block(cursor);
            } else {
                skip_to_semicolon(cursor); // prototype (untagged)
            }
            return;
        }
        if let Some((type_start, _type_end, name)) = head.named() {
            head.with_typeref(out.tag("v", name, (type_start, name)), cursor, "v")
                .emit();
            // Only plain names belong in this range. A declarator with an
            // initializer, array suffix, or parameter list needs its own parser.
            let start = cursor.mark();
            while cursor.peek(0).is_some_and(|t| t.kind == c::COMMA)
                && cursor.peek(1).is_some_and(|t| t.kind == c::IDENTIFIER)
                && cursor
                    .peek(2)
                    .is_some_and(|t| matches!(t.kind, c::COMMA | c::SEMI))
            {
                cursor.next(); // `,`
                cursor.next(); // name
            }
            let names = SeparatedRange {
                range: TokenRange {
                    start,
                    end: cursor.mark(),
                },
                item: c::IDENTIFIER,
            };
            for name in names.items(cursor).expect("range from this cursor") {
                head.with_typeref(out.tag("v", name, (name, name)), cursor, "v")
                    .emit();
            }
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
        while consume_attribute(cursor) {}
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
    // Jump the argument `(...)`; `range.end` is one past the matching `)`.
    let range = cursor.peek_balanced(i + 1)?;
    let after = range.end - cursor.mark();
    if cursor.peek(after).is_some_and(|t| {
        matches!(
            t.kind,
            c::LBRACE | c::EQ | c::COMMA | c::LPAREN | c::LBRACKET
        )
    }) {
        return None;
    }
    Some(TokenRange {
        start: cursor.mark(),
        end: range.end,
    })
}

/// Inspect a declaration before its handler consumes the signature. References
/// keep the enclosing scope; named definitions are emitted by their handlers.
/// Stop at a body or statement boundary so skipped function bodies stay skipped.
fn scan_type_references(cursor: &TokenCursor<'_>, out: &mut TagEmitter<'_>, scanned_end: &mut u32) {
    // The oracle puts the entire definition signature in function scope and
    // suppresses struct references there (but retains enum/union references).
    let mut function = None;
    let mut previous = None;
    let mut candidate = None;
    let mut i = 0;
    while let Some(token) = cursor.peek(i) {
        if let Some(end) = attribute_end(cursor, i) {
            i = end;
            continue;
        }
        match token.kind {
            c::LITERAL | c::SEMI => break,
            c::LPAREN => {
                // The identifier before the first top-level group is the function
                // name candidate; jump the group rather than counting depth.
                if candidate.is_none() {
                    candidate = previous;
                }
                match cursor.peek_balanced(i) {
                    Some(range) => {
                        i = range.end - cursor.mark();
                        continue;
                    }
                    None => break,
                }
            }
            c::LBRACE => {
                function = candidate;
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
    specifier: Option<(Tok, Tok)>,
    pointer: bool,
}

impl DeclHead {
    /// cpp.rs reads the type-specifier node, not the entire declaration prefix.
    /// Its member-pointer spelling deliberately omits the `typename:` prefix.
    fn with_typeref<'e, 'a>(
        self,
        builder: TagBuilder<'e, 'a>,
        cursor: &TokenCursor<'_>,
        kind: &str,
    ) -> TagBuilder<'e, 'a> {
        let Some((first, last)) = self.specifier else {
            return builder;
        };
        if matches!(first.kind, c::KW_STRUCT | c::KW_UNION | c::KW_ENUM) {
            if first.kind != c::KW_STRUCT || kind == "f" {
                return builder;
            }
            let name = cursor.text(last);
            return if kind == "m" {
                // Members spell the pointer as `struct:Name *`; a plain member as
                // `typename:struct:Name`.
                if self.pointer {
                    builder.typeref_as("struct", TextValue::Owned(format!("{name} *")))
                } else {
                    builder.typeref(TextValue::Owned(format!("struct:{name}")))
                }
            } else {
                // Variables never carry the pointer star on the struct typeref.
                builder.typeref_as("struct", TextValue::Owned(name.to_owned()))
            };
        }
        if kind == "m" && self.pointer {
            builder.typeref_raw(TextValue::Owned(format!(
                "{} *",
                cursor.span_text(first, last)
            )))
        } else {
            builder.typeref(TextValue::Span(first.start, last.end))
        }
    }

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
        specifier: None,
        pointer: false,
    };
    let mut tokens = Vec::new();
    if attribute_keyword(first) && cursor.peek(0).is_some_and(|t| t.kind == c::LPAREN) {
        cursor.consume_balanced_pair(c::LPAREN, c::RPAREN);
        head.type_start = Some(first);
    } else {
        tokens.push(first);
        absorb(&mut head, first);
    }
    while let Some(next) = cursor.peek(0) {
        if consume_attribute(cursor) {
            continue;
        }
        if matches!(
            next.kind,
            c::LPAREN | c::SEMI | c::EQ | c::LBRACKET | c::COMMA | c::LBRACE
        ) {
            break;
        }
        cursor.next();
        tokens.push(next);
        absorb(&mut head, next);
    }
    if let Some(name) = head.name {
        let prefix = &tokens[..tokens.iter().position(|t| t.start == name.start).unwrap()];
        head.specifier = type_specifier(prefix);
        head.pointer = prefix.iter().any(|t| t.kind == c::STAR);
    }
    head
}

/// Call-shaped declaration modifiers in the C grammar. Unknown identifiers
/// remain type/declarator tokens; their spelling is not an attribute heuristic.
fn attribute_keyword(token: Tok) -> bool {
    matches!(
        token.kind,
        c::KW___ATTRIBUTE | c::KW___ATTRIBUTE__ | c::KW___DECLSPEC | c::KW_ALIGNAS | c::KW__ALIGNAS
    )
}

fn attribute_end(cursor: &TokenCursor<'_>, start: usize) -> Option<usize> {
    if !attribute_keyword(cursor.peek(start)?) || cursor.peek(start + 1)?.kind != c::LPAREN {
        return None;
    }
    // Jump the whole `(...)` group; `range.end` is the absolute index just past
    // the matching `)`, which we return as a peek offset from the cursor.
    let range = cursor.peek_balanced(start + 1)?;
    Some(range.end - cursor.mark())
}

fn consume_attribute(cursor: &mut TokenCursor<'_>) -> bool {
    let Some(end) = attribute_end(cursor, 0) else {
        return false;
    };
    for _ in 0..end {
        cursor.next();
    }
    true
}

fn type_qualifier(token: Tok) -> bool {
    matches!(
        token.kind,
        c::KW_CONST
            | c::KW_CONSTEXPR
            | c::KW_VOLATILE
            | c::KW_RESTRICT
            | c::KW___RESTRICT__
            | c::KW___EXTENSION__
            | c::KW__NONNULL
            | c::KW__ATOMIC
            | c::KW__NORETURN
            | c::KW_NORETURN
    )
}

fn storage_or_qualifier(token: Tok) -> bool {
    type_qualifier(token)
        || matches!(
            token.kind,
            c::KW_STATIC
                | c::KW_EXTERN
                | c::KW_TYPEDEF
                | c::KW_INLINE
                | c::KW___INLINE
                | c::KW___INLINE__
                | c::KW___FORCEINLINE
                | c::KW___THREAD
                | c::KW_THREAD_LOCAL
                | c::KW_AUTO
                | c::KW_REGISTER
        )
}

fn size_specifier(token: Tok) -> bool {
    matches!(
        token.kind,
        c::KW_SIGNED | c::KW_UNSIGNED | c::KW_LONG | c::KW_SHORT
    )
}

/// Mirror C's primitive/type-identifier and sized-type-specifier productions.
/// An unknown annotation is a type identifier: `__init int` selects `__init`,
/// while `__init unsigned long` is one sized specifier. Keep the source span
/// (including internal qualifiers/whitespace) exactly as the oracle does.
fn type_specifier(tokens: &[Tok]) -> Option<(Tok, Tok)> {
    let start = tokens.iter().position(|t| !storage_or_qualifier(*t))?;
    let first = tokens[start];
    if matches!(first.kind, c::KW_STRUCT | c::KW_UNION | c::KW_ENUM) {
        return tokens
            .get(start + 1)
            .filter(|t| t.kind == c::IDENTIFIER)
            .map(|last| (first, *last));
    }
    if first.kind != c::IDENTIFIER && !size_specifier(first) {
        return None;
    }
    let mut last = first;
    let mut has_type = first.kind == c::IDENTIFIER;
    let mut has_size = size_specifier(first);
    for token in &tokens[start + 1..] {
        if size_specifier(*token) {
            has_size = true;
            last = *token;
        } else if has_size && !has_type && type_qualifier(*token) {
            last = *token;
        } else if has_size && !has_type && token.kind == c::IDENTIFIER {
            has_type = true;
            last = *token;
        } else {
            break;
        }
    }
    Some((first, last))
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
    let (mut name, mut star) = (None, false);
    while let Some(token) = cursor.peek(0) {
        if consume_attribute(cursor) {
            continue;
        }
        match token.kind {
            c::LBRACE | c::LPAREN | c::LBRACKET => skip_group(cursor, token),
            c::SEMI => {
                cursor.next();
                break;
            }
            c::STAR => {
                cursor.next();
                star = true;
            }
            c::IDENTIFIER => {
                cursor.next();
                name = Some(token);
            }
            _ => {
                cursor.next();
            }
        }
    }
    (name, star)
}

/// Consumes the balanced group opened by `open` whole. Falls back to consuming
/// the opener and scanning to its close family when no [`BlockMap`] is available
/// or the group is unmatched, keeping the walk total either way.
fn skip_group(cursor: &mut TokenCursor<'_>, open: Tok) {
    if cursor.skip_balanced() {
        return;
    }
    let close = match open.kind {
        c::LBRACE => c::RBRACE,
        c::LPAREN => c::RPAREN,
        _ => c::RBRACKET,
    };
    cursor.next(); // opener
    consume_until(cursor, close, None);
}

/// The oracle ignores array field declarators, including aggregate arrays.
fn read_field_declarator(cursor: &mut TokenCursor<'_>) -> (Option<Tok>, bool) {
    let (mut name, mut star) = (None, false);
    while let Some(token) = cursor.peek(0) {
        if consume_attribute(cursor) {
            continue;
        }
        match token.kind {
            c::SEMI | c::RBRACE => {
                cursor.consume_if(c::SEMI);
                break;
            }
            c::LBRACKET => {
                // The oracle ignores array declarators, so drop the name.
                name = None;
                skip_group(cursor, token);
            }
            c::LPAREN => skip_group(cursor, token),
            c::IDENTIFIER => {
                name = Some(token);
                cursor.next();
            }
            c::STAR => {
                star = true;
                cursor.next();
            }
            _ => {
                cursor.next();
            }
        }
    }
    (name, star)
}

/// Consume through a top-level separator, leaving an optional owner close for
/// the caller. C statements continue across newlines.
fn consume_until(
    cursor: &mut TokenCursor<'_>,
    separator: TokenKind,
    owner_close: Option<TokenKind>,
) {
    cursor.consume_balanced_until(BalancedUntil {
        delimiters: super::linear::DelimiterKinds {
            semicolon: separator,
            ..c::DELIMITERS
        },
        owner_close,
        logical_line: false,
        can_terminate_line: |_| false,
    });
}

/// Consumes to the `}` matching an already-consumed `{`.
fn skip_block(cursor: &mut TokenCursor<'_>) {
    consume_until(cursor, c::RBRACE, None);
}

/// Consumes through the next top-level `;`, skipping balanced groups so a `;`
/// inside an initializer does not end the statement early.
fn skip_to_semicolon(cursor: &mut TokenCursor<'_>) {
    consume_until(cursor, c::SEMI, None);
}

/// Advances through the next top-level `,` or up to `}` (left unconsumed),
/// skipping any balanced initializer expression after `=`.
fn skip_to_enum_separator(cursor: &mut TokenCursor<'_>) {
    consume_until(cursor, c::COMMA, Some(c::RBRACE));
}

fn filename_hash(path: &str) -> String {
    let mut hash: u32 = 5381;
    for byte in path.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    format!("{hash:08x}")
}

/// C preprocessor lexer: the engine-level home for directive boundaries, the
/// treetags analog of tree-sitter's external scanner. At the beginning of a line
/// it claims a whole `#...` directive — including backslash-newline continuations,
/// via [`directive_end`] — and emits a self-delimited token run: the `#keyword`
/// introducer, the macro name (`#define`) or header path (`#include`), and a
/// virtual `;` terminator. The body is consumed opaquely, so macro bodies never
/// reach the tag hook or the delimiter map, and a directive (which has no real
/// `;`) still terminates like a statement.
#[derive(Default)]
pub(crate) struct CPreprocLexer;

impl ExternalLexer for CPreprocLexer {
    fn scan(
        &mut self,
        input: ExternalLexInput<'_>,
        out: &mut ExternalLexemeSink<'_>,
    ) -> ExternalScan {
        if !input.beginning_of_line {
            return ExternalScan::NoMatch;
        }
        let source = input.source;
        let bytes = source.as_bytes();
        let base = input.offset as usize;
        let is_space = |b: u8| matches!(b, b' ' | b'\t');
        let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';

        // Optional indentation, then the directive's `#`.
        let mut i = base;
        while bytes.get(i).is_some_and(|&b| is_space(b)) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'#') {
            return ExternalScan::NoMatch;
        }
        let hash = i;
        let end = directive_end(source, hash);

        // Introducer: `#`, interior whitespace, and the keyword as one token, so
        // `#  define` reaches the hook verbatim (matching the previous lexer).
        i += 1;
        while bytes.get(i).is_some_and(|&b| is_space(b)) {
            i += 1;
        }
        let keyword_start = i;
        while bytes.get(i).is_some_and(|&b| is_ident(b)) {
            i += 1;
        }
        let keyword = &source[keyword_start..i];
        out.emit(Tok {
            kind: c::LITERAL,
            flags: TokenFlags(0),
            start: hash as u32,
            end: i as u32,
            row: input.row,
        });

        // The name/path operand follows on the same physical line.
        while i < end && bytes.get(i).is_some_and(|&b| is_space(b)) {
            i += 1;
        }
        if keyword == "define" {
            if bytes
                .get(i)
                .is_some_and(|&b| b.is_ascii_alphabetic() || b == b'_')
            {
                let name_start = i;
                while i < end && bytes.get(i).is_some_and(|&b| is_ident(b)) {
                    i += 1;
                }
                out.emit(Tok {
                    kind: c::IDENTIFIER,
                    flags: TokenFlags(0),
                    start: name_start as u32,
                    end: i as u32,
                    row: input.row,
                });
            }
        } else if keyword == "include" {
            let mut path_end = end;
            while path_end > i
                && bytes
                    .get(path_end - 1)
                    .is_some_and(|&b| matches!(b, b' ' | b'\t' | b'\r'))
            {
                path_end -= 1;
            }
            if path_end > i {
                out.emit(Tok {
                    kind: c::LITERAL,
                    flags: TokenFlags(0),
                    start: i as u32,
                    end: path_end as u32,
                    row: input.row,
                });
            }
        }

        // A virtual `;` at the logical-line end lets every `;`-based path
        // (top-level dispatch, member fragmenting) treat a directive as a
        // complete statement without knowing anything about directives.
        out.emit_virtual(c::SEMI, end as u32, input.row);
        ExternalScan::Consumed(
            NonZeroU32::new((end - base) as u32).expect("directive_end lies past the offset"),
        )
    }
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
    let stream = c::scan::<CPreprocLexer>(source)?;
    let input = HookInput {
        source,
        path,
        options,
        line_starts: &stream.line_starts,
    };
    let blocks = BlockMap::new(&stream.tokens, c::DELIMITERS);
    let mut tags = Vec::new();
    let mut emitter = TagEmitter::new(input, &mut tags, &blocks);
    let mut hooks = CHooks {
        sequence: 1,
        hash: filename_hash(path),
        references_end: 0,
    };
    hooks.generate(
        input,
        TokenCursor::with_blocks(source, &stream.tokens, &blocks),
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
    fn comma_separated_variables_match_oracle() {
        for source in [
            "int x, y;\n",
            "unsigned long x, y, z;\nint following;\n",
            "const int x,\n y,\n z;\n",
            "int x, y, function(void);\n",
            "int x, y, array[3];\n",
        ] {
            assert_matches_oracle(source);
        }
        let tags = actual("int x, y;\n");
        assert_eq!(
            tags.iter().map(|tag| tag.name.as_str()).collect::<Vec<_>>(),
            ["x", "y"]
        );
    }

    #[test]
    fn balanced_declaration_boundaries_match_oracle() {
        assert_matches_oracle(
            "enum Values { A = sizeof((int[]){1, 2}), B = (3 + (4)), C };\n\
             typedef struct { int member; } Alias;\n\
             void body(void) { if (1) { int hidden; } }\n\
             int following;\n",
        );
    }

    #[test]
    fn statement_boundaries_skip_nested_groups() {
        for source in [
            "int values[] = {1, 2}; following",
            "int computed = ({ int local = 1; local; }); following",
            "int value = call(\n1, array[index]);\nfollowing",
        ] {
            let stream = c::scan::<CPreprocLexer>(source).unwrap();
            let mut cursor = TokenCursor::new(source, &stream.tokens);
            skip_to_semicolon(&mut cursor);
            let next = cursor.next().unwrap();
            assert_eq!(cursor.text(next), "following");
            assert!(cursor.next().is_none());
        }
    }

    #[test]
    fn define_inside_struct_body_matches_oracle() {
        // A `#define` inside an aggregate body is a macro scoped to the struct,
        // not a member, and (having no `;`) must not swallow the member after it.
        assert_matches_oracle(
            "struct Config {\n\
             \tint width;\n\
             #define CONFIG_FLAG_A (1 << 0)\n\
             \tint height;\n\
             };\n",
        );
    }

    #[test]
    fn include_inside_struct_body_is_kept() {
        // The oracle omits an `#include` inside an aggregate body; native keeps it
        // (a legitimate header tag), still consuming the whole line so the member
        // that follows is not swallowed.
        let tags = actual(
            "struct Config {\n\
             \tint width;\n\
             #include <bits.h>\n\
             \tint height;\n\
             };\n",
        );
        let kind_of = |name: &str| {
            tags.iter()
                .find(|t| t.name == name)
                .and_then(|t| t.kind.as_deref())
        };
        assert_eq!(kind_of("bits.h"), Some("h"));
        assert_eq!(kind_of("width"), Some("m"));
        assert_eq!(kind_of("height"), Some("m"));
    }

    #[test]
    fn pointer_struct_typerefs_match_oracle() {
        // A pointer struct *variable* carries no `*` on its typeref (`struct:Foo`),
        // while a pointer struct *member* does (`struct:Node *`). The oracle only
        // ever attaches the star to members, so variables must drop it.
        assert_matches_oracle(
            "struct Foo *p = init;\n\
             struct Bar b;\n\
             struct Node { struct Node *next; int v; } node_var;\n\
             typedef struct Baz Baz;\n\
             struct Baz *bp = init;\n",
        );
    }

    #[test]
    fn declaration_type_specifiers_match_oracle() {
        for source in [
            "static inline int plain(void) {}\n",
            "const\nint multiline(void) {}\n",
            "static\ninline\nint storage(void) {}\n",
            "__init int prefix(void) {}\nint __init suffix(void) {}\n",
            "static __attribute_const__ unsigned long sized(void) {}\n",
            "unsigned const int qualified(void) {}\n",
            "const struct Object aggregate(void) {}\n",
            "union Value union_result(void) {}\n",
            "static const int value;\nvolatile unsigned long count;\n",
            "const struct Object object;\n",
            "struct Fields { const int value; volatile unsigned long count; int *pointer; };\n",
            "struct Fields { volatile unsigned long csr __attribute__((aligned(16))); };\n",
            "struct Fields { __init int value; unsigned const int sized; };\n",
        ] {
            assert_matches_oracle(source);
        }
    }

    #[test]
    fn attribute_call_boundaries_match_oracle() {
        for declaration in [
            "__attribute__((unused)) int prefix(void) {}",
            "static void __attribute__ ((unused)) middle(void) {}",
            "int trailing(void) __attribute__((unused)) {}",
            "struct __attribute__((packed)) Packed { int value; };",
            "struct Outer { struct __attribute__((packed)) { int inner; } field; int after; };",
            "__attribute__((aligned(sizeof(union Alignment)))) int refs(enum Mode m) {}",
            "__declspec(dllexport) int exported(void) {}",
            "_Alignas(16) int aligned;",
            "int __attribute__((aligned(16))) aligned;",
            "int aligned __attribute__((aligned(16)));",
            "struct Fields { int __attribute__((aligned(16))) aligned; int after; };",
            "struct Fields { struct Object field __attribute__((aligned(16))); int after; };",
            "__attribute__((unused)) int refs(union Value *v, enum Mode m, struct Object *p) {}",
            "void refs(union Value *, enum Mode, struct Object *) __attribute__((noreturn));",
        ] {
            let source = format!("{declaration}\n#define AFTER_ATTRIBUTE 1\nint following(void) {{}}\nint after_variable;\n");
            assert_matches_oracle(&source);
        }
    }

    #[test]
    fn normalized_member_typeref_respects_disabled_field() {
        let kinds = TagKindConfig::from_string("", C_KIND_DEFAULTS, C_KIND_OPTIONALS);
        let config = crate::config::Config::for_test();
        let mut options = super::super::linear::HookOptions::from_config(&kinds, &config);
        options.typeref = false;
        let tags = generate(
            "struct Fields { const int *pointer; };",
            "source.c",
            options,
        )
        .unwrap();
        let member = tags.iter().find(|tag| tag.name == "pointer").unwrap();
        assert!(member
            .extension_fields
            .as_ref()
            .unwrap()
            .get("typeref")
            .is_none());
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
    fn indented_directives_match_oracle() {
        // Nested-conditional headers indent directives as `#  define` / `#  include`
        // with whitespace between `#` and the keyword. The lexer captures this as a
        // single directive token, so the hook must match past the interior spaces.
        assert_matches_oracle(
            "#ifdef CONFIG_FOO\n\
             #  define GUARDED_MACRO\t\t(1 << 0)\n\
             #  define GUARDED_VALUE 42\n\
             #  include <nested/header.h>\n\
             #endif\n",
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
    fn nested_aggregates_match_oracle() {
        assert_matches_oracle(
            "struct Outer {\n\
               int before;\n\
               struct { int anonymous_field; struct { int deep; } inner; };\n\
               union { int integer; struct { int promoted; }; } value;\n\
               struct Named { int named_field; union Choice { int choice; } selected; } named;\n\
               struct Named *pointer;\n\
               union Choice *union_pointer;\n\
               int after;\n\
             };\n\
             typedef struct { struct { int child; } nested; } Alias;\n\
             int following(void) { return 0; }\n",
        );
    }

    #[test]
    fn nested_aggregate_declarators_match_oracle() {
        assert_matches_oracle(
            "struct Outer {\n\
                struct\n Named { int member; } *pointer;\n\
                struct { int anonymous; } *anonymous_pointer;\n\
                union NamedUnion { int member; } *union_pointer;\n\
                union { int member; } *anonymous_union_pointer;\n\
                struct { int element; } array[4];\n\
                union { int element; } union_array[4];\n\
                int following;\n };\n",
        );
    }

    #[test]
    fn nested_aggregate_scope_options_match_oracle() {
        let source = "struct Outer { union Inner { struct { int field; } child; } value; };";
        let kinds = TagKindConfig::from_string("", C_KIND_DEFAULTS, C_KIND_OPTIONALS);
        for args in [
            vec!["treetags", "--fields=-s"],
            vec!["treetags", "--fields=-s", "--extras=+q"],
        ] {
            let config = crate::config::Config::parse_from(args);
            let expected = crate::parser::cpp::generate(
                &mut tree_sitter::Parser::new(),
                source.as_bytes(),
                "source.c",
                &kinds,
                &config,
            )
            .unwrap();
            let actual = generate(
                source,
                "source.c",
                super::super::linear::HookOptions::from_config(&kinds, &config),
            )
            .unwrap();
            assert_eq!(sorted(actual), sorted(expected));
        }
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

    fn preproc_texts(source: &str) -> Vec<(TokenKind, &str)> {
        c::scan::<CPreprocLexer>(source)
            .unwrap()
            .tokens
            .iter()
            .map(|t| (t.kind, &source[t.start as usize..t.end as usize]))
            .collect()
    }

    #[test]
    fn preproc_lexer_self_delimits_define_and_keeps_body_opaque() {
        let source = "#define ADD(a, b) ((a) + (b))\nint x;\n";
        let stream = c::scan::<CPreprocLexer>(source).unwrap();
        let view = preproc_texts(source);
        // Introducer, macro name, a zero-width virtual `;`, then the next stmt.
        assert_eq!(view[0], (c::LITERAL, "#define"));
        assert_eq!(view[1], (c::IDENTIFIER, "ADD"));
        assert_eq!(stream.tokens[2].kind, c::SEMI);
        assert_ne!(stream.tokens[2].flags.0 & TokenFlags::VIRTUAL, 0);
        assert_eq!(view[2].1, "");
        // The replacement list never becomes tokens.
        assert!(!view
            .iter()
            .any(|(_, text)| matches!(*text, "(" | "+" | "a")));
        assert_eq!(
            view[3..].iter().map(|(_, t)| *t).collect::<Vec<_>>(),
            ["int", "x", ";"]
        );
    }

    #[test]
    fn preproc_lexer_emits_include_path_as_one_token() {
        assert_eq!(
            preproc_texts("#include <stdlib.h>\n"),
            [
                (c::LITERAL, "#include"),
                (c::LITERAL, "<stdlib.h>"),
                (c::SEMI, ""),
            ]
        );
    }

    #[test]
    fn preproc_lexer_spans_indentation_and_line_continuations() {
        // `#  define` interior whitespace, and a body spliced across two lines,
        // collapse to introducer + name + virtual `;` before the next statement.
        let texts: Vec<&str> = preproc_texts("#  define A \\\n  1 + \\\n  2\nint y;\n")
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert_eq!(texts, ["#  define", "A", "", "int", "y", ";"]);
    }
}
