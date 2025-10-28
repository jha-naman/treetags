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
- [x] Go
- [x] Rust
- [x] C
- [x] C++

### Basic navigation support without extension fields
- [x] Bash/Sh
- [x] C#
- [x] Elixir
- [x] ~Haskell~
- [x] Java
- [x] JavaScript
- [x] Julia
- [x] Lua
- [x] Ocaml
- [x] PHP
- [x] Python
- [x] Ruby
- [x] Scala

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

## Generate shell autocomplete scripts

Users can generate completions like:

```bash
# refer to your shell documentation for determining the correct path for autcomplete files
treetags completions bash > ~/.local/share/bash-completion/completions/treetags
treetags completions zsh > ~/.local/share/zsh/site-functions/_treetags
treetags completions fish > ~/.config/fish/completions/treetags.fish
```



## Running Integration Tests

Integration tests are built from test cases on demand

```bash
cargo build  # Generates test files
cargo test   # Runs all tests including generated ones
```

## Usage

Use the `--help` option to see supported command line arguments.

```
$ target/release/treetags --help
Generate vi compatible tags for multiple languages

Usage: treetags [OPTIONS] [FILE_NAMES]...

Arguments:
  [FILE_NAMES]...  List of file names to be processed when `--append` option is passed

Options:
  ... # Options omitted for brevity
```

