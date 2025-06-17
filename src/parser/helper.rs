use indexmap::IndexMap;
use std::collections::HashMap;
use std::collections::HashSet;
use tree_sitter::{Node, TreeCursor};

use crate::tag;

/// Configuration for which tag kinds to generate
#[derive(Debug, Clone)]
pub struct TagKindConfig {
    pub enabled_kinds: HashSet<String>,
    // Cache for optimization - whether we need to traverse certain node types
    pub needs_traversal_cache: HashMap<String, bool>,
}

impl TagKindConfig {
    /// Check if a tag kind is enabled
    pub fn is_kind_enabled(&self, kind: &str) -> bool {
        self.enabled_kinds.contains(kind)
    }

    /// Check if we need to traverse into a specific node type for optimization
    pub fn needs_traversal(&self, node_kind: &str) -> bool {
        self.needs_traversal_cache
            .get(node_kind)
            .copied()
            .unwrap_or(true)
    }
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

/// Creates a tag with the given parameters
pub fn create_tag(
    name: String,
    kind_char: &str,
    node: Node,
    context: &mut Context,
    extra_fields: Option<IndexMap<String, String>>,
) {
    if name.is_empty() || name == "_" {
        return; // Don't tag empty or placeholder names
    }

    // Check if this tag kind is enabled in the configuration
    if !context.tag_config.is_kind_enabled(kind_char) {
        return; // Skip creating this tag if the kind is disabled
    }

    let row = node.start_position().row;
    let address = address_string_from_line(row, context);

    // Create basic extension fields
    let mut extension_fields = IndexMap::new();

    // Add basic fields if enabled
    if context.user_config.fields_config.is_field_enabled("kind") {
        extension_fields.insert(String::from("kind"), kind_char.to_string());
    }
    if context.user_config.fields_config.is_field_enabled("line") {
        extension_fields.insert(String::from("line"), (row + 1).to_string());
    }
    if context.user_config.fields_config.is_field_enabled("file") {
        extension_fields.insert(String::from("file"), context.file_name.to_string());
    }

    // Add end field if the tag spans multiple lines
    if context.user_config.fields_config.is_field_enabled("end") {
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;
        if end_line > start_line {
            extension_fields.insert(String::from("end"), (end_line + 1).to_string());
        }
    }

    // Add extra fields if provided
    if let Some(extras) = extra_fields {
        for (key, value) in extras {
            extension_fields.insert(key, value);
        }
    }

    let final_extension_fields = if extension_fields.is_empty() {
        None
    } else {
        Some(extension_fields)
    };

    context.tags.push(tag::Tag {
        name,
        file_name: context.file_name.to_string(),
        address,
        kind: Some(String::from(kind_char)),
        extension_fields: final_extension_fields,
    });
}

/// Creates a tag with the given parameters (deprecated - use language-specific methods)
pub fn create_tag_with_language(
    name: String,
    kind_char: &str,
    node: Node,
    context: &mut Context,
    extra_fields: Option<IndexMap<String, String>>,
    _language: Option<&str>,
) {
    // This is now a simple wrapper around create_tag for backward compatibility
    create_tag(name, kind_char, node, context, extra_fields);
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
