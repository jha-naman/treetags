//! Runtime compilation of the WASM plugins used by integration tests.
//!
//! Plugin compilation deliberately lives here rather than in `build.rs` so that
//! a plain `cargo build`/`cargo run` never pays for building the plugins. The
//! `integration_tests` binary invokes [`test_plugins_dir`] from a `#[ctor]`
//! constructor, so every plugin is built exactly once, up front, before libtest
//! starts running test functions.
//!
//! Discovers every subdirectory of `plugins/` that has both a `Cargo.toml` and a
//! `plugin.toml` template, compiles each to a `.wasm` component targeting
//! `wasm32-wasip2`, and assembles the result under
//! `<target>/treetags-test-plugins/plugins/{plugin_name}/`.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Returns the directory holding the compiled test plugins, building them on
/// first call. Returns `""` if the plugins could not be built (e.g. the
/// `wasm32-wasip2` target is not installed), matching the previous behaviour
/// where `TREETAGS_TEST_PLUGINS_DIR` was simply unset.
pub fn test_plugins_dir() -> &'static str {
    static DIR: OnceLock<String> = OnceLock::new();
    DIR.get_or_init(|| {
        build_all()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    })
}

fn build_all() -> Option<PathBuf> {
    let target_available = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("wasm32-wasip2"))
        .unwrap_or(false);

    if !target_available {
        eprintln!(
            "warning: wasm32-wasip2 target not installed; plugin-dependent tests will fail. \
             Run: rustup target add wasm32-wasip2"
        );
        return None;
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let plugins_src_dir = manifest_dir.join("plugins");
    if !plugins_src_dir.exists() {
        return None;
    }

    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("target"));
    let out_root = target_dir.join("treetags-test-plugins");
    let plugins_out_dir = out_root.join("plugins");
    // Separate target dir avoids lock contention with the outer cargo invocation.
    let wasm_target_dir = out_root.join("wasm-target");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut any_built = false;

    let mut entries: Vec<_> = std::fs::read_dir(&plugins_src_dir)
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

        let cargo_toml = match std::fs::read_to_string(&cargo_toml_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: cannot read {cargo_toml_path:?}: {e}");
                continue;
            }
        };
        let crate_name = match parse_cargo_package_name(&cargo_toml) {
            Some(n) => n,
            None => {
                eprintln!("warning: no package name found in {cargo_toml_path:?}");
                continue;
            }
        };

        let manifest_template = match std::fs::read_to_string(&manifest_template_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: cannot read {manifest_template_path:?}: {e}");
                continue;
            }
        };
        let plugin_name = match parse_toml_string_field(&manifest_template, "name") {
            Some(n) => n,
            None => {
                eprintln!("warning: no 'name' field in {manifest_template_path:?}");
                continue;
            }
        };

        let status = std::process::Command::new(&cargo)
            .args([
                "build",
                "--target",
                "wasm32-wasip2",
                "--release",
                "-p",
                &crate_name,
            ])
            .env("CARGO_TARGET_DIR", &wasm_target_dir)
            .status();

        let wasm_filename = format!("{}.wasm", crate_name.replace('-', "_"));
        let wasm_src = wasm_target_dir
            .join("wasm32-wasip2/release")
            .join(&wasm_filename);

        match status {
            Ok(s) if s.success() && wasm_src.exists() => {}
            Ok(s) => {
                eprintln!(
                    "warning: {plugin_name} plugin compilation failed (exit {:?})",
                    s.code()
                );
                continue;
            }
            Err(e) => {
                eprintln!("warning: could not invoke cargo for {plugin_name} plugin: {e}");
                continue;
            }
        }

        let plugin_out_dir = plugins_out_dir.join(&plugin_name);
        if let Err(e) = std::fs::create_dir_all(&plugin_out_dir) {
            eprintln!("warning: cannot create {plugin_out_dir:?}: {e}");
            continue;
        }

        let wasm_out_path = plugin_out_dir.join("plugin.wasm");
        if let Err(e) = std::fs::copy(&wasm_src, &wasm_out_path) {
            eprintln!("warning: cannot copy {plugin_name} .wasm: {e}");
            continue;
        }

        // Prepend `wasm_file` as a top-level key. Appending it would fold the
        // key into a trailing table section (e.g. `[[kinds]]` or
        // `[disambiguation]`) and corrupt the manifest.
        let manifest = format!(
            "wasm_file = \"{}\"\n{}\n",
            wasm_out_path.display(),
            manifest_template.trim_start()
        );
        if let Err(e) = std::fs::write(plugin_out_dir.join("plugin.toml"), &manifest) {
            eprintln!("warning: cannot write {plugin_name} plugin.toml: {e}");
            continue;
        }

        any_built = true;
    }

    any_built.then_some(plugins_out_dir)
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
