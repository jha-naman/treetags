#![allow(dead_code)]
use super::{
    generated::go,
    go_syntax::{
        member_until, next_import_spec, next_interface_member, next_struct_field, next_type_spec,
        next_value_spec, parse_function, GoDeclGroup, GoInterfaceMember, GoStructField,
        GoTypeSpecRhs,
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
        _input: HookInput<'_>,
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
                    if let Some(function) = self.function(&mut cursor, output, token) {
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
        let mut group = GoDeclGroup::new(cursor, start);
        while let Some(spec) = next_import_spec(&mut group, cursor) {
            out.tag("P", spec.alias, (spec.alias, spec.path))
                .scope(
                    "package",
                    cursor.text(spec.path).trim_matches('"').to_string(),
                )
                .emit();
        }
    }

    fn values(
        &self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        start: Tok,
        kind: &'static str,
    ) {
        let mut group = GoDeclGroup::new(cursor, start);
        while let Some(spec) = next_value_spec(&mut group, cursor) {
            let Some(names) = spec.names.items(cursor) else {
                continue;
            };
            for name in names {
                let mut b = out.tag(kind, name, (name, spec.ty.map_or(name, |span| span.last)));
                if !self.package.is_empty() {
                    b = b.scope("package", self.package.clone())
                }
                if kind == "v" {
                    if let Some(span) = spec.ty {
                        b = b.typeref(TextValue::Span(span.first.start, span.last.end))
                    }
                }
                b.emit();
            }
        }
    }

    fn types(&self, cursor: &mut TokenCursor<'_>, out: &mut TagEmitter<'_>, start: Tok) {
        let mut group = GoDeclGroup::new(cursor, start);
        while let Some(spec) = next_type_spec(&mut group, cursor) {
            let name = spec.name;
            if let Some(type_params) = spec.type_params {
                let mut generic = out.tag("t", name, (name, type_params.close));
                if !self.package.is_empty() {
                    generic = generic.scope("package", self.package.clone())
                }
                generic
                    .typeref(TextValue::Span(
                        type_params.open.start,
                        type_params.close.end,
                    ))
                    .emit();
            }
            match spec.rhs {
                GoTypeSpecRhs::Alias => {}
                GoTypeSpecRhs::Aggregate {
                    keyword: _,
                    open,
                    is_struct,
                } => {
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
                GoTypeSpecRhs::Type(span) => {
                    if spec.type_params.is_none() {
                        let mut b = out.tag("t", name, (name, span.map_or(name, |s| s.last)));
                        if !self.package.is_empty() {
                            b = b.scope("package", self.package.clone())
                        }
                        if let Some(s) = span {
                            let (a, z) = s.byte_range();
                            b = b.typeref(TextValue::Span(a, z));
                        }
                        b.emit();
                    }
                }
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
        loop {
            while cursor.consume_if(go::PUNCT_3B).is_some() {}
            if let Some(close) = cursor.consume_if(go::PUNCT_7D) {
                return close;
            }
            if cursor.peek(0).is_none() {
                return open;
            }

            let range = cursor.consume_balanced_until(member_until());
            if is_struct {
                let mut member = cursor.view(range).expect("syntax-produced range");
                let Some(field) = next_struct_field(&mut member) else {
                    continue;
                };
                if let GoStructField::Named { names, ty } = field {
                    let Some(names) = names.items(&member) else {
                        continue;
                    };
                    for name in names {
                        let mut b = out
                            .tag("m", name, (name, ty.map_or(name, |s| s.last)))
                            .scope("struct", format!("{}.{}", self.package, cursor.text(owner)));
                        if let Some(s) = ty {
                            if s.is_direct_named_family() {
                                let (a, z) = s.byte_range();
                                b = b.typeref(TextValue::Span(a, z));
                            }
                        }
                        b.emit();
                    }
                }
            } else {
                let mut member_cursor = cursor.view(range).expect("syntax-produced range");
                if let Some(member) = next_interface_member(&mut member_cursor) {
                    self.emit_interface_member(cursor, out, owner, member);
                }
            }
        }
    }

    fn emit_interface_member(
        &self,
        cursor: &TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        owner: Tok,
        member: GoInterfaceMember,
    ) {
        let GoInterfaceMember::Method {
            name,
            params_open: _,
            params_close,
            result,
        } = member
        else {
            return;
        };
        let mut b = out
            .tag("n", name, (name, result.map_or(params_close, |s| s.last)))
            .scope(
                "interface",
                format!("{}.{}", self.package, cursor.text(owner)),
            );
        if let Some(s) = result {
            let (a, z) = s.byte_range();
            b = b.typeref(TextValue::Span(a, z));
        }
        b.emit();
    }

    fn function(
        &self,
        cursor: &mut TokenCursor<'_>,
        out: &mut TagEmitter<'_>,
        start: Tok,
    ) -> Option<FunctionOutcome> {
        let function = parse_function(cursor)?;
        let receiver_scope = function.receiver.map(|span| {
            cursor
                .span_text(span.first, span.last)
                .trim_start_matches('*')
                .to_string()
        });
        let declaration_end = function
            .body_open
            .or_else(|| function.result.map(|span| span.last))
            .unwrap_or(function.params_close);
        let mut builder = out.tag("f", function.name, (start, declaration_end));
        if let Some(receiver_scope) = receiver_scope {
            builder = builder.scope("struct", format!("{}.{}", self.package, receiver_scope))
        } else if !self.package.is_empty() {
            builder = builder.scope("package", self.package.clone())
        }
        builder = builder.signature(TextValue::Span(
            function.params_open.start,
            function.params_close.end,
        ));
        if let Some(result) = function.result {
            builder = builder.typeref(TextValue::Span(result.first.start, result.last.end))
        }
        Some(FunctionOutcome {
            body_open: function.body_open,
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
    fn delimiter_aware_members_match_oracle() {
        let source = r#"package p
import "io"
type RegPtr struct{ name string }
type Carry uint
const (
    SetCarry Carry = iota
    AddCarry
)
type Node struct {
    io.Reader
    *Node
    Set[int]
    left, right *Node
    Pair struct{ X int }
    Grid [3][4]int
    Tag string `json:"tag"`
    fn func(int) error
    ch <-chan int
}
type Compact struct{ a, b int; c string }
type Constraint interface {
    ~int | ~string
    io.Reader
    Do(x int) (int, error)
}
"#;
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "members.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "members.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert!(actual
            .iter()
            .any(|t| &*t.name == "Carry" && t.kind.as_deref() == Some("t")));
        assert!(actual
            .iter()
            .any(|t| &*t.name == "SetCarry" && t.kind.as_deref() == Some("c")));
        assert!(!actual.iter().any(|t| &*t.name == "Reader"));
        assert!(!actual.iter().any(|t| &*t.name == "X"));
    }
    #[test]
    fn array_and_generic_type_definitions_match_oracle() {
        let source = r#"package p
type ActionID [HashSize]byte
type Grid [3][4]int
type Bytes []byte
type Ptr *int
type Fn func(int) error
type Set[T comparable] map[T]bool
type Box[T any] struct {
    Value T
}
type List[T any] []T
type (
    Grouped[T any] map[string]T
    Plain uint
)
"#;
        let config = crate::config::Config::parse_from(["treetags"]);
        let kinds = TagKindConfig::from_string("", KIND_DEFAULTS, KIND_OPTIONALS);
        let expected = crate::parser::go::oracle::generate(
            &mut tree_sitter::Parser::new(),
            source.as_bytes(),
            "types.go",
            &kinds,
            &config,
        )
        .unwrap();
        let actual = generate(
            source,
            "types.go",
            super::super::linear::HookOptions::from_config(&kinds, &config),
        )
        .unwrap();
        assert_eq!(actual, expected);
        let action = actual
            .iter()
            .filter(|t| &*t.name == "ActionID")
            .collect::<Vec<_>>();
        assert_eq!(action.len(), 1);
        assert_eq!(
            action[0].extension_fields.as_ref().unwrap().get("typeref"),
            Some("typename:[HashSize]byte")
        );
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
