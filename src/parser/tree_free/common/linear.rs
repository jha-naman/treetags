//! Tree-free runtime for generated builtin scanners.
#![allow(dead_code)] // API surface is intentionally ahead of the first consumer.

pub(crate) use super::scanner::{GeneratedLexeme, GeneratedLexicon};
use std::num::NonZeroU32;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TokenKind(pub u16);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TokenFlags(pub u16);
impl TokenFlags {
    pub const BOL: u16 = 1;
    pub const VIRTUAL: u16 = 2;
    pub const ERROR: u16 = 4;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Tok {
    pub kind: TokenKind,
    pub flags: TokenFlags,
    pub start: u32,
    pub end: u32,
    pub row: u32,
}

pub(crate) struct TokenStream {
    pub tokens: Vec<Tok>,
    pub line_starts: Vec<u32>,
}

#[derive(Clone, Copy)]
pub(crate) struct ExternalLexInput<'a> {
    pub source: &'a str,
    pub offset: u32,
    pub row: u32,
    pub column: u32,
    pub beginning_of_line: bool,
    pub previous_significant: Option<Tok>,
}
impl ExternalLexInput<'_> {
    pub fn remainder(&self) -> &str {
        &self.source[self.offset as usize..]
    }
    pub fn starts_with(&self, s: &str) -> bool {
        self.remainder().starts_with(s)
    }
    pub fn peek(&self) -> Option<char> {
        self.remainder().chars().next()
    }
}

pub(crate) struct ExternalLexemeSink<'a> {
    tokens: &'a mut Vec<Tok>,
    first: usize,
    consumed_start: u32,
    consumed_end: u32,
    invalid: bool,
}
impl<'a> ExternalLexemeSink<'a> {
    pub fn new(tokens: &'a mut Vec<Tok>, start: u32, end: u32) -> Self {
        let first = tokens.len();
        Self {
            tokens,
            first,
            consumed_start: start,
            consumed_end: end,
            invalid: false,
        }
    }
    pub fn emit(&mut self, tok: Tok) {
        let virtual_token = tok.flags.0 & TokenFlags::VIRTUAL != 0;
        if (!virtual_token
            && (tok.start < self.consumed_start
                || tok.end > self.consumed_end
                || tok.start > tok.end))
            || self.tokens.last().is_some_and(|last| last.end > tok.start)
        {
            self.invalid = true;
            return;
        }
        self.tokens.push(tok);
    }
    pub fn emit_virtual(&mut self, kind: TokenKind, at: u32, row: u32) {
        self.emit(Tok {
            kind,
            flags: TokenFlags(TokenFlags::VIRTUAL),
            start: at,
            end: at,
            row,
        });
    }
    pub fn is_valid(&self) -> bool {
        !self.invalid
    }
    pub fn validate_range(&mut self, start: u32, end: u32) -> bool {
        if self.tokens[self.first..]
            .iter()
            .any(|t| t.flags.0 & TokenFlags::VIRTUAL == 0 && (t.start < start || t.end > end))
        {
            self.invalid = true;
        }
        !self.invalid
    }
}

pub(crate) enum ExternalScan {
    NoMatch,
    Consumed(NonZeroU32),
}

/// Optional, statically-dispatched lexer seam. A scanner calls this before
/// extras and generated rules. `NoMatch` consumes nothing. `Consumed` covers
/// every non-virtual emitted span. `finish` may emit zero-width EOF tokens.
///
/// A no-op implementation is simply `#[derive(Default)] struct L; impl
/// ExternalLexer for L {}`. An indentation lexer can inspect `remainder()` at
/// BOL, return the bytes through the indentation prefix, emit multiple INDENT
/// or DEDENT virtual tokens, update its state across newlines, and flush any
/// remaining DEDENTs from `finish`.
pub(crate) trait ExternalLexer: Default {
    fn reset(&mut self) {}
    fn scan(
        &mut self,
        _input: ExternalLexInput<'_>,
        _output: &mut ExternalLexemeSink<'_>,
    ) -> ExternalScan {
        ExternalScan::NoMatch
    }
    fn finish(&mut self, _input: ExternalLexInput<'_>, _output: &mut ExternalLexemeSink<'_>) {}
}
#[derive(Default)]
pub(crate) struct NoExternalLexer;
impl ExternalLexer for NoExternalLexer {}

