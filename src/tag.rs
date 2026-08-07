//! # Tag Module
//!
//! This module defines the `Tag` struct which represents a Vi compatible tag
//! and provides functionality for creating and manipulating tags.
//!
//! Tags are used by text editors like Vim to navigate to specific definitions
//! across a codebase. This module handles the parsing and formatting of tags
//! in a format compatible with Vi/Vim.

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::Arc;

/// A single extension field: a `(key, value)` pair.
///
/// Both sides are `Cow<'static, str>` so the common case — a static key like
/// `"line"` and, where applicable, a static value — borrows without allocating,
/// while dynamic values (line numbers, identifiers) are owned.
pub type Field = (Cow<'static, str>, Cow<'static, str>);

/// An ordered collection of ctags extension fields.
///
/// ctags output requires the fields to appear in a specific, stable order, so
/// this preserves insertion order. Keys are unique: re-inserting an existing key
/// updates its value in place rather than appending a duplicate.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExtensionFields(Vec<Field>);

impl ExtensionFields {
    pub fn new() -> Self {
        ExtensionFields(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k.as_ref() == key)
            .map(|(_, v)| v.as_ref())
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k.as_ref() == key)
    }

    // Replace an existing key's value in place (preserving its position),
    // otherwise append. Accepts anything convertible to `Cow<'static, str>`, so
    // static `&'static str` keys/values borrow while owned `String`s move in.
    pub fn insert(
        &mut self,
        key: impl Into<Cow<'static, str>>,
        value: impl Into<Cow<'static, str>>,
    ) {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.0.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.0.push((key, value));
        }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Field> {
        self.0.iter()
    }
}

