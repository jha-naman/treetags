#![allow(dead_code)]
use super::linear::{
    ExternalLexInput, ExternalLexemeSink, ExternalLexer, ExternalScan, Tok, TokenFlags, TokenKind,
    TokenStream,
};

pub(crate) struct GeneratedLexicon {
    pub identifier: TokenKind,
    pub keyword: TokenKind,
    pub literal: TokenKind,
    pub punctuation: TokenKind,
    pub unknown: TokenKind,
    pub keywords: &'static [&'static str],
    pub punctuation_strings: &'static [&'static str],
}

pub(crate) fn scan<E: ExternalLexer>(
    source: &str,
    lex: &GeneratedLexicon,
) -> Result<TokenStream, String> {
    if source.len() > u32::MAX as usize {
        return Err("input exceeds the 4 GiB token-offset limit".into());
    }
    let bytes = source.as_bytes();
    let mut tokens = Vec::with_capacity(bytes.len() / 4);
    let mut lines = vec![0];
    let (mut at, mut row, mut bol) = (0usize, 0u32, true);
    let mut external = E::default();
    external.reset();
    let mut previous = None;
    while at < bytes.len() {
        let input = ExternalLexInput {
            source,
            offset: at as u32,
            row,
            column: at as u32 - lines[row as usize],
            beginning_of_line: bol,
            previous_significant: previous,
        };
        let before = tokens.len();
        let result;
        {
            let mut sink = ExternalLexemeSink::new(&mut tokens, at as u32, u32::MAX);
            result = external.scan(input, &mut sink);
            if let ExternalScan::Consumed(n) = result {
                let end = at
                    .checked_add(n.get() as usize)
                    .filter(|e| *e <= bytes.len())
                    .ok_or("external lexer consumed beyond EOF")?;
                if !sink.validate_range(at as u32, end as u32) {
                    return Err("external lexer emitted outside consumed range".into());
                }
            }
        }
        match result {
            ExternalScan::NoMatch if tokens.len() != before => {
                return Err("external lexer emitted with NoMatch".into())
            }
            ExternalScan::NoMatch => {}
            ExternalScan::Consumed(n) => {
                let end = at + n.get() as usize;
                advance(bytes, at, end, &mut row, &mut lines);
                at = end;
                bol = lines[row as usize] as usize == at;
                previous = tokens.last().copied();
                continue;
            }
        }
        let start = at;
        let start_row = row;
        match bytes[at] {
            b' ' | b'\t' | b'\r' => {
                at += 1;
                continue;
            }
            b'\n' => {
                at += 1;
                row += 1;
                lines.push(at as u32);
                bol = true;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at += 2;
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1
                }
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                at += 2;
                while at < bytes.len() {
                    if bytes[at] == b'\n' {
                        row += 1;
                        lines.push((at + 1) as u32)
                    }
                    if bytes[at] == b'*' && bytes.get(at + 1) == Some(&b'/') {
                        at += 2;
                        break;
                    }
                    at += 1
                }
                continue;
            }
            b'"' | b'\'' | b'`' => {
                let quote = bytes[at];
                at += 1;
                while at < bytes.len() {
                    if bytes[at] == b'\n' {
                        row += 1;
                        lines.push((at + 1) as u32)
                    }
                    if quote != b'`' && bytes[at] == b'\\' {
                        at = (at + 2).min(bytes.len());
                        continue;
                    }
                    let done = bytes[at] == quote;
                    at += 1;
                    if done {
                        break;
                    }
                }
                push(&mut tokens, lex.literal, start, at, start_row, bol, false)
            }
            _ => scan_regular(source, lex, &mut tokens, start, &mut at, row, bol),
        }
        previous = tokens.last().copied();
        bol = false;
    }
    let input = ExternalLexInput {
        source,
        offset: at as u32,
        row,
        column: at as u32 - lines[row as usize],
        beginning_of_line: bol,
        previous_significant: previous,
    };
    {
        let mut sink = ExternalLexemeSink::new(&mut tokens, at as u32, at as u32);
        external.finish(input, &mut sink);
        if !sink.validate_range(at as u32, at as u32) {
            return Err("external lexer emitted invalid EOF lexeme".into());
        }
    }
    Ok(TokenStream {
        tokens,
        line_starts: lines,
    })
}