pub(crate) struct TokenCursor<'a> {
    source: &'a str,
    tokens: &'a [Tok],
    /// Optional whole-stream delimiter map for structural lookahead. Absent for
    /// cursors built without one (e.g. unit tests); present on the production
    /// path so [`peek_balanced`](Self::peek_balanced) can jump balanced groups.
    blocks: Option<&'a BlockMap>,
    start: usize,
    at: usize,
    end: usize,
}

/// A half-open range in a generated token stream. Ranges are cheap, borrowed
/// indirectly through their cursor, and can be revisited without rewinding the
/// parent cursor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TokenRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DelimiterKinds {
    pub paren_open: TokenKind,
    pub paren_close: TokenKind,
    pub bracket_open: TokenKind,
    pub bracket_close: TokenKind,
    pub brace_open: TokenKind,
    pub brace_close: TokenKind,
    pub semicolon: TokenKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BalancedPair {
    pub open: Tok,
    pub close: Tok,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BalancedUntil {
    pub delimiters: DelimiterKinds,
    /// A close delimiter owned by the caller. It is reported but not consumed.
    pub owner_close: Option<TokenKind>,
    /// End on a row transition while all inner delimiters are balanced.
    pub logical_line: bool,
    /// Language-specific test for whether the previous significant token may
    /// terminate a logical line. A function pointer keeps the hot path
    /// monomorphic and allocation-free.
    pub can_terminate_line: fn(TokenKind) -> bool,
}

/// How [`TokenCursor::members`] frames one `{ ... }` body: which token closes
/// the body, which separators to skip between members, and how to bound a single
/// member fragment (usually a [`BalancedUntil`] stopping at the body's own
/// `close` and/or a top-level separator).
#[derive(Clone, Copy, Debug)]
pub(crate) struct MemberRule {
    pub close: TokenKind,
    pub skip: &'static [TokenKind],
    pub fragment: BalancedUntil,
}

#[derive(Default)]
struct DelimiterDepth {
    parens: u32,
    brackets: u32,
    braces: u32,
}

impl DelimiterDepth {
    fn is_top_level(&self) -> bool {
        self.parens == 0 && self.brackets == 0 && self.braces == 0
    }

    fn observe(&mut self, kind: TokenKind, d: DelimiterKinds) {
        if kind == d.paren_open {
            self.parens += 1;
        } else if kind == d.paren_close {
            self.parens = self.parens.saturating_sub(1);
        } else if kind == d.bracket_open {
            self.brackets += 1;
        } else if kind == d.bracket_close {
            self.brackets = self.brackets.saturating_sub(1);
        } else if kind == d.brace_open {
            self.braces += 1;
        } else if kind == d.brace_close {
            self.braces = self.braces.saturating_sub(1);
        }
    }
}

/// Precomputed matching-delimiter map for a whole token stream. For every open
/// paren/bracket/brace it records the row of the matching close, so a hook can
/// set a tag's `end:` line from the token that opens a body without tracking
/// nesting depth itself. Built once, O(n), and total on malformed input:
/// unmatched or mismatched delimiters simply have no recorded close.
pub(crate) struct BlockMap {
    /// Keyed by the open token's byte start (unique per token) → close row.
    close_rows: std::collections::HashMap<u32, u32>,
    /// Keyed by the open token's index → the matching close token's index.
    /// Lets a cursor jump a balanced group by lookahead (see
    /// [`TokenCursor::peek_balanced`]) instead of hand-rolling a depth counter.
    close_index: std::collections::HashMap<usize, usize>,
}

impl BlockMap {
    pub fn new(tokens: &[Tok], d: DelimiterKinds) -> Self {
        let family = |kind: TokenKind| -> Option<(u8, bool)> {
            if kind == d.paren_open {
                Some((0, true))
            } else if kind == d.paren_close {
                Some((0, false))
            } else if kind == d.bracket_open {
                Some((1, true))
            } else if kind == d.bracket_close {
                Some((1, false))
            } else if kind == d.brace_open {
                Some((2, true))
            } else if kind == d.brace_close {
                Some((2, false))
            } else {
                None
            }
        };
        let mut close_rows = std::collections::HashMap::new();
        let mut close_index = std::collections::HashMap::new();
        let mut stack: Vec<(usize, u32, u8)> = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            match family(token.kind) {
                Some((f, true)) => stack.push((index, token.start, f)),
                Some((f, false)) => {
                    if stack.last().is_some_and(|&(_, _, open)| open == f) {
                        let (open_index, open_start, _) = stack.pop().expect("checked non-empty");
                        close_rows.insert(open_start, token.row);
                        close_index.insert(open_index, index);
                    }
                }
                None => {}
            }
        }
        Self {
            close_rows,
            close_index,
        }
    }

    /// The row of the delimiter that closes `open`, if it is matched.
    pub fn close_row(&self, open: Tok) -> Option<u32> {
        self.close_rows.get(&open.start).copied()
    }

    /// The half-open token range `[open_index, close_index + 1)` spanning a
    /// matched balanced group, or `None` if the opener at `open_index` is
    /// unmatched (or not an opener at all).
    pub fn match_range(&self, open_index: usize) -> Option<TokenRange> {
        let close = *self.close_index.get(&open_index)?;
        Some(TokenRange {
            start: open_index,
            end: close + 1,
        })
    }
}

