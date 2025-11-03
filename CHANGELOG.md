# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
 - `ctags` like handling for `-f <tagfile>` command line args param

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
