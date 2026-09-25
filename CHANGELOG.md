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
- `ccbox update` installs the latest release (or `--version X.Y.Z`; `--force`
  for a downgrade) after checking its SHA-256, then rewires `settings.json`
  for the new version and prints the release notes. `--check` only reports.
- A daily background check for a new release. When one is out, the bottom
  border shows `⬆ ccbox <version> available · run ccbox update`.
  `CCBOX_UPDATE_CHECK=0` turns the check and the notice off.
- `CCBOX_RELEASES_URL` points `ccbox update`, the check and `install.sh` at
  another release source.
- `ccbox status` and `ccbox version` (also `ccbox --version`); `ccbox status`
  adds the installed version, the latest known release and whether the check
  is on. `--snapshot` reports the same under `update`.

### Changed

- **Upgrading from a source build:** a ccbox installed before this release has no
  `ccbox update`. Re-run the install one-liner once; later releases arrive
  through `ccbox update`.
- Row toggles are verbs: `ccbox show|hide|flip tasks|subagents`. `/ccbox`
  passes its arguments straight to `ccbox`. `ccbox toggle …` still works.
- `install.sh` installs the prebuilt binary and verifies its checksum, and
  builds with cargo only when there is no prebuilt binary for the platform or
  `CCBOX_BUILD_FROM_SOURCE=1`. It wires `settings.json` by running the binary
  it just installed, so it no longer needs python3, and it no longer rewrites
  or backs up `settings.json` when nothing changes. `CCBOX_BIN_DIR` sets the
  install directory.
- `uninstall.sh` also removes a prebuilt binary that cargo did not install.
- `ccbox --help` lists the commands and the new variables, keeps its table
  indentation, and names the real built-in themes.
- The minimum Rust version for source builds is 1.85.
