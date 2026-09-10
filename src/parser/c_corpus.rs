//! A/B corpus-diff harness (test-only) comparing the two C tag backends over a
//! large external source tree (the Linux kernel by default). This is NOT part of
//! the shipping product; it exists purely to drive the native C hook toward
//! parity with the tree-sitter oracle.
//!
//! Run it with:
//!   cargo test --release --lib c_corpus_diff -- --ignored --nocapture
//!
//! Configuration via env vars:
//!   TT_CORPUS_ROOT   root directory to walk (default: $HOME/play/linux)
//!   TT_CORPUS_LIMIT  max number of files to process (default: 3000; 0 = all)
//!   TT_CORPUS_SUBDIRS comma-separated subdirs (relative to root) to restrict to
//!                     (default: unrestricted). Example: "kernel,mm,fs".

#![cfg(test)]

use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use crate::parser::cpp::{C_KIND_DEFAULTS, C_KIND_OPTIONALS};
use crate::parser::tree_free::common::linear::HookOptions;
use crate::parser::TagKindConfig;
use crate::tag::Tag;
use clap::Parser as _;

fn oracle(code: &[u8], path: &str) -> Vec<Tag> {
    let kinds = TagKindConfig::from_string("", C_KIND_DEFAULTS, C_KIND_OPTIONALS);
    let config = crate::config::Config::parse_from(["treetags"]);
    crate::parser::cpp::generate(&mut tree_sitter::Parser::new(), code, path, &kinds, &config)
        .unwrap_or_default()
}

fn native(source: &str, path: &str) -> Vec<Tag> {
    let kinds = TagKindConfig::from_string("", C_KIND_DEFAULTS, C_KIND_OPTIONALS);
    let config = crate::config::Config::parse_from(["treetags"]);
    crate::parser::tree_free::c::generate(source, path, HookOptions::from_config(&kinds, &config))
        .unwrap_or_default()
}

fn sort_key(t: &Tag) -> (String, Option<String>, String) {
    (
        t.name.clone(),
        t.kind.as_ref().map(|k| k.to_string()),
        t.address.clone(),
    )
}

fn sorted(mut tags: Vec<Tag>) -> Vec<Tag> {
    tags.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
    tags
}

/// Recursively collect `*.c` and `*.h` files under `root`, honoring an optional
/// subdir allow-list, capped at `limit` (0 = unbounded).
fn collect_files(root: &Path, subdirs: &[String], limit: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let roots: Vec<PathBuf> = if subdirs.is_empty() {
        vec![root.to_path_buf()]
    } else {
        subdirs.iter().map(|s| root.join(s)).collect()
    };
    for r in roots {
        walk(&r, &mut files, limit);
        if limit != 0 && files.len() >= limit {
            break;
        }
    }
    if limit != 0 {
        files.truncate(limit);
    }
    files
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, limit: usize) {
    if limit != 0 && out.len() >= limit {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // Deterministic order so a capped run is reproducible.
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if limit != 0 && out.len() >= limit {
            return;
        }
        if path.is_dir() {
            // Skip the git metadata dir; nothing else is excluded.
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            walk(&path, out, limit);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("c") | Some("h")
        ) {
            out.push(path);
        }
    }
}

/// Classify a divergent tag into a coarse root-cause bucket, using the tag's
/// kind letter plus a peek at the source line it points at.
fn categorize(tag: &Tag, source: &str) -> &'static str {
    let line = tag_line(tag, source);
    let kind = tag.kind.as_deref().unwrap_or("?");

    // Construct-driven signals first (more specific), then fall back to kind.
    if line.contains("__attribute__")
        || line.contains("__packed")
        || line.contains("__aligned")
        || line.contains("__must_check")
        || line.contains("__init")
        || line.contains("__exit")
    {
        return "gnu-attribute/section-macro";
    }
    if line.contains("asm(") || line.contains("__asm__") {
        return "inline-asm";
    }
    // A comma at top level of the declarator line hints multi-declarator.
    if kind == "v" && line.matches(',').count() >= 1 && line.contains(';') {
        return "multi-declarator-variable";
    }
    if (kind == "v" || kind == "t") && line.contains("(*") {
        return "function-pointer-decl";
    }
    // K&R style: identifier list in parens with no types, params on following
    // lines. Heuristic: function tag whose line ends without `{` or `;`.
    if kind == "f" {
        let t = line.trim_end();
        if !t.ends_with('{') && !t.ends_with(';') && !t.ends_with(')') {
            return "knr-or-multiline-function";
        }
        return "function-other";
    }
    if kind == "m" {
        return "struct-member";
    }
    match kind {
        "d" => "macro-#define",
        "h" => "include-header",
        "e" => "enumerator",
        "g" => "enum",
        "s" => "struct",
        "u" => "union",
        "t" => "typedef",
        "v" => "variable",
        _ => "other",
    }
}

