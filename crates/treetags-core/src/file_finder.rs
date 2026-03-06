//! Module for finding files and tag files in the filesystem.
//!
//! This module provides functionality to search for tag files,
//! recursively scan directories for source files, and apply
//! file exclusion patterns.

use crate::shell_to_regex;
use crate::tag::{parse_tag_file as parse_tags, Tag};
use regex::RegexSet;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Result type for file finding operations that can have partial failures.
///
/// This allows operations to continue even when some individual files or directories
/// cannot be accessed, while still reporting what went wrong.
pub struct FileFinderResult {
    /// Successfully found files
    pub files: Vec<String>,
    /// Errors encountered during the operation
    pub errors: Vec<String>,
}

impl FileFinderResult {
    /// Creates a new result with empty files and errors
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Returns true if any errors were encountered
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Prints all errors to stderr as warnings
    pub fn print_errors(&self) {
        for error in &self.errors {
            eprintln!("Warning: {}", error);
        }
    }
}

/// A structure for finding and filtering files in a directory.
///
/// FileFinder explores directories and filters files
/// based on exclude patterns provided in the configuration.
pub struct FileFinder {
    /// A set of regular expressions for file exclusion
    exclude_patterns: RegexSet,

    /// Whether to recurse into directories
    recurse: bool,
}

impl FileFinder {
    /// Creates a new FileFinder instance from exclude patterns.
    ///
    /// # Arguments
    ///
    /// * `exclude_patterns` - Shell-style patterns for files to exclude
    /// * `recurse` - Whether to recurse into directories
    ///
    /// # Returns
    ///
    /// A Result containing a new FileFinder instance or an error message
    pub fn from_patterns(exclude_patterns: Vec<String>, recurse: bool) -> Result<Self, String> {
        let exclude_regexes = exclude_patterns
            .iter()
            .map(|pattern| shell_to_regex::shell_to_regex(pattern))
            .collect::<Vec<_>>();

        let exclude_patterns = RegexSet::new(exclude_regexes)
            .map_err(|e| format!("Failed to compile exclude patterns: {}", e))?;

        Ok(Self {
            exclude_patterns,
            recurse,
        })
    }

    /// Processes a list of files and directories, expanding any directories
    /// to include all files contained within them.
    ///
    /// # Arguments
    ///
    /// * `paths` - A list of file and directory paths to process
    ///
    /// # Returns
    ///
    /// A FileFinderResult containing found files and any errors encountered
    pub fn get_files_from_paths(&self, paths: &[String]) -> FileFinderResult {
        let mut result = FileFinderResult::new();

        for path_str in paths {
            let path = Path::new(path_str);

            if path.is_file() {
                // If it's a file, add it directly
                result.files.push(path_str.clone());
            } else if path.is_dir() {
                if self.recurse {
                    // If recursion is enabled, scan the directory
                    let dir_result = self.scan_directory(path);
                    result.files.extend(dir_result.files);
                    result.errors.extend(dir_result.errors);
                } else {
                    // Without recursion, skip directories
                    result.errors.push(format!(
                        "Skipping directory '{}' (use -R to recurse into directories)",
                        path_str
                    ));
                }
            } else {
                // Path doesn't exist or is inaccessible, record error but continue
                result
                    .errors
                    .push(format!("Path not found or inaccessible: {}", path_str));
            }
        }

        result
    }

    /// Helper method to scan a directory for files, applying exclusion filters.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - The directory path to scan
    ///
    /// # Returns
    ///
    /// A FileFinderResult containing found files and any errors encountered
    fn scan_directory(&self, dir_path: &Path) -> FileFinderResult {
        let mut result = FileFinderResult::new();
        let walker = WalkDir::new(dir_path).into_iter();

        for entry in walker {
            match entry {
                Ok(entry) => {
                    // Check if path should be excluded
                    let path_str = entry.path().to_str().unwrap_or("");
                    if self.exclude_patterns.is_match(path_str) {
                        continue;
                    }

                    // Only process files
                    if entry.file_type().is_file() {
                        if let Some(path_str) = entry.path().to_str() {
                            result.files.push(path_str.to_string());
                        } else {
                            result.errors.push(format!(
                                "Failed to convert path to string: {}",
                                entry.path().display()
                            ));
                        }
                    }
                }
                Err(e) => {
                    result.errors.push(format!("Failed to access path: {}", e));
                }
            }
        }

        result
    }
}

/// Validates that a file is a proper tags file by checking its first line.
///
/// # Arguments
///
/// * `path` - Path to the tag file to validate
///
/// # Returns
///
/// A Result indicating whether the file is valid or an error message
fn validate_existing_tag_file(path: &str) -> Result<(), String> {
    let file =
        fs::File::open(path).map_err(|e| format!("Cannot read tag file '{}': {}", path, e))?;

    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF reached - empty file is valid
                return Ok(());
            }
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    // Validate the first non-empty line
                    return validate_tag_line(trimmed, path);
                }
            }
            Err(e) => {
                return Err(format!("Error reading tag file '{}': {}", path, e));
            }
        }
    }
}

/// Validates that a line looks like a valid tag entry.
///
/// # Arguments
///
/// * `line` - The line to validate
/// * `path` - Path to the tag file (for error messages)
///
/// # Returns
///
/// A Result indicating whether the line is valid or an error message
fn validate_tag_line(line: &str, path: &str) -> Result<(), String> {
    if line.starts_with("!_TAG_") {
        return Ok(());
    }

    let parts: Vec<&str> = line.split('\t').collect();
    if parts.len() >= 3 {
        return Ok(());
    }

    Err(format!(
        "Tag file '{}' doesn't look like a tags file (first line: '{}')",
        path, line
    ))
}

/// Determines the path to the tag file based on configuration.
///
/// # Arguments
///
/// * `tag_file_name` - Name of the tag file (default is "tags")
/// * `append` - If true, tags are added to tags file
///
/// # Returns
///
/// A Result containing either the tag file path or an error message
pub fn determine_tag_file_path(tag_file_name: &str, append: bool) -> Result<String, String> {
    // Handle stdout output
    if tag_file_name == "-" {
        return Ok("-".to_string());
    }

    if tag_file_name.len() > 1 && tag_file_name.starts_with('-') {
        return Err(format!(
            "Refusing to use '{}' as tag file name (begins with '-'). Use absolute path or add path separator if you really want this name. eg './{}'",
            tag_file_name, tag_file_name
        ));
    }

    let tag_file_path = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?
        .join(tag_file_name)
        .to_string_lossy()
        .into_owned();

    if append {
        if Path::new(&tag_file_path).exists() {
            if let Err(e) = validate_existing_tag_file(&tag_file_path) {
                return Err(e);
            }
        } else {
            fs::File::create(&tag_file_path)
                .map_err(|e| format!("Failed to create tag file '{}': {}", tag_file_path, e))?;
        }
    }

    Ok(tag_file_path)
}

/// Parses an existing tag file and returns its tags.
///
/// # Arguments
///
/// * `path` - Path to the tag file
///
/// # Returns
///
/// A vector of parsed tags
pub fn parse_tag_file(path: &str) -> Vec<Tag> {
    parse_tags(&PathBuf::from(path))
}
