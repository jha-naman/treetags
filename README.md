# TreeTags

Generate vi compatible tags for multiple languages. Uses the tags queries defined
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


## Supported Languages
- [x] C
- [x] C++
- [x] Elixir
- [x] Go
- [x] Java
- [x] JavaScript
- [x] Lua
- [x] Ocaml
- [x] PHP
- [x] Python
- [x] Ruby
- [x] Rust

## Installation

```
cargo build --release
cp target/release/treetags /somewhere/in/the/PATH/
```

## Usage

```
$ target/release/treetags --help
Generate vi compatible tags for multiple languages

Usage: treetags [OPTIONS] [FILE_NAMES]...

Arguments:
  [FILE_NAMES]...  List of file names to be processed when `--append` option is passed

Options:
  -f <TAG_FILE>            Name to be used for the tagfile, should not contain path separator [default: tags]
      --append             Append tags to existing tag file instead of reginerating the file from scratch.
                           Need to pass in list of file names for which new tags are to be generated.
                           Will panic if the tag file doesn't already exist in current or one of the parent
                           directories.
      --workers <WORKERS>  Number of threads to use for parsing files [default: 4]
  -h, --help               Print help
```
