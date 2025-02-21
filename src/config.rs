use clap::Parser;
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
    /// Number of threads to use for parsing files
    #[arg(long, default_value = "4")]
    pub workers: usize,
    /// Files/directories matching the pattern will not be used while generating tags
    #[arg(long)]
    pub exclude: Vec<String>,
}

impl Config {
    pub fn new() -> Config {
        let config = Self::parse();
        config.validate();

        config
    }

    fn validate(&self) {
        let tag_file = Path::new(&self.tag_file);
        let mut path_components = tag_file.components();
        let _ = path_components.next();
        if path_components.next() != None {
            eprintln!(
                "tagfile should only contain the tagfile name, not the path: {}",
                tag_file.display()
            );
            std::process::exit(1);
        }
    }
}
