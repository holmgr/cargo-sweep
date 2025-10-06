# Changelog

All changes that are relevant to `cargo-sweep` users should be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixes

- Update hash algorithm for Rust >= 1.85 [#139](https://github.com/holmgr/cargo-sweep/pull/139)
- Don't panic when timestamp file is not found [#126](https://github.com/holmgr/cargo-sweep/pull/126)

### Changes

- Don't bail if cleaning fails in one project [#132](https://github.com/holmgr/cargo-sweep/pull/132)
- Add `--all` flag [#122](https://github.com/holmgr/cargo-sweep/pull/122)
- Improve output text when nothing is cleared [#127](https://github.com/holmgr/cargo-sweep/pull/127)
- Clarify help message for --time option [#125](https://github.com/holmgr/cargo-sweep/pull/125)
- Enable LTO for release builds [#135](https://github.com/holmgr/cargo-sweep/pull/135)

## [`0.7.0`](https://github.com/holmgr/cargo-sweep/compare/0.6.2...0.7.0)

### Fixes

- When rustc fails, show stderr if stdout is empty [#63](https://github.com/holmgr/cargo-sweep/pull/63)
- Kibibytes are now printed as `KiB`, not `kiB` [#69](https://github.com/holmgr/cargo-sweep/pull/69)
- Exit with non-zero status on failure [#72](https://github.com/holmgr/cargo-sweep/pull/72)
- Keep stamp file on dry run [#100](https://github.com/holmgr/cargo-sweep/pull/100)
- Fix invisible output in white-themed terminals [#103](https://github.com/holmgr/cargo-sweep/pull/103)
- Fix `--toolchains` not validating the provided toolchains [#115](https://github.com/holmgr/cargo-sweep/pull/115)

### Changes

- Display the total cleaned amount when sweeping multiple projects [#45](https://github.com/holmgr/cargo-sweep/pull/45)
- No longer give a hard error when a custom toolchain gives an error [#67](https://github.com/holmgr/cargo-sweep/pull/67)
- Don't print colors when stdout is not a terminal [#69](https://github.com/holmgr/cargo-sweep/pull/69)
- Add long `--verbose` and `--recursive` flags [#73](https://github.com/holmgr/cargo-sweep/pull/73)
- Make `-r/--recursive` traverse beyond Cargo directories [#78](https://github.com/holmgr/cargo-sweep/pull/78)
- Reduce non-verbose output to make it less noisy [#81](https://github.com/holmgr/cargo-sweep/pull/81)
- Refactor and add `-m` as a short flag for `--maxsize` [#87](https://github.com/holmgr/cargo-sweep/pull/87)
- Only show toolchain list once when using `--installed` [#88](https://github.com/holmgr/cargo-sweep/pull/88)
- Support multiple projects as input via CLI [#101](https://github.com/holmgr/cargo-sweep/pull/101)
- Add a `TRACE` log level enabled by two `--verbose` flags [#113](https://github.com/holmgr/cargo-sweep/pull/113)
- Tell user when `--recursive` is busy traversing directories [#120](https://github.com/holmgr/cargo-sweep/pull/120)
- Allow `--maxsize` to accept byte-unit sizes [#114](https://github.com/holmgr/cargo-sweep/pull/114)

## **0.6.2** and prior

Not documented, `CHANGELOG.md` was introduced after **0.6.2**.
