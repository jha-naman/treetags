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
        let mut parts = line.split('\t');
        if let Some(name) = parts.next() {
            if let Some(file_name) = parts.next() {
                if let Some(address) = parts.next() {
                    tags.push(Tag {
                        name: name.to_string(),
                        file_name: file_name.to_string(),
                        address: format!("{}\t", address),
                        kind: None,             // FIXME
                        extension_fields: None, // FIXME
                    });
                }
            }
        }
    }

    tags
}
