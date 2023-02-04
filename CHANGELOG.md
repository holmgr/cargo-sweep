# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Don't print colors when stdout is not a terminal [#69](https://github.com/holmgr/cargo-sweep/pull/69)
- Kibibytes are now printed as `KiB`, not `kiB` [#69](https://github.com/holmgr/cargo-sweep/pull/69)
- No longer give a hard error when a custom toolchain gives an error [#67](https://github.com/holmgr/cargo-sweep/pull/67)
- Reduce non-verbose output to make it less noisy [#81](https://github.com/holmgr/cargo-sweep/pull/81)
- Refactor and add `-m` as a short flag for `--maxsize` [#87](https://github.com/holmgr/cargo-sweep/pull/87)

### Fixed

- Exit with non-zero status on failure [#72](https://github.com/holmgr/cargo-sweep/pull/72)
- When rustc fails, show stderr if stdout is empty [#63](https://github.com/holmgr/cargo-sweep/pull/63)
