use cc::Build;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() {
    // Compile the Rust parser with aggressive optimizations
    compile_rust_parser();
    compile_ocaml_parser();

    // Continue with existing test generation
    let out_dir = env::var("OUT_DIR").unwrap();
    let generated_tests_dir = Path::new(&out_dir).join("generated_tests");

    // Clean existing generated tests directory
    if generated_tests_dir.exists() {
        fs::remove_dir_all(&generated_tests_dir).unwrap();
    }

    // Create the generated tests directory
    fs::create_dir_all(&generated_tests_dir).unwrap();

    let test_cases = discover_test_cases();

    // Generate individual test files
    for test_case in test_cases.iter() {
        generate_individual_test_file(&generated_tests_dir, test_case);
    }

    // Generate a single file that includes all test functions
    generate_tests_include_file(&generated_tests_dir, &test_cases);

    println!("cargo:rerun-if-changed=tests/test_cases");
    println!("cargo:rerun-if-changed=src/parsers/rust");
    println!("cargo:rerun-if-changed=src/parsers/ocaml");
}

/// Compile the Rust parser C code with aggressive size optimizations
fn compile_rust_parser() {
    let mut build = Build::new();

    build
        .file("src/parsers/rust/parser.c")
        .file("src/parsers/rust/scanner.c")
        .include("src/parsers/rust")
        // Aggressive size optimization flags
        .flag_if_supported("-Os") // Optimize for size
        .flag_if_supported("-ffunction-sections") // Put each function in separate section
        .flag_if_supported("-fdata-sections") // Put each data item in separate section
        .flag_if_supported("-fno-stack-protector") // Remove stack protection overhead
        .flag_if_supported("-fomit-frame-pointer") // Remove frame pointer for smaller code
        .flag_if_supported("-fno-unwind-tables") // Remove unwind tables
        .flag_if_supported("-fno-asynchronous-unwind-tables") // Remove async unwind tables
        .flag_if_supported("-fvisibility=hidden") // Hide symbols by default
        .flag_if_supported("-flto") // Link-time optimization
        .flag_if_supported("-fno-ident") // Remove compiler identification
        .flag_if_supported("-s") // Strip symbols at object level
        .flag_if_supported("-Wl,-s") // Strip symbols at link level
        .flag_if_supported("-fmerge-all-constants") // Merge identical constants
        .flag_if_supported("-fno-exceptions") // Remove exception handling
        // Preprocessor definitions
        .define("NDEBUG", None) // Remove debug assertions
        .define("TREE_SITTER_HIDE_SYMBOLS", None) // Hide internal symbols
        .define("TREE_SITTER_NO_DEBUG", None) // Remove debug code
        // Optimization level
        // .opt_level(3)                                // Maximum optimization
        .compile("tree_sitter_rust");

    // Add linker flags for additional size reduction
    println!("cargo:rustc-link-arg=-Wl,--gc-sections"); // Remove unused sections
    println!("cargo:rustc-link-arg=-Wl,--as-needed"); // Only link needed libraries

    // For release builds, add symbol stripping
    if env::var("PROFILE").unwrap_or_default() == "release" {
        println!("cargo:rustc-link-arg=-Wl,--strip-all"); // Strip all symbols
    }

    println!("cargo:rustc-link-lib=static=tree_sitter_rust");
}

/// Compile the Ocaml parser C code with aggressive size optimizations
fn compile_ocaml_parser() {
    let mut build = Build::new();

    build
        .file("src/parsers/ocaml/parser.c")
        .file("src/parsers/ocaml/scanner.c")
        .include("src/parsers/ocaml")
        // Aggressive size optimization flags
        .flag_if_supported("-Os") // Optimize for size
        .flag_if_supported("-ffunction-sections") // Put each function in separate section
        .flag_if_supported("-fdata-sections") // Put each data item in separate section
        .flag_if_supported("-fno-stack-protector") // Remove stack protection overhead
        .flag_if_supported("-fomit-frame-pointer") // Remove frame pointer for smaller code
        .flag_if_supported("-fno-unwind-tables") // Remove unwind tables
        .flag_if_supported("-fno-asynchronous-unwind-tables") // Remove async unwind tables
        .flag_if_supported("-fvisibility=hidden") // Hide symbols by default
        .flag_if_supported("-flto") // Link-time optimization
        .flag_if_supported("-fno-ident") // Remove compiler identification
        .flag_if_supported("-s") // Strip symbols at object level
        .flag_if_supported("-Wl,-s") // Strip symbols at link level
        .flag_if_supported("-fmerge-all-constants") // Merge identical constants
        .flag_if_supported("-fno-exceptions") // Remove exception handling
        // Preprocessor definitions
        .define("NDEBUG", None) // Remove debug assertions
        .define("TREE_SITTER_HIDE_SYMBOLS", None) // Hide internal symbols
        .define("TREE_SITTER_NO_DEBUG", None) // Remove debug code
        // Optimization level
        // .opt_level(3)                                // Maximum optimization
        .compile("tree_sitter_ocaml");

    // Add linker flags for additional size reduction
    println!("cargo:rustc-link-arg=-Wl,--gc-sections"); // Remove unused sections
    println!("cargo:rustc-link-arg=-Wl,--as-needed"); // Only link needed libraries

    // For release builds, add symbol stripping
    if env::var("PROFILE").unwrap_or_default() == "release" {
        println!("cargo:rustc-link-arg=-Wl,--strip-all"); // Strip all symbols
    }

    println!("cargo:rustc-link-lib=static=tree_sitter_ocaml");
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
