//! Forward-only token primitives shared by Go tag-hook declaration parsers.

#![allow(dead_code)] // Consumers are migrated independently from this foundation.

use super::{
    generated::go,
    linear::{BalancedSpan, BalancedUntil, DelimiterKinds, Tok, TokenCursor},
};

pub(crate) const DELIMITERS: DelimiterKinds = DelimiterKinds {
    paren_open: go::PUNCT_28,
    paren_close: go::PUNCT_29,
    bracket_open: go::PUNCT_5B,
    bracket_close: go::PUNCT_5D,
    brace_open: go::PUNCT_7B,
    brace_close: go::PUNCT_7D,
    semicolon: go::PUNCT_3B,
};

pub(crate) fn consume_declaration(
    cursor: &mut TokenCursor<'_>,
    owner_close: Option<super::linear::TokenKind>,
    logical_line: bool,
) -> BalancedSpan {
    cursor.consume_balanced_until(BalancedUntil {
        delimiters: DELIMITERS,
        owner_close,
        logical_line,
        can_terminate_line,
    })
}

/// Go's semicolon-insertion eligibility for the preceding significant token.
/// Scanner extras are absent from the token stream, so this can be applied
/// directly at a row transition.
pub(crate) fn can_terminate_line(kind: super::linear::TokenKind) -> bool {
    matches!(
        kind,
        go::IDENTIFIER
            | go::LITERAL
            | go::KW_BREAK
            | go::KW_CONTINUE
            | go::KW_FALSE
            | go::KW_FALLTHROUGH
            | go::KW_IOTA
            | go::KW_MAKE
            | go::KW_NEW
            | go::KW_NIL
            | go::KW_RETURN
            | go::KW_TRUE
            | go::PUNCT_2B_2B
            | go::PUNCT_2D_2D
            | go::PUNCT_29
            | go::PUNCT_5D
            | go::PUNCT_7D
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GoTypeCategory {
    Named,
    Pointer,
    Slice,
    Array,
    Map,
    Channel,
    Function,
    Interface,
    Struct,
    ParameterList,
    Parenthesized,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GoTypeContext {
    /// A leading `(` is a parenthesized type.
    Type,
    /// A leading `(` is a function result parameter list.
    FunctionResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GoTypeBoundary {
    Comma(Tok),
    Equals(Tok),
    Semicolon(Tok),
    RowTransition,
    OwnerClose(Tok),
    StructTag(Tok),
    Eof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GoTypeSpan {
    pub first: Tok,
    pub last: Tok,
    pub category: GoTypeCategory,
    /// The token that stopped parsing remains unconsumed.
    pub boundary: GoTypeBoundary,
}

impl GoTypeSpan {
    pub fn byte_range(self) -> (u32, u32) {
        (self.first.start, self.last.end)
    }

    /// Category information only. The eventual oracle-compatible policy stays
    /// in Go hooks because eligibility differs by declaration context.
    pub fn is_direct_named_family(self) -> bool {
        matches!(
            self.category,
            GoTypeCategory::Named
                | GoTypeCategory::Pointer
                | GoTypeCategory::Slice
                | GoTypeCategory::Map
                | GoTypeCategory::Channel
                | GoTypeCategory::Interface
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GoTypeUntil {
    pub context: GoTypeContext,
    pub owner_close: Option<super::linear::TokenKind>,
    pub logical_line: bool,
    pub comma: bool,
    pub equals: bool,
    pub struct_tag: bool,
}

/// Consumes a complete Go type-like span using explicit delimiter depths.
///
/// All boundary tokens are reported but left unconsumed. This deliberately
/// returns syntax category rather than deciding whether a
/// `typeref` field should be emitted. It is total on malformed input: unmatched
/// delimiters are consumed and EOF returns the maximal available span.
pub(crate) fn consume_type(cursor: &mut TokenCursor<'_>, until: GoTypeUntil) -> Option<GoTypeSpan> {
    let first = cursor.peek(0)?;
    let category = classify(cursor, until.context);
    let mut last = None;
    let mut parens = 0u32;
    let mut brackets = 0u32;
    let mut braces = 0u32;

    loop {
        let Some(next) = cursor.peek(0) else {
            return Some(finish(
                first,
                last.unwrap_or(first),
                category,
                GoTypeBoundary::Eof,
            ));
        };
        let top = parens == 0 && brackets == 0 && braces == 0;
        let boundary = if top && until.owner_close == Some(next.kind) {
            Some(GoTypeBoundary::OwnerClose(next))
        } else if top && until.comma && next.kind == go::PUNCT_2C {
            Some(GoTypeBoundary::Comma(next))
        } else if top && until.equals && next.kind == go::PUNCT_3D {
            Some(GoTypeBoundary::Equals(next))
        } else if top && next.kind == go::PUNCT_3B {
            Some(GoTypeBoundary::Semicolon(next))
        } else if top && until.struct_tag && next.kind == go::LITERAL && last.is_some() {
            Some(GoTypeBoundary::StructTag(next))
        } else if top
            && until.logical_line
            && last.is_some_and(|token| next.row > token.row && can_terminate_line(token.kind))
        {
            Some(GoTypeBoundary::RowTransition)
        } else {
            None
        };
        if let Some(boundary) = boundary {
            return Some(finish(first, last.unwrap_or(first), category, boundary));
        }

        let token = cursor.next().expect("peeked token");
        last = Some(token);
        match token.kind {
            go::PUNCT_28 => parens += 1,
            go::PUNCT_29 => parens = parens.saturating_sub(1),
            go::PUNCT_5B => brackets += 1,
            go::PUNCT_5D => brackets = brackets.saturating_sub(1),
            go::PUNCT_7B => braces += 1,
            go::PUNCT_7D => braces = braces.saturating_sub(1),
            _ => {}
        }
    }
}

fn finish(first: Tok, last: Tok, category: GoTypeCategory, boundary: GoTypeBoundary) -> GoTypeSpan {
    GoTypeSpan {
        first,
        last,
        category,
        boundary,
    }
}

fn classify(cursor: &TokenCursor<'_>, context: GoTypeContext) -> GoTypeCategory {
    let Some(first) = cursor.peek(0) else {
        return GoTypeCategory::Unknown;
    };
    match first.kind {
        go::IDENTIFIER => GoTypeCategory::Named,
        go::PUNCT_2A => GoTypeCategory::Pointer,
        go::KW_MAP => GoTypeCategory::Map,
        go::KW_CHAN | go::PUNCT_3C_2D => GoTypeCategory::Channel,
        go::KW_FUNC => GoTypeCategory::Function,
        go::KW_INTERFACE => GoTypeCategory::Interface,
        go::KW_STRUCT => GoTypeCategory::Struct,
        go::PUNCT_28 => match context {
            GoTypeContext::Type => GoTypeCategory::Parenthesized,
            GoTypeContext::FunctionResult => GoTypeCategory::ParameterList,
        },
        go::PUNCT_5B => {
            if cursor.peek(1).is_some_and(|t| t.kind == go::PUNCT_5D) {
                GoTypeCategory::Slice
            } else {
                GoTypeCategory::Array
            }
        }
        _ => GoTypeCategory::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::linear::{BalancedBoundary, NoExternalLexer};

    fn declaration(
        source: &str,
        owner: Option<super::super::linear::TokenKind>,
    ) -> (String, BalancedBoundary, String) {
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        let mut cursor = TokenCursor::new(source, &stream.tokens);
        let span = consume_declaration(&mut cursor, owner, true);
        let text = match (span.first, span.last) {
            (Some(a), Some(z)) => cursor.span_text(a, z).to_string(),
            _ => String::new(),
        };
        let rest = cursor
            .peek(0)
            .map(|t| cursor.text(t))
            .unwrap_or("")
            .to_string();
        (text, span.boundary, rest)
    }

    fn ty_in(
        source: &str,
        owner: Option<super::super::linear::TokenKind>,
        struct_tag: bool,
        context: GoTypeContext,
    ) -> (GoTypeSpan, String, String) {
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        let mut cursor = TokenCursor::new(source, &stream.tokens);
        let span = consume_type(
            &mut cursor,
            GoTypeUntil {
                context,
                owner_close: owner,
                logical_line: true,
                comma: true,
                equals: true,
                struct_tag,
            },
        )
        .unwrap();
        let text = cursor.span_text(span.first, span.last).to_string();
        let rest = cursor
            .peek(0)
            .map(|t| cursor.text(t))
            .unwrap_or("")
            .to_string();
        (span, text, rest)
    }

    fn ty(
        source: &str,
        owner: Option<super::super::linear::TokenKind>,
        struct_tag: bool,
    ) -> (GoTypeSpan, String, String) {
        ty_in(source, owner, struct_tag, GoTypeContext::Type)
    }

    #[test]
    fn grouped_nested_expressions_stop_only_at_owner_close() {
        for source in [
            "First = fn(1)\n)",
            "First = map[string]int{\"x\": fn(1)}\n)",
            "First = []int{call(1, 2)}\n)",
        ] {
            let (text, boundary, rest) = declaration(source, Some(go::PUNCT_29));
            assert_eq!(text, source.lines().next().unwrap());
            assert!(matches!(boundary, BalancedBoundary::OwnerClose(_)));
            assert_eq!(rest, ")");
        }
    }

    #[test]
    fn multiline_nested_declaration_continues_on_closing_delimiter_row() {
        let source = "A = call(\n  1) + other.field\nB";
        let (text, boundary, rest) = declaration(source, None);
        assert_eq!(text, "A = call(\n  1) + other.field");
        assert_eq!(boundary, BalancedBoundary::RowTransition);
        assert_eq!(rest, "B");
    }

    #[test]
    fn go_semicolon_rules_control_multiline_declaration_boundaries() {
        for source in [
            "A =\n call()\nB",
            "A = 1 +\n 2\nB",
            "A,\n B = 1, 2\nC",
            "A = pkg.\n Value\nB",
            "A = <-\n values\nB",
        ] {
            let (text, boundary, rest) = declaration(source, None);
            assert_eq!(text, source.rsplit_once('\n').unwrap().0, "{source}");
            assert_eq!(boundary, BalancedBoundary::RowTransition, "{source}");
            assert!(matches!(rest.as_str(), "B" | "C"), "{source}: {rest}");
        }

        for source in ["A = literal\nB", "A = value\nB", "A = call()\nB"] {
            let (_, boundary, rest) = declaration(source, None);
            assert_eq!(boundary, BalancedBoundary::RowTransition, "{source}");
            assert_eq!(rest, "B", "{source}");
        }
    }

    #[test]
    fn declaration_boundaries_leave_owner_and_next_line_unconsumed() {
        let (text, boundary, rest) = declaration("A = f(1); B", Some(go::PUNCT_29));
        assert_eq!(text, "A = f(1)");
        assert!(matches!(boundary, BalancedBoundary::Semicolon(_)));
        assert_eq!(rest, "B");

        let (text, boundary, rest) = declaration("A = f(1)\nB", None);
        assert_eq!(text, "A = f(1)");
        assert_eq!(boundary, BalancedBoundary::RowTransition);
        assert_eq!(rest, "B");
    }

    #[test]
    fn bodyless_signature_does_not_consume_the_next_declaration() {
        let source = "func A2e([]byte) (int, error)\nfunc E2a([]byte)";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        let mut cursor = TokenCursor::new(source, &stream.tokens);
        let span = consume_declaration(&mut cursor, None, true);
        assert_eq!(
            cursor.span_text(span.first.unwrap(), span.last.unwrap()),
            "func A2e([]byte) (int, error)"
        );
        assert_eq!(span.boundary, BalancedBoundary::RowTransition);
        assert_eq!(cursor.text(cursor.peek(0).unwrap()), "func");
    }

    #[test]
    fn type_categories_and_balanced_spans() {
        let cases = [
            ("pkg.Box[K, V]\nnext", GoTypeCategory::Named),
            ("*pkg.Box[K]\nnext", GoTypeCategory::Pointer),
            ("[]string\nnext", GoTypeCategory::Slice),
            ("[N + f(1)]byte\nnext", GoTypeCategory::Array),
            ("map[string][]*T\nnext", GoTypeCategory::Map),
            ("<-chan map[K]V\nnext", GoTypeCategory::Channel),
            ("func(int, ...T) (U, error)\nnext", GoTypeCategory::Function),
            ("interface{ M() T }\nnext", GoTypeCategory::Interface),
            ("struct{ X map[K]V }\nnext", GoTypeCategory::Struct),
            ("(int)\nnext", GoTypeCategory::Parenthesized),
        ];
        for (source, category) in cases {
            let (span, _, rest) = ty(source, None, false);
            assert_eq!(span.category, category, "{source}");
            assert_eq!(rest, "next", "{source}");
        }
    }

    #[test]
    fn parenthesized_type_and_result_parameter_list_are_contextual() {
        let (parenthesized, text, rest) = ty("(int)\nnext", None, false);
        assert_eq!(parenthesized.category, GoTypeCategory::Parenthesized);
        assert_eq!(text, "(int)");
        assert_eq!(rest, "next");

        let (results, text, rest) = ty_in(
            "(int, error)\nnext",
            None,
            false,
            GoTypeContext::FunctionResult,
        );
        assert_eq!(results.category, GoTypeCategory::ParameterList);
        assert_eq!(text, "(int, error)");
        assert_eq!(rest, "next");
    }

    #[test]
    fn multiline_type_continues_with_suffix_on_closing_delimiter_row() {
        let (span, text, rest) = ty("map[\n K]V\nnext", None, false);
        assert_eq!(span.category, GoTypeCategory::Map);
        assert_eq!(text, "map[\n K]V");
        assert_eq!(rest, "next");

        let (span, text, rest) = ty("pkg.Box[\n K].Member\nnext", None, false);
        assert_eq!(span.category, GoTypeCategory::Named);
        assert_eq!(text, "pkg.Box[\n K].Member");
        assert_eq!(rest, "next");
    }

    #[test]
    fn go_semicolon_rules_control_multiline_type_boundaries() {
        for (source, expected) in [
            ("pkg.\n Type\nnext", "pkg.\n Type"),
            ("*\n pkg.Type\nnext", "*\n pkg.Type"),
            ("chan <-\n Value\nnext", "chan <-\n Value"),
        ] {
            let (_, text, rest) = ty(source, None, false);
            assert_eq!(text, expected, "{source}");
            assert_eq!(rest, "next", "{source}");
        }

        // `]` is semicolon-eligible, so a suffix on the next row is not part
        // of this (malformed) type declaration.
        let (_, text, rest) = ty("map[K]\nV", None, false);
        assert_eq!(text, "map[K]");
        assert_eq!(rest, "V");
    }

    #[test]
    fn type_stops_before_field_tag_and_compact_owner_close() {
        let (span, text, rest) = ty("[]string `json:\",omitempty\"` }", Some(go::PUNCT_7D), true);
        assert_eq!(span.category, GoTypeCategory::Slice);
        assert_eq!(text, "[]string");
        assert_eq!(rest, "`json:\",omitempty\"`");

        let (_, text, rest) = ty("map[string]struct{ X int }}", Some(go::PUNCT_7D), true);
        assert_eq!(text, "map[string]struct{ X int }");
        assert_eq!(rest, "}");
    }

    #[test]
    fn compact_struct_type_leaves_owner_close_for_member_parser() {
        let source = "map[string]struct{ X []int }} type Carry uint";
        let (span, text, rest) = ty(source, Some(go::PUNCT_7D), true);
        assert_eq!(span.category, GoTypeCategory::Map);
        assert_eq!(text, "map[string]struct{ X []int }");
        assert_eq!(rest, "}");
    }

    #[test]
    fn malformed_and_truncated_input_is_total_and_makes_progress() {
        for source in ["map[string", "func(int", "struct{ X []T", "[N]map[K]"] {
            let stream = go::scan::<NoExternalLexer>(source).unwrap();
            let mut cursor = TokenCursor::new(source, &stream.tokens);
            let before = cursor.mark();
            let span = consume_type(
                &mut cursor,
                GoTypeUntil {
                    context: GoTypeContext::Type,
                    owner_close: None,
                    logical_line: true,
                    comma: true,
                    equals: true,
                    struct_tag: true,
                },
            )
            .unwrap();
            assert!(cursor.mark() > before, "{source}");
            assert_eq!(span.boundary, GoTypeBoundary::Eof, "{source}");
        }
    }
}
