use tree_sitter::{Node, TreeCursor};

use super::tag_config::TagKindConfig;
use crate::{split_by_newlines, tag};

/// Trait for language-specific context behavior
pub trait LanguageContext {
    type ScopeType;

    fn push_scope(&mut self, scope_type: Self::ScopeType, name: String);
    fn pop_scope(&mut self) -> Option<(Self::ScopeType, String)>;
    fn process_node(&mut self, cursor: &mut TreeCursor) -> Option<(Self::ScopeType, String)>;
}

/// Stores context during traversal
pub struct Context<'a> {
    pub source_code: &'a str,
    pub lines: Vec<Vec<u8>>,
    pub file_name: &'a str,
    pub tags: &'a mut Vec<tag::Tag>,
    pub tag_config: &'a TagKindConfig,
    pub user_config: &'a crate::config::Config,
}

impl<'a> Context<'a> {
    /// Helper to get the text content of a node
    pub fn node_text(&self, node: &Node) -> &'a str {
        node.utf8_text(self.source_code.as_bytes())
            .unwrap_or_else(|_| {
                eprintln!(
                    "Warning: Failed to get UTF-8 text for node at range {:?}-{:?}",
                    node.start_position(),
                    node.end_position()
                );
                "" // Return empty string on error
            })
    }
}

/// Generic tag generation function
pub fn generate_tags_with_config(
    ts_parser: &mut tree_sitter::Parser,
    language: tree_sitter::Language,
    code: &[u8],
    file_path: &str,
    action: impl for<'a> FnOnce(&'a str, Vec<Vec<u8>>, &mut TreeCursor<'a>, &mut Vec<tag::Tag>),
) -> Option<Vec<tag::Tag>> {
    let source_code = match std::str::from_utf8(code) {
        Ok(s) => s,
        Err(_) => {
            eprintln!(
                "Warning: Input for {} is not valid UTF-8, skipping.",
                file_path
            );
            return None;
        }
    };

    let lines = split_by_newlines::split_by_newlines(code);

    ts_parser
        .set_language(&language)
        .expect("Error loading grammar");

    let tree = ts_parser.parse(source_code, None)?;
    let mut tags = Vec::new();

    let mut cursor = tree.walk();

    if cursor.goto_first_child() {
        action(source_code, lines, &mut cursor, &mut tags);
    }

    Some(tags)
}

/// Generic tree walking function that can be used by any language implementation
/// that implements the LanguageContext trait
pub fn walk_generic<C: LanguageContext>(cursor: &mut TreeCursor, context: &mut C) {
    loop {
        // Process the current node
        let scope_info = context.process_node(cursor);

        // Manage Scope Stack
        let mut scope_pushed = false;
        if let Some((scope_type, scope_name)) = scope_info {
            if !scope_name.is_empty() {
                context.push_scope(scope_type, scope_name);
                scope_pushed = true;
            }
        }

        // Recurse into children
        if cursor.goto_first_child() {
            walk_generic(cursor, context);
            cursor.goto_parent(); // Return to current node after visiting children
        }

        // Pop Scope if necessary
        if scope_pushed {
            context.pop_scope();
        }

        // Move to the next sibling
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
