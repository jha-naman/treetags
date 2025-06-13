use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub input_dir: PathBuf,
    pub expected_dir: PathBuf,
}

#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub success: bool,
    pub error: Option<String>,
}

impl TestCase {
    pub fn new(name: String, input_dir: PathBuf, expected_dir: PathBuf) -> Self {
        Self {
            name,
            input_dir,
            expected_dir,
        }
    }
}

impl TestResult {
    pub fn success(name: String) -> Self {
        Self {
            name,
            success: true,
            error: None,
        }
    }

    pub fn failure(name: String, error: String) -> Self {
        Self {
            name,
            success: false,
            error: Some(error),
        }
    }
}
