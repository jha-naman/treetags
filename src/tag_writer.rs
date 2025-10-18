// src/tag_writer.rs

//! Module for writing tag data to files.
//!
//! This module handles sorting and writing tags to the output file or standard output.

use crate::tag::Tag;
use std::fs::File;
use std::io::{self, BufWriter, Write};

/// A structure for writing tags to a file.
///
/// TagWriter handles sorting tags and writing them to the output file.
pub struct TagWriter {
    /// Path to the output tag file
    file_path: String,
}

impl TagWriter {
    /// Creates a new TagWriter instance.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the output tag file
    ///
    /// # Returns
    ///
    /// A new TagWriter instance
    pub fn new(file_path: String) -> Self {
        Self { file_path }
    }

    /// Writes a collection of tags to the output file.
    ///
    /// This method first sorts the tags by name and then writes them
    /// to the specified file.
    /// If file_path is "-", tags are written to standard output instead.
    ///
    /// # Arguments
    ///
    /// * `tags` - A mutable reference to a vector of tags to write
    pub fn write_tags(&self, tags: &mut Vec<Tag>, emit_pseudo_tags: bool, sorted: bool) {
        // Create a buffered writer for either stdout or a file
        let mut writer: Box<dyn Write> = if self.file_path == "-" {
            // Write to stdout
            Box::new(BufWriter::new(io::stdout()))
        } else {
            // Open file for writing
            let file = match File::create(&self.file_path) {
                Ok(file) => file,
                Err(e) => {
                    eprintln!("Failed to create tag file: {}", e);
                    return;
                }
            };

            Box::new(BufWriter::new(file))
        };

        if emit_pseudo_tags {
            let s = format!(
                "!_TAG_FILE_SORTED\t{}\t/0=unsorted, 1=sorted/\n",
                if sorted { 1 } else { 0 }
            )
            .into_bytes();
            if let Err(e) = writer.write_all(&s) {
                eprintln!("Failed to write pseudo tag: {}", e);
            }
        }

        // Write tags to file
        for tag in tags {
            if let Err(e) = writer.write_all(&tag.bytes()) {
                eprintln!("Failed to write tag: {}", e);
            }
        }
    }
}
