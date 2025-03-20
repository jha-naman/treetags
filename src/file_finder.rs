//! Module for finding files and tag files in the filesystem.
//!
//! This module provides functionality to search for tag files,
//! recursively scan directories for source files, and apply
//! file exclusion patterns.

use regex::RegexSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use treetags::{parse_tag_file as parse_tags, Tag};
use walkdir::WalkDir;

use crate::shell_to_regex;

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
    /// Creates a new FileFinder instance.
    ///
    /// # Arguments
    ///
    /// * `tag_file_path` - Path to the tag file, used to determine the root directory
    /// * `exclude_patterns` - Shell-style patterns for files to exclude
    ///
    /// # Returns
    ///
    /// A new FileFinder instance configured with the given parameters
    pub fn new(tag_file_path: &Path, exclude_patterns: Vec<String>) -> Self {
        let dir_path = tag_file_path
            .parent()
            .expect("Failed to access tag file's parent directory")
            .to_path_buf();

        let exclude_regexes = exclude_patterns
            .iter()
            .map(|pattern| shell_to_regex::shell_to_regex(pattern))
            .collect::<Vec<_>>();

        let exclude_patterns =
            RegexSet::new(exclude_regexes).expect("Failed to compile exclude patterns");

        Self {
            dir_path,
            exclude_patterns,
        }
    }

    /// Recursively finds all files in the directory that don't match exclude patterns.
    ///
    /// # Returns
    ///
    /// A vector of file paths as strings
    pub fn get_files_from_dir(&self) -> Vec<String> {
        self.scan_directory(&self.dir_path)
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
    /// A vector of file paths as strings
    pub fn get_files_from_paths(&self, paths: &[String]) -> Vec<String> {
        let mut file_names = Vec::new();

        for path_str in paths {
            let path = Path::new(path_str);

            if path.is_file() {
                // If it's a file, add it directly
                file_names.push(path_str.clone());
            } else if path.is_dir() {
                // If it's a directory, recursively find all files
                file_names.extend(self.scan_directory(path));
            } else {
                // Path doesn't exist or is inaccessible, warn but continue
                eprintln!("Warning: Path not found or inaccessible: {}", path_str);
            }
        }

        file_names
    }

    /// Helper method to scan a directory for files, applying exclusion filters.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - The directory path to scan
    ///
    /// # Returns
    ///
    /// A vector of file paths as strings
    fn scan_directory(&self, dir_path: &Path) -> Vec<String> {
        let mut file_names = Vec::new();
        let walker = WalkDir::new(dir_path).into_iter();

        for entry in walker
            .filter_entry(|e| {
                let path_str = e.path().to_str().unwrap_or("");
                !self.exclude_patterns.is_match(path_str)
            })
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            if let Some(path_str) = entry.path().to_str() {
                file_names.push(path_str.to_string());
            }
        }

        file_names
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
    match find_tag_file(tag_file_name) {
        Some(tag_file) => Ok(tag_file),
        None => {
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
/// Option containing the path to the tag file if found, None otherwise
pub fn find_tag_file(filename: &str) -> Option<String> {
    let mut current_dir = std::env::current_dir().ok()?;

    // Check if the file exists in the current directory
    if File::open(current_dir.join(filename)).is_ok() {
        return Some(current_dir.join(filename).to_string_lossy().into_owned());
    }

    // Check parent directories
    while let Some(parent) = current_dir.parent() {
        current_dir = parent.to_path_buf();
        if File::open(current_dir.join(filename)).is_ok() {
            return Some(current_dir.join(filename).to_string_lossy().into_owned());
        }
    }

    None
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
