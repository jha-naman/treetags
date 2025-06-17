use std::fs;
use std::path::Path;

/// Functional utility for reading file content
pub fn read_file_content(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Failed to read file {:?}: {}", path, e))
}

/// Functional utility for reading optional file content
pub fn read_optional_file(dir: &Path, filename: &str) -> Result<Option<String>, String> {
    let file_path = dir.join(filename);
    if file_path.exists() {
        read_file_content(&file_path).map(Some)
    } else {
        Ok(None)
    }
}

/// Parse exit code from file content, defaulting to 0
pub fn parse_exit_code(content: Option<String>) -> Result<i32, String> {
    content
        .unwrap_or_else(|| "0".to_string())
        .trim()
        .parse()
        .map_err(|e| format!("Invalid exit code format: {}", e))
}

/// Normalize output by trimming whitespace and standardizing line endings
pub fn normalize_output(output: &str) -> String {
    output
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// Parse command line arguments from file content
pub fn parse_args(content: &str) -> Result<Vec<String>, String> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            shlex::split(line).ok_or_else(|| format!("Failed to parse arguments: {}", line))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|args_vec| args_vec.into_iter().flatten().collect())
}
