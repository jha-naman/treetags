// src/decode_to_utf8.rs

//! A helper to guess and decode text files
//!
//! This module allows reading files in different text encodings to a standard UTF-8 string
use chardetng::EncodingDetector;
use encoding_rs::{Encoding, UTF_8};
use std::borrow::Cow;

/// Decode `bytes` to UTF-8, guessing the encoding when it isn't already UTF-8.
/// Returns the text, the encoding used, and whether any bytes were replaced with U+FFFD.
pub fn decode_to_utf8(bytes: &[u8]) -> (Cow<'_, str>, &'static Encoding, bool) {
    // An explicit BOM at the start: decode accordingly, but strip it so it doesn't become U+FEFF.
    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        let (text, encoding_used, had_replacements) = encoding.decode(&bytes[bom_len..]);
        return (text, encoding_used, had_replacements);
    }

    // Already valid UTF-8 (the overwhelmingly common case): borrow, no copy.
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (Cow::Borrowed(s), UTF_8, false);
    }

    // Guess a legacy encoding. We don't allow UTF-8 because we know at this
    // point that `bytes` isn't a valid UTF-8.
    let mut detector = EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, chardetng::Utf8Detection::Deny);
    let (text, had_replacements) = encoding.decode_without_bom_handling(bytes);

    (text, encoding, had_replacements)
}
