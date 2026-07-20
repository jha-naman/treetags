# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### ADDED
 - Add support for relative regex when using `--map-<lang>` cli arg
 - Add help text for the `--map-<lang>` cli arg

### FIXED
 - Fix incorrect order of pattern precedence when using `--langmap`
 - Don't read content resolved files twice
 - Make sure builtin grammars are loaded only once on startup
 - Remove some unnecessary allocations from parser matching hotpath
 - Improve parser matching via patterns/aliases/shebang when all file extensions
   for a parser are overridden by a plugin

- [0.10.0]

### Added
 - Many-to-many mapping between file extensions and parsers
 - Add support for `--force-language` cli arg
 - Add support for matching parsers with filename globs (eg "Dockerfile")
 - Add support for configuring parser matching via cli args
   Introduces `--map-<lang>=[.ext|glob]`, `--langmap=<lang>:<[.ext]|[glob]> and `--list-maps`
 - Add support for guessing language based on file content when `-G` is on
   Uses shebang and vim/emacs modelines

### Changed
 - Use rayon's parallel sort for sorting the tags
 - Stop writing to a single Vec<Tag> from across workers
 - Sort each files tags individually

## [0.9.0]

### Added
 - Generate tags for Rust structs/enums/unions in macro invocations

### FIXED
 - Handle string properties in javascript objects
 - Handle edge cases when JS/TS tag names are strings (eg { "string": 1 })
 - Fix bug causing multiple tests to write to and share a single grammar config file

## [0.8.0]

### Added
 - Add support for WASM plugins

### CHANGED
 - Refactor code for handling `--kinds-[language]` CLI arguments to simplify
   code and remove duplicate code

### FIXED
 - Typescript interface type tags generation

## [0.7.0]

### Added
 - Add extension fields support for Python
 - Add extension fields support for TypeScript

## [0.6.0]

### Added
 - Add support for generating tags using tree-sitter grammars and tag queries installed by the user independently of treetags
 - Add default tags query file for Gleam language.
   Users only need to install the gleam tree-sitter grammar to generate tags using treetags.
 - Add extension fields support for JavaScript.

## [0.5.2]

### Fixed
 - Thread stack overflow error when generating tags for large C/C++ files

## [0.5.1] 2025-12-04

### Added
 - Add support for various missing C++ tag kinds
   - h  included header files
   - D  parameters inside macro definitions
   - L  goto labels
   - A  namespace aliases
   - U  using namespace statements
   - Z  template parameters
   - z  function parameters inside function or prototype definitions

## [0.5.0] 2025-11-27

### Changed
 - Updated default value of `recurse` cli option to `true`.

### Fixed
 - Error when handling bool cli arguments without explicit user specified value. eg `treetags -R`

## [0.4.0] 2025-11-05

### Added
 - `ctags` like handling for `-f <tagfile>` command line args param
 - Add support for reading command arguments from single file or directory with `.ctags` files
 - Add basic tagfile validation for existing tagfile in `append` mode

### Removed
 - Recursive search for tag file in current directory and it's parent folders

## [0.3.2] 2025-10-28

### Added
 - Add basic shell autocomplete integration
 - Add `kinds-rust` and `kinds-go` command line arguments (related (issue)[https://github.com/jha-naman/treetags/issues/22])

## [0.3.1] 2025-10-20

### Added
 - Github workflow for running basic lint and tests on pull requests and pushes.
 - Github workflow for creating releases for various platforms on tags matching semver format

## [0.3.0] 2025-10-18

### Added
 - Emit !_TAG_FILE_SORTED pseudo tag to enable faster searching of sorted tag files
 - Extension Fields support for C++
 - Extension Fields support for C
 - Better error handling
    - For tag file detection and opening
    - For UTF8 decoding during tag creation using treesitter queries
    - For errors raised during reading source files for creating tags
    - For errors raised during creating treesitter tag configuration from a tag query

## [0.2.2] 2025-09-01

### Added
 - Support for additional file extensions
    - C++:  `hh`, `hpp`, `hxx`
    - C: `h`
    - Python: `pyw`
    - Typescript: `tsx`
    - Shell script: `bash`

## [0.2.1] 2025-08-29

### Removed
 - Haskell support. See relevant [issue](https://github.com/jha-naman/treetags/issues/7).

## [0.2.0] 2025-08-26

### Added
 - Add Scala support
 - Add Julia support
 - Add Haskell support

## [0.1.3] 2025-07-02

### Added
 - Extension Fields support for Rust
 - Extension Fields support for Go
 - Golden file based integration tests

### Fixed
- Truncate address string if its too long
- Escape backslash in tag address

## [0.1.2] 2025-03-30

### Added
 - Bash/sh script tag support

## [0.1.1] 2025-03-28

### Added
 - C# language tag support

## [0.1.0] 2025-03-27

### Added
 - Initial release with basic tag support for: _C, C++, Elixir, Go, Java, JavaScript, Lua, Ocaml, PHP, Python, Ruby, Rust_
