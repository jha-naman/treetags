#![allow(dead_code)]
use super::{
    generated::go,
    go_syntax::{
        can_terminate_line, consume_declaration, consume_type, GoTypeContext, GoTypeUntil,
    },
    linear::{HookInput, TagHooks, Tok, TokenCursor},
    tag_emitter::{TagEmitter, TextValue},
};

#[derive(Default)]
pub(crate) struct GoHooks {
    package: String,
}

impl TagHooks for GoHooks {
    fn generate(
        &mut self,
        input: HookInput<'_>,
        mut cursor: TokenCursor<'_>,
        output: &mut TagEmitter<'_>,
    ) {
        let mut braces = 0u32;
        let mut functions: Vec<(u32, usize, u32)> = Vec::new();
        while let Some(token) = cursor.next() {
            match token.kind {
                go::KW_PACKAGE => {
                    if let Some(name) = cursor.next() {
                        self.package = cursor.text(name).to_string();
                        output.tag("p", name, (token, name)).emit();
                    }
                }
                go::KW_FUNC => {
                    if let Some(function) = self.function(&input, &mut cursor, output, token) {
                        let Some(body_open) = function.body_open else {
                            continue;
                        };
                        braces += 1;
                        if let Some(handle) = function.handle {
                            functions.push((braces, handle, token.row))
                        }
                        let _ = body_open;
                    }
                }
                go::KW_IMPORT => {
                    self.import(&mut cursor, output, token);
                }
                go::KW_VAR => {
                    self.values(&mut cursor, output, token, "v");
                }
                go::KW_CONST => {
                    self.values(&mut cursor, output, token, "c");
                }
                go::KW_TYPE => {
                    self.types(&mut cursor, output, token);
                }
                go::PUNCT_7B => braces += 1,
                go::PUNCT_7D => {
                    if let Some((depth, handle, start)) = functions.last().copied() {
                        if depth == braces {
                            output.set_end(handle, start, token.row);
                            functions.pop();
                        }
                    }
                    braces = braces.saturating_sub(1)
                }
                _ => {}
            }
        }
    }
}

