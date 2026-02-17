# TreeTags

Generate vi compatible tags for multiple languages.

Uses the tags queries defined in the various official language parsers to detect tags.
It can also make use of tree-sitter grammars and queries installed by the user on their system.

The goal is to have code navigation available in vim/nvim for multiple languages
with minimum effort and have reasonable performance.
[Extension Fields](https://docs.ctags.io/en/latest/man/ctags.1.html#extension-fields)
support is missing by design for most languages to make it easier to support multiple languages and
keep the program trivially easy to maintain.

Refer to [this](https://github.com/jha-naman/treetags/issues/1)
issue to see how tags compare to LSP.

By default, it will generate a new tag file in the current directory and look
for tags in files list passed during command line invokation. By default it
recursively traverses directories present in the list. Pass the `-R no`
or `--recurse no` options do not want directories to be recursively looked
into for tags.
If the `--append` option is used it will  update the existing tag file
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

### User provided grammars and queries
Users need to provide below things for treetags to be able to generate tags for
languages that do not have a builtin grammar supplied.

 - Precompiled tree-sitter grammar for the language.
   [tree-sitter-langs](https://github.com/emacs-tree-sitter/tree-sitter-langs/releases) project is one source with many pre-compiled grammars
 - [Tags query](https://tree-sitter.github.io/tree-sitter/4-code-navigation.html) file for the language
 - List of file extensions for which the grammar/tag combo is to be used
 - Add an entry to the `[[user_grammars]]` toml array of the treetags config file

 An example config for Kotlin language located at the defualt location of `~/.config/treetags/config.toml`
 is shown below:

 ```toml
[[user_grammars]]
language_name = "kotlin"
grammar_lib_path = "/home/naman/.local/share/nvim/lazy/nvim-treesitter/parser/kotlin.so"
query_file_path = "/home/naman/.config/treetags/queries/kotlin.scm"
extensions = ["kt", "kts"]
 ```

#### Languages with built-in fallback tags query and extensions
Some languages have tags query and extensions built-in into treetags. Users only
need to provide the tree-sitter grammar in that case for treetags to be able to
generate tags for that language.

 - [x] Kotlin

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

