# TreeTags

Add code navigation for multiple languages in Vi/Vim/Neovim.

Leverages tree-sitter to fulfill the goal of supporting multiple programming
languages with minimal effort without sacrificing maintainability or performance.

To get a brief overview of what treetags does and how it compares to Universal
ctags see [here](#what-does-treetags-do).

### More information

- [Installation](#installation)
- [Natively Supported Languages](#natively-supported-languages)
- [Languages supported by WASM plugins](#wasm-plugins)
- [Recommended usage](#recommended-usage)

## Natively Supported Languages

Support for these languages is available out of the box in treetags

### Full support with extension fields
- [x] C
- [x] C++
- [x] Go
- [x] JavaScript
- [x] Python
- [x] Rust
- [x] TypeScript

Refer to Universal ctags [documentation](https://docs.ctags.io/en/latest/man/ctags.1.html#extension-fields)
for more about extension fields.

### Basic navigation support without extension fields
- [x] Bash/Sh
- [x] C#
- [x] Elixir
- [x] ~Haskell~
- [x] Java
- [x] Julia
- [x] Lua
- [x] Ocaml
- [x] PHP
- [x] Ruby
- [x] Scala

## WASM plugins

> [!NOTE]
> WASM plugins are still a work in progress feature. They provide all of the
> same functionality as natively supported languages but the discovery and
> installation flows for users are being worked out. See [here](WASM_PLUGINS.md)
> for more details on how to install the plugins and for more details of
> the implementation.

Treetags has support for these languages via user installable WASM plugins

### Full support with extension fields
- [x] Java

## Extending treetags language support via tree-sitter tag queries and grammars

Users need to provide two things for treetags to be able to generate tags for
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

### Languages with preprovided tags query and extensions

Some languages have tags query and extensions built-in into treetags. Users only
need to provide the tree-sitter grammar in that case for treetags to be able to
generate tags for that language. Leave the `query_file_path` empty for these
languages  to use the tags queries provided with treetags.

 - [x] Kotlin
 - [x] Gleam

## Installation
There are three prerequisites to build `treetags`

1. Install Rust and C developmet toolchains on your system

2. Add wasm32-wasip2 target for rustc

```
rustup target add wasm32-wasip2
```

3. Make sure that rustc uses the clang shipped with [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases)
by setting the `binCC` environment varaible.

```
export WASI_SDK_PATH=/home/username/play/wasi-sdk-30.0-arm64-linux
export binCC="${WASI_SDK_PATH}/bin/clang --sysroot=${WASI_SDK_PATH}/share/wasi-sysroot"
```

Building treetags once development setup is complete:

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

## Command line options

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

## What does treetags do

Treetags creates a tags file that vim can use for allowing the user to easily
navigate their source code files.

We can quote vim help files to get an idea of what a tag and a tags file are:

> What is a tag?  It is a location where an identifier is defined.  An example
> is a function definition in a C or C++ program.  A list of tags is kept in a
> tags file.  This can be used by Vim to directly jump from any place to the
> tag, the place where an identifier is defined.

To get a full overview of tags related functionality backed into vim/neovim one
can use `:help tagsrch` in vim or refer to the [online docs](https://vimhelp.org/tagsrch.txt.html)

This is similar to what [Universal ctags](https://ctags.io/) and other verions
of ctags do. The ctags versions are much more mature and battle tested than
treetags. Universal ctags in particular also has support for many more languages
and is actively maintained.

Treetags differs from these in two ways technically. First is that it uses
tree-sitter for parsing code. Second is that treetags is multithreaded and can
parse multiple files simultaneously. Another important difference is that
treetags is primarily maintained by a single person.

#### How treetags compares to  LSP

Refer to [this](https://github.com/jha-naman/treetags/issues/1)
issue to see how tags compare to LSP.
