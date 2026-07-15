use tree_sitter::TreeCursor;

pub use super::common::tag_config::{kinds_from_mappings, KindInfo, TagKindConfig};
pub use super::common::tree_walker::{
    generate_tags_with_config, walk_generic, Context, LanguageContext,
};

/// Interprets a single JavaScript escape sequence (e.g. `\\` → `\`, `\t` → tab).
pub fn decode_string_identifier(escape_seq: &str) -> String {
    let bytes = escape_seq.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'\\' {
        return escape_seq.to_string();
    }
    match bytes[1] {
        b'n' => "\n".to_string(),
        b't' => "\t".to_string(),
        b'r' => "\r".to_string(),
        b'0' => "\0".to_string(),
        b'b' => "\x08".to_string(),
        b'f' => "\x0C".to_string(),
        b'v' => "\x0B".to_string(),
        b'\\' => "\\".to_string(),
        b'\'' => "'".to_string(),
        b'"' => "\"".to_string(),
        b'x' if bytes.len() >= 4 => {
            if let Ok(hex) = std::str::from_utf8(&bytes[2..4]) {
                if let Ok(byte_val) = u8::from_str_radix(hex, 16) {
                    return (byte_val as char).to_string();
                }
            }
            escape_seq[1..].to_string()
        }
        b'u' if bytes.len() >= 6 && bytes[2] != b'{' => {
            if let Ok(hex) = std::str::from_utf8(&bytes[2..6]) {
                if let Ok(code_point) = u32::from_str_radix(hex, 16) {
                    if let Some(ch) = char::from_u32(code_point) {
                        return ch.to_string();
                    }
                }
            }
            escape_seq[1..].to_string()
        }
        _ => escape_seq[1..].to_string(),
    }
}

/// Applies ctags-compatible escaping to an interpreted string value for use as a tag name.
/// Strips trailing whitespace, escapes leading space/`!`, and escapes backslashes and tabs.
pub fn escape_string_identifier(s: &str) -> String {
    let s = s.trim_end();
    if s.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '\t' => result.push_str("\\t"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            ' ' if i == 0 => result.push_str("\\x20"),
            '!' if i == 0 => result.push_str("\\x21"),
            _ => result.push(ch),
        }
    }
    result
}

/// Extracts a ctags-compatible tag name from a JS/TS string literal node.
/// The cursor must be positioned on the `string` node; this navigates into its children,
/// decodes escape sequences, and applies ctags name escaping.
pub fn extract_string_tag_name(cursor: &mut TreeCursor, context: &Context) -> String {
    let mut raw_value = String::new();
    iterate_children!(cursor, |string_child| {
        match string_child.kind() {
            "string_fragment" => {
                raw_value.push_str(context.node_text(&string_child));
            }
            "escape_sequence" => {
                let seq = context.node_text(&string_child);
                raw_value.push_str(&decode_string_identifier(seq));
            }
            _ => {}
        }
        Continue
    });
    escape_string_identifier(&raw_value)
}

/// Finds the first child node matching any of the specified kinds and returns its text content.
/// IMPORTANT: Temporarily modifies the cursor but restores it.
pub fn get_node_name(cursor: &mut TreeCursor, context: &Context, kinds: &[&str]) -> Option<String> {
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let current_node = cursor.node();
        if kinds.contains(&current_node.kind()) {
            cursor.goto_parent(); // Restore cursor
            return Some(String::from(context.node_text(&current_node))); // Found the first match
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    cursor.goto_parent(); // Restore cursor
    None
}

/// Control flow for child iteration
pub enum IterationControl {
    Continue,
    Break,
}

// Re-export for convenience
pub use IterationControl::{Break, Continue};

/// Iterate over the children of the cursor's current node
macro_rules! iterate_children {
    ($cursor:expr, |$node:ident| $body:block) => {
        if $cursor.goto_first_child() {
            loop {
                let $node = $cursor.node();
                let control = $body;
                match control {
                    $crate::parser::helper::Break => break,
                    $crate::parser::helper::Continue => {}
                }
                if !$cursor.goto_next_sibling() {
                    break;
                }
            }
            $cursor.goto_parent();
        }
    };
}

// Make the macro available to other modules
pub(crate) use iterate_children;

/// Generates the ctags address string
pub fn address_string_from_line(row: usize, context: &Context) -> String {
    if row >= context.lines.len() {
        return format!("/^{}$/;", row + 1);
    }
    let line_bytes = &context.lines[row];
    let escaped = String::from_utf8_lossy(line_bytes)
        .replace('\\', "\\\\")
        .replace('/', "\\/")
        .replace('^', "\\^")
        .replace('$', "\\$");
    format!("/^{}$/;\"", escaped)
}