impl GoHooks {
    fn import(&self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, start: Tok) {
        let grouped = cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_28);
        if grouped {
            cursor.next();
        }
        loop {
            let Some(first) = cursor.peek(0) else { return };
            if grouped && first.kind == go::PUNCT_29 {
                cursor.next();
                return;
            }
            if !grouped && first.row > start.row {
                return;
            }
            let Some(alias) = cursor.next() else { return };
            let Some(path) = cursor.peek(0) else { return };
            if alias.kind == go::IDENTIFIER && path.kind == go::LITERAL {
                let path = cursor.next().unwrap();
                out.tag("P", alias, (alias, path))
                    .scope("package", cursor.text(path).trim_matches('"').to_string())
                    .emit();
            } else {
                while cursor
                    .peek(0)
                    .is_some_and(|t| t.row == alias.row && t.kind != go::PUNCT_3B)
                {
                    cursor.next();
                }
            }
            if !grouped {
                return;
            }
        }
    }

    fn values(
        &self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        start: Tok,
        kind: &'static str,
    ) {
        let grouped = cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_28);
        if grouped {
            cursor.next();
        }
        loop {
            let Some(first) = cursor.peek(0) else { return };
            if grouped && first.kind == go::PUNCT_29 {
                cursor.next();
                return;
            }
            if !grouped && first.row > start.row {
                return;
            }
            if first.kind != go::IDENTIFIER {
                cursor.next();
                if !grouped {
                    return;
                }
                continue;
            }
            let mut names = Vec::new();
            names.push(cursor.next().unwrap());
            while cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_2C) {
                cursor.next();
                if let Some(name) = cursor.next() {
                    if name.kind == go::IDENTIFIER {
                        names.push(name)
                    }
                }
            }
            let last_name = *names.last().unwrap();
            let owner_close = grouped.then_some(go::PUNCT_29);
            let at_spec_boundary = cursor.peek(0).is_none_or(|next| {
                next.kind == go::PUNCT_3B
                    || owner_close == Some(next.kind)
                    || (next.row > last_name.row && can_terminate_line(last_name.kind))
            });
            let mut type_span = None;
            if !at_spec_boundary && cursor.peek(0).is_some_and(|t| t.kind != go::PUNCT_3D) {
                type_span = consume_type(
                    cursor,
                    GoTypeUntil {
                        context: GoTypeContext::Type,
                        owner_close,
                        logical_line: true,
                        comma: false,
                        equals: true,
                        struct_tag: false,
                    },
                );
            }

            if cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_3D) {
                cursor.next();
                consume_declaration(cursor, owner_close, true);
            } else if cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_3B) {
                cursor.next();
            }
            for name in names {
                let mut b = out.tag(kind, name, (name, type_span.map_or(name, |span| span.last)));
                if !self.package.is_empty() {
                    b = b.scope("package", self.package.clone())
                }
                if kind == "v" {
                    if let Some(span) = type_span {
                        b = b.typeref(TextValue::Span(span.first.start, span.last.end))
                    }
                }
                b.emit();
            }
            if !grouped {
                return;
            }
        }
    }

    fn types(&self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, start: Tok) {
        let grouped = cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_28);
        if grouped {
            cursor.next();
        }
        loop {
            let Some(name) = cursor.peek(0) else { return };
            if grouped && name.kind == go::PUNCT_29 {
                cursor.next();
                return;
            }
            if !grouped && name.row > start.row {
                return;
            }
            if name.kind != go::IDENTIFIER {
                cursor.next();
                if !grouped {
                    return;
                }
                continue;
            }
            let name = cursor.next().unwrap();
            if cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_5B) {
                let open = cursor.peek(0).unwrap();
                if let Some(close) = cursor.skip_balanced("[", "]") {
                    let mut generic = out.tag("t", name, (name, close));
                    if !self.package.is_empty() {
                        generic = generic.scope("package", self.package.clone())
                    }
                    generic
                        .typeref(TextValue::Span(open.start, close.end))
                        .emit();
                }
            }
            if cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_3D) {
                let row = name.row;
                while cursor
                    .peek(0)
                    .is_some_and(|t| t.row == row && !matches!(t.kind, go::PUNCT_3B | go::PUNCT_29))
                {
                    cursor.next();
                }
                if !grouped {
                    return;
                } else {
                    continue;
                }
            }
            let Some(ty) = cursor.next() else { return };
            match ty.kind {
                go::KW_STRUCT | go::KW_INTERFACE => {
                    let is_struct = ty.kind == go::KW_STRUCT;
                    let Some(open) = cursor.next() else { return };
                    if open.kind != go::PUNCT_7B {
                        continue;
                    }
                    let mut b = out.tag(if is_struct { "s" } else { "i" }, name, (name, open));
                    if !self.package.is_empty() {
                        b = b.scope("package", self.package.clone())
                    }
                    let handle = b.emit();
                    let close = self.members(cursor, out, name, is_struct, open);
                    if let Some(handle) = handle {
                        out.set_end(handle, name.row, close.row);
                    }
                }
                _ => {
                    let row = ty.row;
                    let mut last = ty;
                    while cursor.peek(0).is_some_and(|t| {
                        t.row == row && !matches!(t.kind, go::PUNCT_3B | go::PUNCT_29)
                    }) {
                        last = cursor.next().unwrap()
                    }
                    let mut b = out.tag("t", name, (name, last));
                    if !self.package.is_empty() {
                        b = b.scope("package", self.package.clone())
                    }
                    b.typeref(TextValue::Span(ty.start, last.end)).emit();
                }
            }
            if !grouped {
                return;
            }
        }
    }

    fn members(
        &self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        owner: Tok,
        is_struct: bool,
        open: Tok,
    ) -> Tok {
        let mut depth = 1;
        let mut close = open;
        while let Some(first) = cursor.next() {
            match first.kind {
                go::PUNCT_7B => {
                    depth += 1;
                    continue;
                }
                go::PUNCT_7D => {
                    depth -= 1;
                    if depth == 0 {
                        return first;
                    }
                    continue;
                }
                _ => {}
            }
            if depth != 1 || first.kind != go::IDENTIFIER {
                continue;
            }
            let row = first.row;
            if !is_struct && cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_28) {
                let params_open = cursor.next().unwrap();
                let Some(params_close) = cursor.skip_balanced_after_open(params_open, "(", ")")
                else {
                    continue;
                };
                let mut result_first = None;
                let mut result_last = None;
                while cursor.peek(0).is_some_and(|t| t.row == row) {
                    let t = cursor.next().unwrap();
                    result_first.get_or_insert(t);
                    result_last = Some(t)
                }
                let mut b = out
                    .tag("n", first, (first, result_last.unwrap_or(params_close)))
                    .scope(
                        "interface",
                        format!("{}.{}", self.package, cursor.text(owner)),
                    );
                if let (Some(a), Some(z)) = (result_first, result_last) {
                    b = b.typeref(TextValue::Span(a.start, z.end))
                }
                b.emit();
            } else if is_struct {
                let mut names = vec![first];
                while cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_2C) {
                    cursor.next();
                    if let Some(n) = cursor.next() {
                        names.push(n)
                    }
                }
                let type_first = cursor
                    .peek(0)
                    .filter(|t| t.row == row)
                    .and_then(|_| cursor.next());
                let mut type_last = type_first;
                while cursor.peek(0).is_some_and(|t| t.row == row) {
                    type_last = cursor.next()
                }
                for name in names {
                    let mut b = out
                        .tag("m", name, (name, type_last.unwrap_or(name)))
                        .scope("struct", format!("{}.{}", self.package, cursor.text(owner)));
                    if let (Some(a), Some(z)) = (type_first, type_last) {
                        if a.kind != go::KW_STRUCT {
                            b = b.typeref(TextValue::Span(a.start, z.end))
                        }
                    }
                    b.emit();
                }
            }
            close = first
        }
        close
    }

    fn function(
        &self,
        input: &HookInput<'_>,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        start: Tok,
    ) -> Option<FunctionOutcome> {
        let mut receiver_scope = None;
        if cursor.consume_if(go::PUNCT_28).is_some() {
            // Optional receiver name: an identifier directly followed by another
            // identifier or `*` (the start of the receiver type).
            if cursor.peek(0).map(|t| t.kind) == Some(go::IDENTIFIER)
                && matches!(
                    cursor.peek(1).map(|t| t.kind),
                    Some(go::IDENTIFIER) | Some(go::PUNCT_2A)
                )
            {
                cursor.next();
            }
            let span = consume_type(
                cursor,
                GoTypeUntil {
                    context: GoTypeContext::Type,
                    owner_close: Some(go::PUNCT_29),
                    logical_line: false,
                    comma: false,
                    equals: false,
                    struct_tag: false,
                },
            );
            cursor.consume_if(go::PUNCT_29);
            receiver_scope = span.map(|s| {
                cursor
                    .span_text(s.first, s.last)
                    .trim_start_matches('*')
                    .to_string()
            });
        }
        let name = cursor.next()?;
        if name.kind != go::IDENTIFIER {
            return None;
        }
        if cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_5B) {
            cursor.skip_balanced("[", "]")?;
        }
        let params_open = cursor.peek(0)?;
        if params_open.kind != go::PUNCT_28 {
            return None;
        }
        let params_close = cursor.skip_balanced("(", ")")?;
        let mut result = None;
        let mut body_open = None;
        let next = cursor.peek(0);
        if next.is_some_and(|t| t.kind == go::PUNCT_7B) {
            body_open = cursor.next();
        } else if next.is_some_and(|t| t.kind == go::PUNCT_3B) {
            cursor.next();
        } else if next
            .is_some_and(|t| t.row == params_close.row || !can_terminate_line(params_close.kind))
        {
            result = consume_type(
                cursor,
                GoTypeUntil {
                    context: GoTypeContext::FunctionResult,
                    owner_close: Some(go::PUNCT_7B),
                    logical_line: true,
                    comma: false,
                    equals: false,
                    struct_tag: false,
                },
            );
            if cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_7B) {
                body_open = cursor.next();
            } else if cursor.peek(0).is_some_and(|t| t.kind == go::PUNCT_3B) {
                cursor.next();
            }
        }
        let declaration_end = body_open
            .or_else(|| result.map(|span| span.last))
            .unwrap_or(params_close);
        let mut builder = out.tag("f", name, (start, declaration_end));
        if let Some(receiver_scope) = receiver_scope {
            builder = builder.scope("struct", format!("{}.{}", self.package, receiver_scope))
        } else if !self.package.is_empty() {
            builder = builder.scope("package", self.package.clone())
        }
        builder = builder.signature(TextValue::Span(params_open.start, params_close.end));
        if let Some(result) = result {
            builder = builder.typeref(TextValue::Span(result.first.start, result.last.end))
        }
        let _ = input;
        Some(FunctionOutcome {
            body_open,
            handle: builder.emit(),
        })
    }
}

