//! # Tag Module
//!
//! This module defines the `Tag` struct which represents a Vi compatible tag
//! and provides functionality for creating and manipulating tags.
//!
//! Tags are used by text editors like Vim to navigate to specific definitions
//! across a codebase. This module handles the parsing and formatting of tags
//! in a format compatible with Vi/Vim.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Represents a Vi compatible tag
///
/// A tag consists of:
/// - name: The identifier (e.g., function name, class name, etc.)
/// - file_name: The file where the identifier is defined
/// - address: A search pattern to locate the identifier in the file
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    /// The name of the tag (e.g., function name, class name)
    pub name: String,
    /// The file where the tag is defined
    pub file_name: String,
    /// The search pattern to locate the tag in the file
    pub address: String,
    /// The tag kind
    pub kind: Option<String>,
    /// The extension fields associated with the tag
    pub extension_fields: Option<HashMap<String, String>>,
}

impl Tag {
    /// Creates a new `Tag` from a tree-sitter tag and source code
    ///
    /// # Arguments
    ///
    /// * `tag` - The tree-sitter tag
    /// * `code` - The source code bytes
    /// * `file_path` - The file path to associate with the tag
    ///
    /// # Returns
    ///
    /// A new `Tag` instance
    pub fn new(tag: tree_sitter_tags::Tag, code: &[u8], file_path: &str) -> Self {
        Tag {
            name: String::from_utf8(code[tag.name_range.start..tag.name_range.end].to_vec())
                .expect("expected function name to be a valid utf8 string"),
            file_name: String::from(file_path),
            // Need the trailing `;"\t` to not break parsing by fzf.vim and Telescope plugins
            address: format!(
                "/^{}$/;\"\t",
                String::from_utf8(
                    code[(tag.name_range.start - tag.span.start.column)..tag.line_range.end]
                        .to_vec()
                )
                .expect("expected line range to be a valid utf8 string")
            ),
            kind: None,
            extension_fields: None,
        }
    }

    /// Converts the tag into a byte representation suitable for writing to a tags file
    ///
    /// # Returns
    ///
    /// A vector of bytes representing the tag in the format:
    /// `name\tfile_name\taddress[;"\tkind:kind_value"][;"\tfield_name:field_value"]...\n`
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut output = format!("{}\t{}\t{}", self.name, self.file_name, self.address);

        if let Some(ref kind) = self.kind {
            output.push_str(&format!("\t{}", kind));
        }

        if let Some(ref fields) = self.extension_fields {
            for (key, value) in fields {
                output.push_str(&format!("\t{}:{}", key, value));
            }
        }

        output.push('\n');
        output.into_bytes()
    }
}

/// Parses a tags file and returns a vector of `Tag` objects
///
/// # Arguments
///
/// * `tag_file_path` - Path to the tags file
///
/// # Returns
///
/// A vector of `Tag` objects parsed from the file
pub fn parse_tag_file(tag_file_path: &Path) -> Vec<Tag> {
    let file = File::open(tag_file_path).expect("Failed to read the tags file");
    let reader = BufReader::new(file);
    let mut tags = Vec::new();

    for line in reader.lines().map_while(Result::ok) {
        // Skip comment lines in tags files
        if line.starts_with('!') {
            continue;
        }

        if let Some(tag) = parse_tag_line(&line) {
            tags.push(tag);
        }
    }

    tags
}

/// Parses a single line from a tags file and returns a `Tag` object
///
/// # Arguments
///
/// * `line` - A line from the tags file
///
/// # Returns
///
/// An Option containing a `Tag` if the line was successfully parsed, None otherwise
pub fn parse_tag_line(line: &str) -> Option<Tag> {
    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() < 3 {
        return None;
    }

    let name = parts[0];
    let file_name = parts[1];
    let address = parts[2];

    // Extract the kind if available (typically in the extension fields)
    let mut kind = None;
    let mut extension_fields = None;

    // Process extension fields (starting from index 3)
    if parts.len() > 3 {
        let mut fields_map = HashMap::new();

        for field in parts.iter().skip(3) {
            // Skip empty fields
            if field.is_empty() {
                continue;
            }

            // Handle both cases: with "key:value" format and standalone kind value
            if let Some(colon_pos) = field.find(':') {
                let key = field[..colon_pos].trim().to_string();
                let value = field[colon_pos + 1..].trim().to_string();

                // Store the kind separately if it's the "kind" field
                if key == "kind" {
                    kind = Some(value.clone());
                } else {
                    fields_map.insert(key, value);
                }
            } else {
                // In ctags, a standalone field without colon is typically the kind
                // Only use the first such field as the kind
                if kind.is_none() {
                    kind = Some(field.to_string());
                }
            }
        }

        if !fields_map.is_empty() {
            extension_fields = Some(fields_map);
        }
    }

    Some(Tag {
        name: name.to_string(),
        file_name: file_name.to_string(),
        address: format!("{}\t", address), // Keep the tab as in the original code
        kind,
        extension_fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tag_line_basic() {
        let line = "function_name\tfile.rs\t/^pub fn function_name() {/;\"\tf\tline:10";
        let tag = parse_tag_line(line).unwrap();

        assert_eq!(tag.name, "function_name");
        assert_eq!(tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^pub fn function_name() {/;\"\t");
        assert_eq!(tag.kind, Some("f".to_string()));

        let extension_fields = tag.extension_fields.unwrap();
        assert!(!extension_fields.contains_key("kind"));
        assert_eq!(extension_fields.get("line").unwrap(), "10");
    }

    #[test]
    fn test_parse_tag_line_with_explicit_kind() {
        let line = "method\tfile.rs\t/^pub fn method(&self) {/;\"\tkind:m\taccess:public\tline:42";
        let tag = parse_tag_line(line).unwrap();

        assert_eq!(tag.name, "method");
        assert_eq!(tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^pub fn method(&self) {/;\"\t");
        assert_eq!(tag.kind, Some("m".to_string()));

        let extension_fields = tag.extension_fields.unwrap();
        assert!(!extension_fields.contains_key("kind"));
        assert_eq!(extension_fields.get("access").unwrap(), "public");
        assert_eq!(extension_fields.get("line").unwrap(), "42");
    }

    #[test]
    fn test_parse_tag_line_no_extension_fields() {
        let line = "variable\tfile.rs\t/^let variable = 42;/;\"";
        let tag = parse_tag_line(line).unwrap();

        assert_eq!(tag.name, "variable");
        assert_eq!(tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^let variable = 42;/;\"\t");
        assert_eq!(tag.kind, None);
        assert_eq!(tag.extension_fields, None);
    }

    #[test]
    fn test_parse_tag_line_kind_without_prefix() {
        let line = "struct_name\tfile.rs\t/^pub struct struct_name {/;\"\ts";
        let tag = parse_tag_line(line).unwrap();

        assert_eq!(tag.name, "struct_name");
        assert_eq!(tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^pub struct struct_name {/;\"\t");
        assert_eq!(tag.kind, Some("s".to_string()));
        assert_eq!(tag.extension_fields, None);
    }

    #[test]
    fn test_parse_tag_line_invalid() {
        let line = "not_enough_fields";
        assert!(parse_tag_line(line).is_none());
    }
}
