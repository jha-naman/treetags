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
    pub fn write_tags(&self, tags: &mut Vec<Tag>) {
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

        // Write tags to file
        for tag in tags {
            if let Err(e) = writer.write_all(&tag.bytes()) {
                eprintln!("Failed to write tag: {}", e);
            }
        }
    }
}
