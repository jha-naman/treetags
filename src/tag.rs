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
            address: {
                let line_content = String::from_utf8(
                    code[(tag.name_range.start - tag.span.start.column)..tag.line_range.end]
                        .to_vec(),
                )
                .expect("expected line range to be a valid utf8 string");
                let escaped_line = Self::escape_address(&line_content);
                format!("/^{}$/;\"\t", escaped_line)
            },
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
            // Extract module value if present
            let module_value = fields.get("module").map(|s| s.as_str());

            // Count non-module keys to determine if module is the only field
            let non_module_keys_count = fields.keys().filter(|k| *k != "module").count();
            let module_only = non_module_keys_count == 0 && module_value.is_some();

            // Process module field if it's the only field
            if module_only {
                if let Some(module) = fields.get("module") {
                    output.push_str(&format!("\tmodule:{}", module));
                }
            }

            // Process all non-module fields
            for (key, value) in fields.iter().filter(|(k, _)| *k != "module") {
                // For other fields, prepend module value if it exists
                let formatted_value = if let Some(module) = module_value {
                    format!("{}::{}", module, value)
                } else {
                    value.clone()
                };
                output.push_str(&format!("\t{}:{}", key, formatted_value));
            }
        }

        output.push('\n');
        output.into_bytes()
    }
    ///
    /// Escapes backslashes and forward slashes in the address field
    ///
    /// # Arguments
    ///
    /// * `address` - The address string to escape
    ///
    /// # Returns
    ///
    /// A new string with backslashes and forward slashes escaped
    fn escape_address(address: &str) -> String {
        address.replace('\\', "\\\\").replace('/', "\\/")
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
    dbg!(tag_file_path);
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

    // Tests for `into_bytes`
    #[test]
    fn test_into_bytes_basic() {
        let tag = Tag {
            name: "test_function".to_string(),
            file_name: "test.rs".to_string(),
            address: "/^fn test_function() {$/".to_string(),
            kind: Some("function".to_string()),
            extension_fields: None,
        };

        let expected = "test_function\ttest.rs\t/^fn test_function() {$/\tfunction\n";
        assert_eq!(String::from_utf8(tag.into_bytes()).unwrap(), expected);
    }

    #[test]
    fn test_into_bytes_no_kind() {
        let tag = Tag {
            name: "TEST_CONSTANT".to_string(),
            file_name: "constants.rs".to_string(),
            address: "/^const TEST_CONSTANT: i32 = 42;$/".to_string(),
            kind: None,
            extension_fields: None,
        };

        let expected = "TEST_CONSTANT\tconstants.rs\t/^const TEST_CONSTANT: i32 = 42;$/\n";
        assert_eq!(String::from_utf8(tag.into_bytes()).unwrap(), expected);
    }

    #[test]
    fn test_into_bytes_with_module_only() {
        let mut extension_fields = HashMap::new();
        extension_fields.insert("module".to_string(), "example".to_string());

        let tag = Tag {
            name: "Model".to_string(),
            file_name: "model.rs".to_string(),
            address: "/^struct Model {$/".to_string(),
            kind: Some("struct".to_string()),
            extension_fields: Some(extension_fields),
        };

        let expected = "Model\tmodel.rs\t/^struct Model {$/\tstruct\tmodule:example\n";
        assert_eq!(String::from_utf8(tag.into_bytes()).unwrap(), expected);
    }

    #[test]
    fn test_into_bytes_with_non_module_field() {
        let mut extension_fields = HashMap::new();
        extension_fields.insert("implementation".to_string(), "Circle".to_string());

        let tag = Tag {
            name: "draw".to_string(),
            file_name: "shapes.rs".to_string(),
            address: "/^fn draw(&self) {$/".to_string(),
            kind: Some("method".to_string()),
            extension_fields: Some(extension_fields),
        };

        let expected = "draw\tshapes.rs\t/^fn draw(&self) {$/\tmethod\timplementation:Circle\n";
        assert_eq!(String::from_utf8(tag.into_bytes()).unwrap(), expected);
    }

    #[test]
    fn test_into_bytes_with_module_and_implementation() {
        let mut extension_fields = HashMap::new();
        extension_fields.insert("implementation".to_string(), "Circle".to_string());
        extension_fields.insert("module".to_string(), "example".to_string());

        let tag = Tag {
            name: "draw".to_string(),
            file_name: "shapes.rs".to_string(),
            address: "/^fn draw(&self) {$/".to_string(),
            kind: Some("method".to_string()),
            extension_fields: Some(extension_fields),
        };

        // Module should be prepended to the implementation value and module key should not appear
        let expected =
            "draw\tshapes.rs\t/^fn draw(&self) {$/\tmethod\timplementation:example::Circle\n";
        assert_eq!(String::from_utf8(tag.into_bytes()).unwrap(), expected);
    }

    #[test]
    fn test_into_bytes_with_module_and_trait() {
        let mut extension_fields = HashMap::new();
        extension_fields.insert("trait".to_string(), "Shape".to_string());
        extension_fields.insert("module".to_string(), "example".to_string());

        let tag = Tag {
            name: "area".to_string(),
            file_name: "traits.rs".to_string(),
            address: "/^fn area(&self) -> f64 {$/".to_string(),
            kind: Some("method".to_string()),
            extension_fields: Some(extension_fields),
        };

        // Module should be prepended to the trait value and module key should not appear
        let expected =
            "area\ttraits.rs\t/^fn area(&self) -> f64 {$/\tmethod\ttrait:example::Shape\n";
        assert_eq!(String::from_utf8(tag.into_bytes()).unwrap(), expected);
    }

    #[test]
    fn test_into_bytes_with_multiple_extension_fields() {
        let mut extension_fields = HashMap::new();
        extension_fields.insert("trait".to_string(), "Shape".to_string());
        extension_fields.insert("implementation".to_string(), "Circle".to_string());
        extension_fields.insert("module".to_string(), "geometry".to_string());

        let tag = Tag {
            name: "calculate".to_string(),
            file_name: "geometry.rs".to_string(),
            address: "/^fn calculate(&self) -> f64 {$/".to_string(),
            kind: Some("method".to_string()),
            extension_fields: Some(extension_fields),
        };

        // Module should be prepended to all other fields and module key should not appear
        let bytes = tag.into_bytes();
        let output = String::from_utf8(bytes).unwrap();

        // Since HashMap iteration order is not guaranteed, check for individual components
        assert!(output
            .starts_with("calculate\tgeometry.rs\t/^fn calculate(&self) -> f64 {$/\tmethod\t"));
        assert!(output.contains("trait:geometry::Shape"));
        assert!(output.contains("implementation:geometry::Circle"));
        assert!(!output.contains("module:geometry"));
        assert!(output.ends_with("\n"));
    }

    #[test]
    fn test_into_bytes_with_no_extension_fields() {
        let tag = Tag {
            name: "MyEnum".to_string(),
            file_name: "types.rs".to_string(),
            address: "/^enum MyEnum {$/".to_string(),
            kind: Some("enum".to_string()),
            extension_fields: Some(HashMap::new()), // Empty HashMap
        };

        let expected = "MyEnum\ttypes.rs\t/^enum MyEnum {$/\tenum\n";
        assert_eq!(String::from_utf8(tag.into_bytes()).unwrap(), expected);
    }

    #[test]
    fn test_escape_address() {
        assert_eq!(
            Tag::escape_address("/^fn test() {$/"),
            "\\/^fn test() {$\\/"
        );
        assert_eq!(Tag::escape_address("\\n\\t"), "\\\\n\\\\t");
        assert_eq!(
            Tag::escape_address("/path/to/file\\with\\backslashes"),
            "\\/path\\/to\\/file\\\\with\\\\backslashes"
        );
        assert_eq!(Tag::escape_address("no_special_chars"), "no_special_chars");
    }
}
