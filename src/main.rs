#![doc = include_str!("../README.md")]

use std::process;

mod built_in_grammars;
mod config;
mod file_finder;
mod parser;
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

use clap::CommandFactory;
use clap_complete::{generate, Shell};

/// The main entry point for the application.
///
/// Parses command line arguments, finds or creates a tag file,
/// processes source files to generate tags, and writes them to the output file.
fn main() {
    // Parse command line arguments
    let config = Config::new();

    // Handle completion subcommand
    if let Some(command) = &config.command {
        match command {
            config::Commands::Completions { shell } => {
                generate_completions(*shell);
                return;
            }
        }
    }

    // Determine tag file path
    let tag_file_path = match file_finder::determine_tag_file_path(&config.tag_file, config.append)
    {
        Ok(path) => path,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    // Get files to process
    let file_finder = match FileFinder::from_patterns(config.exclude.clone(), config.recurse) {
        Ok(finder) => finder,
        Err(err) => {
            eprintln!("{}", err);
            process::exit(1);
        }
    };

    let file_result = if !config.file_names.is_empty() {
        // Process specified files and directories
        file_finder.get_files_from_paths(&config.file_names)
    } else {
        // Default to current directory when no files specified
        file_finder.get_files_from_paths(&[".".to_string()])
    };

    // Print any warnings about file access issues
    file_result.print_errors();

    // Exit if no files were found and there were errors
    if file_result.files.is_empty() && file_result.has_errors() {
        eprintln!("No files found due to errors");
        process::exit(1);
    }

    let files = file_result.files;

    // Process files and generate tags
    let tag_processor = TagProcessor::new(tag_file_path.clone(), config.workers, config.clone());
    let mut tags = tag_processor.process_files(files);

    // Append existing tags if needed
    if config.append {
        let existing_tags = file_finder::parse_tag_file(&tag_file_path);
        tags.extend(existing_tags);
    }

    if config.sort {
        // Sort tags by name
        tags.sort_by(|a, b| a.name.cmp(&b.name));
    }

    // Write tags to file
    let tag_writer = TagWriter::new(tag_file_path);
    tag_writer.write_tags(&mut tags, true, config.sort);
}

/// Generate shell completions for the specified shell
fn generate_completions(shell: Shell) {
    let mut cmd = Config::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
}
