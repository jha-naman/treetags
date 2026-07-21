//! treetags-build-index — generate the per-ABI plugin distribution index.
//!
//! Reads one or more built plugin directories (each the `<name>/` output of
//! `treetags-build-plugin`, containing `plugin.wasm` + `plugin.toml`), and emits
//! the `index.json` for a single ABI bucket. Optionally merges the ABI into a
//! shared `abis.json`. Asset URLs point at the GitHub Release that hosts the
//! blobs, following the `<base>/<name>.{wasm,toml}` naming convention.
//!
//! Usage:
//!   treetags-build-index \
//!     --asset-base-url https://github.com/OWNER/treetags/releases/download/plugin-store-v3 \
//!     --out gh-pages/3/index.json \
//!     [--abi 3] [--abis-file gh-pages/abis.json] [--generated-at 2026-07-21T00:00:00Z] \
//!     dist/java dist/echo
//!
//! Build:
//!   cargo build --release --bin treetags-build-index
use clap::Parser;
use std::path::{Path, PathBuf};

use treetags::plugin::index::{sha256_hex, AbisFile, IndexEntry, PluginIndex};
use treetags::plugin::manifest::PluginManifest;

#[derive(Parser)]
#[command(
    name = "treetags-build-index",
    about = "Generate the per-ABI plugin distribution index (index.json / abis.json)"
)]
struct Args {
    /// Built plugin directories, each containing plugin.wasm + plugin.toml.
    plugin_dirs: Vec<PathBuf>,

    /// ABI version for this index. If omitted, derived from the manifests
    /// (which must all agree).
    #[arg(long)]
    abi: Option<u32>,

    /// Base URL under which `<name>.wasm` / `<name>.toml` assets are hosted,
    /// e.g. a GitHub Release download URL.
    #[arg(long)]
    asset_base_url: String,

    /// Path to write index.json.
    #[arg(long)]
    out: PathBuf,

    /// Optional abis.json to merge this ABI into (read-modify-write).
    #[arg(long)]
    abis_file: Option<PathBuf>,

    /// Optional RFC3339 timestamp stamped into the generated files. CI can pass
    /// `$(date -u +%Y-%m-%dT%H:%M:%SZ)`; omitted keeps the field out of the JSON.
    #[arg(long)]
    generated_at: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut entries: Vec<IndexEntry> = Vec::with_capacity(args.plugin_dirs.len());
    let mut derived_abi: Option<u32> = None;

    for dir in &args.plugin_dirs {
        let Some((entry, abi)) = process_plugin_dir(dir, &args.asset_base_url)? else {
            println!("Skipping internal plugin at {}", dir.display());
            continue;
        };

        // All plugins in one index must target the same ABI.
        if let Some(prev) = derived_abi {
            if prev != abi {
                anyhow::bail!(
                    "plugin `{}` targets ABI {abi} but a previous plugin targets ABI {prev}",
                    entry.name
                );
            }
        }
        derived_abi = Some(abi);

        if let Some(requested) = args.abi {
            if requested != abi {
                anyhow::bail!(
                    "plugin `{}` targets ABI {abi} but --abi {requested} was requested",
                    entry.name
                );
            }
        }

        entries.push(entry);
    }

    let abi = args
        .abi
        .or(derived_abi)
        .ok_or_else(|| anyhow::anyhow!("no plugins given and no --abi provided; nothing to do"))?;

    // Deterministic output regardless of argument order.
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let index = PluginIndex {
        abi_version: abi,
        generated_at: args.generated_at.clone(),
        plugins: entries,
    };

    write_json(&args.out, &index)?;
    println!(
        "Wrote {} ({} plugin{}, ABI {abi})",
        args.out.display(),
        index.plugins.len(),
        if index.plugins.len() == 1 { "" } else { "s" }
    );

    if let Some(abis_path) = &args.abis_file {
        let existing = read_json_if_exists::<AbisFile>(abis_path)?;
        let merged = AbisFile::merged(existing, abi, args.generated_at.clone());
        write_json(abis_path, &merged)?;
        println!(
            "Updated {} (abis: {:?}, latest {})",
            abis_path.display(),
            merged.abis,
            merged.latest
        );
    }

    Ok(())
}

/// Parse a built plugin directory into an [`IndexEntry`], returning the ABI it
/// targets so the caller can enforce a single ABI across the index. Returns
/// `None` for internal (dev/test-only) plugins, which are excluded from the
/// published index so end users never see or install them.
fn process_plugin_dir(
    dir: &Path,
    asset_base_url: &str,
) -> anyhow::Result<Option<(IndexEntry, u32)>> {
    let manifest_path = dir.join("plugin.toml");
    let manifest_str = std::fs::read_to_string(&manifest_path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", manifest_path.display()))?;
    let manifest: PluginManifest = toml::from_str(&manifest_str)
        .map_err(|e| anyhow::anyhow!("cannot parse {}: {e}", manifest_path.display()))?;

    if manifest.internal {
        return Ok(None);
    }

    let wasm_path = manifest.wasm_path(dir);
    let wasm_bytes = std::fs::read(&wasm_path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", wasm_path.display()))?;
    let sha256 = sha256_hex(&wasm_bytes);
    let size = wasm_bytes.len() as u64;

    let abi = manifest.abi_version;
    let entry = IndexEntry::from_manifest(&manifest, asset_base_url, sha256, size);
    Ok(Some((entry, abi)))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", parent.display()))?;
        }
    }
    let json = serde_json::to_string_pretty(value)?;
    std::fs::write(path, format!("{json}\n"))
        .map_err(|e| anyhow::anyhow!("cannot write {}: {e}", path.display()))
}

fn read_json_if_exists<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()))?;
    let value = serde_json::from_str(&s)
        .map_err(|e| anyhow::anyhow!("cannot parse {}: {e}", path.display()))?;
    Ok(Some(value))
}
