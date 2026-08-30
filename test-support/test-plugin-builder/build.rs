//! Build script for the WASM plugins used by integration tests.
//!
//! Discovers every subdirectory of `plugins/` that has both a `Cargo.toml` and a
//! `plugin.toml` template, compiles each to a `.wasm` component targeting
//! `wasm32-wasip2`, and assembles the result under this package's `OUT_DIR`.

use std::path::PathBuf;

fn main() {
    let plugins_dir = build_all()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=TREETAGS_TEST_PLUGINS_DIR={plugins_dir}");
    println!("cargo:rerun-if-changed=../../plugins");
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");
    println!("cargo:rerun-if-env-changed=binCC");
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
    let workspace_dir = manifest_dir.join("../..");
    let plugins_src_dir = workspace_dir.join("plugins");
    if !plugins_src_dir.exists() {
        return None;
    }

    let out_root = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
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

        let mut command = std::process::Command::new(&cargo);
        for name in ["WASI_SDK_PATH", "binCC"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }

        command
            .args([
                "build",
                "--target",
                "wasm32-wasip2",
                "--release",
                "-p",
                &crate_name,
            ])
            .current_dir(&workspace_dir)
            .env("CARGO_TARGET_DIR", &wasm_target_dir);

        let wasm_filename = format!("{}.wasm", crate_name.replace('-', "_"));
        let wasm_src = wasm_target_dir
            .join("wasm32-wasip2/release")
            .join(&wasm_filename);

        build_plugin(&mut command, &plugin_name, &wasm_src)
            .unwrap_or_else(|error| panic!("{error}"));

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

fn build_plugin(
    command: &mut std::process::Command,
    plugin_name: &str,
    expected_wasm: &std::path::Path,
) -> Result<(), String> {
    // Keep successful builds quiet, but retain the nested Cargo diagnostics so
    // the outer build can display them if plugin compilation fails.
    let output = command
        .output()
        .map_err(|error| format!("could not invoke Cargo for the {plugin_name} plugin: {error}"))?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        return Err(format!(
            "failed to compile the {plugin_name} plugin \
             (Cargo exit code: {:?})\n\
             \n--- Cargo stdout ---\n\
             {stdout}\
             \n--- Cargo stderr ---\n\
             {stderr}",
            output.status.code(),
        ));
    }

    if !expected_wasm.is_file() {
        return Err(format!(
            "Cargo successfully compiled the {plugin_name} plugin, \
             but the expected WASM artifact was not produced at {}",
            expected_wasm.display(),
        ));
    }

    Ok(())
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
