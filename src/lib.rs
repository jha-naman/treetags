/*!
Generate vi compatible tags for multiple languages.

Uses the tags queries defined
in the various official language parsers to detect tags.

## Usage

```rust,compile_fail
use treetags::Parser;

let file_path_relative_to_tag_file = "path/to/source_file.rs"; // file path relative to tag file
let extension = "rs";
let file_path_str = "source.file"; // file path relative to current directory
let mut parser = Parser::new();
let tags = parser.parse_file(&file_path_relative_to_tag_file, &file_path, extension);
```
 */

pub mod config;
pub mod file_finder;
pub mod parser;
pub mod parsers;
pub mod queries;
pub mod shell_to_regex;
pub mod split_by_newlines;
pub mod tag;
pub mod tag_processor;
pub mod tag_writer;
pub mod tags_config;

// Re-export commonly used items
pub use config::Config;
pub use parser::Parser;
pub use tag::{parse_tag_file, Tag};
