/*!
Generate vi compatible tags for multiple languages.

Uses the tags queries defined
in the various official language parsers to detect tags.

The goal is to have code navigation available in vim/nvim for multiple languages
with minimum effort and have reasonable performance.
[Extension Fields](https://docs.ctags.io/en/latest/man/ctags.1.html#extension-fields)
support is missing by design to make it easier to support multiple languages and
keep the program trivially easy to maintain.

By default, it will generate a new tag file in the current directory and look
for tags recursively in the current directory and its children.
If the `--append` option is used, it will look for a tag file in the current
directory or any of its parent directories, and update the tag file if it exists
with tags generated from the list of files passed via command line.

## Usage

```text
$ target/release/treetags --help
Generate vi compatible tags for multiple languages

Usage: treetags [OPTIONS] [FILE_NAMES]...

Arguments:
  [FILE_NAMES]...  List of file and directory names to be processed when `--append` option is passed

Options:
  -f <TAG_FILE>            Name to be used for the tagfile, should not contain path separator [default: tags]
      --append             Append tags to existing tag file instead of reginerating the file from scratch.
                           Need to pass in list of file and/or directory names for which new tags are to be generated.
                           Will panic if the tag file doesn't already exist in current or one of the parent
                           directories.
      --workers <WORKERS>  Number of threads to use for parsing files [default: 4]
      --exclude <EXCLUDE>  Files/directories matching the pattern will not be used while generating tags
      --options <OPTIONS>  Value passed in this arg is currently being ignored.
                           Kept for compatibility with `vim-gutentags` plugin. [default: ]
  -h, --help               Print help```

 */

use std::path::Path;
use std::process;

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
