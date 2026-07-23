//! treetags-build-site — generate the plugin distribution content for the site.
//!
//! Reads built plugin directories (each the `<name>/` output of
//! `treetags-build-plugin`, containing `plugin.wasm` + `plugin.toml`) and writes,
//! under `--out-dir`, everything the Jekyll site needs for one ABI bucket:
//!
//!   <out-dir>/<abi>/index.json   machine index the treetags client fetches
//!   <out-dir>/<abi>/index.md     human-readable plugin table (Jekyll page)
//!   <out-dir>/abis.json          list of published ABI versions
//!   <out-dir>/index.md           landing page linking every ABI bucket
//!
//! Point `--out-dir` at a staging folder (e.g. `plugins_index`) that is later
//! moved into the site under `treetags/`, served at `/treetags/…` (e.g. the
//! client fetches `/treetags/<abi>/index.json`). Asset URLs point at the GitHub
//! Release that hosts the `.wasm`/`.toml` blobs.
//!
//! Only the current ABI's files are (re)written; other ABI buckets already in
//! `out-dir` are left untouched, and the landing page is regenerated from the
//! merged `abis.json`. Internal (dev/test) plugins are excluded.
//!
//! Usage:
//!   treetags-build-site \
//!     --asset-base-url https://github.com/jha-naman/treetags/releases/download/plugin-store-v3 \
//!     --out-dir plugins_index \
//!     [--abi 3] [--generated-at 2026-07-21T00:00:00Z] \
//!     dist/java
//!
//! Build:
//!   cargo build --release --locked --bin treetags-build-site
use clap::Parser;
use std::path::{Path, PathBuf};

use treetags::plugin::index::{sha256_hex, AbisFile, IndexEntry, PluginIndex};
use treetags::plugin::manifest::PluginManifest;

#[derive(Parser)]
#[command(
    name = "treetags-build-site",
    about = "Generate the plugin index.json + Jekyll Markdown pages for the site"
)]
struct Args {
    /// Built plugin directories, each containing plugin.wasm + plugin.toml.
    plugin_dirs: Vec<PathBuf>,

    /// ABI version for this bucket. If omitted, derived from the manifests
    /// (which must all agree).
    #[arg(long)]
    abi: Option<u32>,

    /// Base URL under which `<name>.wasm` / `<name>.toml` assets are hosted,
    /// e.g. a GitHub Release download URL.
    #[arg(long)]
    asset_base_url: String,

    /// Output root (e.g. `plugins_index`, later moved into the site under `treetags/`).
    #[arg(long)]
    out_dir: PathBuf,

    /// Optional RFC3339 timestamp stamped into the generated files. Pass
    /// `$(date -u +%Y-%m-%dT%H:%M:%SZ)`; omitted keeps the field out.
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

        // All plugins in one bucket must target the same ABI.
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

    let generated_at = args.generated_at.as_deref();
    let out = &args.out_dir;

    // Per-ABI machine index + human page.
    let index = PluginIndex {
        abi_version: abi,
        generated_at: args.generated_at.clone(),
        plugins: entries.clone(),
    };
    let index_path = out.join(abi.to_string()).join("index.json");
    write_json(&index_path, &index)?;
    let page_path = out.join(abi.to_string()).join("index.md");
    write_str(&page_path, &render_abi_page(abi, &entries, generated_at))?;

    // Merge this ABI into abis.json and regenerate the landing page.
    let abis_path = out.join("abis.json");
    let existing = read_json_if_exists::<AbisFile>(&abis_path)?;
    let merged = AbisFile::merged(existing, abi, args.generated_at.clone());
    write_json(&abis_path, &merged)?;
    write_str(
        &out.join("index.md"),
        &render_landing(&merged, generated_at),
    )?;

    println!(
        "Wrote {} ({} plugin{}, ABI {abi}); abis: {:?}, latest {}",
        index_path.display(),
        index.plugins.len(),
        if index.plugins.len() == 1 { "" } else { "s" },
        merged.abis,
        merged.latest
    );
    Ok(())
}

/// Parse a built plugin directory into an [`IndexEntry`], returning the ABI it
/// targets so the caller can enforce a single ABI across the bucket. Returns
/// `None` for internal (dev/test-only) plugins, which are excluded so end users
/// never see or install them.
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

// ---------------------------------------------------------------------------
// Markdown rendering (Jekyll pages)
// ---------------------------------------------------------------------------

/// JSON-style front matter, matching the existing site's convention.
fn front_matter(title: &str) -> String {
    format!(
        "---\n{{\n  \"layout\": \"page\",\n  \"title\": \"{}\"\n}}\n---\n\n",
        json_escape(title)
    )
}

/// Human-readable page listing the plugins in one ABI bucket.
fn render_abi_page(abi: u32, entries: &[IndexEntry], generated_at: Option<&str>) -> String {
    let mut s = front_matter(&format!("TreeTags plugins (ABI {abi})"));
    s.push_str(&format!(
        "Language plugins compatible with treetags builds that support plugin ABI **{abi}**. \
         Run `treetags plugin available` to see what your build can install, or \
         `treetags plugin install <name>`.\n\n"
    ));

    if entries.is_empty() {
        s.push_str("_No plugins published for this ABI yet._\n");
    } else {
        s.push_str("| Plugin | Version | Language | Extensions | Install |\n");
        s.push_str("| --- | --- | --- | --- | --- |\n");
        for e in entries {
            let lang = e.language.clone().unwrap_or_default();
            let exts = e
                .extensions
                .iter()
                .map(|x| format!("`.{x}`"))
                .collect::<Vec<_>>()
                .join(" ");
            s.push_str(&format!(
                "| {} | {} | {} | {} | `treetags plugin install {}` |\n",
                md(&e.name),
                md(&e.version),
                md(&lang),
                exts,
                md(&e.name),
            ));
        }
        s.push_str("\nMachine-readable index: [`index.json`](index.json)\n");
    }
    push_generated(&mut s, generated_at);
    s
}

/// Landing page linking every published ABI bucket (relative links so the mount
/// path doesn't matter).
fn render_landing(abis: &AbisFile, generated_at: Option<&str>) -> String {
    let mut s = front_matter("TreeTags plugins");
    s.push_str(
        "Downloadable language plugins for \
         [TreeTags](https://github.com/jha-naman/treetags), grouped by the plugin ABI \
         version your treetags build supports. The CLI selects the right set automatically:\n\n",
    );
    s.push_str("```\ntreetags plugin available     # list plugins your build can install\ntreetags plugin install NAME  # install one\n```\n\n");
    s.push_str("## Versions\n\n");
    for abi in &abis.abis {
        let latest = if *abi == abis.latest {
            " — latest"
        } else {
            ""
        };
        s.push_str(&format!("- [ABI {abi}]({abi}/){latest}\n"));
    }
    push_generated(&mut s, generated_at);
    s
}

fn push_generated(s: &mut String, generated_at: Option<&str>) {
    if let Some(ts) = generated_at {
        s.push_str(&format!("\n<small>Generated {ts}</small>\n"));
    }
}

/// Escape Markdown table cell content (only `|` is special in a cell).
fn md(s: &str) -> String {
    s.replace('|', "\\|")
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ---------------------------------------------------------------------------
// IO helpers
// ---------------------------------------------------------------------------

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    write_str(path, &format!("{}\n", serde_json::to_string_pretty(value)?))
}

fn write_str(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", parent.display()))?;
        }
    }
    std::fs::write(path, contents)
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