struct FunctionOutcome {
    body_open: Option<Tok>,
    handle: Option<usize>,
}

pub(crate) fn generate(
    source: &str,
    path: &str,
    mut options: super::linear::HookOptions<'_>,
) -> Result<Vec<crate::tag::Tag>, String> {
    // The existing Go backend intentionally never emits `kind:` or `file:` as
    // extension fields; it retains the shorthand kind column instead.
    options.kind = false;
    options.file = false;
    let stream = go::scan::<super::linear::NoExternalLexer>(source)?;
    let input = HookInput {
        source,
        path,
        options,
        line_starts: &stream.line_starts,
    };
    let mut tags = Vec::new();
    let mut emitter = TagEmitter::new(input, &mut tags);
    GoHooks::default().generate(
        input,
        TokenCursor::new(source, &stream.tokens),
        &mut emitter,
    );
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{
        go::{KIND_DEFAULTS, KIND_OPTIONALS},
        TagKindConfig,
    };
    use clap::Parser as _;
    fn options<'a>(kinds: &'a TagKindConfig) -> super::super::linear::HookOptions<'a> {
        super::super::linear::HookOptions {
            tag_config: kinds,
            line: true,
            kind: false,
            file: false,
            scope: true,
            signature: true,
            typeref: true,
            access: false,
            end: true,
            qualified: false,
        }
    }
    #[test]
    fn forward_hook_emits_package_and_function() {
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let tags = generate(
            "package p\nfunc f(x int) string {\n\ta := 1\n}\n",
            "x.go",
            options(&kinds),
        )
        .unwrap();
        assert_eq!(
            tags.iter()
                .map(|t| (&*t.name, t.kind.as_deref()))
                .collect::<Vec<_>>(),
            [("p", Some("p")), ("f", Some("f"))]
        );
        assert_eq!(
            tags[1].extension_fields.as_ref().unwrap().get("signature"),
            Some("(x int)")
        );
        assert_eq!(
            tags[1].extension_fields.as_ref().unwrap().get("typeref"),
            Some("typename:string")
        );
        assert_eq!(
            tags[1].extension_fields.as_ref().unwrap().get("end"),
            Some("4")
        );
    }
    #[test]
    fn cursor_has_no_rewind_and_balanced_consumption_is_forward() {
        let source = "(a [b]) c";
        let stream = go::scan::<super::super::linear::NoExternalLexer>(source).unwrap();
        let mut cursor = TokenCursor::new(source, &stream.tokens);
        let close = cursor.skip_balanced("(", ")").unwrap();
        assert_eq!(cursor.text(close), ")");
        let next = cursor.next().unwrap();
        assert_eq!(cursor.text(next), "c");
    }
    #[test]
    fn emitted_address_uses_shared_escaping() {
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let tags = generate("package p\nfunc f() { // /$\n}\n", "x.go", options(&kinds)).unwrap();
        let mut bytes = Vec::new();
        tags[1].write_into(&mut bytes);
        assert!(String::from_utf8(bytes).unwrap().contains("\\/\\$"));
    }

    #[test]
    fn basic_fixture_matches_tree_sitter_oracle_with_default_fields() {
        let source = include_str!("../../tests/test_cases/go/basic/input/source.go");
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "source.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "source.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn basic_fixture_matches_oracle_with_all_go_fields() {
        let source = include_str!("../../tests/test_cases/go/basic/input/source.go");
        let mut config = crate::config::Config::parse_from(["treetags"]);
        for field in ["line", "kind", "file", "signature", "access", "end"] {
            config.fields_config.enabled_fields.insert(field.into());
        }
        config.extras_config.qualified = true;
        let kinds = TagKindConfig::from_string("picsmtfv", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "source.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "source.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn modern_constructs_match_tree_sitter_oracle() {
        let source = r#"package corpus
import alias "example.com/x"
type Box[T any] struct {
    Value T
    Pair struct{ X int }
}
type Reader[T any] interface {
    Read([]byte) (int, error)
}
type (
    IDs = map[string]int
    Name string
)
var (
    first, second *Box[int]
)
const (
    one, two = 1, 2
)
func Make[T any](v T) *Box[T] {
    local, other := v, v
    if true { nested := local; _ = nested }
    return &Box[T]{Value: other}
}
var raw = `func Fake() {}`
// type Comment struct{}
"#;
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "corpus.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "corpus.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    #[test]
    fn receiver_scopes_match_tree_sitter_oracle() {
        let source = r#"package sync
type entry[K comparable, V any] struct {
    key K
}
type Point struct {
    x int
}
func (head *entry[K, V]) swap(next *entry[K, V]) *entry[K, V] { return head }
func (p Point) String() string { return "" }
func (p *Point) Move() {}
func (*Point) Reset() {}
func (Point) Zero() {}
func Free() {}
"#;
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "receivers.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "receivers.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
        let swap = actual.iter().find(|t| &*t.name == "swap").unwrap();
        assert_eq!(swap.kind.as_deref(), Some("f"));
        assert_eq!(
            swap.extension_fields.as_ref().unwrap().get("struct"),
            Some("sync.entry[K, V]")
        );
    }
    #[test]
    fn balanced_grouped_values_match_tree_sitter_oracle() {
        let source = r#"package p
const (
    A = (1 + iota)
    B
    C = fn(1, map[string]int{"x": 2})
    D
)
var (
    x = make([]int, f(2))
    y int
)
"#;
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "balanced.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "balanced.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    #[test]
    fn bodyless_functions_do_not_consume_following_declarations() {
        let source = "package p\nfunc A2e([]byte)\nfunc E2a([]byte)\nconst C = 1\n";
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "prototypes.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "prototypes.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert!(actual
            .iter()
            .filter(|tag| tag.kind.as_deref() == Some("f"))
            .all(|tag| tag
                .extension_fields
                .as_ref()
                .is_none_or(|fields| fields.get("end").is_none())));
    }
    #[test]
    fn short_var_behavior_matches_oracle() {
        let source = "package p\nfunc f(){\n a:=1\n}\n";
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "x.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "x.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    #[test]
    fn malformed_declarations_are_total() {
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        for source in [
            "package p\nfunc cut(",
            "package p\nvar (\n x int\n y = `unterminated",
            "package p\ntype S struct {\n Field []byte",
            "package p\n/* unterminated",
        ] {
            let result = generate(
                source,
                "broken.go",
                super::super::linear::HookOptions::from_config(&kinds, &config),
            );
            assert!(result.is_ok(), "failed on {source:?}: {result:?}");
        }
    }
}
