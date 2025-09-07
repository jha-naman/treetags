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

/// Creates a tag with unified extension field handling for all languages
pub fn create_tag(
    name: String,
    kind_char: &str,
    node: tree_sitter::Node,
    context: &mut Context,
    extra_fields: Option<indexmap::IndexMap<String, String>>,
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
    let mut extension_fields = indexmap::IndexMap::new();

    // Insert fields in ctags order (alphabetical by field letter, with special cases first):

    // 1. Kind field (k) - if enabled as extension field
    if context.user_config.fields_config.is_field_enabled("kind") {
        extension_fields.insert(String::from("kind"), kind_char.to_string());
    }

    // 2. Line number (n) - typically second
    if context.user_config.fields_config.is_field_enabled("line") {
        extension_fields.insert(String::from("line"), (row + 1).to_string());
    }

    // 3. Access field (a) - access modifier
    if let Some(extras) = &extra_fields {
        if let Some(access) = extras.get("access") {
            if context.user_config.fields_config.is_field_enabled("access") {
                extension_fields.insert("access".to_string(), access.clone());
            }
        }
    }

    // 4. File field (f) - file-restricted scoping
    if context.user_config.extras_config.file_scope {
        extension_fields.insert(String::from("file"), String::new());
    }

    // 5. Signature field (S) - function signature
    if let Some(extras) = &extra_fields {
        if let Some(signature) = extras.get("signature") {
            if context
                .user_config
                .fields_config
                .is_field_enabled("signature")
            {
                extension_fields.insert("signature".to_string(), signature.clone());
            }
        }
    }

    // 6. Scope information (s) - scope of tag definition
    if context.user_config.fields_config.is_field_enabled("scope")
        || context.user_config.extras_config.qualified
    {
        if let Some(extras) = &extra_fields {
            for (key, value) in extras {
                match key.as_str() {
                    "struct" | "enum" | "union" | "interface" | "implementation" | "package"
                    | "class" | "namespace" | "function" | "module" | "trait" => {
                        extension_fields.insert(key.clone(), value.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    // 7. Typeref field (t) - type reference
    if let Some(extras) = &extra_fields {
        if let Some(typeref) = extras.get("typeref") {
            if context
                .user_config
                .fields_config
                .is_field_enabled("typeref")
            {
                extension_fields.insert("typeref".to_string(), typeref.clone());
            }
        }
    }

    // 8. End position (e) - end line number
    if context.user_config.fields_config.is_field_enabled("end") {
        extension_fields.insert(
            String::from("end"),
            (node.end_position().row + 1).to_string(),
        );
    }

    context.tags.push(crate::tag::Tag {
        name,
        file_name: context.file_name.to_string(),
        address,
        kind: Some(String::from(kind_char)),
        extension_fields: if extension_fields.is_empty() {
            None
        } else {
            Some(extension_fields)
        },
    });
}