impl Extend<Field> for ExtensionFields {
    fn extend<T: IntoIterator<Item = Field>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl IntoIterator for ExtensionFields {
    type Item = Field;
    type IntoIter = std::vec::IntoIter<Field>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

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
    pub file_name: Arc<str>,
    /// The search pattern to locate the tag in the file
    pub address: String,
    /// The tag kind.
    ///
    /// Tags produced by the parsers use one of a fixed set of `&'static str`
    /// kind labels, so this is a `Cow` that borrows those without allocating;
    /// only kinds read back from an existing tags file (or supplied by a plugin)
    /// are owned.
    pub kind: Option<Cow<'static, str>>,
    /// The extension fields associated with the tag
    pub extension_fields: Option<ExtensionFields>,
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
    /// A Result containing a new `Tag` instance or an error message
    pub fn from_ts_tag(
        tag: tree_sitter_tags::Tag,
        code: &[u8],
        file_name: Arc<str>,
    ) -> Result<Self, String> {
        let name = String::from_utf8(code[tag.name_range.start..tag.name_range.end].to_vec())
            .map_err(|e| {
                format!(
                    "Failed to decode tag name as UTF-8 in file '{}': {}",
                    file_name, e
                )
            })?;

        let line_content = String::from_utf8(
            code[(tag.name_range.start - tag.span.start.column)..tag.line_range.end].to_vec(),
        )
        .map_err(|e| {
            format!(
                "Failed to decode line content as UTF-8 in file '{}': {}",
                file_name, e
            )
        })?;

        let mut address = String::with_capacity(line_content.len() + 16);
        address.push_str("/^");
        let prefix_len = address.len();
        Self::escape_address_into(&line_content, &mut address);

        // Truncate the escaped content to 96 bytes maximum
        if address.len() - prefix_len > 96 {
            let limit = prefix_len + 96;
            let at = (prefix_len..=limit)
                .rev()
                .find(|&i| address.is_char_boundary(i))
                .unwrap_or(prefix_len);
            address.truncate(at);
            address.push_str("/;\"\t"); // No '$' if truncated
        } else {
            address.push_str("$/;\"\t");
        }

        Ok(Tag {
            name,
            file_name,
            address,
            kind: None,
            extension_fields: None,
        })
    }

    pub fn sort_cmp(&self, other: &Tag) -> std::cmp::Ordering {
        self.name
            .cmp(&other.name)
            .then_with(|| self.file_name.cmp(&other.file_name))
            .then_with(|| self.address.cmp(&other.address))
            .then_with(|| self.kind.cmp(&other.kind))
            .then_with(|| match (&self.extension_fields, &other.extension_fields) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(af), Some(bf)) => af
                    .iter()
                    .map(|(k, v)| (k.as_ref(), v.as_ref()))
                    .cmp(bf.iter().map(|(k, v)| (k.as_ref(), v.as_ref()))),
            })
    }

    /// Converts the tag into a byte representation suitable for writing to a tags file
    ///
    /// # Returns
    ///
    /// A vector of bytes representing the tag in the format:
    /// `name\tfile_name\taddress[;"\tkind:kind_value"][;"\tfield_name:field_value"]...\n`
    #[cfg(test)]
    pub fn bytes(&self) -> Vec<u8> {
        let mut output = Vec::new();
        self.write_into(&mut output);
        output
    }

    /// Appends the tag's byte representation to `output`.
    ///
    /// This is the allocation-free core used by [`Tag::bytes`]. Callers writing
    /// many tags should reuse a single buffer (calling `output.clear()` between
    /// tags) to avoid one allocation per tag
    pub fn write_into(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.name.as_bytes());
        output.push(b'\t');
        output.extend_from_slice(self.file_name.as_bytes());
        output.push(b'\t');
        output.extend_from_slice(self.address.as_bytes());

        // Only output shorthand kind if we don't have extension fields with a kind field
        let has_kind_extension = self
            .extension_fields
            .as_ref()
            .map(|fields| fields.contains_key("kind"))
            .unwrap_or(false);

        if let Some(ref kind) = self.kind {
            if !has_kind_extension {
                output.push(b'\t');
                output.extend_from_slice(kind.as_bytes());
            }
        }

        if let Some(ref fields) = self.extension_fields {
            // Extract module value if present
            let module_value = fields.get("module");

            // Count non-module keys to determine if module is the only field
            let non_module_keys_count = fields
                .iter()
                .filter(|(k, _)| k.as_ref() != "module")
                .count();
            let module_only = non_module_keys_count == 0 && module_value.is_some();

            // Process module field if it's the only field
            if module_only {
                if let Some(module) = module_value {
                    output.extend_from_slice(b"\tmodule:");
                    output.extend_from_slice(module.as_bytes());
                }
            }

            // Process all non-module fields
            for (key, value) in fields.iter().filter(|(k, _)| k.as_ref() != "module") {
                output.push(b'\t');
                output.extend_from_slice(key.as_bytes());
                output.push(b':');
                match key.as_ref() {
                    // These fields should never have module prefixes
                    "line" | "end" | "kind" | "file" | "signature" | "access" => {}
                    // For scope-related fields, prepend module value if it exists
                    _ => {
                        if let Some(module) = module_value {
                            output.extend_from_slice(module.as_bytes());
                            output.extend_from_slice(b"::");
                        }
                    }
                }
                output.extend_from_slice(value.as_bytes());
            }
        }

        output.push(b'\n');
    }
    ///
    /// Escapes backslashes, forward slashes, and the regex anchors `^`/`$` in
    /// the address field
    ///
    /// # Arguments
    ///
    /// * `address` - The address string to escape
    ///
    /// # Returns
    ///
    /// A new string with the regex significant characters escaped
    #[cfg(test)]
    fn escape_address(address: &str) -> String {
        let mut out = String::with_capacity(address.len() + 8);
        Self::escape_address_into(address, &mut out);
        out
    }

    /// Builds a regex search-pattern address from a raw source line.
    pub(crate) fn address_from_line(line: &[u8]) -> String {
        let line = String::from_utf8_lossy(line);
        let mut address = String::with_capacity(line.len() + 16);
        address.push_str("/^");
        let prefix_len = address.len();
        Self::escape_address_into(&line, &mut address);

        if address.len() - prefix_len > 96 {
            let limit = prefix_len + 96;
            let at = (prefix_len..=limit)
                .rev()
                .find(|&i| address.is_char_boundary(i))
                .unwrap_or(prefix_len);
            address.truncate(at);
            address.push_str("/;\""); // No '$' anchor when truncated.
        } else {
            address.push_str("$/;\"");
        }
        address
    }

    /// Appends `address` to `out`, escaping the characters that are significant
    /// in a regex search-pattern address: backslash, forward slash, and the
    /// regex anchors `^` and `$`.
    pub(crate) fn escape_address_into(address: &str, out: &mut String) {
        for ch in address.chars() {
            match ch {
                '\\' => out.push_str("\\\\"),
                '/' => out.push_str("\\/"),
                '^' => out.push_str("\\^"),
                '$' => out.push_str("\\$"),
                _ => out.push(ch),
            }
        }
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
    let mut parts = line.split('\t');
    let name = parts.next()?;
    let file_name = parts.next()?;
    let address = parts.next()?;

    // Extract the kind if available (typically in the extension fields)
    let mut kind = None;
    let mut fields_map = ExtensionFields::new();

    // Process extension fields (everything after the address)
    for field in parts {
        // Skip empty fields
        if field.is_empty() {
            continue;
        }

        // Handle both cases: with "key:value" format and standalone kind value
        if let Some(colon_pos) = field.find(':') {
            let key = field[..colon_pos].trim();
            let value = field[colon_pos + 1..].trim();

            // Store the kind separately if it's the "kind" field
            if key == "kind" {
                kind = Some(Cow::Owned(value.to_string()));
            } else {
                fields_map.insert(key.to_string(), value.to_string());
            }
        } else if kind.is_none() {
            // In ctags, a standalone field without colon is typically the kind
            // Only use the first such field as the kind
            kind = Some(Cow::Owned(field.to_string()));
        }
    }

    Some(Tag {
        name: name.to_string(),
        file_name: Arc::from(file_name),
        address: format!("{}\t", address), // Keep the tab as in the original code
        kind,
        extension_fields: if fields_map.is_empty() {
            None
        } else {
            Some(fields_map)
        },
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
        assert_eq!(&*tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^pub fn function_name() {/;\"\t");
        assert_eq!(tag.kind.as_deref(), Some("f"));

        let extension_fields = tag.extension_fields.unwrap();
        assert!(!extension_fields.contains_key("kind"));
        assert_eq!(extension_fields.get("line").unwrap(), "10");
    }

    #[test]
    fn test_parse_tag_line_with_explicit_kind() {
        let line = "method\tfile.rs\t/^pub fn method(&self) {/;\"\tkind:m\taccess:public\tline:42";
        let tag = parse_tag_line(line).unwrap();

        assert_eq!(tag.name, "method");
        assert_eq!(&*tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^pub fn method(&self) {/;\"\t");
        assert_eq!(tag.kind.as_deref(), Some("m"));

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
        assert_eq!(&*tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^let variable = 42;/;\"\t");
        assert_eq!(tag.kind, None);
        assert_eq!(tag.extension_fields, None);
    }

    #[test]
    fn test_parse_tag_line_kind_without_prefix() {
        let line = "struct_name\tfile.rs\t/^pub struct struct_name {/;\"\ts";
        let tag = parse_tag_line(line).unwrap();

        assert_eq!(tag.name, "struct_name");
        assert_eq!(&*tag.file_name, "file.rs");
        assert_eq!(tag.address, "/^pub struct struct_name {/;\"\t");
        assert_eq!(tag.kind.as_deref(), Some("s"));
        assert_eq!(tag.extension_fields, None);
    }

    #[test]
    fn test_parse_tag_line_invalid() {
        let line = "not_enough_fields";
        assert!(parse_tag_line(line).is_none());
    }

    // Tests for `bytes`
    #[test]
    fn test_bytes_basic() {
        let tag = Tag {
            name: "test_function".to_string(),
            file_name: "test.rs".into(),
            address: "/^fn test_function() {$/".to_string(),
            kind: Some("function".into()),
            extension_fields: None,
        };

        let expected = "test_function\ttest.rs\t/^fn test_function() {$/\tfunction\n";
        assert_eq!(String::from_utf8(tag.bytes()).unwrap(), expected);
    }

    #[test]
    fn test_bytes_no_kind() {
        let tag = Tag {
            name: "TEST_CONSTANT".to_string(),
            file_name: "constants.rs".into(),
            address: "/^const TEST_CONSTANT: i32 = 42;$/".to_string(),
            kind: None,
            extension_fields: None,
        };

        let expected = "TEST_CONSTANT\tconstants.rs\t/^const TEST_CONSTANT: i32 = 42;$/\n";
        assert_eq!(String::from_utf8(tag.bytes()).unwrap(), expected);
    }

    #[test]
    fn test_bytes_with_module_only() {
        let mut extension_fields = ExtensionFields::new();
        extension_fields.insert("module".to_string(), "example".to_string());

        let tag = Tag {
            name: "Model".to_string(),
            file_name: "model.rs".into(),
            address: "/^struct Model {$/".to_string(),
            kind: Some("struct".into()),
            extension_fields: Some(extension_fields),
        };

        let expected = "Model\tmodel.rs\t/^struct Model {$/\tstruct\tmodule:example\n";
        assert_eq!(String::from_utf8(tag.bytes()).unwrap(), expected);
    }

    #[test]
    fn test_bytes_with_non_module_field() {
        let mut extension_fields = ExtensionFields::new();
        extension_fields.insert("implementation".to_string(), "Circle".to_string());

        let tag = Tag {
            name: "draw".to_string(),
            file_name: "shapes.rs".into(),
            address: "/^fn draw(&self) {$/".to_string(),
            kind: Some("method".into()),
            extension_fields: Some(extension_fields),
        };

        let expected = "draw\tshapes.rs\t/^fn draw(&self) {$/\tmethod\timplementation:Circle\n";
        assert_eq!(String::from_utf8(tag.bytes()).unwrap(), expected);
    }

    #[test]
    fn test_bytes_with_module_and_implementation() {
        let mut extension_fields = ExtensionFields::new();
        extension_fields.insert("implementation".to_string(), "Circle".to_string());
        extension_fields.insert("module".to_string(), "example".to_string());

        let tag = Tag {
            name: "draw".to_string(),
            file_name: "shapes.rs".into(),
            address: "/^fn draw(&self) {$/".to_string(),
            kind: Some("method".into()),
            extension_fields: Some(extension_fields),
        };

        // Module should be prepended to the implementation value and module key should not appear
        let expected =
            "draw\tshapes.rs\t/^fn draw(&self) {$/\tmethod\timplementation:example::Circle\n";
        assert_eq!(String::from_utf8(tag.bytes()).unwrap(), expected);
    }

    #[test]
    fn test_bytes_with_module_and_trait() {
        let mut extension_fields = ExtensionFields::new();
        extension_fields.insert("trait".to_string(), "Shape".to_string());
        extension_fields.insert("module".to_string(), "example".to_string());

        let tag = Tag {
            name: "area".to_string(),
            file_name: "traits.rs".into(),
            address: "/^fn area(&self) -> f64 {$/".to_string(),
            kind: Some("method".into()),
            extension_fields: Some(extension_fields),
        };

        // Module should be prepended to the trait value and module key should not appear
        let expected =
            "area\ttraits.rs\t/^fn area(&self) -> f64 {$/\tmethod\ttrait:example::Shape\n";
        assert_eq!(String::from_utf8(tag.bytes()).unwrap(), expected);
    }

    #[test]
    fn test_bytes_with_multiple_extension_fields() {
        let mut extension_fields = ExtensionFields::new();
        extension_fields.insert("trait".to_string(), "Shape".to_string());
        extension_fields.insert("implementation".to_string(), "Circle".to_string());
        extension_fields.insert("module".to_string(), "geometry".to_string());

        let tag = Tag {
            name: "calculate".to_string(),
            file_name: "geometry.rs".into(),
            address: "/^fn calculate(&self) -> f64 {$/".to_string(),
            kind: Some("method".into()),
            extension_fields: Some(extension_fields),
        };

        // Module should be prepended to all other fields and module key should not appear
        let bytes = tag.bytes();
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
    fn test_bytes_with_no_extension_fields() {
        let tag = Tag {
            name: "MyEnum".to_string(),
            file_name: "types.rs".into(),
            address: "/^enum MyEnum {$/".to_string(),
            kind: Some("enum".into()),
            extension_fields: Some(ExtensionFields::new()), // Empty ExtensionFields
        };

        let expected = "MyEnum\ttypes.rs\t/^enum MyEnum {$/\tenum\n";
        assert_eq!(String::from_utf8(tag.bytes()).unwrap(), expected);
    }

    #[test]
    fn test_escape_address() {
        assert_eq!(
            Tag::escape_address("/^fn test() {$/"),
            "\\/\\^fn test() {\\$\\/"
        );
        assert_eq!(Tag::escape_address("\\n\\t"), "\\\\n\\\\t");
        assert_eq!(
            Tag::escape_address("/path/to/file\\with\\backslashes"),
            "\\/path\\/to\\/file\\\\with\\\\backslashes"
        );
        assert_eq!(Tag::escape_address("no_special_chars"), "no_special_chars");
    }
}
