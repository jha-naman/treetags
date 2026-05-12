use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let generated_tests_dir = Path::new(&out_dir).join("generated_tests");

    // Clean existing generated tests directory
    if generated_tests_dir.exists() {
        fs::remove_dir_all(&generated_tests_dir).unwrap();
    }

    // Create the generated tests directory
    fs::create_dir_all(&generated_tests_dir).unwrap();

    compile_test_grammars(Path::new(&out_dir));
    build_wasm_plugins(Path::new(&out_dir));

    let test_cases = discover_test_cases();

    // Generate individual test files
    for test_case in test_cases.iter() {
        generate_individual_test_file(&generated_tests_dir, test_case);
    }

    // Generate a single file that includes all test functions
    generate_tests_include_file(&generated_tests_dir, &test_cases);

    println!("cargo:rerun-if-changed=tests/test_cases");
    println!("cargo:rerun-if-changed=plugins/common");
}

/// Build plugins and make it possible to use them in integration tests
///
/// Discovers every subdirectory of `plugins/` that has both a `Cargo.toml` and a
/// `plugin.toml` template, compiles each to a `.wasm` component targeting
/// `wasm32-wasip2`, and writes the result to `$OUT_DIR/plugins/{plugin_name}/`.
/// A single env var `TREETAGS_TEST_PLUGINS_DIR` is emitted pointing at that
/// directory so integration tests can pass `--plugin-dir {TREETAGS_TEST_PLUGINS_DIR}`
/// without embedding any absolute paths in the checked-in source tree.
fn build_wasm_plugins(out_dir: &Path) {
    let target_available = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-wasip2"))
        .unwrap_or(false);

    if !target_available {
        println!("cargo:warning=wasm32-wasip2 target not installed; skipping plugin compilation. Run: rustup target add wasm32-wasip2");
        return;
    }

    let plugins_src_dir = Path::new("plugins");
    if !plugins_src_dir.exists() {
        return;
    }

    let plugins_out_dir = out_dir.join("plugins");
    // Use a separate target dir to avoid lock contention with the parent cargo invocation.
    let wasm_target_dir = out_dir.join("wasm-target");
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut any_built = false;

    let mut entries: Vec<_> = fs::read_dir(plugins_src_dir)
        .expect("cannot read plugins directory")
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(|e| e.path());

    for entry in entries {
        let plugin_src = entry.path();
        if !plugin_src.is_dir() {
            continue;
        }

        let cargo_toml_path = plugin_src.join("Cargo.toml");
        let manifest_template_path = plugin_src.join("plugin.toml");
        if !cargo_toml_path.exists() || !manifest_template_path.exists() {
            continue;
        }

        let cargo_toml = match fs::read_to_string(&cargo_toml_path) {
            Ok(s) => s,
            Err(e) => {
                println!("cargo:warning=cannot read {cargo_toml_path:?}: {e}");
                continue;
            }
        };
        let crate_name = match parse_cargo_package_name(&cargo_toml) {
            Some(n) => n,
            None => {
                println!("cargo:warning=no package name found in {cargo_toml_path:?}");
                continue;
            }
        };

        let manifest_template = match fs::read_to_string(&manifest_template_path) {
            Ok(s) => s,
            Err(e) => {
                println!("cargo:warning=cannot read {manifest_template_path:?}: {e}");
                continue;
            }
        };
        let plugin_name = match parse_toml_string_field(&manifest_template, "name") {
            Some(n) => n,
            None => {
                println!("cargo:warning=no 'name' field in {manifest_template_path:?}");
                continue;
            }
        };

        let mut cmd = std::process::Command::new(&cargo);
        cmd.args([
            "build",
            "--target",
            "wasm32-wasip2",
            "--release",
            "-p",
            &crate_name,
        ])
        .env("CARGO_TARGET_DIR", &wasm_target_dir);

        let status = cmd.status();

        let wasm_filename = format!("{}.wasm", crate_name.replace('-', "_"));
        let wasm_src = wasm_target_dir
            .join("wasm32-wasip2/release")
            .join(&wasm_filename);

        match status {
            Ok(s) if s.success() && wasm_src.exists() => {}
            Ok(s) => {
                println!(
                    "cargo:warning={plugin_name} plugin compilation failed (exit {:?}); tests will be skipped",
                    s.code()
                );
                continue;
            }
            Err(e) => {
                println!(
                    "cargo:warning=could not invoke cargo for {plugin_name} plugin: {e}; tests will be skipped"
                );
                continue;
            }
        }

        let plugin_out_dir = plugins_out_dir.join(&plugin_name);
        if let Err(e) = fs::create_dir_all(&plugin_out_dir) {
            println!("cargo:warning=cannot create {plugin_out_dir:?}: {e}");
            continue;
        }

        let wasm_out_path = plugin_out_dir.join("plugin.wasm");
        if let Err(e) = fs::copy(&wasm_src, &wasm_out_path) {
            println!("cargo:warning=cannot copy {plugin_name} .wasm: {e}");
            continue;
        }

        let manifest = format!(
            "{}\nwasm_file = \"{}\"\n",
            manifest_template.trim_end(),
            wasm_out_path.display()
        );
        if let Err(e) = fs::write(plugin_out_dir.join("plugin.toml"), &manifest) {
            println!("cargo:warning=cannot write {plugin_name} plugin.toml: {e}");
            continue;
        }

        println!("cargo:rerun-if-changed={}", plugin_src.display());
        any_built = true;
    }

    if any_built {
        println!(
            "cargo:rustc-env=TREETAGS_TEST_PLUGINS_DIR={}",
            plugins_out_dir.display()
        );
    }
}

