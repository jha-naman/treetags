#![doc = include_str!("../README.md")]

use std::path::Path;
use std::process;

mod built_in_grammars;
mod builtin_langs;
mod config;
mod file_finder;
mod kinds_listing;
mod lang_resolve;
mod language_parser;
mod parser;
mod plugin;
mod queries;
mod shell_to_regex;
mod split_by_newlines;
mod tag;
mod tag_processor;
mod tag_writer;
mod tags_config;
mod user_grammars;

use crate::config::Config;
use crate::file_finder::FileFinder;
use crate::tag_processor::TagProcessor;
use crate::tag_writer::TagWriter;
use rayon::slice::ParallelSliceMut;

use clap_complete::generate;

fn main() {
    let config = Config::new();

    if handle_early_exit_commands(&config) {
        return;
    }

    let tag_file_path = match file_finder::determine_tag_file_path(&config.tag_file, config.append)
    {
        Ok(path) => path,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    let file_finder = match FileFinder::from_patterns(config.exclude.clone(), config.recurse) {
        Ok(finder) => finder,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    let file_result = if !config.file_names.is_empty() {
        file_finder.get_files_from_paths(&config.file_names)
    } else {
        file_finder.get_files_from_paths(&[".".to_string()])
    };

    file_result.print_errors();

    if file_result.files.is_empty() && file_result.has_errors() {
        eprintln!("No files found due to errors");
        process::exit(1);
    }

    let files = file_result.files;

    if config.suggest_plugins {
        suggest_plugins(&config, &files);
        return;
    }

    let tag_processor = TagProcessor::new(tag_file_path.clone(), config.workers, config.clone());
    let mut tags = tag_processor.process_files(files);

    if config.append {
        let existing_tags = file_finder::parse_tag_file(&tag_file_path);
        tags.extend(existing_tags);
    }

    if config.sort {
        tags.par_sort_unstable_by(|a, b| a.sort_cmp(b));
    }

    let tag_writer = TagWriter::new(tag_file_path);
    tag_writer.write_tags(&mut tags, true, config.sort);
}

fn handle_early_exit_commands(config: &Config) -> bool {
    if let Some(command) = &config.command {
        match command {
            config::Commands::Completions { shell } => {
                let mut cmd = config.augmented_command_for_completions();
                let bin_name = cmd.get_name().to_string();
                generate(*shell, &mut cmd, bin_name, &mut std::io::stdout());
            }
            config::Commands::ListPlugins => {
                plugin::print_plugin_list(&config.plugin_dirs, &config.plugins_dir);
            }
            config::Commands::Plugin { action } => {
                if let Err(err) = handle_plugin_command(action, config) {
                    eprintln!("error: {err:#}");
                    process::exit(1);
                }
            }
        }
        return true;
    }
    if let Some(lang) = &config.list_kinds {
        let lang_opt = if lang.is_empty() {
            None
        } else {
            Some(lang.as_str())
        };
        kinds_listing::handle(lang_opt, config);
        return true;
    }
    if config.print_language {
        print_languages(config);
        return true;
    }
    if let Some(lang) = &config.list_maps {
        list_maps(config, lang);
        return true;
    }
    false
}

/// Dispatches a `treetags plugin ...` subcommand.
fn handle_plugin_command(action: &config::PluginCommands, config: &Config) -> anyhow::Result<()> {
    use config::PluginCommands;
    use plugin::client;

    let base = client::resolve_base_url(config.plugin_index_url.as_deref());
    let plugins_dir = &config.plugins_dir;
    match action {
        PluginCommands::Available { refresh } => client::available(&base, *refresh, plugins_dir),
        PluginCommands::Install {
            name,
            force,
            refresh,
        } => client::install(&base, name, *force, *refresh, plugins_dir),
        PluginCommands::Uninstall { name } => client::uninstall(name, plugins_dir),
        PluginCommands::Installed => {
            plugin::print_plugin_list(&config.plugin_dirs, plugins_dir);
            Ok(())
        }
        PluginCommands::Update { name, refresh } => {
            client::update(&base, name.as_deref(), *refresh, plugins_dir)
        }
    }
}

/// Reports available (uninstalled) plugins that could handle files in the tree —
/// both types nothing handles today and natively-supported types a plugin offers
/// an enhanced take on. Reuses the discovered file set from the main flow.
fn suggest_plugins(config: &Config, files: &[String]) {
    use std::collections::BTreeSet;

    let registry = language_parser::LanguageParserRegistry::new(config);
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

    // Resolve each distinct extension once — name-based resolution is stable per
    // extension, so this stays cheap even on large trees.
    let mut discovered: BTreeSet<String> = BTreeSet::new();
    let mut handled: BTreeSet<String> = BTreeSet::new();
    for name in files {
        let rel = Path::new(name);
        let Some(ext) = rel.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if !discovered.insert(ext.clone()) {
            continue; // already classified this extension
        }
        if tag_processor::select_language(&registry, config, &cwd.join(name), rel).is_some() {
            handled.insert(ext);
        }
    }

    if discovered.is_empty() {
        println!("No files with recognizable extensions found; no plugin suggestions.");
        return;
    }

    let base = plugin::client::resolve_base_url(config.plugin_index_url.as_deref());
    if let Err(err) = plugin::client::suggest(&base, &discovered, &handled, &config.plugins_dir) {
        eprintln!("error: {err:#}");
        process::exit(1);
    }
}

/// Prints the effective extension/pattern maps (optionally for one language).
fn list_maps(config: &Config, only: &str) {
    let registry = language_parser::LanguageParserRegistry::new(config);
    let filter = if only.is_empty() { None } else { Some(only) };
    for (lang, exts, patterns) in registry.language_maps() {
        if filter.is_some_and(|f| !lang.eq_ignore_ascii_case(f)) {
            continue;
        }
        let mut items: Vec<String> = exts.iter().map(|e| format!(".{e}")).collect();
        items.extend(patterns.iter().cloned());
        println!("{:<12} {}", lang, items.join(" "));
    }
}

/// Prints the resolved language (or `NONE`) for each supplied file and returns.
fn print_languages(config: &Config) {
    let registry = language_parser::LanguageParserRegistry::new(config);
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    for name in &config.file_names {
        let path = cwd.join(name);
        let lang = tag_processor::select_language(&registry, config, &path, Path::new(name))
            .map(|sel| registry.parser(sel.lang).language_name().to_string())
            .unwrap_or_else(|| "NONE".to_string());
        println!("{}: {}", name, lang);
    }
}
