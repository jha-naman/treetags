use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, PartialEq)]
pub struct Tag {
    pub name: String,
    pub file_name: String,
    pub address: String,
}

impl Tag {
    pub fn new(tag: tree_sitter_tags::Tag, code: &Vec<u8>, file_path: &str) -> Self {
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
        }
    }

    pub fn into_bytes(&self) -> Vec<u8> {
        format!("{}\t{}\t{}\n", self.name, self.file_name, self.address).into_bytes()
    }
}

pub fn parse_tag_file(tag_file_path: &Path) -> Vec<Tag> {
    let file = File::open(tag_file_path).expect("Failed to read the tags file");
    let reader = BufReader::new(file);
    let mut tags = Vec::new();

    for line in reader.lines() {
        if let Ok(line) = line {
            let mut parts = line.split('\t');
            if let Some(name) = parts.next() {
                if let Some(file_name) = parts.next() {
                    if let Some(address) = parts.next() {
                        tags.push(Tag {
                            name: name.to_string(),
                            file_name: file_name.to_string(),
                            address: format!("{}\t", address.to_string()),
                        });
                    }
                }
            }
        }
    }

    tags
}
