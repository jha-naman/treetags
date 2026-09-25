//! User-facing selection and compatibility tests, isolated from host configuration.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::{tempdir, TempDir};

struct Project(TempDir);

impl Project {
    fn new(config: &str) -> Self {
        let project = Self(tempdir().unwrap());
        project.write("config/treetags/config.toml", config);
        project
    }

    fn write(&self, name: &str, content: &str) {
        let path = self.0.path().join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_treetags"))
            .current_dir(self.0.path())
            .env("XDG_CONFIG_HOME", self.0.path().join("config"))
            .env("XDG_CACHE_HOME", self.0.path().join("cache"))
            .args(["--plugins-dir", "empty-plugins"])
            .args(args)
            .output()
            .unwrap()
    }

    fn success(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn selection(&self, args: &[&str], lang: &str) -> Vec<String> {
        let output = self.success(args);
        output
            .lines()
            .find(|line| line.starts_with(&format!("{lang}\t")))
            .unwrap_or_else(|| panic!("missing {lang}: {output}"))
            .split('\t')
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn default_and_configured_preferences_are_visible_without_loading_grammars() {
    let p = Project::new("[tags]\nextended = [' RUST ', 'rust']\nbasic = ['BASH']\n");
    let rust = p.selection(&["--list-tag-styles", "rust"], "rust");
    assert_eq!(&rust[3..5], &["extended", "extended"]);
    assert_eq!(rust[2], "basic,extended");
    assert_eq!(rust[5], "preferred style available");
    let zig = p.selection(&["--list-tag-styles", "zig"], "zig");
    assert_eq!(&zig[1..5], &["wasm", "basic,extended", "basic", "basic"]);
    assert_eq!(zig[5], "preferred style available");
    let shell = p.selection(&["--list-tag-styles", "SH"], "shell");
    assert_eq!(&shell[3..5], &["basic", "basic"]);
    assert!(!p.0.path().join("cache").exists());
    assert!(!p.0.path().join("tags").exists());
}

#[test]
fn cli_replaces_lists_clears_inheritance_and_overrides_options_file() {
    let p = Project::new("[tags]\ndefault = 'extended'\nextended = ['rust']\nbasic = ['ruby']\n");
    p.write(
        "options",
        "--tag-style=extended\n--basic-tags=java\n--tags-with-extension-fields=go\n",
    );
    let args = [
        "--options",
        "options",
        "--tag-style=basic",
        "--basic-tags=python",
        "--basic-tags=ruby",
        "--tags-with-extension-fields=",
        "--list-tag-styles",
    ];
    let rust = p.selection(&args, "rust");
    assert_eq!(rust[3], "basic");
    let go = p.selection(&args, "go");
    assert_eq!(go[3], "basic");
    let java = p.selection(&["--options", "options", "--list-tag-styles"], "java");
    assert_eq!(java[3], "basic");
    let python = p.selection(
        &[
            "--tag-style=extended",
            "--basic-tags=python",
            "--basic-tags=ruby",
            "--list-tag-styles",
        ],
        "python",
    );
    assert_eq!(python[3], "extended", "last list replaces earlier values");
}

#[test]
fn effective_list_conflicts_and_bad_configuration_fail_before_output_changes() {
    for (config, args, expected) in [
        ("[tags]\nbasic=['rust']\nextended=['RUST']", vec![], "both"),
        (
            "[tags]\nbasic=['not-a-language']",
            vec![],
            "unknown official language",
        ),
        ("[tags]\ndefault='query'", vec![], "Failed to parse"),
        (
            "[tags]\ndefault='with_extension_fields'",
            vec![],
            "Failed to parse",
        ),
        ("[tags]\nbaisc=['rust']", vec![], "Failed to parse"),
        ("", vec!["--tag-style=walker"], "invalid value"),
        (
            "",
            vec!["--tag-style=with_extension_fields"],
            "invalid value",
        ),
        (
            "",
            vec!["--basic-tags=rust,,go"],
            "unknown official language",
        ),
    ] {
        let p = Project::new(config);
        p.write("tags", "existing tags\n");
        p.write("source.rs", "pub fn example() {}\n");
        let mut args = args;
        args.push("source.rs");
        let output = p.run(&args);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{:?}",
            output
        );
        assert_eq!(
            fs::read_to_string(p.0.path().join("tags")).unwrap(),
            "existing tags\n"
        );
    }
    let p = Project::new("[tags]\nbasic=['rust']\nextended=['RUST']");
    p.success(&["--basic-tags=", "--list-tag-styles"]);
}

#[test]
fn explicit_config_path_and_global_override_retain_language_preferences() {
    let p = Project::new("[tags]\nextended=['rust']");
    p.write("other.toml", "[tags]\ndefault='extended'\nbasic=['ruby']");
    let rust = p.selection(
        &[
            "--user-languages-config",
            "other.toml",
            "--tag-style=basic",
            "--list-tag-styles",
        ],
        "rust",
    );
    assert_eq!(rust[3], "basic");
    let ruby = p.selection(
        &["--user-languages-config", "other.toml", "--list-tag-styles"],
        "ruby",
    );
    assert_eq!(ruby[3], "basic");
}

#[test]
fn basic_output_ignores_rich_options_and_single_style_fallback_preserves_current_output() {
    let p = Project::new("");
    p.write("source.rb", "def greet\nend\n");
    p.write("source.rs", "pub fn hello() {}\n");
    p.write("source.scala", "def greet = 1\n");
    let plain = p.success(&["-f", "-", "source.rb"]);
    assert!(plain.contains("greet\tsource.rb\t"));
    assert!(plain
        .lines()
        .filter(|line| !line.starts_with('!'))
        .all(|line| line.trim_end_matches('\t').split('\t').count() == 3));
    assert_eq!(
        plain,
        p.success(&[
            "-f",
            "-",
            "--tag-style=basic",
            "--fields=+n,+S",
            "--extras=+q",
            "--kinds-ruby=anything",
            "source.rb"
        ])
    );
    let scala_basic = p.success(&["-f", "-", "source.scala"]);
    assert!(scala_basic.contains("greet\tsource.scala\t"));
    assert_eq!(
        scala_basic,
        p.success(&[
            "-f",
            "-",
            "--tag-style=basic",
            "--fields=+n,+S",
            "--extras=+q",
            "--kinds-scala=anything",
            "source.scala"
        ])
    );
    let scala_rich = p.success(&["-f", "-", "--tag-style=extended", "source.scala"]);
    assert!(scala_rich.contains("greet\tsource.scala\t"));
    assert!(scala_rich.contains("\tf"));
    let rust_basic = p.success(&["-f", "-", "source.rs"]);
    assert!(rust_basic.contains("hello\tsource.rs\t"));
    assert!(rust_basic
        .lines()
        .filter(|line| !line.starts_with('!'))
        .all(|line| line.trim_end_matches('\t').split('\t').count() == 3));
    let rust_rich = p.success(&["-f", "-", "--tag-style=extended", "source.rs"]);
    assert!(rust_rich.contains("hello\tsource.rs\t"));
    assert!(rust_rich.contains("\tf"));
    assert_eq!(
        p.success(&["-f", "-", "--workers=1", "source.rb", "source.rs"]),
        p.success(&["-f", "-", "--workers=4", "source.rb", "source.rs"])
    );
}

#[test]
fn rust_preference_selects_both_backends_and_basic_ignores_rich_controls() {
    let p = Project::new("[tags]\ndefault='extended'\nbasic=['rust']\n");
    p.write("source.rs", "pub fn hello() {}\n");
    let basic = p.success(&["-f", "-", "source.rs"]);
    assert!(basic.contains("hello\tsource.rs\t"));
    assert!(basic
        .lines()
        .filter(|line| !line.starts_with('!'))
        .all(|line| line.trim_end_matches('\t').split('\t').count() == 3));
    assert_eq!(
        basic,
        p.success(&[
            "-f",
            "-",
            "--fields=+n,+S",
            "--extras=+q",
            "--kinds-rust=-f",
            "source.rs"
        ])
    );
    let rich = p.success(&["-f", "-", "--basic-tags=", "source.rs"]);
    assert!(rich.contains("hello\tsource.rs\t"));
    assert!(rich.contains("\tf"));
    assert_ne!(basic, rich);
    assert_eq!(
        basic,
        p.success(&["-f", "-", "--tag-style=basic", "source.rs"])
    );
}

#[test]
fn basic_query_identity_survives_language_forcing_and_extension_remapping() {
    let p = Project::new("");
    p.write("source.custom", "def greet\nend\n");
    let forced = p.success(&["-f", "-", "--language-force=ruby", "source.custom"]);
    let mapped = p.success(&["-f", "-", "--map-ruby=+.custom", "source.custom"]);
    assert_eq!(forced, mapped);
    assert!(mapped.contains("greet\tsource.custom\t"));
}

#[test]
fn wasm_queries_keep_the_same_style_selection_and_offline_installation() {
    let p = Project::new("[tags]\ndefault='basic'");
    let directory = p.0.path().join("config/treetags/wasm_grammars/14");
    fs::create_dir_all(&directory).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammars/wasm/14/tree-sitter-ocaml.wasm"),
        directory.join("tree-sitter-ocaml.wasm"),
    )
    .unwrap();
    p.write("source.ml", "let double x = x * 2\n");
    let output = p.success(&["-f", "-", "source.ml"]);
    assert!(output.contains("double\tsource.ml\t"));
    assert_eq!(
        output,
        p.success(&["-f", "-", "--tag-style=basic", "source.ml"])
    );
}

#[test]
fn custom_native_grammar_retains_existing_query_override() {
    let library = Path::new(env!("OUT_DIR")).join(format!(
        "{}tree_sitter_gleam{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let config = format!(
        "[[user_grammars]]\nlanguage_name='gleam'\ngrammar_lib_path='{}'\nextensions=['lua']\n",
        library.display()
    );
    let p = Project::new(&config);
    p.write("source.lua", "pub fn custom_definition() { 1 }\n");
    assert!(p
        .success(&["-f", "-", "source.lua"])
        .contains("custom_definition\tsource.lua\t"));
}