/// The full source line that a tag's address points at (best-effort, by matching
/// the escaped `/^…$/` pattern back to a raw line prefix). We approximate by
/// scanning for the tag name; good enough for bucketing.
fn tag_line<'a>(tag: &Tag, source: &'a str) -> &'a str {
    for line in source.lines() {
        if line.contains(tag.name.as_str()) {
            return line;
        }
    }
    ""
}

#[derive(Default)]
struct CategoryStat {
    count: usize,
    examples: Vec<(String, String)>, // (path, tag debug)
}

/// Debug helper: set TT_CORPUS_FILE=<relative path under root> to dump the exact
/// oracle-only and native-only tags for a single file, then this test asserts
/// nothing (it just prints). Useful for pinning a specific divergence.
#[test]
#[ignore = "single-file C backend diff; set TT_CORPUS_FILE and run with --ignored"]
fn c_corpus_one() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let root = std::env::var("TT_CORPUS_ROOT").unwrap_or_else(|_| format!("{home}/play/linux"));
    let root = PathBuf::from(root);
    let rel = std::env::var("TT_CORPUS_FILE").expect("set TT_CORPUS_FILE");
    let path = root.join(&rel);
    let bytes = std::fs::read(&path).expect("read file");
    let source = std::str::from_utf8(&bytes).expect("utf-8");
    let want = sorted(oracle(&bytes, &rel));
    let got = sorted(native(source, &rel));

    let want_keys: std::collections::BTreeSet<String> =
        want.iter().map(|t| format!("{t:?}")).collect();
    let got_keys: std::collections::BTreeSet<String> =
        got.iter().map(|t| format!("{t:?}")).collect();

    eprintln!("=== {rel} : oracle={} native={} ===", want.len(), got.len());
    eprintln!("\n--- ORACLE-ONLY (missing from native) ---");
    for t in &want {
        if !got_keys.contains(&format!("{t:?}")) {
            eprintln!("  {t:?}");
        }
    }
    eprintln!("\n--- NATIVE-ONLY (extra) ---");
    for t in &got {
        if !want_keys.contains(&format!("{t:?}")) {
            eprintln!("  {t:?}");
        }
    }
}

