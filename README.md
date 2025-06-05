# TreeTags

Generate vi compatible tags for multiple languages.

Uses the tags queries defined in the various official language parsers to detect tags.

The goal is to have code navigation available in vim/nvim for multiple languages
with minimum effort and have reasonable performance.
[Extension Fields](https://docs.ctags.io/en/latest/man/ctags.1.html#extension-fields)
support is missing by design for most languages to make it easier to support multiple languages and
keep the program trivially easy to maintain.

Refer to [this](https://github.com/jha-naman/treetags/issues/1)
issue to see how tags compare to LSP.

By default, it will generate a new tag file in the current directory and look
for tags recursively in the current directory and its children.
If the `--append` option is used, it will look for a tag file in the current
directory or any of its parent directories, and update the tag file if it exists
with tags generated from the list of files passed via command line.


## Supported Languages

### Full support with extension fields
- [x] Rust

### Basic navigation support without extension fields
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
- [x] C#
- [x] Bash/Sh

## Installation
Install Rust and C developmet toolchains to build `treetags`

```
cargo build --release
cp target/release/treetags /somewhere/in/the/PATH/
```

## Recommended Usage

While it is fine to manually invoke `treetags` to generate tags file for a project,
the recommended way is to use the [gutentags](https://github.com/ludovicchabant/vim-gutentags)
plugin to manage the tags file. There is a nice write-up on setting up gutentags
[here](https://www.reddit.com/r/vim/comments/d77t6j/guide_how_to_setup_ctags_with_gutentags_properly/),
which can be useful for setting things up.

You will have to configure gutentags to use `treetags` as the tags generator at
a minimum in your vim/nvim configuration file.

```vimscript
let g:gutentags_ctags_executable = 'treetags'
```

Or, if you are using lua for configuration

```lua
vim.g.gutentags_ctags_executable = 'treetags'
```

## Usage

```
$ target/release/treetags --help
Generate vi compatible tags for multiple languages

Usage: treetags [OPTIONS] [FILE_NAMES]...

Arguments:
  [FILE_NAMES]...  List of file names to be processed when `--append` option is passed

Options:
  -f <TAG_FILE>
          Name to be used for the tagfile, should not contain path separator [default: tags]
      --append <APPEND_RAW>
          Append tags to existing tag file instead of reginerating the file from scratch.
          Need to pass in list of file names for which new tags are to be generated.
          Will panic if the tag file doesn't already exist in current or one of the parent
          directories. [default: no]
      --workers <WORKERS>
          Number of threads to use for parsing files [default: 4]
      --exclude <EXCLUDE>
          Files/directories matching the pattern will not be used while generating tags
      --options <OPTIONS>
          Value passed in this arg is currently being ignored.
          Kept for compatibility with `vim-gutentags` plugin. [default: ]
      --sort <SORT_RAW>
          Whether to sort the files or not.
          Values of 'yes', 'on', 'true', '1' set it to true
          Values of 'no', '0', 'off', 'false' set it to false [default: yes]
      --extras <EXTRAS>
          Enable extra tag information (e.g., +q for qualified tags, +f for file scope) [default: ]
      --format <FORMAT>
          Value passed in this arg is currently being ignored.
          Kept for compatibility with `tagbar` plugin. [default: ]
      --excmd <EXCMD>
          Value passed in this arg is currently being ignored.
          Kept for compatibility with `tagbar` plugin. [default: ]
      --fields <FIELDS>
          Include selected extension fields (e.g., +l for line numbers, +S for signatures) [default: ]
      --language-force <LANGUAGE_FORCE>
          Value passed in this arg is currently being ignored.
          Kept for compatibility with `tagbar` plugin. [default: ]
      --rust-kinds <RUST_KINDS>
          Comma-separated list of Rust tag kinds to generate.
          Available kinds: n(module), s(struct), g(enum), u(union), i(trait),
          f(function), c(impl), m(field), e(enum_variant), C(constant), v(variable),
          t(type_alias), M(macro) [default: ]
  -h, --help
          Print help
```
