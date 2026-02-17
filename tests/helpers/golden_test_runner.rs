use assert_cmd::prelude::*;
use serde::Deserialize;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{
    file_utils::{
        normalize_output, parse_args, parse_exit_code, read_file_content, read_optional_file,
    },
    test_runner::TestCase,
};

#[derive(Deserialize)]
struct UserGrammarConfig {
    language_name: String,
    extensions: Vec<String>,
}

/// Execute the test case and validate results
pub fn run_test_case(test_case: &TestCase) -> Result<(), String> {
    // Read and parse arguments
    let args_path = test_case.input_dir.join("args.txt");
    let args_content = read_file_content(&args_path)?;
    let mut args = parse_args(&args_content)?;

    let user_grammar_toml_path = test_case.input_dir.join("user_grammar.toml");

    if user_grammar_toml_path.exists() {
        let toml_content_in = fs::read_to_string(&user_grammar_toml_path)
            .map_err(|e| format!("Failed to read user_grammar.toml: {}", e))?;
        let grammar_config: UserGrammarConfig = toml::from_str(&toml_content_in)
            .map_err(|e| format!("Failed to parse user_grammar.toml: {}", e))?;

        let lib_name = format!(
            "{}tree_sitter_{}{}",
            std::env::consts::DLL_PREFIX,
            grammar_config.language_name,
            std::env::consts::DLL_SUFFIX
        );

        let lib_path = PathBuf::from(env!("OUT_DIR")).join(lib_name);

        let mut queries_config = String::new();
        let queries_path = test_case.input_dir.join("queries.scm");
        if queries_path.exists() {
            let dest_path = PathBuf::from(env!("OUT_DIR"))
                .join(format!("{}_queries.scm", grammar_config.language_name));
            fs::copy(&queries_path, &dest_path)
                .map_err(|e| format!("Failed to copy queries.scm: {}", e))?;
            queries_config = format!("query_file_path = {:?}\n", dest_path);
        }

        let toml_content_out = format!(
            r#"
[[user_grammars]]
language_name = "{}"
grammar_lib_path = "{}"
extensions = {:?}
{}
"#,
            grammar_config.language_name,
            lib_path.display(),
            grammar_config.extensions,
            queries_config
        );

        let config_file_path = PathBuf::from(env!("OUT_DIR"))
            .join(format!("{}_user_config.toml", grammar_config.language_name));
        fs::write(&config_file_path, toml_content_out)
            .map_err(|e| format!("Failed to write temporary config file: {}", e))?;

        args.push("--user-languages-config".to_string());
        args.push(config_file_path.to_string_lossy().to_string());
    }

    // Execute command
    let output = execute_command(&test_case.input_dir, &args)?;

    // Validate results
    validate_exit_code(test_case, output.status.code().unwrap_or(-1))?;
    validate_output(
        test_case,
        &String::from_utf8_lossy(&output.stdout),
        "stdout",
    )?;
    validate_output(
        test_case,
        &String::from_utf8_lossy(&output.stderr),
        "stderr",
    )?;

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

/// Validate output (stdout or stderr) against expected output
fn validate_output(
    test_case: &TestCase,
    actual_output: &str,
    output_type: &str,
) -> Result<(), String> {
    let filename = format!("{}.txt", output_type);
    if let Some(expected_output) = read_optional_file(&test_case.expected_dir, &filename)? {
        let expected_normalized = normalize_output(&expected_output);
        let actual_normalized = normalize_output(actual_output);

        if expected_normalized != actual_normalized {
            let diff = create_diff(&expected_normalized, &actual_normalized, output_type);
            return Err(format!(
                "{} mismatch:\n{}",
                output_type
                    .chars()
                    .next()
                    .unwrap()
                    .to_uppercase()
                    .collect::<String>()
                    + &output_type[1..],
                diff
            ));
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