/// Temporary triage pass: classifies each divergence by whether it is a
/// kind-mismatch (same name+address, different kind), a field-diff (same
/// name+kind+address, different extension fields), or a pure oracle-only /
/// native-only tag. Prints the biggest buckets so we can pick the next fix.
#[test]
#[ignore = "triage helper; run explicitly with --ignored"]
fn c_corpus_analyze() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let root = PathBuf::from(
        std::env::var("TT_CORPUS_ROOT").unwrap_or_else(|_| format!("{home}/play/linux")),
    );
    let limit: usize = std::env::var("TT_CORPUS_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);
    let files = collect_files(&root, &[], limit);

    // bucket label -> (count, up to 4 examples, distinct files)
    type Bucket = (usize, Vec<String>, std::collections::BTreeSet<String>);
    let mut buckets: BTreeMap<String, Bucket> = BTreeMap::new();
    let mut bump = |label: String, example: String, file: &str| {
        let e = buckets.entry(label).or_default();
        e.0 += 1;
        if e.1.len() < 4 {
            e.1.push(example);
        }
        e.2.insert(file.to_string());
    };

    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let Ok(source) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let res = catch_unwind(AssertUnwindSafe(|| {
            (oracle(&bytes, &rel), native(source, &rel))
        }));
        let Ok((want, got)) = res else { continue };
        if want == got {
            continue;
        }

        // Key by (name, address). Value: list of (kind, full-fields-repr).
        type Val = Vec<(Option<String>, String)>;
        let mut want_map: BTreeMap<(String, String), Val> = BTreeMap::new();
        let mut got_map: BTreeMap<(String, String), Val> = BTreeMap::new();
        let fields = |t: &Tag| format!("{:?}", t.extension_fields);
        for t in &want {
            want_map
                .entry((t.name.clone(), t.address.clone()))
                .or_default()
                .push((t.kind.as_ref().map(|k| k.to_string()), fields(t)));
        }
        for t in &got {
            got_map
                .entry((t.name.clone(), t.address.clone()))
                .or_default()
                .push((t.kind.as_ref().map(|k| k.to_string()), fields(t)));
        }

        let all_keys: std::collections::BTreeSet<_> =
            want_map.keys().chain(got_map.keys()).cloned().collect();
        for key in all_keys {
            let w = want_map.get(&key);
            let g = got_map.get(&key);
            match (w, g) {
                (Some(w), Some(g)) if w == g => {}
                (Some(w), Some(g)) => {
                    let wk = w[0].0.as_deref().unwrap_or("?");
                    let gk = g[0].0.as_deref().unwrap_or("?");
                    let label = if wk != gk {
                        format!("KIND-MISMATCH oracle={wk} native={gk}")
                    } else {
                        format!("FIELD-DIFF kind={wk}")
                    };
                    bump(
                        label,
                        format!("{rel}: {} | oracle={:?} native={:?}", key.0, w, g),
                        &rel,
                    );
                }
                (Some(w), None) => bump(
                    format!("ORACLE-ONLY kind={}", w[0].0.as_deref().unwrap_or("?")),
                    format!("{rel}: {} @ {}", key.0, key.1),
                    &rel,
                ),
                (None, Some(g)) => bump(
                    format!("NATIVE-ONLY kind={}", g[0].0.as_deref().unwrap_or("?")),
                    format!("{rel}: {} @ {}", key.0, key.1),
                    &rel,
                ),
                (None, None) => {}
            }
        }
    }

    let mut sorted: Vec<_> = buckets.into_iter().collect();
    sorted.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    eprintln!("\n================ DIVERGENCE TRIAGE ================");
    for (label, (count, examples, files)) in sorted.iter().take(25) {
        eprintln!("\n[{count:>5}] {label}  ({} files)", files.len());
        for ex in examples {
            eprintln!("    {ex}");
        }
    }
    eprintln!("==================================================\n");
}