fn parse_cargo_package_name(content: &str) -> Option<String> {
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && trimmed.starts_with('[') {
            break;
        }
        if in_package && trimmed.starts_with("name") {
            if let Some(val) = trimmed.splitn(2, '=').nth(1) {
                return Some(val.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn parse_toml_string_field(content: &str, field: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(field) {
            if let Some(val) = trimmed.splitn(2, '=').nth(1) {
                return Some(val.trim().trim_matches('"').to_string());
            }
        }
    }
    None
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
        r#"// Auto-generated test for: {}

#[test]
fn test_{}() {{
    use std::path::PathBuf;
    use crate::helpers::{{
        test_runner::TestCase,
        golden_test_runner::run_test_case,
    }};

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

fn generate_tests_include_file(tests_dir: &Path, test_cases: &[TestCase]) {
    let include_file_path = tests_dir.join("all_tests.rs");

    let mut include_content =
        String::from("// Auto-generated file that includes all test functions\n\n");

    // Include each test file
    for test_case in test_cases {
        let test_name = sanitize_test_name(&test_case.name);
        include_content.push_str(&format!(
            "include!(concat!(env!(\"OUT_DIR\"), \"/generated_tests/{}.rs\"));\n",
            test_name
        ));
    }

    fs::write(&include_file_path, include_content).unwrap();
}

fn sanitize_test_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn compile_test_grammars(out_dir: &Path) {
    let grammars_dir = Path::new("tests/grammars");
    if !grammars_dir.exists() {
        return;
    }

    for entry in fs::read_dir(grammars_dir).unwrap().filter_map(Result::ok) {
        let grammar_path = entry.path();
        if !grammar_path.is_dir() {
            continue;
        }

        let grammar_name = grammar_path.file_name().unwrap().to_str().unwrap();
        let lang_name = if let Some(name) = grammar_name.strip_prefix("tree-sitter-") {
            name
        } else {
            continue;
        };

        let parser_path = grammar_path.join("parser.c");
        if !parser_path.exists() {
            continue;
        }

        let mut paths_to_compile = vec![parser_path];
        if grammar_path.join("scanner.c").exists() {
            paths_to_compile.push(grammar_path.join("scanner.c"));
        }

        let mut build = cc::Build::new();
        build.include(&grammar_path).warnings(false);

        let compiler = build.get_compiler();
        let mut command = compiler.to_command();

        command.arg("-shared").arg("-fPIC");

        let (lib_prefix, lib_suffix) = if cfg!(target_os = "macos") {
            ("lib", ".dylib")
        } else {
            ("lib", ".so")
        };

        let out_file_name = format!("{}tree_sitter_{}{}", lib_prefix, lang_name, lib_suffix);
        let out_path = out_dir.join(&out_file_name);

        command.arg("-o").arg(&out_path);

        for path in &paths_to_compile {
            command.arg(path);
        }

        let status = command
            .status()
            .unwrap_or_else(|e| panic!("Failed to execute compiler: {}", e));

        if !status.success() {
            panic!(
                "Failed to compile grammar for {}. Exit code: {:?}",
                lang_name,
                status.code()
            );
        }

        println!("cargo:rerun-if-changed={}", grammar_path.display());
    }
}
