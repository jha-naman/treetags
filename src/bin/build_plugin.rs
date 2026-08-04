/// treetags-build-plugin — compile a WASM plugin project for distribution in one step.
///
/// Usage:
///   treetags-build-plugin [OPTIONS] [PLUGIN_DIR]
///
/// Build:
///   cargo build --release --bin treetags-build-plugin
use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

/// ABI version written into the distributed plugin.toml.
/// Keep in sync with PLUGIN_ABI_VERSION in src/plugin/mod.rs.
const PLUGIN_ABI_VERSION: u32 = 3;

#[derive(Parser)]
#[command(
    name = "treetags-build-plugin",
    about = "Build a treetags WASM plugin project for distribution"
)]
struct Args {
    /// Path to the plugin project directory (default: current directory)
    plugin_dir: Option<PathBuf>,

    /// Root output directory; plugin artifacts placed in <DIR>/<plugin-name>/
    #[arg(long)]
    output_dir: Option<PathBuf>,

    /// Skip `cargo build`; use the existing .wasm artifact in target/
    #[arg(long)]
    no_build: bool,
}

#[derive(Deserialize, Default)]
struct FileConfig {
    output_dir: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let plugin_dir = match args.plugin_dir {
        Some(p) => std::fs::canonicalize(&p).unwrap_or(p),
        None => std::env::current_dir().expect("cannot get current directory"),
    };

    let mut file_cfg = load_global_config();
    merge_project_config(&mut file_cfg, &plugin_dir);

    let output_dir_root: PathBuf = if let Some(p) = args.output_dir {
        p
    } else if let Some(s) = file_cfg.output_dir {
        expand_tilde(&s)
    } else {
        plugin_dir.join("dist")
    };

    // Parse Cargo.toml.
    let cargo_toml_path = plugin_dir.join("Cargo.toml");
    let cargo_toml_str = std::fs::read_to_string(&cargo_toml_path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", cargo_toml_path.display()))?;
    let pkg_name = parse_package_name(&cargo_toml_str).ok_or_else(|| {
        anyhow::anyhow!("no [package] name found in {}", cargo_toml_path.display())
    })?;
    check_cdylib(&cargo_toml_str, &cargo_toml_path)?;

    // Read or scaffold plugin.toml.
    let plugin_toml_path = plugin_dir.join("plugin.toml");
    if !plugin_toml_path.exists() {
        write_plugin_toml_template(&plugin_toml_path)?;
        eprintln!(
            "Created plugin.toml template at {}",
            plugin_toml_path.display()
        );
        eprintln!("Edit it to set `extensions` (and optionally `language`), then re-run.");
        std::process::exit(1);
    }
    let plugin_toml_str = std::fs::read_to_string(&plugin_toml_path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", plugin_toml_path.display()))?;
    let plugin_name = parse_toml_string_field(&plugin_toml_str, "name")
        .ok_or_else(|| anyhow::anyhow!("no `name` field in {}", plugin_toml_path.display()))?;

    // Verify wasm32-wasip2 target is installed.
    check_wasm_target()?;

    // Run cargo build.
    if !args.no_build {
        println!("Building {} (wasm32-wasip2)...", pkg_name);
        let mut cmd = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()));
        cmd.args([
            "build",
            "--target",
            "wasm32-wasip2",
            "--release",
            "--locked",
            "-p",
            &pkg_name,
        ])
        .current_dir(&plugin_dir);

        // Configure WASI SDK cross-compiler for C code (e.g. tree-sitter parsers).
        if let Some(sdk) = find_wasi_sdk() {
            let cc = sdk.join("bin/wasm32-wasip2-clang");
            let ar = sdk.join("bin/llvm-ar");
            let ld = sdk.join("bin/wasm-component-ld");
            if cc.exists() {
                cmd.env("CC_wasm32_wasip2", &cc);
            }
            if ar.exists() {
                cmd.env("AR_wasm32_wasip2", &ar);
            }
            if ld.exists() {
                cmd.env("CARGO_TARGET_WASM32_WASIP2_LINKER", &ld);
            }
        }

        let status = cmd
            .status()
            .map_err(|e| anyhow::anyhow!("failed to spawn cargo: {e}"))?;

        if !status.success() {
            anyhow::bail!("cargo build failed (exit {:?})", status.code());
        }
    }

    // Locate the .wasm artifact.
    let wasm_filename = format!("{}.wasm", pkg_name.replace('-', "_"));
    let target_dir = cargo_target_dir(&plugin_dir).unwrap_or_else(|| plugin_dir.join("target"));
    let wasm_path = target_dir
        .join("wasm32-wasip2/release")
        .join(&wasm_filename);
    if !wasm_path.exists() {
        anyhow::bail!("expected .wasm at {} — not found", wasm_path.display());
    }