#[test]
#[ignore = "large external corpus diff; run explicitly with --ignored"]
fn c_corpus_diff() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let root = std::env::var("TT_CORPUS_ROOT").unwrap_or_else(|_| format!("{home}/play/linux"));
    let root = PathBuf::from(root);
    let limit: usize = std::env::var("TT_CORPUS_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);
    let subdirs: Vec<String> = std::env::var("TT_CORPUS_SUBDIRS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default();

    assert!(root.is_dir(), "corpus root {root:?} does not exist");

    let files = collect_files(&root, &subdirs, limit);
    eprintln!(
        "corpus root: {}\nsubdir filter: {:?}\nfile cap: {}\nfiles selected: {}",
        root.display(),
        subdirs,
        limit,
        files.len()
    );

    let mut scanned = 0usize;
    let mut identical = 0usize;
    let mut divergent = 0usize;
    let mut skipped_utf8 = 0usize;
    let mut total_oracle = 0usize;
    let mut total_native = 0usize;
    let mut total_divergent_tags = 0usize;
    let mut panicked: Vec<String> = Vec::new();

    // category -> (missing stat, extra stat)
    let mut missing_cats: BTreeMap<&'static str, CategoryStat> = BTreeMap::new();
    let mut extra_cats: BTreeMap<&'static str, CategoryStat> = BTreeMap::new();
    // Raw kind-letter tallies (authoritative, no source-line heuristic).
    let mut missing_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut extra_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut divergent_examples: Vec<String> = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let source = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(_) => {
                skipped_utf8 += 1;
                continue;
            }
        };
        scanned += 1;

        let result = catch_unwind(AssertUnwindSafe(|| {
            let want = sorted(oracle(&bytes, &rel));
            let got = sorted(native(source, &rel));
            (want, got)
        }));

        let (want, got) = match result {
            Ok(pair) => pair,
            Err(_) => {
                panicked.push(rel.clone());
                continue;
            }
        };

        total_oracle += want.len();
        total_native += got.len();

        if want == got {
            identical += 1;
            continue;
        }
        divergent += 1;

        // Compute set differences. Tags are compared by full value; use a
        // multiset keyed on the full debug repr so duplicates are handled.
        let want_keys: Vec<String> = want.iter().map(|t| format!("{t:?}")).collect();
        let got_keys: Vec<String> = got.iter().map(|t| format!("{t:?}")).collect();

        let mut want_ms: BTreeMap<&str, usize> = BTreeMap::new();
        for k in &want_keys {
            *want_ms.entry(k.as_str()).or_default() += 1;
        }
        let mut got_ms: BTreeMap<&str, usize> = BTreeMap::new();
        for k in &got_keys {
            *got_ms.entry(k.as_str()).or_default() += 1;
        }

        // Missing: in oracle but not native.
        for (t, k) in want.iter().zip(&want_keys) {
            let in_got = got_ms.get(k.as_str()).copied().unwrap_or(0);
            let in_want = want_ms.get(k.as_str()).copied().unwrap_or(0);
            if in_got < in_want {
                total_divergent_tags += 1;
                *missing_kind
                    .entry(t.kind.as_deref().unwrap_or("?").to_string())
                    .or_default() += 1;
                let cat = categorize(t, source);
                let stat = missing_cats.entry(cat).or_default();
                stat.count += 1;
                if stat.examples.len() < 3 {
                    stat.examples.push((rel.clone(), format!("{t:?}")));
                }
            }
        }
        // Extra: in native but not oracle.
        for (t, k) in got.iter().zip(&got_keys) {
            let in_got = got_ms.get(k.as_str()).copied().unwrap_or(0);
            let in_want = want_ms.get(k.as_str()).copied().unwrap_or(0);
            if in_got > in_want {
                total_divergent_tags += 1;
                *extra_kind
                    .entry(t.kind.as_deref().unwrap_or("?").to_string())
                    .or_default() += 1;
                let cat = categorize(t, source);
                let stat = extra_cats.entry(cat).or_default();
                stat.count += 1;
                if stat.examples.len() < 3 {
                    stat.examples.push((rel.clone(), format!("{t:?}")));
                }
            }
        }

        if divergent_examples.len() < 20 {
            divergent_examples.push(format!(
                "  {} : oracle={} native={}",
                rel,
                want.len(),
                got.len()
            ));
        }
    }

    // ---- report ----
    eprintln!("\n================ C CORPUS A/B DIFF ================");
    eprintln!("files scanned (utf-8 C/H): {scanned}");
    eprintln!("  identical:  {identical}");
    eprintln!("  divergent:  {divergent}");
    eprintln!("  panicked:   {}", panicked.len());
    eprintln!("  skipped (non-utf8): {skipped_utf8}");
    let ident_pct = if scanned > 0 {
        identical as f64 / scanned as f64 * 100.0
    } else {
        0.0
    };
    eprintln!("  identical rate: {ident_pct:.2}%");
    eprintln!("total oracle tags: {total_oracle}");
    eprintln!("total native tags: {total_native}");
    eprintln!("total divergent tags: {total_divergent_tags}");
    let div_rate = if total_oracle > 0 {
        total_divergent_tags as f64 / total_oracle as f64 * 100.0
    } else {
        0.0
    };
    eprintln!("divergent-tag rate (divergent/oracle): {div_rate:.3}%");

    let report_cats = |title: &str, cats: &BTreeMap<&'static str, CategoryStat>| {
        eprintln!("\n----- {title} -----");
        let mut ordered: Vec<_> = cats.iter().collect();
        ordered.sort_by(|a, b| b.1.count.cmp(&a.1.count));
        for (cat, stat) in ordered {
            eprintln!("[{}] x{}", cat, stat.count);
            for (p, dbg) in &stat.examples {
                eprintln!("    {p}");
                eprintln!("      {dbg}");
            }
        }
    };
    let report_kinds = |title: &str, kinds: &BTreeMap<String, usize>| {
        eprintln!("\n----- {title} -----");
        let mut ordered: Vec<_> = kinds.iter().collect();
        ordered.sort_by(|a, b| b.1.cmp(a.1));
        for (k, n) in ordered {
            eprintln!("  kind '{k}': {n}");
        }
    };
    report_kinds("MISSING divergent tags by kind letter", &missing_kind);
    report_kinds("EXTRA divergent tags by kind letter", &extra_kind);

    report_cats(
        "MISSING (in oracle, absent from native) by category",
        &missing_cats,
    );
    report_cats(
        "EXTRA (in native, absent from oracle) by category",
        &extra_cats,
    );

    if !panicked.is_empty() {
        eprintln!("\n----- PANICKED FILES ({}) -----", panicked.len());
        for p in panicked.iter().take(50) {
            eprintln!("  {p}");
        }
    }

    eprintln!("\n----- sample divergent files -----");
    for line in &divergent_examples {
        eprintln!("{line}");
    }
    eprintln!("==================================================\n");
}
