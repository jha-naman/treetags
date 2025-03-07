use clap::Parser;
use std::fs;
use std::path::Path;

#[derive(Parser)]
#[command(about = "Generate vi compatible tags for multiple languages", long_about = None)]
pub struct Config {
    /// Name to be used for the tagfile, should not contain path separator
    #[arg(short = 'f', default_value = "tags")]
    pub tag_file: String,
    #[arg(long, verbatim_doc_comment)]
    /// Append tags to existing tag file instead of reginerating the file from scratch.
    /// Need to pass in list of file names for which new tags are to be generated.
    /// Will panic if the tag file doesn't already exist in current or one of the parent
    /// directories.
    pub append: bool,
    /// List of file names to be processed when `--append` option is passed
    pub file_names: Vec<String>,
    #[arg(long, default_value = "4")]
    /// Number of threads to use for parsing files
    pub workers: usize,
    /// Files/directories matching the pattern will not be used while generating tags
    #[arg(long)]
    pub exclude: Vec<String>,
    /// Value passed in this arg is currently being ignored.
    /// Kept for compatibility with `vim-gutentags` plugin.
    #[arg(long = "options", default_value = "", verbatim_doc_comment)]
    pub _options: String,
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

impl Config {
    pub fn new() -> Config {
        let mut config = Self::parse();
        config.validate();
        config.parse_file_args();

        config
    }

    fn parse_file_args(&mut self) {
        for pattern in &self.exclude.clone() {
            match pattern.strip_prefix("@") {
                None => continue,
                Some(file_name) => {
                    let file = match fs::read_to_string(file_name) {
                        Ok(contents) => contents,
                        Err(_) => {
                            eprintln!("Could not read file from exclude pattern: {}", pattern);
                            std::process::exit(1);
                        }
                    };

                    self.exclude
                        .extend(file.lines().map(|line| line.to_string()));
                }
            }
        }
    }

    fn validate(&self) {
        let tag_file = Path::new(&self.tag_file);
        let mut path_components = tag_file.components();
        let _ = path_components.next();
        if path_components.next().is_some() {
            eprintln!(
                "tagfile should only contain the tagfile name, not the path: {}",
                tag_file.display()
            );
            std::process::exit(1);
        }

        self.validate_file_args();
    }

    fn validate_file_args(&self) {
        for pattern in &self.exclude {
            match pattern.strip_prefix("@") {
                None => continue,
                Some(file_name) => match fs::exists(file_name) {
                    Ok(_) => {}
                    Err(_) => {
                        eprintln!("Could not read file from exclude pattern: {}", pattern);
                        std::process::exit(1);
                    }
                },
            }
        }
    }
}
