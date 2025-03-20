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

mod config;
pub use config::Config;
mod tag;
pub use tag::parse_tag_file;
pub use tag::Tag;
mod parser;
pub use parser::Parser;

mod tags_config;
