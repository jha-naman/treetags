//! Offline end-to-end coverage for external grammars and project suggestions.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{tempdir, TempDir};

struct Project {
    dir: TempDir,
}
impl Project {
    fn new() -> Self {
        let project = Self {
            dir: tempdir().unwrap(),
        };
        fs::create_dir_all(project.config_dir()).unwrap();
        project.write("source.zig", "pub fn greet() void {}\n");
        project.write("source.ml", "let double x = x * 2\n");
        project.write("source.rs", "pub fn native() {}\n");
        project
    }
    fn config_dir(&self) -> PathBuf {
        self.dir.path().join("config/treetags")
    }
    fn write(&self, name: &str, source: &str) {
        fs::write(self.dir.path().join(name), source).unwrap();
    }
    fn install(&self, lang: &str) {
        let dir = self.config_dir().join("wasm_grammars/14");
        fs::create_dir_all(&dir).unwrap();
        let file = format!("tree-sitter-{lang}.wasm");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/grammars/wasm/14")
                .join(&file),
            dir.join(file),
        )
        .unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_treetags"))
            .current_dir(self.dir.path())
            .env("XDG_CONFIG_HOME", self.dir.path().join("config"))
            .env("XDG_CACHE_HOME", self.dir.path().join("cache"))
            .args(["--plugins-dir", "empty-plugins"])
            .args(args)
            .output()
            .unwrap()
    }
}
fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}
fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

fn compiled_entries(project: &Project) -> Vec<PathBuf> {
    walkdir::WalkDir::new(
        project
            .dir
            .path()
            .join("cache/treetags/wasm_grammars/modules"),
    )
    .into_iter()
    .filter_map(Result::ok)
    .filter(|entry| entry.file_type().is_file() && entry.path().extension().is_none())
    .map(|entry| entry.into_path())
    .collect()
}

#[test]
fn compiled_grammar_cache_survives_processes_and_recovers_from_corruption() {
    let p = Project::new();
    p.install("ocaml");
    let args = ["-f", "-", "source.ml"];
    let cold = p.run(&args);
    assert!(cold.status.success());
    assert_eq!(stderr(&cold), "");
    assert!(stdout(&cold).contains("double"));
    let entries = compiled_entries(&p);
    assert!(
        !entries.is_empty(),
        "compilation must populate the disk cache"
    );
    let timestamps: Vec<_> = entries
        .iter()
        .map(|path| fs::metadata(path).unwrap().modified().unwrap())
        .collect();
    let warm = p.run(&args);
    assert!(warm.status.success());
    assert_eq!(stderr(&warm), "");
    assert_eq!(cold.stdout, warm.stdout);
    for (path, timestamp) in entries.iter().zip(timestamps) {
        assert_eq!(
            fs::metadata(path).unwrap().modified().unwrap(),
            timestamp,
            "a cache hit should not rewrite compiled code"
        );
        fs::write(path, b"broken cache entry").unwrap();
    }
    let recovered = p.run(&args);
    assert!(recovered.status.success());
    assert_eq!(stderr(&recovered), "");
    assert_eq!(cold.stdout, recovered.stdout);
    for path in entries {
        assert_ne!(fs::read(path).unwrap(), b"broken cache entry");
    }

    // A populated cache must never hide a changed or missing grammar file.
    let grammar = p
        .config_dir()
        .join("wasm_grammars/14/tree-sitter-ocaml.wasm");
    let previous_count = compiled_entries(&p).len();
    let mut changed = fs::read(&grammar).unwrap();
    // Append a valid, inert custom section so the module still parses identically.
    changed.extend_from_slice(&[0, 2, 1, b'x']);
    fs::write(&grammar, changed).unwrap();
    let replaced = p.run(&args);
    assert!(replaced.status.success());
    assert_eq!(stderr(&replaced), "");
    assert_eq!(cold.stdout, replaced.stdout);
    assert!(compiled_entries(&p).len() > previous_count);
    fs::write(&grammar, b"invalid replacement").unwrap();
    assert!(stderr(&p.run(&args)).contains("grammar 'ocaml'"));
    fs::remove_file(grammar).unwrap();
    assert!(stderr(&p.run(&args)).contains("cannot read grammar"));
}

