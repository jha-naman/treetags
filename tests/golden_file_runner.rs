use assert_cmd::prelude::*;
use similar::{ChangeTag, TextDiff};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use walkdir::WalkDir;

mod helpers;
use helpers::{
    file_utils::{
        normalize_output, parse_args, parse_exit_code, read_file_content, read_optional_file,
    },
    test_runner::{TestCase, TestResult},
};

#[test]
fn test_all_golden_files() {
    let test_cases_dir = Path::new("tests/test_cases");

    if !test_cases_dir.exists() {
        panic!("Test cases directory not found: {:?}", test_cases_dir);
    }

    let test_cases = discover_test_cases(test_cases_dir);

    if test_cases.is_empty() {
        panic!("No test cases found in {:?}", test_cases_dir);
    }

    let results: Vec<TestResult> = test_cases
        .into_iter()
        .map(|test_case| run_single_test(test_case))
        .collect();

    let failed_tests: Vec<&TestResult> = results.iter().filter(|result| !result.success).collect();

    // Print summary
    let total = results.len();
    let passed = total - failed_tests.len();
    std::io::stderr()
        .write_all(format!("\nTest Summary: {}/{} passed\n", passed, total).as_bytes())
        .unwrap();

    if !failed_tests.is_empty() {
        let mut error_msg = format!("\n{} test(s) failed:\n", failed_tests.len());
        for result in failed_tests {
            if let Some(ref error) = result.error {
                error_msg.push_str(&format!("\n--- {} ---\n{}\n", result.name, error));
            }
        }
        panic!("{}", error_msg);
    }
}

/// Discover test cases by finding all args.txt files
fn discover_test_cases(test_cases_dir: &Path) -> Vec<TestCase> {
    WalkDir::new(test_cases_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "args.txt")
        .filter_map(|entry| create_test_case_from_args_file(entry.path(), test_cases_dir))
        .collect()
}

/// Create a test case from an args.txt file path
fn create_test_case_from_args_file(args_path: &Path, test_cases_dir: &Path) -> Option<TestCase> {
    let input_dir = args_path.parent()?;
    let test_dir = input_dir.parent()?;
    let expected_dir = test_dir.join("expected");

    if !expected_dir.exists() {
        eprintln!(
            "Warning: No expected directory found for test at {:?}",
            test_dir
        );
        return None;
    }

    let test_name = test_dir
        .strip_prefix(test_cases_dir)
        .ok()?
        .to_string_lossy()
        .replace(['/', '\\'], "_");

    Some(TestCase::new(
        test_name,
        input_dir.to_path_buf(),
        expected_dir,
    ))
}

/// Run a single test case and return the result
fn run_single_test(test_case: TestCase) -> TestResult {
    match run_test_case(&test_case) {
        Ok(()) => {
            std::io::stderr()
                .write_all(format!("✓ {}\n", test_case.name).as_bytes())
                .unwrap();
            TestResult::success(test_case.name)
        }
        Err(error) => {
            std::io::stderr()
                .write_all(format!("✗ {}\n", test_case.name).as_bytes())
                .unwrap();
            TestResult::failure(test_case.name, error)
        }
    }
}

/// Execute the test case and validate results
fn run_test_case(test_case: &TestCase) -> Result<(), String> {
    // Read and parse arguments
    let args_path = test_case.input_dir.join("args.txt");
    let args_content = read_file_content(&args_path)?;
    let args = parse_args(&args_content)?;

    // Execute command
    let output = execute_command(&test_case.input_dir, &args)?;

    // Validate results
    validate_exit_code(test_case, output.status.code().unwrap_or(-1))?;
    validate_stdout(test_case, &String::from_utf8_lossy(&output.stdout))?;
    validate_stderr(test_case, &String::from_utf8_lossy(&output.stderr))?;

    Ok(())
}

/// Execute the treetags command with given arguments
fn execute_command(working_dir: &Path, args: &[String]) -> Result<std::process::Output, String> {
    Command::cargo_bin("treetags")
        .map_err(|e| format!("Failed to create command: {}", e))?
        .current_dir(working_dir)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute command: {}", e))
}

/// Validate exit code against expected value
fn validate_exit_code(test_case: &TestCase, actual_exit_code: i32) -> Result<(), String> {
    let expected_exit_code =
        read_optional_file(&test_case.expected_dir, "exit_code.txt").and_then(parse_exit_code)?;

    if actual_exit_code != expected_exit_code {
        return Err(format!(
            "Exit code mismatch: expected {}, got {}",
            expected_exit_code, actual_exit_code
        ));
    }

    Ok(())
}

/// Validate stdout against expected output
fn validate_stdout(test_case: &TestCase, actual_stdout: &str) -> Result<(), String> {
    if let Some(expected_stdout) = read_optional_file(&test_case.expected_dir, "stdout.txt")? {
        let expected_normalized = normalize_output(&expected_stdout);
        let actual_normalized = normalize_output(actual_stdout);

        if expected_normalized != actual_normalized {
            let diff = create_diff(&expected_normalized, &actual_normalized, "stdout");
            return Err(format!("Stdout mismatch:\n{}", diff));
        }
    }

    Ok(())
}

/// Validate stderr against expected output
fn validate_stderr(test_case: &TestCase, actual_stderr: &str) -> Result<(), String> {
    if let Some(expected_stderr) = read_optional_file(&test_case.expected_dir, "stderr.txt")? {
        let expected_normalized = normalize_output(&expected_stderr);
        let actual_normalized = normalize_output(actual_stderr);

        if expected_normalized != actual_normalized {
            let diff = create_diff(&expected_normalized, &actual_normalized, "stderr");
            return Err(format!("Stderr mismatch:\n{}", diff));
        }
    }

    Ok(())
}

/// Create a diff using the similar crate
fn create_diff(expected: &str, actual: &str, label: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut result = format!("=== {} DIFF ===\n", label.to_uppercase());

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        result.push_str(&format!("{}{}", sign, change));
    }

    result
}