impl<'a> TokenCursor<'a> {
    pub fn new(source: &'a str, tokens: &'a [Tok]) -> Self {
        Self {
            source,
            tokens,
            blocks: None,
            start: 0,
            at: 0,
            end: tokens.len(),
        }
    }
    /// Like [`new`](Self::new) but carrying a whole-stream [`BlockMap`], enabling
    /// [`peek_balanced`](Self::peek_balanced). Views inherit the map.
    pub fn with_blocks(source: &'a str, tokens: &'a [Tok], blocks: &'a BlockMap) -> Self {
        Self {
            blocks: Some(blocks),
            ..Self::new(source, tokens)
        }
    }
    fn contains_range(&self, range: TokenRange) -> bool {
        self.start <= range.start && range.start <= range.end && range.end <= self.end
    }
    pub fn view(&self, range: TokenRange) -> Option<Self> {
        self.contains_range(range).then_some(Self {
            source: self.source,
            tokens: self.tokens,
            blocks: self.blocks,
            start: range.start,
            at: range.start,
            end: range.end,
        })
    }
    /// Non-consuming: the half-open token range of the balanced group opened by
    /// the token at `peek(n)`, when it is a delimiter matched within this
    /// cursor's window. `None` if there is no [`BlockMap`], the token is not a
    /// matched opener, or the group would escape the current view. Lets a hook
    /// skip a `(...)`/`[...]`/`{...}` group without a manual depth counter.
    pub fn peek_balanced(&self, n: usize) -> Option<TokenRange> {
        let index = self.at.checked_add(n)?;
        if index >= self.end {
            return None;
        }
        let range = self.blocks?.match_range(index)?;
        (range.end <= self.end).then_some(range)
    }
    /// Consuming twin of [`peek_balanced`]: if `peek(0)` opens a balanced group
    /// matched within this window, advances past the matching close and returns
    /// `true`; otherwise leaves the cursor unmoved and returns `false`. Lets a
    /// hook skip a whole `(...)`/`[...]`/`{...}` group without hand-rolling a
    /// depth counter (or consuming the opener and scanning to its close).
    pub fn skip_balanced(&mut self) -> bool {
        match self.peek_balanced(0) {
            Some(range) => {
                self.at = range.end;
                true
            }
            None => false,
        }
    }
    pub fn next(&mut self) -> Option<Tok> {
        let value = self.peek(0)?;
        self.at += 1;
        Some(value)
    }
    pub fn peek(&self, n: usize) -> Option<Tok> {
        let index = self.at.checked_add(n)?;
        (index < self.end)
            .then(|| self.tokens.get(index).copied())
            .flatten()
    }
    pub fn consume_if(&mut self, kind: TokenKind) -> Option<Tok> {
        (self.peek(0)?.kind == kind).then(|| self.next().unwrap())
    }
    pub fn mark(&self) -> usize {
        self.at
    }
    pub fn source(&self) -> &'a str {
        self.source
    }
    pub fn text(&self, token: Tok) -> &'a str {
        &self.source[token.start as usize..token.end as usize]
    }
    pub fn span_text(&self, start: Tok, end: Tok) -> &'a str {
        &self.source[start.start as usize..end.end as usize]
    }
    /// Consumes one balanced token pair, including nested instances of that
    /// same pair. On malformed input the cursor remains forward-only and ends
    /// at EOF.
    pub fn consume_balanced_pair(
        &mut self,
        open_kind: TokenKind,
        close_kind: TokenKind,
    ) -> Option<BalancedPair> {
        let open = self.consume_if(open_kind)?;
        let mut depth = 1u32;
        while let Some(token) = self.next() {
            if token.kind == open_kind {
                depth += 1;
            } else if token.kind == close_kind {
                depth -= 1;
                if depth == 0 {
                    return Some(BalancedPair { open, close: token });
                }
            }
        }
        None
    }
    /// Consumes one declaration fragment without crossing a top-level logical
    /// boundary. Owner closes and row-transition tokens remain available to
    /// the caller; explicit semicolons are consumed. Every successful loop
    /// iteration consumes a token, including malformed unmatched closes.
    pub fn consume_balanced_until(&mut self, rules: BalancedUntil) -> TokenRange {
        let mut depth = DelimiterDepth::default();
        let start = self.at;
        let mut last: Option<Tok> = None;

        loop {
            let Some(next) = self.peek(0) else {
                return TokenRange {
                    start,
                    end: self.at,
                };
            };
            let top = depth.is_top_level();
            if top && rules.owner_close == Some(next.kind) {
                return TokenRange {
                    start,
                    end: self.at,
                };
            }
            if top && next.kind == rules.delimiters.semicolon {
                self.next().expect("peeked token");
                return TokenRange {
                    start,
                    end: self.at - 1,
                };
            }
            if top
                && rules.logical_line
                && last.is_some_and(|token| {
                    next.row > token.row && (rules.can_terminate_line)(token.kind)
                })
            {
                return TokenRange {
                    start,
                    end: self.at,
                };
            }

            let token = self.next().expect("peeked token");
            last = Some(token);
            depth.observe(token.kind, rules.delimiters);
        }
    }

    /// Iterates the members of a `{ ... }` body, the flat-token analog of the
    /// tree walkers' `iterate_children!`. With the cursor positioned just past
    /// the opening delimiter, each step skips leading `rule.skip` separators,
    /// stops after consuming `rule.close` (or at EOF), and otherwise bounds one
    /// member via `rule.fragment` and hands a **bounded view** of it to `f`. The
    /// view isolates the member, so a handler cannot run past its own fragment
    /// into the next one. Total on malformed input: a fragment that makes no
    /// progress still advances one token.
    pub fn members(&mut self, rule: MemberRule, mut f: impl FnMut(&mut TokenCursor<'a>)) {
        loop {
            while rule
                .skip
                .iter()
                .any(|&kind| self.consume_if(kind).is_some())
            {}
            match self.peek(0) {
                None => return,
                Some(token) if token.kind == rule.close => {
                    self.next();
                    return;
                }
                _ => {}
            }
            let range = self.consume_balanced_until(rule.fragment);
            if range.end == range.start {
                // Defensive: keep the walk total if a fragment consumed nothing.
                if self.next().is_none() {
                    return;
                }
                continue;
            }
            if let Some(mut view) = self.view(range) {
                f(&mut view);
            }
        }
    }
}

