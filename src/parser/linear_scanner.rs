#![allow(dead_code)]
use super::linear::{
    ExternalLexInput, ExternalLexemeSink, ExternalLexer, ExternalScan, Tok, TokenFlags, TokenKind,
    TokenStream,
};

pub(crate) trait GeneratedLexicon {
    const UNKNOWN: TokenKind;
    fn lex(source: &str, offset: usize) -> GeneratedLexeme;
}
pub(crate) struct GeneratedLexeme {
    pub len: usize,
    pub kind: TokenKind,
    pub skip: bool,
    pub error: bool,
}

pub(crate) fn scan<E: ExternalLexer, L: GeneratedLexicon>(
    source: &str,
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
        let lexeme = L::lex(source, at);
        let len = lexeme
            .len
            .max(source[at..].chars().next().unwrap().len_utf8());
        at = (at + len).min(bytes.len());
        advance(bytes, start, at, &mut row, &mut lines);
        if !lexeme.skip {
            push(
                &mut tokens,
                lexeme.kind,
                start,
                at,
                start_row,
                bol,
                lexeme.error,
            );
            previous = tokens.last().copied();
        }
        if row != start_row {
            bol = lines[row as usize] as usize == at;
        } else if !lexeme.skip {
            bol = false;
        }
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
        let source = "package p\nx := y <<= 2; z &^= 1; f(...)";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(
            texts(source, &stream),
            [
                "package", "p", "x", ":=", "y", "<<=", "2", ";", "z", "&^=", "1", ";", "f", "(",
                "...", ")"
            ]
        );
        assert_eq!(stream.tokens[0].kind, go::KW_PACKAGE);
        assert_eq!(stream.tokens[1].kind, go::IDENTIFIER);
        assert_eq!(stream.tokens[3].kind, go::PUNCT_3A_3D);
        assert_eq!(stream.tokens[5].kind, go::PUNCT_3C_3C_3D);
        assert_eq!(stream.tokens[9].kind, go::PUNCT_26_5E_3D);
        assert_eq!(stream.tokens[14].kind, go::PUNCT_2E_2E_2E);
        assert_ne!(go::KW_PACKAGE, go::KW_FUNC);
        assert_ne!(go::PUNCT_3A_3D, go::PUNCT_3D);
    }
    #[test]
    fn extras_strings_positions_and_unknown_progress_are_total() {
        let source = "// {\n/* ignored\n} */ `a\nb` § x";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(texts(source, &stream), ["`a\nb`", "§", "x"]);
        assert_eq!(stream.tokens[0].row, 2);
        assert_eq!(stream.tokens[1].row, 3);
        assert_ne!(stream.tokens[1].flags.0 & TokenFlags::ERROR, 0);
        assert_eq!(stream.line_starts, [0, 5, 16, 24]);
    }

    #[test]
    fn generated_go_uses_xid_identifier_boundaries() {
        let source = "π cafe\u{301} ·";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(texts(source, &stream), ["π", "cafe\u{301}", "·"]);
        assert_eq!(stream.tokens[0].kind, go::IDENTIFIER);
        assert_eq!(stream.tokens[1].kind, go::IDENTIFIER);
        assert_ne!(stream.tokens[2].flags.0 & TokenFlags::ERROR, 0);
    }

    #[test]
    fn generated_go_lexes_numeric_and_string_families() {
        let source = r#"0 0b101_0 0o755 0xCA_FE 1.25 1e-3 0x1.fp2 42i .5 "a\n\x41" `a
b` '\u03c0'"#;
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(
            texts(source, &stream),
            [
                "0",
                "0b101_0",
                "0o755",
                "0xCA_FE",
                "1.25",
                "1e-3",
                "0x1.fp2",
                "42i",
                ".5",
                r#""a\n\x41""#,
                "`a\nb`",
                r#"'\u03c0'"#
            ]
        );
        assert!(stream.tokens.iter().all(|t| t.kind == go::LITERAL));
        assert_eq!(stream.tokens[10].row, 0);
        assert_eq!(stream.tokens[11].row, 1);
    }

    #[test]
    fn generated_go_comments_extras_and_unterminated_literals_are_total() {
        let source = " \t// one\n/* two\n */ x \"unterminated\n`raw";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(texts(source, &stream), ["x", "\"unterminated", "`raw"]);
        assert_eq!(stream.tokens[0].row, 2);
        assert_ne!(stream.tokens[1].flags.0 & TokenFlags::ERROR, 0);
        assert_ne!(stream.tokens[2].flags.0 & TokenFlags::ERROR, 0);
    }

    #[test]
    fn generated_go_grammar_controls_malformed_numeric_and_escape_boundaries() {
        let source = "0b_1 0b 1__2 1e 0x1p 1.foo \"\\x1\"";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(
            texts(source, &stream),
            ["0b_1", "0", "b", "1", "__2", "1", "e", "0x1", "p", "1.", "foo", "\"\\x1\""]
        );
        assert_ne!(stream.tokens[11].flags.0 & TokenFlags::ERROR, 0);
    }

    #[test]
    fn generated_go_rune_newline_behavior_follows_the_pinned_pattern() {
        // The canonical grammar's rune content is [^'\\], which includes a newline.
        let source = "'\n' '\\x4'";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(texts(source, &stream), ["'\n'", "'\\x4'"]);
        assert_eq!(stream.tokens[0].kind, go::LITERAL);
        assert_ne!(stream.tokens[1].flags.0 & TokenFlags::ERROR, 0);
    }

    #[test]
    fn generated_go_unterminated_block_comment_is_recovered_as_an_extra() {
        let source = "x /* unterminated\ny";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(texts(source, &stream), ["x"]);
        assert_eq!(stream.line_starts, [0, 18]);
    }

    #[test]
    fn generated_go_uses_ecmascript_digit_whitespace_and_dot_semantics() {
        let source = "١\u{a0}x\u{85}y";
        let stream = go::scan::<NoExternalLexer>(source).unwrap();
        assert_eq!(texts(source, &stream), ["١", "x", "\u{85}", "y"]);
        assert_ne!(stream.tokens[0].flags.0 & TokenFlags::ERROR, 0);
        assert_ne!(stream.tokens[2].flags.0 & TokenFlags::ERROR, 0);
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
