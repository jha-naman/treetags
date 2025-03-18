use std::fs::File;
use std::io::{BufWriter, Write};
use treetags::Tag;

pub struct TagWriter {
    file_path: String,
}

impl TagWriter {
    pub fn new(file_path: String) -> Self {
        Self { file_path }
    }

    pub fn write_tags(&self, tags: &mut Vec<Tag>) {
        // Sort tags by name
        tags.sort_by(|a, b| a.name.cmp(&b.name));

        // Open file for writing
        let file = match File::create(&self.file_path) {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Failed to create tag file: {}", e);
                return;
            }
        };

        let mut writer = BufWriter::new(file);

        // Write tags to file
        for tag in tags {
            if let Err(e) = writer.write_all(&tag.into_bytes()) {
                eprintln!("Failed to write tag: {}", e);
            }
        }
    }
}
