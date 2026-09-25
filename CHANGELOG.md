# Changelog

All notable changes to ccbox are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). One version covers the binary, the
plugin and the marketplace entry; `scripts/release.sh` bumps them together.

## [Unreleased]

### Added

- Tagged releases: pushing a `vX.Y.Z` tag builds prebuilt `ccbox` binaries for
  macOS (arm64, x86_64) and Linux (x86_64, aarch64; static musl) and publishes
  them with a `SHA256SUMS` manifest. Pull requests build the same four
  tarballs without publishing.
- `scripts/release.sh X.Y.Z` bumps every version field, moves this section
  under the new version, commits `release: vX.Y.Z` and creates the tag.
