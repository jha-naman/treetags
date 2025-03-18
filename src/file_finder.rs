use regex::RegexSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use treetags::{parse_tag_file as parse_tags, Tag};
use walkdir::WalkDir;

use crate::shell_to_regex;

pub struct FileFinder {
    dir_path: PathBuf,
    exclude_patterns: RegexSet,
}

impl FileFinder {
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

    pub fn get_files_from_dir(&self) -> Vec<String> {
        let mut file_names = Vec::new();
        let walker = WalkDir::new(&self.dir_path).into_iter();

        for entry in walker
            .filter_entry(|e| {
                let path_str = e.path().to_str().unwrap_or("");
                !self.exclude_patterns.is_match(path_str)
            })
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }

            if let Some(path_str) = entry.path().to_str() {
                file_names.push(path_str.to_string());
            }
        }

        file_names
    }
}

/// Determines the path to the tag file based on configuration.
///
/// If `append` is true, it will search for an existing tag file.
/// Otherwise, it will create a new path in the current directory.
pub fn determine_tag_file_path(tag_file_name: &str, append: bool) -> Result<String, String> {
    if append {
        find_tag_file(tag_file_name)
            .ok_or_else(|| format!("Could not find the tag file: {}", tag_file_name))
    } else {
        Ok(std::env::current_dir()
            .map_err(|e| format!("Failed to get current directory: {}", e))?
            .join(tag_file_name)
            .to_string_lossy()
            .into_owned())
    }
}

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

pub fn parse_tag_file(path: &str) -> Vec<Tag> {
    parse_tags(&PathBuf::from(path))
}
