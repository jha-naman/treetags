use std::path::Path;
use std::process;

mod config;
mod file_finder;
mod shell_to_regex;
mod tag_processor;
mod tag_writer;

use crate::config::Config;
use crate::file_finder::FileFinder;
use crate::tag_processor::TagProcessor;
use crate::tag_writer::TagWriter;

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
    let file_finder = FileFinder::new(Path::new(&tag_file_path), config.exclude.clone());
    let files = if !config.file_names.is_empty() {
        config.file_names.clone()
    } else {
        file_finder.get_files_from_dir()
    };

    // Process files and generate tags
    let tag_processor = TagProcessor::new(tag_file_path.clone(), config.workers);
    let mut tags = tag_processor.process_files(files);

    // Append existing tags if needed
    if config.append {
        let existing_tags = file_finder::parse_tag_file(&tag_file_path);
        tags.extend(existing_tags);
    }

    // Write tags to file
    let tag_writer = TagWriter::new(tag_file_path);
    tag_writer.write_tags(&mut tags);
}