#[test]
fn unavailable_cache_does_not_prevent_grammar_loading() {
    let p = Project::new();
    p.install("ocaml");
    fs::write(p.dir.path().join("cache"), b"not a directory").unwrap();
    let out = p.run(&["-f", "-", "source.ml"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(stdout(&out).contains("double"));
}

#[test]
fn concurrent_processes_can_populate_the_same_cache() {
    let p = Project::new();
    p.install("zig");
    let outputs = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| p.run(&["-f", "-", "source.zig"])))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(!compiled_entries(&p).is_empty());
    let warm = p.run(&["-f", "-", "source.zig"]);
    for out in outputs.iter().chain(std::iter::once(&warm)) {
        assert!(out.status.success());
        assert_eq!(stderr(out), "");
        assert!(stdout(out).contains("greet"));
        assert_eq!(out.stdout, warm.stdout);
    }
}

#[test]
fn mixed_languages_have_identical_output_with_multiple_workers() {
    let p = Project::new();
    p.install("zig");
    p.install("ocaml");
    for i in 0..24 {
        p.write(&format!("copy{i}.zig"), "pub fn repeated() void {}\n");
    }
    let one = p.run(&["-f", "-", "--workers", "1"]);
    let many = p.run(&["-f", "-", "--workers", "4"]);
    assert!(one.status.success() && many.status.success());
    assert_eq!(stderr(&one), "");
    assert_eq!(stderr(&many), "");
    assert_eq!(one.stdout, many.stdout);
    for name in ["greet", "double", "native", "repeated"] {
        assert!(stdout(&one).contains(name));
    }
}

#[test]
fn missing_grammars_warn_once_including_config_requests_and_continue() {
    let p = Project::new();
    fs::write(
        p.config_dir().join("config.toml"),
        "[wasm_grammars]\nlanguages = ['ZIG', 'zig', 'ocaml', 'unknown', 'unknown']\n",
    )
    .unwrap();
    for i in 0..24 {
        p.write(&format!("copy{i}.zig"), "pub fn missing() void {}\n");
    }
    let out = p.run(&["-f", "-", "--workers", "4"]);
    assert!(out.status.success());
    let err = stderr(&out);
    assert_eq!(err.matches("grammar 'zig'").count(), 1, "{err}");
    assert_eq!(err.matches("grammar 'ocaml'").count(), 1, "{err}");
    assert_eq!(err.matches("grammar 'unknown'").count(), 1, "{err}");
    assert!(err.contains("wasm_grammars/14/tree-sitter-zig.wasm"));
    assert!(stdout(&out).contains("native"));
}

