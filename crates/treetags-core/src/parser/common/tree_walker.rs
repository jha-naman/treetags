use tree_sitter::{Node, TreeCursor};
use treetags_common::tree_walker::LanguageContext;

use super::tag_config::TagKindConfig;
use crate::{split_by_newlines, tag};

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

pub use treetags_common::tree_walker::walk_generic;
