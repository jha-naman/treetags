use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() {
    let generated_tests_dir = Path::new("tests/generated");

    // Clean existing generated tests directory
    if generated_tests_dir.exists() {
        fs::remove_dir_all(generated_tests_dir).unwrap();
    }

    // Create the generated tests directory
    fs::create_dir_all(generated_tests_dir).unwrap();

    let test_cases = discover_test_cases();

    // Generate individual test files
    for test_case in test_cases.iter() {
        generate_individual_test_file(generated_tests_dir, test_case);
    }

    // Generate main.rs to include all test modules
    generate_main_file(generated_tests_dir, &test_cases);

    println!("cargo:rerun-if-changed=tests/test_cases");
}

#[derive(Debug, Clone)]
struct TestCase {
    name: String,
    input_dir: PathBuf,
    expected_dir: PathBuf,
}

fn discover_test_cases() -> Vec<TestCase> {
    let test_cases_dir = Path::new("tests/test_cases");

    WalkDir::new(test_cases_dir)
        .min_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .filter_map(|entry| create_test_case_from_directory(entry.path(), test_cases_dir))
        .collect()
}

fn create_test_case_from_directory(test_dir: &Path, test_cases_dir: &Path) -> Option<TestCase> {
    let input_dir = test_dir.join("input");
    let expected_dir = test_dir.join("expected");

    if !input_dir.exists() || !input_dir.join("args.txt").exists() {
        return None;
    }

    if !expected_dir.exists() {
        return None;
    }

    let test_name = test_dir
        .strip_prefix(test_cases_dir)
        .ok()?
        .to_string_lossy()
        .replace(['/', '\\'], "_");

    Some(TestCase {
        name: test_name,
        input_dir,
        expected_dir,
    })
}

fn generate_individual_test_file(tests_dir: &Path, test_case: &TestCase) {
    let test_name = sanitize_test_name(&test_case.name);
    let test_file_path = tests_dir.join(format!("{}.rs", test_name));

    let test_content = format!(
        r#"//! Auto-generated test for: {}

use std::path::PathBuf;

#[path = "../helpers/mod.rs"]
mod helpers;

use helpers::{{
    test_runner::TestCase,
    golden_test_runner::run_test_case,
}};

#[test]
fn test_{}() {{
    let test_case = TestCase::new(
        "{}".to_string(),
        PathBuf::from("{}"),
        PathBuf::from("{}")
    );

    if let Err(error) = run_test_case(&test_case) {{
        panic!("Test '{}' failed: {{}}", error);
    }}
}}
"#,
        test_case.name,
        test_name,
        test_case.name,
        test_case.input_dir.display(),
        test_case.expected_dir.display(),
        test_case.name
    );

    fs::write(&test_file_path, test_content).unwrap();
}

fn generate_main_file(tests_dir: &Path, test_cases: &[TestCase]) {
    let main_file_path = tests_dir.join("main.rs");

    let mut main_content = String::from("//! Auto-generated main file for integration tests\n\n");

    // Add mod declarations for each test case
    for test_case in test_cases {
        let test_name = sanitize_test_name(&test_case.name);
        main_content.push_str(&format!("mod {};\n", test_name));
    }

    fs::write(&main_file_path, main_content).unwrap();
}

fn sanitize_test_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}
