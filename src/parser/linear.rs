//! Tree-free runtime for generated builtin scanners.
#![allow(dead_code)] // API surface is intentionally ahead of the first consumer.

pub(crate) use super::linear_scanner::GeneratedLexicon;
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
    at: usize,
}
impl<'a> TokenCursor<'a> {
    pub fn new(source: &'a str, tokens: &'a [Tok]) -> Self {
        Self {
            source,
            tokens,
            at: 0,
        }
    }
    pub fn next(&mut self) -> Option<Tok> {
        let value = self.peek(0)?;
        self.at += 1;
        Some(value)
    }
    pub fn peek(&self, n: usize) -> Option<Tok> {
        self.tokens.get(self.at + n).copied()
    }
    pub fn consume_if(&mut self, kind: TokenKind) -> Option<Tok> {
        (self.peek(0)?.kind == kind).then(|| self.next().unwrap())
    }
    pub fn mark(&self) -> usize {
        self.at
    }
    pub fn text(&self, token: Tok) -> &'a str {
        &self.source[token.start as usize..token.end as usize]
    }
    pub fn span_text(&self, start: Tok, end: Tok) -> &'a str {
        &self.source[start.start as usize..end.end as usize]
    }
    pub fn skip_balanced(&mut self, open: &str, close: &str) -> Option<Tok> {
        let first = self.next()?;
        if self.text(first) != open {
            return None;
        }
        self.skip_balanced_after_open(first, open, close)
    }
    pub fn skip_balanced_after_open(
        &mut self,
        _first: Tok,
        open: &str,
        close: &str,
    ) -> Option<Tok> {
        let mut depth = 1;
        while let Some(token) = self.next() {
            match self.text(token) {
                x if x == open => depth += 1,
                x if x == close => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(token);
                    }
                }
                _ => {}
            }
        }
        None
    }
    pub fn consume_through_row(&mut self, row: u32) -> Option<Tok> {
        let mut last = None;
        while self.peek(0).is_some_and(|t| t.row <= row) {
            last = self.next()
        }
        last
    }
}

#[derive(Clone, Copy)]
pub(crate) struct HookOptions<'a> {
    pub tag_config: &'a super::TagKindConfig,
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
        tag_config: &'a super::TagKindConfig,
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
}
