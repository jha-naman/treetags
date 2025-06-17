use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TestCase {
    pub _name: String,
    pub input_dir: PathBuf,
    pub expected_dir: PathBuf,
}

impl TestCase {
    pub fn new(_name: String, input_dir: PathBuf, expected_dir: PathBuf) -> Self {
        Self {
            _name,
            input_dir,
            expected_dir,
        }
    }
}