/// A validated prefix made from repeated item/separator tokens. The range can
/// be revisited after the rest of the enclosing syntax has been parsed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SeparatedRange {
    pub range: TokenRange,
    pub item: TokenKind,
}

impl SeparatedRange {
    pub fn items<'a>(self, cursor: &TokenCursor<'a>) -> Option<SeparatedItems<'a>> {
        Some(SeparatedItems {
            cursor: cursor.view(self.range)?,
            item: self.item,
        })
    }
}

pub(crate) struct SeparatedItems<'a> {
    cursor: TokenCursor<'a>,
    item: TokenKind,
}

impl Iterator for SeparatedItems<'_> {
    type Item = Tok;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(token) = self.cursor.next() {
            if token.kind == self.item {
                return Some(token);
            }
        }
        None
    }
}

#[derive(Clone, Copy)]
pub(crate) struct HookOptions<'a> {
    pub tag_config: &'a crate::parser::TagKindConfig,
    pub line: bool,
    pub kind: bool,
    pub file: bool,
    pub scope: bool,
    pub signature: bool,
    pub typeref: bool,
    pub access: bool,
    pub end: bool,
    pub qualified: bool,
}
impl<'a> HookOptions<'a> {
    pub fn from_config(
        tag_config: &'a crate::parser::TagKindConfig,
        config: &crate::config::Config,
    ) -> Self {
        let f = &config.fields_config;
        Self {
            tag_config,
            line: f.is_field_enabled("line"),
            kind: f.is_field_enabled("kind"),
            file: f.is_field_enabled("file"),
            scope: f.is_field_enabled("scope"),
            signature: f.is_field_enabled("signature"),
            typeref: f.is_field_enabled("typeref"),
            access: f.is_field_enabled("access"),
            end: f.is_field_enabled("end"),
            qualified: config.extras_config.qualified,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct HookInput<'a> {
    pub source: &'a str,
    pub path: &'a str,
    pub options: HookOptions<'a>,
    pub line_starts: &'a [u32],
}
pub(crate) trait TagHooks: Default {
    fn generate(
        &mut self,
        input: HookInput<'_>,
        tokens: TokenCursor<'_>,
        output: &mut super::tag_emitter::TagEmitter<'_>,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_token_layout() {
        assert_eq!(std::mem::size_of::<Tok>(), 16);
    }

    #[test]
    fn bounded_views_do_not_move_the_parent() {
        let tokens = [tok(0, 0), tok(1, 1), tok(2, 2), tok(3, 3)];
        let parent = TokenCursor::new("abcd", &tokens);
        let mut child = parent.view(TokenRange { start: 1, end: 3 }).unwrap();
        assert_eq!(child.next(), Some(tokens[1]));
        assert_eq!(child.next(), Some(tokens[2]));
        assert_eq!(child.next(), None);
        assert_eq!(parent.peek(0), Some(tokens[0]));
    }

    #[test]
    fn bounded_views_reject_ranges_outside_the_parent() {
        let tokens = [tok(0, 0), tok(1, 0), tok(2, 0), tok(3, 0)];
        let root = TokenCursor::new("abcd", &tokens);
        let child = root.view(TokenRange { start: 1, end: 3 }).unwrap();

        assert!(child.view(TokenRange { start: 0, end: 2 }).is_none());
        assert!(child.view(TokenRange { start: 2, end: 4 }).is_none());
        assert!(child.view(TokenRange { start: 3, end: 2 }).is_none());
    }

    #[test]
    fn balanced_pair_returns_tokens_and_consumes_close() {
        let open = TokenKind(2);
        let close = TokenKind(3);
        let tokens = [
            tok_kind(open, 0, 0),
            tok_kind(TokenKind(1), 1, 0),
            tok_kind(open, 2, 0),
            tok_kind(TokenKind(1), 3, 0),
            tok_kind(close, 4, 0),
            tok_kind(close, 5, 0),
            tok_kind(TokenKind(1), 6, 0),
        ];
        let mut cursor = TokenCursor::new("(a(b))c", &tokens);
        let pair = cursor.consume_balanced_pair(open, close).unwrap();
        assert_eq!(pair.open, tokens[0]);
        assert_eq!(pair.close, tokens[5]);
        assert_eq!(cursor.next(), Some(tokens[6]));
    }

    const TEST_DELIMS: DelimiterKinds = DelimiterKinds {
        paren_open: TokenKind(10),
        paren_close: TokenKind(11),
        bracket_open: TokenKind(12),
        bracket_close: TokenKind(13),
        brace_open: TokenKind(14),
        brace_close: TokenKind(15),
        semicolon: TokenKind(16),
    };

    #[test]
    fn block_map_records_matching_close_rows_and_ignores_unmatched() {
        // rows:  { ( ) }  [   (nested + one dangling open bracket)
        let tokens = [
            tok_kind(TokenKind(14), 0, 0), // {  -> matched at row 3
            tok_kind(TokenKind(10), 1, 1), // (  -> matched at row 2
            tok_kind(TokenKind(11), 2, 2), // )
            tok_kind(TokenKind(15), 3, 3), // }
            tok_kind(TokenKind(12), 4, 4), // [  -> never closed
        ];
        let map = BlockMap::new(&tokens, TEST_DELIMS);
        assert_eq!(map.close_row(tokens[0]), Some(3));
        assert_eq!(map.close_row(tokens[1]), Some(2));
        assert_eq!(map.close_row(tokens[4]), None);
    }

    #[test]
    fn block_map_leaves_family_mismatched_delimiters_unmatched() {
        // `{ ( }` — the `}` does not match the innermost `(`, so nothing closes.
        let tokens = [
            tok_kind(TokenKind(14), 0, 0), // {
            tok_kind(TokenKind(10), 1, 1), // (
            tok_kind(TokenKind(15), 2, 2), // }
        ];
        let map = BlockMap::new(&tokens, TEST_DELIMS);
        assert_eq!(map.close_row(tokens[0]), None);
        assert_eq!(map.close_row(tokens[1]), None);
    }

    #[test]
    fn peek_balanced_returns_group_range_and_ignores_unmatched() {
        // ( )  ( [ ]  — first `()` matches, the third `(` is never closed.
        let tokens = [
            tok_kind(TokenKind(10), 0, 0), // ( idx0 -> ) idx1
            tok_kind(TokenKind(11), 1, 0), // )
            tok_kind(TokenKind(10), 2, 0), // ( idx2 unmatched
            tok_kind(TokenKind(12), 3, 0), // [ idx3 -> ] idx4
            tok_kind(TokenKind(13), 4, 0), // ]
        ];
        let blocks = BlockMap::new(&tokens, TEST_DELIMS);
        let cursor = TokenCursor::with_blocks("()([]", &tokens, &blocks);
        assert_eq!(
            cursor.peek_balanced(0),
            Some(TokenRange { start: 0, end: 2 })
        );
        assert_eq!(cursor.peek_balanced(1), None); // a close is not an opener
        assert_eq!(cursor.peek_balanced(2), None); // unmatched open
        assert_eq!(
            cursor.peek_balanced(3),
            Some(TokenRange { start: 3, end: 5 })
        );
    }

    #[test]
    fn peek_balanced_respects_the_view_window_and_absent_map() {
        let tokens = [
            tok_kind(TokenKind(10), 0, 0), // ( idx0 -> ) idx3
            tok_kind(TokenKind(1), 1, 0),
            tok_kind(TokenKind(1), 2, 0),
            tok_kind(TokenKind(11), 3, 0), // )
        ];
        let blocks = BlockMap::new(&tokens, TEST_DELIMS);
        let cursor = TokenCursor::with_blocks("(  )", &tokens, &blocks);
        assert_eq!(
            cursor.peek_balanced(0),
            Some(TokenRange { start: 0, end: 4 })
        );
        // A view that excludes the closing `)` cannot see past its window.
        let view = cursor.view(TokenRange { start: 0, end: 3 }).unwrap();
        assert_eq!(view.peek_balanced(0), None);
        // Without a BlockMap, structural lookahead degrades to `None`.
        assert_eq!(TokenCursor::new("(  )", &tokens).peek_balanced(0), None);
    }

    #[test]
    fn skip_balanced_consumes_a_matched_group_only() {
        // ( a ) b
        let tokens = [
            tok_kind(TokenKind(10), 0, 0), // ( idx0 -> ) idx2
            tok(1, 0),                     // a
            tok_kind(TokenKind(11), 2, 0), // )
            tok(3, 0),                     // b
        ];
        let blocks = BlockMap::new(&tokens, TEST_DELIMS);
        let mut cursor = TokenCursor::with_blocks("(a)b", &tokens, &blocks);
        assert!(cursor.skip_balanced());
        assert_eq!(cursor.next(), Some(tokens[3])); // landed just past `)`
                                                    // At a non-opener the cursor does not move.
        let mut inside = TokenCursor::with_blocks("(a)b", &tokens, &blocks);
        inside.next(); // now at `a`
        assert!(!inside.skip_balanced());
        assert_eq!(inside.next(), Some(tokens[1]));
    }

    fn member_rule() -> MemberRule {
        MemberRule {
            close: TEST_DELIMS.brace_close,
            skip: &[TEST_DELIMS.semicolon],
            fragment: BalancedUntil {
                delimiters: TEST_DELIMS,
                owner_close: Some(TEST_DELIMS.brace_close),
                logical_line: false,
                can_terminate_line: |_| false,
            },
        }
    }

    #[test]
    fn members_iterates_fragments_skips_separators_and_stops_at_close() {
        // x ; y ; }
        let tokens = [
            tok(0, 0),
            tok_kind(TEST_DELIMS.semicolon, 1, 0),
            tok(2, 0),
            tok_kind(TEST_DELIMS.semicolon, 3, 0),
            tok_kind(TEST_DELIMS.brace_close, 4, 0),
        ];
        let mut cursor = TokenCursor::new("x;y;}", &tokens);
        let mut seen = Vec::new();
        cursor.members(member_rule(), |member| {
            while let Some(token) = member.next() {
                seen.push(token.start);
            }
        });
        assert_eq!(seen, vec![0, 2]);
        assert_eq!(cursor.next(), None); // consumed through the closing brace
    }

    #[test]
    fn members_isolate_each_fragment_from_the_next() {
        // one two ; three }  — a greedy handler stays within its own fragment.
        let tokens = [
            tok(0, 0),
            tok(1, 0),
            tok_kind(TEST_DELIMS.semicolon, 2, 0),
            tok(3, 0),
            tok_kind(TEST_DELIMS.brace_close, 4, 0),
        ];
        let mut cursor = TokenCursor::new("a b;c}", &tokens);
        let mut counts = Vec::new();
        cursor.members(member_rule(), |member| {
            let mut n = 0;
            while member.next().is_some() {
                n += 1;
            }
            counts.push(n);
        });
        assert_eq!(counts, vec![2, 1]);
    }

    #[test]
    fn members_is_total_without_a_closing_delimiter() {
        // x ; y  — unterminated body still terminates the walk.
        let tokens = [tok(0, 0), tok_kind(TEST_DELIMS.semicolon, 1, 0), tok(2, 0)];
        let mut cursor = TokenCursor::new("x;y", &tokens);
        let mut seen = Vec::new();
        cursor.members(member_rule(), |member| {
            while let Some(token) = member.next() {
                seen.push(token.start);
            }
        });
        assert_eq!(seen, vec![0, 2]);
        assert_eq!(cursor.next(), None);
    }

    fn tok(offset: u32, row: u32) -> Tok {
        tok_kind(TokenKind(1), offset, row)
    }

    fn tok_kind(kind: TokenKind, offset: u32, row: u32) -> Tok {
        Tok {
            kind,
            flags: TokenFlags::default(),
            start: offset,
            end: offset + 1,
            row,
        }
    }
}
