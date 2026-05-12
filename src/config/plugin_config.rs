use clap::CommandFactory;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Extracts `--plugin-dir` and `--plugins-dir` values from raw args without clap.
/// Falls back to the default plugins dir when `--plugins-dir` is absent.
pub fn extract_plugin_dirs(args: &[String]) -> (Vec<PathBuf>, PathBuf) {
    let plugin_dirs = super::extract_flag_values(args, "plugin-dir")
        .into_iter()
        .map(PathBuf::from)
        .collect();
    let plugins_dir = super::extract_flag_values(args, "plugins-dir")
        .into_iter()
        .last()
        .map(PathBuf::from)
        .unwrap_or_else(super::paths::get_default_plugins_dir);
    (plugin_dirs, plugins_dir)
}

/// Returns the set of plugin language names discovered from plugin manifests.
/// Used only for help-text injection; kinds routing is handled by `rewrite_all_kinds_args`.
pub fn plugin_language_names(args: &[String]) -> HashSet<String> {
    let (plugin_dirs, plugins_dir) = extract_plugin_dirs(args);
    crate::plugin::registry::scan_language_names(&plugin_dirs, Some(&plugins_dir))
}

/// Rewrites ALL `--kinds-{lang}` / `--kinds-{lang}=VALUE` args to `--_kinds lang=VALUE`
/// for any language (builtin or plugin). Also handles the deprecated `--{lang}-kinds` flags
fn rewrite_all_kinds_args(args: Vec<String>) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];

        // Handle canonical --kinds-{lang}[=VALUE]
        if let Some(rest) = arg.strip_prefix("--kinds-") {
            if let Some((lang, value)) = rest.split_once('=') {
                out.push("--_kinds".to_string());
                out.push(format!("{}={}", lang, value));
            } else {
                let value = if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                    i += 1;
                    args[i].clone()
                } else {
                    String::new()
                };
                out.push("--_kinds".to_string());
                out.push(format!("{}={}", rest, value));
            }
            i += 1;
            continue;
        }

        // Handle deprecated --rust-kinds[=VALUE]
        if let Some(value_part) = arg.strip_prefix("--rust-kinds") {
            let value = extract_flag_value(value_part, &args, &mut i);
            out.push("--_kinds".to_string());
            out.push(format!("rust={}", value));
            i += 1;
            continue;
        }

        // Handle deprecated --go-kinds[=VALUE]
        if let Some(value_part) = arg.strip_prefix("--go-kinds") {
            let value = extract_flag_value(value_part, &args, &mut i);
            out.push("--_kinds".to_string());
            out.push(format!("go={}", value));
            i += 1;
            continue;
        }

        out.push(arg.clone());
        i += 1;
    }
    out
}

/// Extracts a value from either `=VALUE` suffix or the next arg (if not a flag).
fn extract_flag_value(suffix: &str, args: &[String], i: &mut usize) -> String {
    if let Some(value) = suffix.strip_prefix('=') {
        value.to_string()
    } else if suffix.is_empty() && *i + 1 < args.len() && !args[*i + 1].starts_with('-') {
        *i += 1;
        args[*i].clone()
    } else {
        String::new()
    }
}

/// Builds a `lang → kinds` map by scanning `args` for all `--kinds-{lang}`, `--rust-kinds`,
/// and `--go-kinds` arguments. Duplicate keys keep the last value.
pub fn extract_kinds_map(args: &[String]) -> HashMap<String, String> {
    let rewritten = rewrite_all_kinds_args(args.to_vec());
    super::extract_flag_values(&rewritten, "_kinds")
        .into_iter()
        .filter_map(|s| s.split_once('=').map(|(l, k)| (l.to_owned(), k.to_owned())))
        .collect()
}

/// Strips all `--kinds-{lang}`, `--rust-kinds`, and `--go-kinds` args from `args`
/// so that clap does not encounter unknown flags.
pub fn strip_kinds_args(args: Vec<String>) -> Vec<String> {
    let rewritten = rewrite_all_kinds_args(args);
    let mut out = Vec::with_capacity(rewritten.len());
    let mut i = 0;
    while i < rewritten.len() {
        if rewritten[i] == "--_kinds" {
            i += 2; // skip flag and value
        } else {
            out.push(rewritten[i].clone());
            i += 1;
        }
    }
    out
}

/// Builds the clap `Command` augmented with a `--kinds-{lang}` arg for every known language
/// (both builtin and plugin-only), so they appear correctly in `--help` output.
/// These args are stripped before clap sees real user input, so the injected entries are
/// display-only.
pub fn command_with_all_lang_kinds(plugin_langs: &HashSet<String>) -> clap::Command {
    let mut cmd = super::Config::command();

    // Inject help entries for builtin languages
    for desc in crate::builtin_langs::BUILTIN_LANG_DESCRIPTORS {
        let name = format!("kinds-{}", desc.lang);
        let help = format!("{} kinds to generate tags for", desc.lang);
        cmd = cmd.arg(
            clap::Arg::new(name.clone())
                .long(name)
                .value_name("KINDS")
                .default_value("")
                .help(help),
        );
    }

    // Inject help entries for plugin-only languages
    let mut sorted_langs: Vec<&String> = plugin_langs.iter().collect();
    sorted_langs.sort();
    for lang in sorted_langs {
        let name = format!("kinds-{lang}");
        let help = format!("{lang} plugin: kinds to generate tags for");
        cmd = cmd.arg(
            clap::Arg::new(name.clone())
                .long(name)
                .value_name("KINDS")
                .default_value("")
                .help(help),
        );
    }

    cmd
}

/// Augments the completion command with possible-values for `--list-kinds`.
/// Called only for the completion path — not for actual argument parsing —
/// so it does not restrict what values are accepted at runtime.
pub fn augment_list_kinds_for_completion(
    mut cmd: clap::Command,
    plugin_langs: &std::collections::HashSet<String>,
) -> clap::Command {
    let mut lang_names: Vec<String> = crate::builtin_langs::BUILTIN_LANG_DESCRIPTORS
        .iter()
        .map(|desc| desc.lang.to_string())
        .collect();
    for lang in plugin_langs {
        lang_names.push(lang.clone());
    }
    lang_names.sort();
    cmd = cmd.mut_arg("list_kinds", |a| {
        a.value_parser(clap::builder::PossibleValuesParser::new(lang_names))
    });
    cmd
}
