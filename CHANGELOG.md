# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### CHANGED
 - Refactor code for handling `--kinds-[language]` CLI arguments to simplify
   code and remove duplicate code

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