fn scan_regular(
    source: &str,
    lex: &GeneratedLexicon,
    out: &mut Vec<Tok>,
    start: usize,
    at: &mut usize,
    row: u32,
    bol: bool,
) {
    let ch = source[*at..].chars().next().unwrap();
    if ch == '_' || ch.is_alphabetic() {
        *at += ch.len_utf8();
        while *at < source.len() {
            let c = source[*at..].chars().next().unwrap();
            if c == '_' || c.is_alphanumeric() {
                *at += c.len_utf8()
            } else {
                break;
            }
        }
        let text = &source[start..*at];
        let kind = if lex.keywords.binary_search(&text).is_ok() {
            lex.keyword
        } else {
            lex.identifier
        };
        push(out, kind, start, *at, row, bol, false)
    } else if ch.is_ascii_digit() {
        *at += ch.len_utf8();
        while *at < source.len() {
            let c = source[*at..].chars().next().unwrap();
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.') {
                *at += c.len_utf8()
            } else {
                break;
            }
        }
        push(out, lex.literal, start, *at, row, bol, false)
    } else if let Some(p) = lex
        .punctuation_strings
        .iter()
        .filter(|p| source[start..].starts_with(**p))
        .max_by_key(|p| p.len())
    {
        *at += p.len();
        push(out, lex.punctuation, start, *at, row, bol, false)
    } else {
        *at += ch.len_utf8();
        push(out, lex.unknown, start, *at, row, bol, true)
    }
}
fn push(
    out: &mut Vec<Tok>,
    kind: TokenKind,
    start: usize,
    end: usize,
    row: u32,
    bol: bool,
    error: bool,
) {
    out.push(Tok {
        kind,
        flags: TokenFlags(
            if bol { TokenFlags::BOL } else { 0 } | if error { TokenFlags::ERROR } else { 0 },
        ),
        start: start as u32,
        end: end as u32,
        row,
    })
}
fn advance(bytes: &[u8], start: usize, end: usize, row: &mut u32, lines: &mut Vec<u32>) {
    for (i, b) in bytes[start..end].iter().enumerate() {
        if *b == b'\n' {
            *row += 1;
            lines.push((start + i + 1) as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{
        generated::go,
        linear::{ExternalScan, NoExternalLexer},
    };
    use std::num::NonZeroU32;

    fn texts<'a>(source: &'a str, stream: &TokenStream) -> Vec<&'a str> {
        stream
            .tokens
            .iter()
            .map(|t| &source[t.start as usize..t.end as usize])
            .collect()
    }

    #[test]
    fn generated_go_lexes_keywords_identifiers_and_longest_punctuation() {
        let source = "package p\nx := y << 2";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(
            texts(source, &stream),
            ["package", "p", "x", ":=", "y", "<<", "2"]
        );
        assert_eq!(stream.tokens[0].kind, go::KEYWORD);
        assert_eq!(stream.tokens[1].kind, go::IDENTIFIER);
        assert_eq!(stream.tokens[3].kind, go::PUNCTUATION);
    }
    #[test]
    fn extras_strings_positions_and_unknown_progress_are_total() {
        let source = "// {\n`a\nb` § x";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(texts(source, &stream), ["`a\nb`", "§", "x"]);
        assert_eq!(stream.tokens[0].row, 1);
        assert_eq!(stream.tokens[1].row, 2);
        assert_ne!(stream.tokens[1].flags.0 & TokenFlags::ERROR, 0);
        assert_eq!(stream.line_starts, [0, 5, 8]);
    }

    #[derive(Default)]
    struct AtLexer;
    impl ExternalLexer for AtLexer {
        fn scan(
            &mut self,
            input: ExternalLexInput<'_>,
            out: &mut ExternalLexemeSink<'_>,
        ) -> ExternalScan {
            if input.starts_with("@\n") {
                out.emit(Tok {
                    kind: TokenKind(99),
                    flags: TokenFlags(0),
                    start: input.offset,
                    end: input.offset + 1,
                    row: input.row,
                });
                out.emit_virtual(TokenKind(100), input.offset + 2, input.row + 1);
                ExternalScan::Consumed(NonZeroU32::new(2).unwrap())
            } else {
                ExternalScan::NoMatch
            }
        }
        fn finish(&mut self, input: ExternalLexInput<'_>, out: &mut ExternalLexemeSink<'_>) {
            out.emit_virtual(TokenKind(101), input.offset, input.row)
        }
    }
    #[test]
    fn external_lexer_can_emit_real_virtual_and_eof_tokens() {
        let stream = go::scan::<AtLexer>("@\nx").unwrap();
        assert_eq!(
            stream.tokens.iter().map(|t| t.kind.0).collect::<Vec<_>>(),
            [99, 100, go::IDENTIFIER.0, 101]
        );
        assert_eq!(stream.tokens[2].row, 1);
    }

    #[derive(Default)]
    struct SkipHash;
    impl ExternalLexer for SkipHash {
        fn scan(
            &mut self,
            input: ExternalLexInput<'_>,
            _: &mut ExternalLexemeSink<'_>,
        ) -> ExternalScan {
            if input.starts_with("#") {
                ExternalScan::Consumed(NonZeroU32::new(1).unwrap())
            } else {
                ExternalScan::NoMatch
            }
        }
    }
    #[test]
    fn external_can_consume_without_emitting() {
        let stream = go::scan::<SkipHash>("#name").unwrap();
        assert_eq!(texts("#name", &stream), ["name"]);
    }

    #[derive(Default)]
    struct InvalidSpan;
    impl ExternalLexer for InvalidSpan {
        fn scan(
            &mut self,
            input: ExternalLexInput<'_>,
            out: &mut ExternalLexemeSink<'_>,
        ) -> ExternalScan {
            out.emit(Tok {
                kind: TokenKind(9),
                flags: TokenFlags(0),
                start: input.offset,
                end: input.offset + 2,
                row: input.row,
            });
            ExternalScan::Consumed(NonZeroU32::new(1).unwrap())
        }
    }
    #[test]
    fn external_rejects_out_of_range_spans() {
        assert!(go::scan::<InvalidSpan>("xx").is_err());
    }

    #[test]
    fn generated_metadata_matches_go_contract() {
        assert_eq!(go::WORD_TOKEN_RULE, "identifier");
        assert_eq!(go::DECLARED_EXTERNAL_COUNT, 0);
        assert!(go::LEXICAL_PATTERNS.contains(&"[_\\p{XID_Start}][_\\p{XID_Continue}]*"));
    }
}
