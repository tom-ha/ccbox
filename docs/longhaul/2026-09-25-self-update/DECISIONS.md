# DECISIONS — self-update run

Each section: the question, the options, the pick, and what was done meanwhile.

## D1. What counts as a "dirty tree" for `scripts/release.sh`

- **Question:** the spec says a dirty tree refuses the bump. Do untracked files count?
- **Options:** (a) any `git status --porcelain` output; (b) tracked changes only.
- **Pick:** (b). The script stages only the five files it writes, so untracked files cannot leak
  into the release commit, and the owner's main checkout carries untracked files
  (`openspec/changes/add-release-and-update/`, `.claude/goal-runs/`) that would otherwise block
  `scripts/release.sh 0.6.0`.
- **Also:** it refuses an empty `## [Unreleased]`, since the workflow would refuse to publish
  without notes anyway, and a detached HEAD, since it prints `git push origin <branch> vX.Y.Z`.
  If a step fails after editing, an `ERR` trap restores the five files.

## D2. Runner labels (override 9)

- **Checked:** https://docs.github.com/en/actions/reference/runners/github-hosted-runners on
  2026-09-25. `macos-13` is no longer listed; Intel macOS is `macos-15-intel`/`macos-26-intel`.
  Arm64 Linux runners `ubuntu-24.04-arm` are listed for public repos.
- **Options:** (a) `macos-15-intel` for x86_64-darwin; (b) cross-compile x86_64-darwin on arm64
  `macos-15`; Linux aarch64 via `cross`, or native on `ubuntu-24.04-arm`.
- **Pick:** both darwin targets on `macos-15` (x86_64 via `rustup target add`), x86_64 musl on
  `ubuntu-24.04`, aarch64 musl on `ubuntu-24.04-arm`, each with `musl-tools` for ring's C code.
  No `cross`, no Docker, no QEMU: every job is a plain `cargo build --target`.

## D3. Action pins

- `actions/checkout` v7.0.1 `3d3c42e5aac5ba805825da76410c181273ba90b1`,
  `actions/upload-artifact` v7.0.1 `043fb46d1a93c77aae656e7c1c64a875d1fc6a0a`,
  `actions/download-artifact` v8.0.1 `3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c`
  (latest releases on 2026-09-25, resolved with `gh api`). Rust comes from the runner's
  preinstalled rustup, and publishing uses the preinstalled `gh`, so no other action is used.

## D4. `rust-version` goes from 1.74 to 1.85

- **Question:** `ureq` 3.4 (and `ureq-proto`, `zeroize`, `getrandom` 0.4, `indexmap` 2.14,
  `hashbrown` 0.17 under it) declare `rust-version = "1.85"`; `Cargo.toml` said 1.74.
- **Options:** (a) pin older crate versions that still build on 1.74 (ureq 2.x, older rustls);
  (b) raise the declared MSRV.
- **Pick:** (b). `Cargo.lock` at `bbed735` already carried `indexmap` 2.14 and `getrandom`
  0.4.2 (via the dev dependencies), so 1.74 was already not buildable for `cargo test`. Release
  binaries are built on current stable, and `cargo install` users need a toolchain newer than
  Feb 2025. ureq 3 with only the `rustls` feature (ring + webpki-roots) is the smallest TLS client.

## D5. Where releases are read from, and the override's name

- **Question:** the lookup needs a version, the notes and asset URLs; forks and tests must be
  able to point elsewhere with one env var (bar item 10).
- **Options:** (a) GitHub REST API JSON (`/releases/latest`, `/releases/tags/vX.Y.Z`);
  (b) follow the `github.com/…/releases/latest` redirect and build download URLs by name.
- **Pick:** (a), in one function (`release::fetch_release`). It returns the release notes that
  `ccbox update` prints, and a 404 cleanly means "no release". The override is
  `CCBOX_RELEASES_URL`, a GitHub API repository URL (default
  `https://api.github.com/repos/tom-ha/ccbox`); `install.sh` reads the same JSON with grep/sed.
  Unauthenticated API calls are limited to 60/hour per IP, far above one check a day.

## D6. `ccbox setup` matches the Python byte for byte, including its quirks

- `json.dumps(indent=2)` escapes every character outside ASCII 0x20–0x7E as `\uXXXX` and
  prints floats with Python's `repr` (`1e-05`, `1e+16`). `setup` does the same through a
  custom serde_json formatter, so its output is identical on the corpus, which has a
  non-ASCII and float sample.
- Like the Python, a non-object `hooks` leaves the whole file untouched and exits 1 (install
  stops; `ccbox update` reports the binary updated and the wiring failed). A non-list event is
  left alone with a notice. Stale ccbox hooks on events no longer in the list are left, as the
  Python left them.
- Differences, all by design (bar item 4): `setup` writes and backs up only on change (the Python
  always rewrote, and `install.sh` always backed up); it writes through a symlinked
  `settings.json` to its target instead of replacing the link with a file; it keeps the file's
  permissions; and a second backup in the same second gets a `-1` suffix instead of
  overwriting the first. Integers beyond 64 bits would lose precision (serde_json without
  `arbitrary_precision`); no Claude Code setting is that large.

## D7. How `/ccbox` detects a binary that predates the verbs

- **Options:** (a) run `ccbox $ARGUMENTS` and retry with `toggle` when it exits 2; (b) probe
  with `ccbox version` first, which an old binary rejects with exit 2 before reading stdin.
