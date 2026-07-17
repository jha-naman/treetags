#![doc = include_str!("../README.md")]

use std::process;

mod built_in_grammars;
mod builtin_langs;
mod config;
mod file_finder;
mod kinds_listing;
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

    let tag_processor = TagProcessor::new(tag_file_path.clone(), config.workers, config.clone());
    let mut tags = tag_processor.process_files(files);

    if config.append {
        let existing_tags = file_finder::parse_tag_file(&tag_file_path);
        tags.extend(existing_tags);
    }

    if config.sort {
        tags.par_sort_unstable_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.file_name.cmp(&b.file_name))
                .then_with(|| a.address.cmp(&b.address))
                .then_with(|| a.kind.cmp(&b.kind))
        });
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
    false
}