#[test]
fn configured_grammars_load_only_for_matching_inputs() {
    let p = Project::new();
    fs::write(
        p.config_dir().join("config.toml"),
        "[wasm_grammars]\nlanguages = ['zig', 'ocaml']\n",
    )
    .unwrap();
    let out = p.run(&["-f", "-", "source.rs"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(stdout(&out).contains("native"));

    p.install("zig");
    p.install("ocaml");
    let out = p.run(&["-f", "-", "source.rs"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(compiled_entries(&p).is_empty());

    let out = p.run(&["-f", "-", "source.ml"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(stdout(&out).contains("double"));
    assert!(!compiled_entries(&p).is_empty());
}

#[test]
fn malformed_and_unreadable_grammars_warn_once() {
    let p = Project::new();
    p.install("zig");
    p.install("ocaml");
    fs::write(
        p.config_dir().join("wasm_grammars/14/tree-sitter-zig.wasm"),
        b"invalid wasm",
    )
    .unwrap();
    let ocaml = p
        .config_dir()
        .join("wasm_grammars/14/tree-sitter-ocaml.wasm");
    fs::remove_file(&ocaml).unwrap();
    fs::create_dir(&ocaml).unwrap();
    p.write("copy.zig", "pub fn copy() void {}\n");
    let out = p.run(&["-f", "-"]);
    assert!(out.status.success());
    let err = stderr(&out);
    assert_eq!(err.matches("grammar 'zig'").count(), 1, "{err}");
    assert_eq!(err.matches("grammar 'ocaml'").count(), 1, "{err}");
    assert!(stdout(&out).contains("native"));
}

#[test]
fn suggestions_respect_maps_force_exclusions_and_do_not_write_tags() {
    let p = Project::new();
    p.write("custom.txt", "pub fn custom() void {}\n");
    let out = p.run(&["--suggest-grammars", "--exclude", "*.ml", "--map-zig=+.txt"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert_eq!(stdout(&out).matches("tree-sitter-zig.wasm").count(), 1);
    assert!(!stdout(&out).contains("ocaml"));
    assert!(!p.dir.path().join("tags").exists());
    let forced = p.run(&[
        "--suggest-grammars",
        "--language-force",
        "ocaml",
        "custom.txt",
    ]);
    assert!(stdout(&forced).contains("tree-sitter-ocaml.wasm"));
    assert!(!stdout(&forced).contains("zig"));
    p.install("zig");
    p.install("ocaml");
    // Presence is sufficient for suggestions: malformed files aren't compiled here.
    fs::write(
        p.config_dir().join("wasm_grammars/14/tree-sitter-zig.wasm"),
        "bad",
    )
    .unwrap();
    let out = p.run(&["--suggest-grammars"]);
    assert_eq!(
        stdout(&out),
        "No missing WASM grammars for the selected files.\n"
    );
    assert_eq!(stderr(&out), "");
    assert!(!p
        .run(&["--suggest-grammars", "--suggest-plugins"])
        .status
        .success());
}

#[test]
fn metadata_does_not_require_installed_grammars() {
    let p = Project::new();
    fs::write(
        p.config_dir().join("config.toml"),
        "[wasm_grammars]\nlanguages=['zig']\n",
    )
    .unwrap();
    for args in [
        vec!["--list-languages"],
        vec!["--list-kinds", "zig"],
        vec!["--print-language", "source.zig"],
        vec!["--list-maps", "ocaml"],
    ] {
        let out = p.run(&args);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stderr(&out), "");
        assert!(!stdout(&out).is_empty());
    }
}

#[test]
fn existing_plugin_overrides_host_and_suppresses_suggestions() {
    let p = Project::new();
    let plugins = treetags_test_plugin_builder::test_plugins_dir();
    assert!(!plugins.is_empty(), "plugin fixtures must be built");
    let out = p.run(&["-f", "-", "--plugin-dir", plugins, "source.zig"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(stdout(&out).contains("greet"));
    let out = p.run(&["--suggest-grammars", "--plugin-dir", plugins, "source.zig"]);
    assert!(!stdout(&out).contains("tree-sitter-zig.wasm"));
    // Configuring a host grammar does not load it when a plugin handles the file.
    fs::write(
        p.config_dir().join("config.toml"),
        "[wasm_grammars]\nlanguages=['zig']\n",
    )
    .unwrap();
    let out = p.run(&["-f", "-", "--plugin-dir", plugins, "source.zig"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(stdout(&out).contains("greet"));
}

#[test]
fn user_query_override_does_not_suggest_an_unused_wasm_grammar() {
    let p = Project::new();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let library = Path::new(env!("OUT_DIR")).join(format!(
        "{}tree_sitter_kotlin{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let query = repo.join("tests/test_cases/kotlin/basic/input/queries.scm");
    fs::write(p.config_dir().join("config.toml"), format!(
        "[[user_grammars]]\nlanguage_name='kotlin'\ngrammar_lib_path={:?}\nextensions=['ml']\nquery_file_path={:?}\n",
        library, query)).unwrap();
    fs::copy(
        repo.join("tests/test_cases/kotlin/basic/input/source.kt"),
        p.dir.path().join("source.ml"),
    )
    .unwrap();
    let out = p.run(&["--suggest-grammars", "source.ml"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(!stdout(&out).contains("tree-sitter-ocaml.wasm"));
    let out = p.run(&["-f", "-", "source.ml"]);
    assert!(out.status.success());
    assert_eq!(stderr(&out), "");
    assert!(stdout(&out).lines().count() > 1);
}