- **Pick:** (b). With (a) an old binary prints its full "unknown flag" help before the retry,
  and a new binary's genuine exit 2 (`/ccbox show nope`) would be retried as `toggle`. The
  body uses `set -- $ARGUMENTS`, because Claude Code substitutes `$ARGUMENTS` textually, and
  maps no arguments to `--help` so bare `/ccbox` never starts a render waiting on stdin.

## D8. `install.sh` details

- From a local checkout it also prefers the prebuilt binary; `CCBOX_BUILD_FROM_SOURCE=1` builds
  the working tree. (The *Verify* install run expects exactly this.)
- A failure to find a tarball (no release yet, platform not in it, API unreachable or rate
  limited) falls back to a source build with a notice; a failure after finding it (download,
  missing `SHA256SUMS` line, mismatch) exits 1 with no fallback.
- Source builds pass `--root` explicitly (`CARGO_INSTALL_ROOT`, else `CARGO_HOME`, else
  `~/.cargo`), so the script knows which binary it built and wires that one, not the first
  `ccbox` on `PATH`. That overrides a user's `install.root` cargo config; `CCBOX_BIN_DIR`
  applies to prebuilt installs only.
- Under Rosetta (`sysctl.proc_translated` = 1) it installs the arm64 binary.

## D9. Chip text and colours

- `⬆ ccbox <v> available · run ccbox update`, then `⬆ ccbox <v> · ccbox update`, then
  `⬆ ccbox <v>`, right-anchored like the top border's chip. It takes the first form that fits
  and covers no `┴` junction, otherwise none. The arrow, version and command use each theme's
  `safe` colour (bold for the arrow and version), and the connecting words its `label`
  colour. No new theme slots.

## D10. Update-check files

- `update-check.json` (`checked_at`, `latest`, `failures`) is written only by the detached
  child, under `update-check.lock`. The render reads only that file; when a check is due it
  writes `update-check.spawned` and spawns, and skips spawning for 10 minutes after a spawn. So a
  render never rewrites the cache a child may be writing. The child re-checks due-ness
  under the lock, which is what makes 200 concurrent renders produce one request. The backoff
  base is the 24 h interval itself (48 h, 96 h, then the 7-day cap), so even failures stay within
  one check per day.

## D11. act coverage (tasks override 10 / *Verify* 8)

- act ran the `version`, `test` and aarch64-musl `build` jobs green up to `upload-artifact`,
  which can't reach act's artifact server from Docker Desktop's bridge network. The x86_64-musl
  job can't run here: rustc segfaults under QEMU on this arm64 host. Both are covered by the
  real `pull_request` run (`pr-checks.txt`), which is the done-condition for item 2.

## D12. Clippy count is 21, not 22

- The bar says the count stays at 22. It is 21: my new code adds none, and the pre-existing
  "items after a test module" warning in `src/bin/ccbox.rs` went away because the file now ends
  with a test module. Raising `rust-version` (D4) newly enabled `unnecessary_map_or` at
  `src/data/waiting.rs:184`; I fixed that line.

## D13. Round-1 review: what changed, and one design gap

- **Design gap (fixed, found by review):** neither the May design nor the goal's after-merge plan
  says that every existing install (source builds from `main`, the owner's included) predates
  `ccbox update` and never sees the chip. The README's *Updating*, the CHANGELOG and the PR's
  after-merge steps now say: re-run the install one-liner once, then `ccbox update` from there on.
  The after-merge plan keeps the goal's four steps and adds the reinstall before `ccbox update --check`.
- The read-only-directory hint no longer says `sudo ccbox update`: under sudo, `setup` would
  rewire root's `settings.json` (Linux resets `HOME`) or leave it root-owned (macOS). It suggests
  `CCBOX_BIN_DIR` or an elevated replace followed by `ccbox update` as the user.
- A 404 on `releases/latest` is "no release" only when the repository itself answers 200; a 404
  there is an error naming `CCBOX_RELEASES_URL`. That costs one extra request only on the 404 path.
- The README and the install skill no longer name the internal `setup`; manual installs are told
  to run `ccbox update`, which wires `settings.json` even when there is nothing newer.
- The README's `tokyonight` example (issue #9) was fixed here, since the CHANGELOG now claims the
  real theme names are listed.
- My existing-file lines were rustfmt-formatted hunk by hunk (only hunks on lines changed since
  `bbed735`); `cargo fmt --check` drift is 200 lines, all pre-existing apart from my import line
  showing as context inside an old reorder hunk.

## D14. Round-2 review

- `ccbox update` now creates its staging file only after the download and checksum pass (the
  writability check up front is a create-and-remove), so a kill during the up-to-300 s download
  leaves nothing next to the binary. The window left is extraction and the version probe.
- `install.sh` probes the staged copy's `version` inside `CCBOX_BIN_DIR` before the `mv`, so a
  mispackaged release leaves the old binary in place. The probe runs in the install directory,
  not `$TMPDIR`, which may be mounted noexec.
- `CCBOX_BIN_DIR` stays a prebuilt-only setting (D8). `install.sh` now says so when it falls back
  to a source build, and the README row says to uninstall a source build without it.
- The PR body's after-merge steps now include the one-time reinstall recorded in D13.

## D15. Round-3 review (clean: no blocking findings on either axis)

- Fixed: a failed release lookup still wires `settings.json` for the running binary (the
  manual-install path relies on it); unique staging names; `install.sh` tells a failed lookup
  from "no release"; `release.sh` restores on Ctrl-C and explains a failed tag; release steps
  use `git switch main && git pull --ff-only` and `git push --atomic`.
- Left as issues: the non-atomic stale-lock break in `update-check` (#10) and the missing
  timeout on `install.sh`'s version probe (#11).
