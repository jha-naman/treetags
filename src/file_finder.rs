//! Module for finding files and tag files in the filesystem.
//!
//! This module provides functionality to search for tag files,
//! recursively scan directories for source files, and apply
//! file exclusion patterns.

use crate::shell_to_regex;
use crate::tag::{parse_tag_file as parse_tags, Tag};
use regex::RegexSet;
use std::fs::File;
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
/// FileFinder recursively explores directories and filters files
/// based on exclude patterns provided in the configuration.
pub struct FileFinder {
    /// The root directory path to search
    dir_path: PathBuf,

    /// A set of regular expressions for file exclusion
    exclude_patterns: RegexSet,
}

impl FileFinder {
    /// Creates a new FileFinder instance from exclude patterns.
    ///
    /// # Arguments
    ///
    /// * `tag_file_path` - Path to the tag file, used to determine the root directory
    /// * `exclude_patterns` - Shell-style patterns for files to exclude
    ///
    /// # Returns
    ///
    /// A Result containing a new FileFinder instance or an error message
    pub fn from_patterns(
        tag_file_path: &Path,
        exclude_patterns: Vec<String>,
    ) -> Result<Self, String> {
        let dir_path = if tag_file_path.to_str() == Some("-") {
            // If writing to stdout, use current directory as the root
            std::env::current_dir()
                .map_err(|e| format!("Failed to access current directory: {}", e))?
        } else {
            tag_file_path
                .parent()
                .ok_or_else(|| "Failed to access tag file's parent directory".to_string())?
                .to_path_buf()
        };

        let exclude_regexes = exclude_patterns
            .iter()
            .map(|pattern| shell_to_regex::shell_to_regex(pattern))
            .collect::<Vec<_>>();

        let exclude_patterns = RegexSet::new(exclude_regexes)
            .map_err(|e| format!("Failed to compile exclude patterns: {}", e))?;

        Ok(Self {
            dir_path,
            exclude_patterns,
        })
    }

    /// Recursively finds all files in the directory that don't match exclude patterns.
    ///
    /// # Returns
    ///
    /// A FileFinderResult containing found files and any errors encountered
    pub fn get_files_from_dir(&self) -> FileFinderResult {
        let dir_path = match if self.dir_path.to_str() == Some("-") {
            // If writing to stdout, use current directory as the root
            std::env::current_dir()
        } else {
            Ok(self.dir_path.clone())
        } {
            Ok(path) => path,
            Err(e) => {
                let mut result = FileFinderResult::new();
                result
                    .errors
                    .push(format!("Failed to access current directory: {}", e));
                return result;
            }
        };
        self.scan_directory(&dir_path)
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
                // If it's a directory, recursively find all files
                let dir_result = self.scan_directory(path);
                result.files.extend(dir_result.files);
                result.errors.extend(dir_result.errors);
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

/// Determines the path to the tag file based on configuration.
///
/// # Arguments
///
/// * `tag_file_name` - Name of the tag file
/// * `append` - If true, search for an existing tag file; otherwise create a new path
///
/// # Returns
///
/// A Result containing either the tag file path or an error message
pub fn determine_tag_file_path(tag_file_name: &str, append: bool) -> Result<String, String> {
    if tag_file_name == "-" {
        return Ok("-".to_string());
    }

    match find_tag_file(tag_file_name) {
        Ok(tag_file) => Ok(tag_file),
        Err(_) => {
            if append {
                Err(format!("Could not find hte tag file: {}", tag_file_name))
            } else {
                Ok(std::env::current_dir()
                    .map_err(|e| format!("Failed to get current directory: {}", e))?
                    .join(tag_file_name)
                    .to_string_lossy()
                    .into_owned())
            }
        }
    }
}

/// Searches for a tag file in the current directory and its parents.
///
/// # Arguments
///
/// * `filename` - Name of the tag file to search for
///
/// # Returns
///
/// Result containing the path to the tag file if found, or an error message if not found
pub fn find_tag_file(filename: &str) -> Result<String, String> {
    let mut current_dir =
        std::env::current_dir().map_err(|e| format!("Failed to get current directory: {}", e))?;

    // Check if the file exists in the current directory
    match File::open(current_dir.join(filename)) {
        Ok(_) => return Ok(current_dir.join(filename).to_string_lossy().into_owned()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // File not found, continue searching in parent directories
        }
        Err(e) => {
            return Err(format!(
                "Failed to open tag file '{}' in directory '{}': {}",
                filename,
                current_dir.display(),
                e
            ));
        }
    }

    // Check parent directories
    while let Some(parent) = current_dir.parent() {
        current_dir = parent.to_path_buf();
        match File::open(current_dir.join(filename)) {
            Ok(_) => return Ok(current_dir.join(filename).to_string_lossy().into_owned()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File not found, continue searching in parent directories
                continue;
            }
            Err(e) => {
                return Err(format!(
                    "Failed to open tag file '{}' in directory '{}': {}",
                    filename,
                    current_dir.display(),
                    e
                ));
            }
        }
    }

    Err(format!(
        "Tag file '{}' not found in current directory or any parent directory",
        filename
    ))
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
