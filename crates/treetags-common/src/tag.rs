use indexmap::IndexMap;

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
    pub extension_fields: Option<IndexMap<String, String>>,
}

impl Tag {
    /// Creates a new `Tag` from a tree-sitter tag and source code
    pub fn from_ts_tag(
        tag: tree_sitter_tags::Tag,
        code: &[u8],
        file_path: &str,
    ) -> Result<Self, String> {
        let name = String::from_utf8(code[tag.name_range.start..tag.name_range.end].to_vec())
            .map_err(|e| {
                format!(
                    "Failed to decode tag name as UTF-8 in file '{}': {}",
                    file_path, e
                )
            })?;

        let line_content = String::from_utf8(
            code[(tag.name_range.start - tag.span.start.column)..tag.line_range.end].to_vec(),
        )
        .map_err(|e| {
            format!(
                "Failed to decode line content as UTF-8 in file '{}': {}",
                file_path, e
            )
        })?;

        let mut escaped_line = Self::escape_address(&line_content);

        // Truncate line_content to 96 characters maximum
        let address = if escaped_line.len() > 96 {
            escaped_line.truncate(96);
            format!("/^{}/;\"\t", escaped_line) // No '$' if truncated
        } else {
            format!("/^{}$/;\"\t", escaped_line)
        };

        Ok(Tag {
            name,
            file_name: String::from(file_path),
            address,
            kind: None,
            extension_fields: None,
        })
    }

    /// Converts the tag into a byte representation suitable for writing to a tags file
    pub fn bytes(&self) -> Vec<u8> {
        let mut output = format!("{}\t{}\t{}", self.name, self.file_name, self.address);

        // Only output shorthand kind if we don't have extension fields with a kind field
        let has_kind_extension = self
            .extension_fields
            .as_ref()
            .map(|fields| fields.contains_key("kind"))
            .unwrap_or(false);

        if let Some(ref kind) = self.kind {
            if !has_kind_extension {
                output.push_str(&format!("\t{}", kind));
            }
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
                // Only prepend module value for scope-related fields, not metadata fields
                let formatted_value = match key.as_str() {
                    "line" | "end" | "kind" | "file" | "signature" | "access" => {
                        // These fields should never have module prefixes
                        value.clone()
                    }
                    _ => {
                        // For scope-related fields, prepend module value if it exists
                        if let Some(module) = module_value {
                            format!("{}::{}", module, value)
                        } else {
                            value.clone()
                        }
                    }
                };
                output.push_str(&format!("\t{}:{}", key, formatted_value));
            }
        }

        output.push('\n');
        output.into_bytes()
    }

    /// Escapes backslashes and forward slashes in the address field
    pub fn escape_address(address: &str) -> String {
        address.replace('\\', "\\\\").replace('/', "\\/")
    }
}
