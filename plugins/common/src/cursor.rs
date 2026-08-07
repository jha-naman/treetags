//! Language-agnostic tree-sitter traversal helpers shared by plugins.
//!
//! Utilities to help plugins can traverse a whole tree with one TreeCursor

use tree_sitter::{Node, TreeCursor};

/// Iterate the direct children of the node the cursor is currently on.
///
/// Resets cursor to the original node after iteration
/// Use `break` to stop early. Using `continue` breaks going to the next
/// sibling
#[macro_export]
macro_rules! for_each_child {
    ($cursor:expr, $body:block) => {{
        if $cursor.goto_first_child() {
            loop {
                $body
                if !$cursor.goto_next_sibling() {
                    break;
                }
            }
            $cursor.goto_parent();
        }
    }};
}

/// The node's UTF-8 text, or `""` if it is not valid UTF-8.
pub fn node_text<'a>(node: Node<'a>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}

/// The node's 1-based start line.
pub fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// Text + 1-based line of the first direct child whose kind is in `kinds`.
/// The cursor is restored to the node it started on.
pub fn child_ident(
    cursor: &mut TreeCursor,
    source: &[u8],
    kinds: &[&str],
) -> Option<(String, u32)> {
    let mut result = None;
    for_each_child!(cursor, {
        let child = cursor.node();
        if kinds.contains(&child.kind()) {
            result = Some((node_text(child, source).to_string(), line_of(child)));
            break;
        }
    });
    result
}

/// Whether the current node has a direct child of one of `kinds`. Cursor restored.
pub fn has_child(cursor: &mut TreeCursor, kinds: &[&str]) -> bool {
    let mut found = false;
    for_each_child!(cursor, {
        if kinds.contains(&cursor.node().kind()) {
            found = true;
            break;
        }
    });
    found
}
