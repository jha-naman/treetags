mod config;
pub use config::Config;
mod tag;
pub use tag::parse_tag_file;
pub use tag::Tag;
mod parser;
pub use parser::Parser;

mod tags_config;
