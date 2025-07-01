use tree_sitter::TreeCursor;

pub use super::common::tag_config::TagKindConfig;
pub use super::common::tree_walker::{
    generate_tags_with_config, walk_generic, Context, LanguageContext,
};

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
