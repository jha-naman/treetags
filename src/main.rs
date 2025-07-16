#![doc = include_str!("../README.md")]

use std::path::Path;
use std::process;

mod config;
mod file_finder;
pub mod parsers;
mod parser;
mod queries;
mod shell_to_regex;
mod split_by_newlines;
mod tag;
mod tag_processor;
mod tag_writer;
mod tags_config;

use crate::config::Config;
use crate::file_finder::FileFinder;
use crate::tag_processor::TagProcessor;
use crate::tag_writer::TagWriter;

/// The main entry point for the application.
///
/// Parses command line arguments, finds or creates a tag file,
/// processes source files to generate tags, and writes them to the output file.
fn main() {
    // Parse command line arguments
    let config = Config::new();

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
    // If writing to stdout, use the current directory as the base for file finding
    let search_base = Path::new(&tag_file_path);
    // let search_base = if tag_file_path == "-" {
    //     Path::new(".")
    // } else {
    //     Path::new(&tag_file_path)
    // };
    let file_finder = FileFinder::new(search_base, config.exclude.clone());
    let files = if !config.file_names.is_empty() {
        // Process both files and directories from the command line arguments
        file_finder.get_files_from_paths(&config.file_names)
    } else {
        file_finder.get_files_from_dir()
    };

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
    tag_writer.write_tags(&mut tags);
}
