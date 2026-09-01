//! Forward-only token primitives shared by Go tag-hook declaration parsers.

#![allow(dead_code)] // Consumers are migrated independently from this foundation.

use super::{
    generated::go,
    linear::{
        BalancedPair, BalancedUntil, DelimiterKinds, SeparatedRange, Tok, TokenCursor, TokenRange,
    },
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

pub(crate) fn member_until() -> BalancedUntil {
    BalancedUntil {
        delimiters: DELIMITERS,
        owner_close: Some(go::PUNCT_7D),
        logical_line: true,
        can_terminate_line,
    }
}

pub(crate) fn consume_declaration(
    cursor: &mut TokenCursor<'_>,
    owner_close: Option<super::linear::TokenKind>,
    logical_line: bool,
) -> TokenRange {
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct GoDeclGroup {
    start_row: u32,
    grouped: bool,
    done: bool,
}

impl GoDeclGroup {
    pub fn new(cursor: &mut TokenCursor<'_>, start: Tok) -> Self {
        let grouped = cursor.consume_if(go::PUNCT_28).is_some();
        Self {
            start_row: start.row,
            grouped,
            done: false,
        }
    }

    pub fn owner_close(self) -> Option<super::linear::TokenKind> {
        self.grouped.then_some(go::PUNCT_29)
    }

    fn next_head(&mut self, cursor: &mut TokenCursor<'_>) -> Option<Tok> {
        if self.done {
            return None;
        }
        let next = cursor.peek(0)?;
        if self.grouped && next.kind == go::PUNCT_29 {
            cursor.next();
            self.done = true;
            return None;
        }
        if !self.grouped && next.row > self.start_row {
            self.done = true;
            return None;
        }
        Some(next)
    }

    fn finish_single(&mut self) {
        if !self.grouped {
            self.done = true;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GoImportSpec {
    pub alias: Tok,
    pub path: Tok,
}

pub(crate) fn next_import_spec(
    group: &mut GoDeclGroup,
    cursor: &mut TokenCursor<'_>,
) -> Option<GoImportSpec> {
    loop {
        group.next_head(cursor)?;
        let alias = cursor.next()?;
        let path = cursor.peek(0)?;
        if alias.kind == go::IDENTIFIER && path.kind == go::LITERAL {
            let path = cursor.next().expect("peeked import path");
            group.finish_single();
            return Some(GoImportSpec { alias, path });
        }
        while cursor
            .peek(0)
            .is_some_and(|token| token.row == alias.row && token.kind != go::PUNCT_3B)
        {
            cursor.next();
        }
        if !group.grouped {
            group.done = true;
            return None;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GoValueSpec {
    pub names: SeparatedRange,
    pub ty: Option<GoTypeSpan>,
}

pub(crate) fn next_value_spec(
    group: &mut GoDeclGroup,
    cursor: &mut TokenCursor<'_>,
) -> Option<GoValueSpec> {
    loop {
        let first = group.next_head(cursor)?;
        if first.kind != go::IDENTIFIER {
            cursor.next();
            group.finish_single();
            if group.done {
                return None;
            }
            continue;
        }
        let names_start = cursor.mark();
        let mut last_name = cursor.next().expect("peeked value name");
        while cursor
            .peek(0)
            .is_some_and(|token| token.kind == go::PUNCT_2C)
        {
            cursor.next();
            if let Some(name) = cursor.next() {
                if name.kind == go::IDENTIFIER {
                    last_name = name;
                }
            }
        }
        let names = SeparatedRange {
            range: TokenRange {
                start: names_start,
                end: cursor.mark(),
            },
            item: go::IDENTIFIER,
        };
        let owner_close = group.owner_close();
        let at_spec_boundary = cursor.peek(0).is_none_or(|next| {
            next.kind == go::PUNCT_3B
                || owner_close == Some(next.kind)
                || (next.row > last_name.row && can_terminate_line(last_name.kind))
        });
        let ty = if !at_spec_boundary
            && cursor
                .peek(0)
                .is_some_and(|token| token.kind != go::PUNCT_3D)
        {
            consume_type(
                cursor,
                GoTypeUntil {
                    context: GoTypeContext::Type,
                    owner_close,
                    logical_line: true,
                    comma: false,
                    equals: true,
                    struct_tag: false,
                },
            )
        } else {
            None
        };
        if cursor.consume_if(go::PUNCT_3D).is_some() {
            consume_declaration(cursor, owner_close, true);
        } else {
            cursor.consume_if(go::PUNCT_3B);
        }
        group.finish_single();
        return Some(GoValueSpec { names, ty });
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GoTypeSpecRhs {
    Alias,
    Aggregate {
        keyword: Tok,
        open: Tok,
        is_struct: bool,
    },
    Type(Option<GoTypeSpan>),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GoTypeSpec {
    pub name: Tok,
    pub type_params: Option<BalancedPair>,
    pub rhs: GoTypeSpecRhs,
}

pub(crate) fn next_type_spec(
    group: &mut GoDeclGroup,
    cursor: &mut TokenCursor<'_>,
) -> Option<GoTypeSpec> {
    loop {
        let head = group.next_head(cursor)?;
        if head.kind != go::IDENTIFIER {
            cursor.next();
            group.finish_single();
            if group.done {
                return None;
            }
            continue;
        }
        let name = cursor.next().expect("peeked type name");
        let type_params = if cursor
            .peek(0)
            .is_some_and(|token| token.kind == go::PUNCT_5B)
            && looks_like_type_params(cursor)
        {
            cursor.consume_balanced_pair(go::PUNCT_5B, go::PUNCT_5D)
        } else {
            None
        };
        if cursor
            .peek(0)
            .is_some_and(|token| token.kind == go::PUNCT_3D)
        {
            let row = name.row;
            while cursor.peek(0).is_some_and(|token| {
                token.row == row && !matches!(token.kind, go::PUNCT_3B | go::PUNCT_29)
            }) {
                cursor.next();
            }
            group.finish_single();
            return Some(GoTypeSpec {
                name,
                type_params,
                rhs: GoTypeSpecRhs::Alias,
            });
        }
        let rhs = match cursor.peek(0).map(|token| token.kind) {
            Some(go::KW_STRUCT) | Some(go::KW_INTERFACE) => {
                let keyword = cursor.next().expect("peeked aggregate keyword");
                let Some(open) = cursor.next() else {
                    group.done = true;
                    return None;
                };
                if open.kind != go::PUNCT_7B {
                    if !group.grouped {
                        group.done = true;
                    }
                    continue;
                }
                GoTypeSpecRhs::Aggregate {
                    keyword,
                    open,
                    is_struct: keyword.kind == go::KW_STRUCT,
                }
            }
            None => {
                group.done = true;
                return None;
            }
            _ => GoTypeSpecRhs::Type(consume_type(
                cursor,
                GoTypeUntil {
                    context: GoTypeContext::Type,
                    owner_close: group.owner_close(),
                    logical_line: true,
                    comma: false,
                    equals: false,
                    struct_tag: false,
                },
            )),
        };
        group.finish_single();
        return Some(GoTypeSpec {
            name,
            type_params,
            rhs,
        });
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GoStructField {
    Embedded,
    Named {
        names: SeparatedRange,
        ty: Option<GoTypeSpan>,
    },
}

pub(crate) fn next_struct_field(cursor: &mut TokenCursor<'_>) -> Option<GoStructField> {
    skip_semicolons(cursor);
    if cursor
        .peek(0)
        .is_none_or(|token| token.kind == go::PUNCT_7D)
    {
        return None;
    }
    if field_shape(cursor) == FieldShape::Embedded {
        consume_type(cursor, field_until());
        consume_field_tag(cursor);
        return Some(GoStructField::Embedded);
    }
    let start = cursor.mark();
    while cursor.consume_if(go::IDENTIFIER).is_some() {
        if cursor.consume_if(go::PUNCT_2C).is_none() {
            break;
        }
    }
    let names = SeparatedRange {
        range: TokenRange {
            start,
            end: cursor.mark(),
        },
        item: go::IDENTIFIER,
    };
    let ty = consume_type(cursor, field_until());
    consume_field_tag(cursor);
    Some(GoStructField::Named { names, ty })
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum GoInterfaceMember {
    Constraint,
    Method {
        name: Tok,
        params_open: Tok,
        params_close: Tok,
        result: Option<GoTypeSpan>,
    },
}

pub(crate) fn next_interface_member(cursor: &mut TokenCursor<'_>) -> Option<GoInterfaceMember> {
    skip_semicolons(cursor);
    if cursor
        .peek(0)
        .is_none_or(|token| token.kind == go::PUNCT_7D)
    {
        return None;
    }
    let is_method = cursor.peek(0).map(|token| token.kind) == Some(go::IDENTIFIER)
        && cursor.peek(1).map(|token| token.kind) == Some(go::PUNCT_28);
    if !is_method {
        consume_declaration(cursor, Some(go::PUNCT_7D), true);
        return Some(GoInterfaceMember::Constraint);
    }
    let name = cursor.next().expect("peeked method name");
    let Some(params) = cursor.consume_balanced_pair(go::PUNCT_28, go::PUNCT_29) else {
        return None;
    };
    let params_open = params.open;
    let params_close = params.close;
    let has_result = cursor.peek(0).is_some_and(|token| {
        !matches!(token.kind, go::PUNCT_7D | go::PUNCT_3B)
            && !(token.row > params_close.row && can_terminate_line(params_close.kind))
    });
    let result = has_result
        .then(|| {
            consume_type(
                cursor,
                GoTypeUntil {
                    context: GoTypeContext::FunctionResult,
                    owner_close: Some(go::PUNCT_7D),
                    logical_line: true,
                    comma: false,
                    equals: false,
                    struct_tag: false,
                },
            )
        })
        .flatten();
    Some(GoInterfaceMember::Method {
        name,
        params_open,
        params_close,
        result,
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GoFunctionDecl {
    pub name: Tok,
    pub receiver: Option<GoTypeSpan>,
    pub params_open: Tok,
    pub params_close: Tok,
    pub result: Option<GoTypeSpan>,
    pub body_open: Option<Tok>,
}

pub(crate) fn parse_function(cursor: &mut TokenCursor<'_>) -> Option<GoFunctionDecl> {
    let receiver = if cursor.consume_if(go::PUNCT_28).is_some() {
        if cursor.peek(0).map(|token| token.kind) == Some(go::IDENTIFIER)
            && matches!(
                cursor.peek(1).map(|token| token.kind),
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
        span
    } else {
        None
    };
    let name = cursor.next()?;
    if name.kind != go::IDENTIFIER {
        return None;
    }
    if cursor
        .peek(0)
        .is_some_and(|token| token.kind == go::PUNCT_5B)
    {
        cursor.consume_balanced_pair(go::PUNCT_5B, go::PUNCT_5D)?;
    }
    let params = cursor.consume_balanced_pair(go::PUNCT_28, go::PUNCT_29)?;
    let params_open = params.open;
    let params_close = params.close;
    let mut result = None;
    let mut body_open = None;
    let next = cursor.peek(0);
    if next.is_some_and(|token| token.kind == go::PUNCT_7B) {
        body_open = cursor.next();
    } else if next.is_some_and(|token| token.kind == go::PUNCT_3B) {
        cursor.next();
    } else if next.is_some_and(|token| {
        token.row == params_close.row || !can_terminate_line(params_close.kind)
    }) {
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
        if cursor
            .peek(0)
            .is_some_and(|token| token.kind == go::PUNCT_7B)
        {
            body_open = cursor.next();
        } else {
            cursor.consume_if(go::PUNCT_3B);
        }
    }
    Some(GoFunctionDecl {
        name,
        receiver,
        params_open,
        params_close,
        result,
        body_open,
    })
}

fn looks_like_type_params(cursor: &TokenCursor<'_>) -> bool {
    debug_assert_eq!(cursor.peek(0).map(|token| token.kind), Some(go::PUNCT_5B));
    let mut depth = 0u32;
    let mut index = 0;
    loop {
        let Some(token) = cursor.peek(index) else {
            return false;
        };
        match token.kind {
            go::PUNCT_5B => depth += 1,
            go::PUNCT_5D => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            go::PUNCT_2C if depth == 1 => return true,
            _ => {}
        }
        index += 1;
    }
    cursor.peek(1).map(|token| token.kind) == Some(go::IDENTIFIER)
        && matches!(
            cursor.peek(2).map(|token| token.kind),
            Some(
                go::IDENTIFIER
                    | go::PUNCT_7E
                    | go::KW_INTERFACE
                    | go::PUNCT_2A
                    | go::PUNCT_5B
                    | go::KW_CHAN
                    | go::KW_FUNC
                    | go::KW_MAP
                    | go::PUNCT_3C_2D
                    | go::PUNCT_28
            )
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FieldShape {
    Named,
    Embedded,
}

fn field_until() -> GoTypeUntil {
    GoTypeUntil {
        context: GoTypeContext::Type,
        owner_close: Some(go::PUNCT_7D),
        logical_line: true,
        comma: false,
        equals: false,
        struct_tag: true,
    }
}

fn field_shape(cursor: &TokenCursor<'_>) -> FieldShape {
    let Some(first) = cursor.peek(0) else {
        return FieldShape::Embedded;
    };
    if first.kind != go::IDENTIFIER {
        return FieldShape::Embedded;
    }
    let Some(second) = cursor.peek(1) else {
        return FieldShape::Embedded;
    };
    if second.row > first.row && can_terminate_line(first.kind) {
        return FieldShape::Embedded;
    }
    match second.kind {
        go::PUNCT_2C => FieldShape::Named,
        go::PUNCT_2E => FieldShape::Embedded,
        go::PUNCT_5B => {
            if array_after_bracket(cursor) {
                FieldShape::Named
            } else {
                FieldShape::Embedded
            }
        }
        kind if starts_type(kind) => FieldShape::Named,
        _ => FieldShape::Embedded,
    }
}

fn consume_field_tag(cursor: &mut TokenCursor<'_>) {
    if cursor
        .peek(0)
        .is_some_and(|token| token.kind == go::LITERAL)
    {
        cursor.next();
    }
}

fn skip_semicolons(cursor: &mut TokenCursor<'_>) {
    while cursor.consume_if(go::PUNCT_3B).is_some() {}
}

fn starts_type(kind: super::linear::TokenKind) -> bool {
    matches!(
        kind,
        go::IDENTIFIER
            | go::PUNCT_2A
            | go::PUNCT_5B
            | go::PUNCT_28
            | go::PUNCT_3C_2D
            | go::KW_MAP
            | go::KW_CHAN
            | go::KW_FUNC
            | go::KW_INTERFACE
            | go::KW_STRUCT
    )
}

fn array_after_bracket(cursor: &TokenCursor<'_>) -> bool {
    debug_assert_eq!(cursor.peek(1).map(|token| token.kind), Some(go::PUNCT_5B));
    let mut depth = 0u32;
    let mut index = 1;
    let close = loop {
        let Some(token) = cursor.peek(index) else {
            return false;
        };
        match token.kind {
            go::PUNCT_5B => depth += 1,
            go::PUNCT_5D => {
                depth -= 1;
                if depth == 0 {
                    break token;
                }
            }
            _ => {}
        }
        index += 1;
    };
    let Some(after) = cursor.peek(index + 1) else {
        return false;
    };
    if after.row > close.row && can_terminate_line(close.kind) {
        return false;
    }
    starts_type(after.kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::linear::NoExternalLexer;

    fn range_text(cursor: &TokenCursor<'_>, range: TokenRange) -> String {
        let mut view = cursor.view(range).expect("syntax-produced range");
        let Some(first) = view.next() else {
            return String::new();
        };
        let mut last = first;
        while let Some(token) = view.next() {
            last = token;
        }
        cursor.span_text(first, last).to_string()
    }

    fn declaration(
        source: &str,
        owner: Option<super::super::linear::TokenKind>,
    ) -> (String, String) {
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        let mut cursor = TokenCursor::new(source, &stream.tokens);
        let span = consume_declaration(&mut cursor, owner, true);
        let text = range_text(&cursor, span);
        let rest = cursor
            .peek(0)
            .map(|t| cursor.text(t))
            .unwrap_or("")
            .to_string();
        (text, rest)
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
            let (text, rest) = declaration(source, Some(go::PUNCT_29));
            assert_eq!(text, source.lines().next().unwrap());
            assert_eq!(rest, ")");
        }
    }

    #[test]
    fn multiline_nested_declaration_continues_on_closing_delimiter_row() {
        let source = "A = call(\n  1) + other.field\nB";
        let (text, rest) = declaration(source, None);
        assert_eq!(text, "A = call(\n  1) + other.field");
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
            let (text, rest) = declaration(source, None);
            assert_eq!(text, source.rsplit_once('\n').unwrap().0, "{source}");
            assert!(matches!(rest.as_str(), "B" | "C"), "{source}: {rest}");
        }

        for source in ["A = literal\nB", "A = value\nB", "A = call()\nB"] {
            let (_, rest) = declaration(source, None);
            assert_eq!(rest, "B", "{source}");
        }
    }

    #[test]
    fn declaration_boundaries_leave_owner_and_next_line_unconsumed() {
        let (text, rest) = declaration("A = f(1); B", Some(go::PUNCT_29));
        assert_eq!(text, "A = f(1)");
        assert_eq!(rest, "B");

        let (text, rest) = declaration("A = f(1)\nB", None);
        assert_eq!(text, "A = f(1)");
        assert_eq!(rest, "B");
    }

    #[test]
    fn bodyless_signature_does_not_consume_the_next_declaration() {
        let source = "func A2e([]byte) (int, error)\nfunc E2a([]byte)";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        let mut cursor = TokenCursor::new(source, &stream.tokens);
        let span = consume_declaration(&mut cursor, None, true);
        assert_eq!(range_text(&cursor, span), "func A2e([]byte) (int, error)");
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