    // Create output directory: <output_dir_root>/<plugin_name>/
    let out_dir = output_dir_root.join(&plugin_name);
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        anyhow::anyhow!("cannot create output directory {}: {e}", out_dir.display())
    })?;

    // wasm32-wasip2 produces a component directly — copy as-is.
    let wasm_out = out_dir.join("plugin.wasm");
    std::fs::copy(&wasm_path, &wasm_out).map_err(|e| anyhow::anyhow!("cannot copy .wasm: {e}"))?;
    println!("Copied {} -> {}", wasm_path.display(), wasm_out.display());

    // Write distribution plugin.toml.
    let out_toml_path = out_dir.join("plugin.toml");
    write_dist_manifest(&plugin_toml_str, &out_toml_path)?;
    println!("Writing {}", out_toml_path.display());

    let wasm_size = std::fs::metadata(&wasm_out).map(|m| m.len()).unwrap_or(0);
    println!();
    println!("Plugin ready for distribution:");
    println!("  {} ({} bytes)", wasm_out.display(), wasm_size);
    println!("  {}", out_toml_path.display());
    println!();
    println!("To test: treetags --plugin-dir {} FILE", out_dir.display());

    Ok(())
}

fn cargo_target_dir(plugin_dir: &Path) -> Option<PathBuf> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string()))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(plugin_dir)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let re = regex::Regex::new(r#""target_directory"\s*:\s*"([^"]+)""#).ok()?;
    re.captures(&stdout)
        .and_then(|c| c.get(1))
        .map(|m| PathBuf::from(m.as_str()))
}

fn load_global_config() -> FileConfig {
    let path = dirs::config_dir().map(|d| d.join("treetags-build-plugin").join("config.toml"));
    read_config_file(path.as_deref())
}

fn merge_project_config(base: &mut FileConfig, plugin_dir: &Path) {
    let local = read_config_file(Some(&plugin_dir.join(".treetags-build-plugin.toml")));
    if local.output_dir.is_some() {
        base.output_dir = local.output_dir;
    }
}

fn read_config_file(path: Option<&Path>) -> FileConfig {
    let path = match path {
        Some(p) if p.exists() => p,
        _ => return FileConfig::default(),
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(s)
}

fn parse_package_name(toml_str: &str) -> Option<String> {
    let table: toml::Table = toml::from_str(toml_str).ok()?;
    table
        .get("package")?
        .as_table()?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

fn parse_toml_string_field(toml_str: &str, field: &str) -> Option<String> {
    let table: toml::Table = toml::from_str(toml_str).ok()?;
    table.get(field)?.as_str().map(str::to_string)
}

fn check_cdylib(toml_str: &str, path: &Path) -> anyhow::Result<()> {
    let table: toml::Table = toml::from_str(toml_str)
        .map_err(|e| anyhow::anyhow!("cannot parse {}: {e}", path.display()))?;
    let crate_types: Vec<&str> = table
        .get("lib")
        .and_then(|l| l.as_table())
        .and_then(|l| l.get("crate-type"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    if !crate_types.contains(&"cdylib") {
        anyhow::bail!(
            "{} must have `crate-type = [\"cdylib\"]` under [lib]",
            path.display()
        );
    }
    Ok(())
}

fn check_wasm_target() -> anyhow::Result<()> {
    match Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
    {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("wasm32-wasip2") => Ok(()),
        Ok(_) => anyhow::bail!(
            "wasm32-wasip2 target not installed\nFix: rustup target add wasm32-wasip2"
        ),
        Err(_) => {
            eprintln!("warning: could not check rustup targets; proceeding anyway");
            Ok(())
        }
    }
}

fn write_plugin_toml_template(path: &Path) -> anyhow::Result<()> {
    let template = r#"name = "my-plugin"
version = "0.1.0"
# File extensions this plugin handles (e.g. ["java", "kt"])
extensions = []
# Language name used with --kinds-{lang} and --language-force (optional)
# language = "java"
# Extra names accepted by --language-force (optional)
# aliases = ["jvm"]
# Filename globs (matched against the basename) that select this plugin (optional)
# patterns = ["Dockerfile", "*.bzl"]
# Interpreter names matched against a #! shebang line (optional)
# interpreters = ["node"]
"#;
    std::fs::write(path, template)
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))
}

fn write_dist_manifest(source_toml: &str, dest_path: &Path) -> anyhow::Result<()> {
    let mut table: toml::Table = toml::from_str(source_toml)
        .map_err(|e| anyhow::anyhow!("cannot parse plugin.toml: {e}"))?;
    table.insert(
        "wasm_file".to_string(),
        toml::Value::String("plugin.wasm".to_string()),
    );
    table.insert(
        "abi_version".to_string(),
        toml::Value::Integer(i64::from(PLUGIN_ABI_VERSION)),
    );
    std::fs::write(dest_path, toml::to_string(&table)?)
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", dest_path.display()))
}

/// Finds the WASI SDK root for C cross-compilation. Checks (in order):
/// 1. `WASI_SDK_PATH` environment variable
/// 2. `$HOME/play/wasi-sdk-*` glob (dev convenience)
fn find_wasi_sdk() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WASI_SDK_PATH") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let play = PathBuf::from(home).join("play");
    if !play.exists() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&play)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("wasi-sdk"))
                    .unwrap_or(false)
        })
        .collect();
    candidates.sort();
    candidates.into_iter().last()
}
