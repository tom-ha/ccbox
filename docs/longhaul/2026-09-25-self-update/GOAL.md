# Goal — ccbox ships tagged releases with prebuilt binaries, notices a newer release on its own, tells the user in the statusline, and updates itself with `ccbox update`

Read this file and implement it. Nobody is available to answer until the run is
over; everything you need is here, or on disk in `STATE.md`, `DECISIONS.md`
and `RETROSPECTIVE.md` by you.

## Where it is now

Measured on `main` at `bbed735` on 2026-09-25. This worktree
(`.claude/worktrees/self-update`, branch `worktree-self-update`) is at the same
commit and clean.

- **Distribution is a source build from `main`.** `install.sh:44` runs
  `cargo install --path` for a local checkout, and `install.sh:48` runs
  `cargo install --git "$REPO_URL" --branch "$REPO_BRANCH" --locked`. It then
  finds the binary with `command -v ccbox` (`install.sh:51`) and writes
  `statusLine` with `refreshInterval: 5` (`install.sh:83`). The README calls
  the build "a minute".
- **Nothing is released.** `git tag -l` is empty and
  `gh release list -R tom-ha/ccbox` is empty. There is no `.github/`
  directory and no CI of any kind. The repo is public (`gh repo view`).
- **The versions have drifted.** `Cargo.toml:3` is `0.1.0`.
  `plugins/ccbox/.claude-plugin/plugin.json` is `0.5.0`, and so are both
  `metadata.version` and `plugins[0].version` in
  `.claude-plugin/marketplace.json`. Releases so far have been plugin-only bump
  commits (`3b4499f chore(plugin): bump to 0.5.0`).
- **The command-line grammar is mixed.** `src/bin/ccbox.rs:40-95` accepts
  these forms:

  | Invocation | Called by |
  |---|---|
  | Bare `ccbox`, with session JSON on stdin and the flags `--theme NAME`, `--width COLS`, `--full-width`, `--bg-shift warm\|cool`, `--snapshot`, `-h`/`--help` | Claude Code's `statusLine` in every install (`install.sh:83`) |
  | `ccbox toggle show\|hide\|flip tasks\|subagents` and `ccbox toggle status` | the `/ccbox` slash command (`plugins/ccbox/commands/ccbox.md:25`, mirrored in `.claude/commands/ccbox.md`), and README examples (`README.md:144-148`) |
  | `ccbox hook`, with event JSON on stdin | the hooks in every install's `settings.json` (`install.sh:87`) |
  | `ccbox usage-refresh` | the statusline itself, detached (`src/data/account_usage.rs:134`) |

  `toggle show tasks` reads as verb, verb, noun. `ccbox-demo` is a
  separate binary that takes a fixture path.
- **The binary can't tell its version or check for a newer one.**
  `ccbox --version` prints `ccbox: unknown flag: --version` and exits 2
  (`src/bin/ccbox.rs:90`). Nothing in `src/` contacts the network except the
  `claude -p` usage probe.
- **The pattern to copy for background work.** The account-usage refresh
  runs off the render path. `load` (`src/data/account_usage.rs:104`) reads a
  cache and, when `refresh_due` (`:97`) says so, spawns a detached
  `ccbox usage-refresh` (`spawn_refresh`, `:126`; dispatched at
  `src/bin/ccbox.rs:42`). That child takes a lock and records the attempt with
  exponential backoff (`refresh`, `:146`).
- **The bottom border is always drawn and carries no text.**
  `Border::border_bottom(width, ups, fill)` (`src/render/border.rs:114`) takes
  no chip, and `src/components/footer.rs` emits only `RowKind::BottomBorder`.
  The top border already carries a right-anchored chip (`border_top`,
  `src/render/border.rs:45`).
- **The env-var tristate parser already exists.** It is
  `parse_bool_tristate` (`src/config.rs:121`), which accepts
  `1/true/yes/on` and `0/false/no/off`.
- **`ccbox --help` prints its env-var table without indentation.** Every
  `\n\` line continuation in `print_help` (`src/bin/ccbox.rs`, near the end)
  strips the next line's leading spaces.
- **Tests.** `cargo test --locked`: 453 passed, 0 failed (7.4 s build and
  about 2.6 s of tests on this machine).
- **Lint and format.** `cargo clippy --locked --all-targets` exits 0 and
  prints 22 lines starting with `warning`. `cargo fmt --check` exits 1 with a
  199-line diff: the tree is not rustfmt-clean.
- **Render time.** Median 115.27 ms, p95 138.99 ms, max 620 ms over 100 renders
  of `tests/fixtures/session-info-example.json` at `--width 140`, with an
  empty temporary `CLAUDE_CONFIG_DIR` (script under *Verify*).
- **Settings wiring lives in Python inside `install.sh`.** The embedded
  script (`install.sh:75-131`) sets `statusLine` and replaces the ccbox hook
  groups for ten events (the `wanted` table, `install.sh:96`). It matches
  existing ccbox hooks with `is_ccbox_hook` (`install.sh:88`) and leaves
  other hooks alone. The installer backs up `settings.json` on every run
  (`install.sh:70`) and requires python3 for this step alone
  (`install.sh:36-37`). The hook list therefore lives only in the installer,
  and the binary can't re-apply it.
- **Running sessions pick up hook changes.** The docs say "Claude Code
  watches your settings files and reloads them when they change, so it
  applies most edits to the running session without a restart, including
  edits to `permissions`, `hooks`, and credential helpers"
  (https://code.claude.com/docs/en/settings, *When edits take effect*).
  They don't say whether `statusLine` edits apply live. `ccbox update`
  leaves the `statusLine` path unchanged, so the next render simply runs the
  new binary.
- **The uninstaller only knows cargo.** It removes the binary with
  `cargo uninstall ccbox` (`uninstall.sh:113`). A binary copied into
  `~/.cargo/bin` by anything else stays, and the script only prints a note
  (`uninstall.sh:124-126`).
- **The plugin's install skill requires cargo.**
  `plugins/ccbox/skills/install-ccbox/SKILL.md:15` stops without cargo, and
  `:42` says the installer builds from source.
- **Tools on this machine.** `actionlint`, `act 0.2.89`, Docker 29.7.2
  (running, `linux/aarch64`) and rustup are installed. The only rustup targets
  installed are `aarch64-apple-darwin` and `wasm32-wasip1`.
- **GitHub access.** `gh` is logged in as `tom-ha` with scopes `gist`,
  `project`, `read:org` and `repo`, with no `workflow` scope. Git pushes go
  over SSH (`origin git@github.com:tom-ha/ccbox.git`).
- **An earlier design exists outside this worktree.** It is untracked and
  lives only in the main checkout (read it with the Read tool):
  `/Users/tomh/git/public/ccbox/openspec/changes/add-release-and-update/`,
  with `proposal.md`, `design.md`, `tasks.md`,
  `specs/release-process/spec.md` and `specs/binary-self-update/spec.md`. It
  was written in May and some of its facts are stale (see the overrides
  below).
- **The owner's own install.** `~/.cargo/bin/ccbox`
  (`ccbox 0.1.0 (path+file:///Users/tomh/git/public/ccbox)` in
  `~/.cargo/.crates2.json`) is wired into `~/.claude/settings.json`. It draws
  your own statusline and is not the code under test.

Read as: *ccbox is rebuilt from main's source on every install, has no
releases, and has no way to learn that a newer version exists.*

## The bar

Done is the May design, with the overrides below, plus the update check and
the chip, which that design listed as a non-goal. The specs
`specs/release-process/spec.md` and `specs/binary-self-update/spec.md` define
the behaviour scenario by scenario. Where this document and the design
disagree, this document wins.

**Overrides to the May design:**

1. **The update check is now a goal.** `design.md` lists "Background 'new
   version available' hints in the statusline" as a non-goal. It is now
   built; see bar items 7 to 9 below.
2. **No `--yes` flag.** `ccbox update` asks nothing: typing it is the consent.
   `--check` only reports.
3. **No install-method refusal.** `proposal.md:9` says `ccbox update` refuses
   a binary cargo didn't install. That is not implemented; `design.md:89`
   holds. `--force` exists only to allow downgrades.
4. **Linux targets are musl.** The Linux targets are
   `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, which give
   static binaries with no glibc floor; rustls needs no OpenSSL. The macOS
   targets stay `aarch64-apple-darwin` and `x86_64-apple-darwin`. Every spec
   scenario that names a `-gnu` triple means the musl one.
5. **Releases publish directly.** `design.md:58` says the release job
   "publishes a draft GitHub Release". Instead it publishes a normal
   release: `releases/latest` does not return drafts, so a draft would stay
   invisible to `ccbox update` and `install.sh`. The maintainer's tag push is
   the approval.
6. **The design's version facts are stale.** It says the manifests are at
   0.4.0; they are at 0.5.0. `plugins/ccbox/.claude-plugin/plugin.json` exists
   and carries a version, so `scripts/release.sh` bumps it too.
7. **This PR bumps no version.** The first release is `v0.6.0`, made by the
   owner after merge (`scripts/release.sh 0.6.0`, then pushing the tag). It
   replaces the design's migration step "Run `scripts/release.sh 0.5.0`".
   The PR records its changes under `## [Unreleased]` in `CHANGELOG.md`.
8. **The workflow also runs on pull requests.** A pull request that touches
   the workflow, `Cargo.*`, `src/**` or `scripts/**` builds, packages and
   writes `SHA256SUMS` for all four targets, runs `cargo test --locked` once,
   and publishes nothing. Publishing runs only on tags matching
   `v[0-9]+.[0-9]+.[0-9]+`.
9. **Check the runner labels.** The design names `macos-13` and `cross` for
   aarch64 Linux. Check GitHub's current runner labels before relying on
   either. `macos-13` may be retired: `x86_64-apple-darwin` cross-compiles on
   an arm64 macOS runner with `rustup target add`, and public repos also get
   native ARM Linux runners. Pick the simplest setup that builds all four
   targets and record the choice in `DECISIONS.md`.
10. **The tag push is replaced.** `tasks.md` §7 (push a pre-release tag to a
    fork, update from it, delete it) is replaced by *Verify* below. No tag is
    pushed anywhere.
11. **No `tiny_http`.** For task 4.13, serve fakes from a test with
    `std::net::TcpListener` or in the proof with `python3 -m http.server`.
    Adding `tiny_http` parks.

**What done looks like:**

1. **Release workflow.** `.github/workflows/release.yml` works as
   `specs/release-process/spec.md` describes, with the targets from
   override 4. Each tarball holds only `ccbox`, mode 0755, and `SHA256SUMS`
   has exactly four lines in `sha256sum` format. The workflow refuses to
   build when the tag, `Cargo.toml`, `marketplace.json` (both fields) and
   `plugin.json` disagree, and refuses to publish without a matching
   `## [X.Y.Z]` section in `CHANGELOG.md`. Third-party actions are pinned to
   commit SHAs, and `permissions:` is `contents: read` except for the
   publish job, which gets `contents: write`.
2. **Release script and changelog.** `scripts/release.sh` works as that spec
   describes. `CHANGELOG.md` is in Keep a Changelog format, and its
   `[Unreleased]` section lists this PR's changes.
3. **`ccbox update`.** It works as `specs/binary-self-update/spec.md`
   describes, minus `--yes`, with `--check`, `--version X.Y.Z` and `--force`.
   After a successful replacement it runs the **new** binary's settings
   step (item 4), because only the new version knows its own hook list.
   Then it prints `<old> -> <new>`, what changed in `settings.json`, and the
   release notes. `ccbox update` is the only command the user ever types:
   after it, nothing is left for them to do. If the settings step fails
   after the swap, the update reports that the binary was updated, prints
   the error, and exits non-zero. The retry is `ccbox update` again: when
   already on the latest version, `ccbox update` still runs the settings
   step, which writes nothing when the wiring is current. That amends the
   spec's "already on the latest version" scenario.
4. **The settings step.** This is the wiring from `install.sh:75-131`, moved
   into the binary. It is an internal entry point, like `usage-refresh` and
   `hook`: `ccbox update` and `install.sh` call it, and users never type it.
   It stays out of the README's commands and out of `--help`. Name it in
   the same style (`ccbox setup` is fine). It points `statusLine` at
   `current_exe()` with
   `refreshInterval: 5`, and replaces the ccbox hook groups with this
   version's list, using the same `is_ccbox_hook` matching. It leaves every
   other hook and key alone, keeping their order (serde_json's
   `preserve_order`; `indexmap` is already in `Cargo.lock`). It leaves a
   non-object `hooks` or a non-list event as the Python does, and respects
   `CLAUDE_CONFIG_DIR`. It is idempotent: a second run changes no byte and
   makes no backup. It backs up `settings.json` to
   `settings.json.bak.YYYYMMDD-HHMMSS` only when it changes something, and
   writes via a temp file and rename. It prints one line per change. The hook
   list lives only here.
5. **Installer.** `install.sh` prefers the prebuilt binary as that spec
   describes: `CCBOX_BUILD_FROM_SOURCE=1` forces a source build, a checksum
   failure exits non-zero with no fallback, and the default install dir is
   `~/.cargo/bin` unless `CCBOX_BIN_DIR` is set. It wires `settings.json` by
   running `"$installed_binary" setup` in place of the embedded Python, so
   installing no longer needs python3. It still works both from a local
   checkout and via `curl | bash`.
6. **Uninstaller and plugin skills.** `uninstall.sh` also removes a
   prebuilt binary that `cargo uninstall` doesn't know about. The plugin's
   `install-ccbox` and `uninstall-ccbox` skills treat cargo as needed only
   when no prebuilt binary exists for the platform.
7. **Background check.** It runs at most once per 24 h per machine, in a
   detached `ccbox update-check` process modelled on `usage-refresh` (lock,
   attempt file, backoff). The result is cached in
   `<claude_dir>/ccbox-cache/update-check.json`. The network timeout is 10 s,
   and failures back off by doubling up to a 7-day cap. On the render path,
   the check costs one read of the cache (plus a spawn when due) and never
   touches the network or waits on a process.
8. **The chip.** When the latest release is newer than `CARGO_PKG_VERSION`
   (semver), the bottom border carries a chip reading
   `⬆ ccbox <latest> available · run ccbox update`. Where that doesn't fit it
   shortens, then disappears; it never breaks the border width at any layout
   (narrow < 55, medium 55–79, wide ≥ 80). The chip is absent when the
   versions are equal or the installed one is newer (dev builds), when no
   release exists yet, and when the check is disabled. `--snapshot` exposes
   the installed version, the cached latest version, when it was checked,
   and whether the chip shows.
9. **The off switch.** `CCBOX_UPDATE_CHECK=0|false|no|off`, read with
   `parse_bool_tristate`, means no spawn, no network call and no chip.
   Unset or truthy means on. It takes effect after a Claude Code restart
   (like every other env var). There is no state-file switch.
10. **Release source override.** One documented env var overrides where
    releases are read from. Both `ccbox` and `install.sh` honour it, so forks
    and tests can point elsewhere. The default is `tom-ha/ccbox` on GitHub.
11. **Command-line grammar: verbs, kubectl-style.** User commands take the
    form `ccbox <verb> [thing]`:
    ```
    ccbox show|hide|flip tasks|subagents
    ccbox status            # the row table, plus installed version, latest known release, check on/off
    ccbox update [--check] [--version X.Y.Z] [--force]
    ccbox version           # same output as `ccbox --version`: `ccbox <CARGO_PKG_VERSION>`
    ```
    - **Bare `ccbox`** reading stdin stays the statusline render. It keeps
      its flags, adds `--version`, and keeps `--snapshot` as a render flag.
    - **Internal entry points** (`hook`, `usage-refresh`, `update-check` and
      the settings step) keep working under their current names and stay
      out of `--help` and the README. `hook` must keep its spelling:
      every installed `settings.json` calls it.
    - **`ccbox toggle …`** keeps working as a hidden alias with today's
      behaviour and output. An installed plugin that predates this release
      still calls it.
    - **Errors.** An unknown verb or thing exits 2 and prints the usage.
    - **Slash command.** Both copies of the `/ccbox` command
      (`plugins/ccbox/commands/ccbox.md` and `.claude/commands/ccbox.md`)
      run `ccbox $ARGUMENTS`, so `/ccbox show tasks` is exactly
      `ccbox show tasks`. When the binary exits 2 because it predates the
      verbs, they retry with `ccbox toggle $ARGUMENTS`.
    - **`toggle status` output.** `ccbox status` keeps the row table
      byte-for-byte as `ccbox toggle status` prints it today, with the
      update lines after it.
12. **Docs.** The README gains *Updating* (the chip, `ccbox update` and its
    flags, the fact that it rewires hooks itself, `CCBOX_UPDATE_CHECK`) and
    *Releasing* (`scripts/release.sh`, the
    tag push, the PR dry run). It also gets the new env vars in its tables
    and the macOS Gatekeeper note (`xattr -d com.apple.quarantine`) for
    manually downloaded tarballs. Its Install and Requirements sections say
    prebuilt first with cargo as the fallback, and that python3 is no longer
    needed to install (the uninstaller still uses it). The README's toggle
    section uses the verb forms. `ccbox --help` lists the verbs and env vars
    with its table indentation fixed. Internal entry points and the `toggle`
    alias stay out of both.

**The numbers:**

- Render median within 5 ms of the baseline binary, with a fresh
  update-check cache, both measured in the same bench run (script under
  *Verify*).
- At most one network check per 24 h across any number of renders and
  sessions.
- A 10 s network timeout, and backoff capped at 7 days.
- Four tarballs and one four-line `SHA256SUMS` per release.
- Zero new clippy warnings: the count stays at 22 lines starting with
  `warning`.
- Test count ≥ 453, all passing.
- For every sample settings file, `ccbox setup` produces output identical
  to the Python it replaces.
- A second `setup` run changes 0 bytes and writes 0 backups.
- Nothing is left for the user after `ccbox update`.

Read as: *ccbox tells you in the statusline when a new release is out, and
`ccbox update` installs it in seconds with its checksum verified, wires
whatever hooks the new version needs, and leaves nothing to do by hand.
`CCBOX_UPDATE_CHECK=0` turns the whole thing off.*

## Do this

0. **Set up.** Work in this worktree on `worktree-self-update`, and push it
   as `feat/self-update` (`git push -u origin HEAD:feat/self-update`). Create
   the proof directory (see *Verify*). Before changing any code, build the
   baseline binary and copy it to `$PROOF/ccbox-baseline`, then record the
   mtime and sha256 of `~/.claude/settings.json` and `~/.cargo/bin/ccbox` in
   `$PROOF/owner-files-before.txt`.
1. **Release scaffolding.** Add `CHANGELOG.md` with `[Unreleased]`, and add
   `scripts/release.sh`. Done when the rehearsal under *Verify* passes.
2. **Release workflow.** Add `.github/workflows/release.yml` (bar item 1,
   overrides 4, 5, 8 and 9). Once it passes `actionlint` and `act`, push and
   open the PR as a draft against `main`, so the `pull_request` trigger runs
   on real runners. Done when the PR's own checks are green for all four
   targets, with the tarballs and `SHA256SUMS` attached as workflow
   artifacts.
3. **Command-line grammar.** Move the dispatcher in `src/bin/ccbox.rs` to
   the verb form (bar item 11): `show`, `hide`, `flip`, `status` and
   `version`, plus `--version`, with the `toggle` alias and the internal
   entry points kept. Update both copies of the slash command, with the
   fallback for old binaries, and the existing tests in `src/bin/ccbox.rs`.
   Doing this before the update work means `update` is born as a verb.
   Done when the CLI evidence under *Verify* passes.
4. **The settings step and `ccbox update`.** Add new modules under `src/`,
   wired into the verb dispatcher (bar items 3 and 4). Build the settings
   step first: `update` calls it, and `install.sh` switches to it in item
   6. The lookup of the latest (or a pinned) release is one function, so the
   source can change without touching the rest. The crates `ureq` (rustls),
   `flate2` (rust_backend), `tar` and `sha2` are approved, as is serde_json's
   `preserve_order` feature. Any other runtime or dev dependency parks.
   Done when:
   - the `setup` comparison against the Python under *Verify* is identical;
   - unit tests cover triple mapping, semver compare and downgrade refusal,
     and `SHA256SUMS` parsing and mismatch;
   - the end-to-end update under *Verify* passes, including the new-hook
     case.
5. **Background check, chip and off switch.** Add the internal `update-check`
   entry point, the cache, and `Env` fields fed from `CCBOX_UPDATE_CHECK`.
   Give `border_bottom` a chip, and add the snapshot fields (bar items 7 to
   9). Done when the chip evidence under *Verify* shows at all three widths
   and the negative controls show no chip.
6. **Installer, uninstaller, skills and docs.** Update `install.sh`,
   `uninstall.sh`, both plugin skills, the README, `--help` and the
   CHANGELOG (bar items 5, 6 and 12). Done when the install and uninstall
   runs under *Verify* pass against temporary directories.
7. **Review, proof and PR.** Run the review loop (*Review*), re-run the whole
   proof, finish the PR body, and mark the PR ready for review
   (`gh pr ready`).

**Parameters outside the obvious files are in scope.** These are the timing
constants (24 h interval, 10 s timeout, 7-day backoff cap, lock staleness),
the chip text and its colour in each theme, runner labels, action SHAs, and
the fact that every session re-renders every 5 s (`install.sh:83`). That last
one is why the due-check must stay a single file read.

**Nothing waits on the owner.** Every decision this run needs was made in
the interview. A new question goes to `DECISIONS.md` with your pick, and
work continues under it. That covers:
- a crate beyond the four;
- a change to the hook events, matchers or `statusLine` values themselves,
  as opposed to moving them into `setup` unchanged;
- anything that would write to the owner's real `~/.claude` or
  `~/.cargo/bin`.

**What counts as this run versus an issue.** A finding belongs to this run
when it lies on the path from "a tag is pushed" to "the user sees the chip,
runs `ccbox update`, and has the new binary wired in". That includes install,
uninstall, the plugin skills, the docs and `--help` text this work touches,
and a bug in the May design found while implementing it (fix it and note it
in `DECISIONS.md`). Anything else found is an issue. Border cases already
decided:

- the `--help` indentation → this run;
- "skip this version" → issue;
- the 115 ms render median → issue;
- the non-rustfmt-clean tree and the 22 clippy warnings → one issue;
- a `/ccbox update` slash-command flow → neither (decided against);
- code signing, Windows and crates.io → neither (design non-goals).

**Formatting.** Do not run `cargo fmt` over the tree. Format only the files
you create, with `rustfmt <file>`, and leave existing files' formatting as it
is outside the lines you change.

**Leave alone:**

- `openspec/` in the main checkout (read-only);
- the worktrees `.claude/worktrees/responsive-frame` (locked by another
  session) and `.claude/worktrees/zesty-stargazing-eclipse`, and their
  branches;
- the shared git stash;
- the owner's `~/.claude/` and `~/.cargo/bin/ccbox`;
- repository settings, tags and releases. Push no tag, create no release,
  and dispatch no publish.

## Verify by looking, not by asserting

**Proof directory.** Use `$PROOF=/Users/tomh/git/public/ccbox/.claude/goal-runs/self-update/proof/`,
which is in the main checkout and outside this worktree. `cargo test`
empties nothing (tests use `tempfile`), but `cargo clean` wipes `target/`, so
no evidence lives there. Logs go to files in `$PROOF`; read their tails or
grep them.

**Every run uses throwaway directories.** Each command below runs with a
temporary `CLAUDE_CONFIG_DIR`, a temporary `CCBOX_BIN_DIR` or
`CARGO_INSTALL_ROOT`, and a `PATH` that excludes `~/.cargo/bin`. At the end,
write `$PROOF/owner-files-after.txt` and diff it against the before file; the
diff must be empty.

**Evidence:**

1. **Tests and lint.** Run `cargo test --locked > $PROOF/cargo-test.log 2>&1`:
   ≥ 453 passed, 0 failed. Run `cargo clippy --locked --all-targets` into
   `$PROOF/clippy.log`: still 22 lines starting with `warning`.
2. **Render time.** Build the branch binary and benchmark it against
   `$PROOF/ccbox-baseline` in the same run, both with a fresh update-check
   cache in the temp config dir. Save the output to `$PROOF/bench.txt`. The
   script used for the baseline figure:
   ```bash
   export CLAUDE_CONFIG_DIR="$(mktemp -d)"
   python3 - "$BIN" <<'PY'
   import subprocess, sys, time, statistics
   fx = open("tests/fixtures/session-info-example.json", "rb").read()
   ts = []
   for _ in range(100):
       t = time.perf_counter()
       subprocess.run([sys.argv[1], "--width", "140"], input=fx, stdout=subprocess.DEVNULL)
       ts.append((time.perf_counter() - t) * 1000)
   ts.sort()
   print(f"renders=100 p50={statistics.median(ts):.2f}ms p95={ts[94]:.2f}ms max={ts[-1]:.2f}ms")
   PY
   ```
3. **The chip.** Serve a fake release source whose latest is `9.9.9` from a
   local HTTP server that logs requests. Let the background check fill the
   cache, then render the fixture at `--width 50`, `70` and `140`. Strip ANSI
   and save the output as `$PROOF/chip-{50,70,140}.txt`, and save the
   `--snapshot` fields as `$PROOF/chip-snapshot.json`.
   - **Negative controls:** with the latest at `0.0.1`, and with no release,
     there is no chip (`$PROOF/nochip-*.txt`).
   - **Once a day:** 200 renders with a fresh cache produce exactly one
     request in the server log (`$PROOF/check-requests.log`).
   - **Off switch:** `CCBOX_UPDATE_CHECK=0` with a stale cache gives zero
     requests and no chip (`$PROOF/disabled.txt`).
4. **`ccbox setup` against the Python it replaces.** Before deleting the
   embedded Python from `install.sh`, extract it to `$PROOF/setup-python.py`
   and build a corpus of `settings.json` samples in `$PROOF/setup-corpus/`:
   - no file, `{}`, and a file with only unrelated keys;
   - a foreign `statusLine`;
   - other tools' hooks on the same events, with and without matchers;
   - old ccbox hook groups, including a group that mixes ccbox and foreign
     hooks;
   - `hooks` that is not an object, and an event that is not a list;
   - a quoted binary path containing a space.

   Run both implementations on copies of each sample, then diff the outputs
   byte for byte and as parsed JSON, including key order. Every diff must be
   empty, apart from backup naming, which `setup` does only on change. Save
   the per-sample verdicts to `$PROOF/setup-diff.txt`. Then run `setup`
   twice on each sample: the second run changes 0 bytes and writes no
   backup. Finally, run `uninstall.sh` on each `setup` output: no ccbox hook
   remains, and every foreign hook survives.
5. **`ccbox update`, end to end.** Build a real tarball from this branch,
   named as version `0.1.1` with a correct `SHA256SUMS`, and serve it
   locally. Build that tarball's binary with one extra hook event added to
   `setup`'s list; that patch is uncommitted and reverted afterwards. Copy
   the branch binary into a temp dir, point a temp `settings.json` at it
   through `setup`, and run its `ccbox update`. It prints `0.1.0 -> 0.1.1`,
   the file's sha256 now equals the tarball's `ccbox`, the temp
   `settings.json` now carries the extra hook, and no staging file is left
   behind. Nothing else is run by hand. Also show:
   - `--check` changes no file (sha256 and mtime before and after);
   - when already on the latest version with correct wiring, `ccbox update`
     writes nothing. With a ccbox hook deleted from the temp
     `settings.json`, the same command puts it back;
   - a corrupted `SHA256SUMS` gives a non-zero exit, an unchanged binary,
     and no staging file;
   - a read-only directory is refused with the remediation hint;
   - `--version 0.0.9` without `--force` is refused, and with `--force` it
     proceeds;
   - `--version 9.9.9` gives "not found".

   Save all of this to `$PROOF/update-e2e.txt`. Run
   `ccbox update --check` against the real GitHub too (there is no release
   yet: the message must read as "no release published yet", not as a
   failure) and save it to `$PROOF/update-check-github.txt`.
6. **Install and uninstall.** Run `install.sh` against the local fake
   release with temporary `CCBOX_BIN_DIR` and `CLAUDE_CONFIG_DIR`. It
   installs the prebuilt binary without invoking cargo, and the temp
   `settings.json` points at the temp binary. Then show that
   `CCBOX_BUILD_FROM_SOURCE=1` builds from source into a temp
   `CARGO_INSTALL_ROOT`, and that a checksum mismatch exits non-zero with no
   binary written. `uninstall.sh` against the same temp dirs removes the
   prebuilt binary. Save to `$PROOF/install.txt` and `$PROOF/uninstall.txt`.
7. **Release script rehearsal.** Clone this worktree into a temp dir and run
   `scripts/release.sh 0.6.0` there. The diff shows all four version fields
   at 0.6.0, an updated `Cargo.lock`, a `release: v0.6.0` commit and an
   annotated tag, and the `CHANGELOG` `[Unreleased]` section moved under
   `[0.6.0]`. Also show that it refuses a dirty tree, an existing tag and
   `0.6`. Save to `$PROOF/release-rehearsal.txt`, then delete the clone.
   Nothing is pushed from it.
8. **Workflow.** Save `actionlint` output to `$PROOF/actionlint.txt`. Run
   the Linux jobs under `act pull_request` and save the tail to
   `$PROOF/act.txt`. After pushing, save
   `gh pr checks <n>` and the run's artifact list (`gh run view <id> --json`)
   to `$PROOF/pr-checks.txt`. Then download the aarch64 musl artifact and run
   it in Docker, `docker run --rm -v …:/x alpine /x/ccbox --version` and the
   same in `debian:bullseye`, and save to `$PROOF/musl-runs.txt`.
9. **Version gate.** Run the workflow's version-agreement check locally with
   a tag that disagrees with `Cargo.toml`. It fails and names both values
   (`$PROOF/version-gate.txt`).

10. **Command line.** Using the branch binary and a temp config dir, run
    each verb:
    - `show`, `hide` and `flip` for both rows;
    - `status`, `version` and `--version`;
    - an unknown verb (exit 2 with usage);
    - every `toggle …` spelling, whose output diffs empty against the same
      spelling on `$PROOF/ccbox-baseline`;
    - `hook` and `usage-refresh`, which still dispatch.

    Then run the slash command's shell body twice, once with `PATH` putting
    the branch binary first and once with `$PROOF/ccbox-baseline` first. The
    old binary takes the `toggle` fallback, and both runs change the temp
    toggles file identically. Save to `$PROOF/cli.txt`.

**Positive controls.** Before trusting a green test, break the code it
guards, build, and watch the test go red:

- invert the semver comparison;
- make the checksum check always pass;
- make the update-check due-logic always report "due";
- make `setup`'s `is_ccbox_hook` match any hook (the corpus diff must go
  red on the foreign-hook samples).

Build after each mutation, before running the tests, so a mutation that
doesn't compile can't pass as a zero. Record each mutation, its red output
and the revert in `$PROOF/mutations.txt`.

**What green proves, and what it doesn't.** Green proves the four targets
build and package on real runners, update and install work against a
faithful fake, `setup` matches the Python on every sample in the corpus, an
update wires a new version's hooks by itself, and the owner's files are
untouched. It does not prove the
tag-triggered publish job or the real `releases/latest` path. Those run for
the first time when the owner pushes `v0.6.0`, and the PR says so.

## Review

**Reviewers.** Two reviewers per round, each a subagent that did not write
the code. Each returns its findings with `path:line`, not its transcript.

1. **Correctness and safety.**
   - The render path never blocks and never touches the network.
   - "Disabled" means zero network calls.
   - Concurrent statuslines don't race on the cache, lock or attempt file.
   - Backoff and clock-skew handling work.
   - The atomic replace works, and staging files are cleaned up on every
     error path.
   - Checksum checking is mandatory.
   - `install.sh` and `uninstall.sh` never touch a binary or settings file
     outside the directories they were given.
   - `ccbox setup` never loses or reorders a user's settings, never removes
     a foreign hook, and writes atomically. A failure after the binary swap
     leaves a working binary and says what to run.
   - The workflow: tag and version gate, pinned SHAs, least-privilege
     `permissions`, and the PR trigger never publishes.
2. **Product and docs.**
   - The chip reads right and fits at narrow, medium and wide widths in
     every theme.
   - `ccbox update` output, including the "no release yet" wording.
   - README, `--help`, CHANGELOG and the plugin skills match the actual
     behaviour.
   - Every user command follows `ccbox <verb> [thing]`, with no stray
     noun-first or flag-as-command form. Internal entry points and the
     `toggle` alias are invisible in `--help` and the README.
   - Every override in *The bar* is honoured.
   - The PR explains what the owner does after merge.

**When the loop ends.** A round is clean when neither reviewer reports a
finding that should block a merge. Stop at the first clean round or after 3
rounds. Findings still open after round 3 go to issues, linked from the PR.
Re-run the whole proof after the last fix.

**The pull request.** One PR, `feat/self-update` → `main`, with commits
grouped by area (release scaffolding, workflow, update, check and chip,
install and docs). Its body has:

- a summary;
- the overrides to the May design;
- **After merge:** run `scripts/release.sh 0.6.0`, then
  `git push origin main v0.6.0`, watch the release workflow, then run
  `ccbox update --check`;
- the review trail, round by round;
- a **Proof of work** section with the commands, output excerpts and the
  paths in `$PROOF`.

## Traps

- **Your real install.** `install.sh` finds the binary with `command -v ccbox`
  (`install.sh:51`), which resolves to the owner's real `~/.cargo/bin/ccbox`.
  An install test run without a temporary `PATH`, bin dir and config dir
  rewires the owner's live statusline.
- **`install.sh` can wire the wrong binary.** It checks `PATH` first, then
  `$CARGO_HOME/bin`, so it can pick up a binary other than the one it just
  installed. That bites a real user with a stale `ccbox` earlier on `PATH`,
  and a test that installs into a temporary `CARGO_INSTALL_ROOT`. Making
  `install.sh` wire the binary it installed is in this run.
- **Your own statusline** is drawn by the owner's installed build, not by the
  code under test. Evidence comes from the branch binary run by hand.
- **Commands with `/git/` in a path.** The repo lives under `~/git/`. The
  interview session's worktree guard refused chained or looped shell
  commands with `git` in a path outside the worktree, and yours may too.
  Plain `mkdir` and `echo >` into `$PROOF` were allowed. Read files outside
  the worktree with the Read tool, and keep shell commands that touch them
  plain.
- **The plugin and the binary update separately.** The marketplace updates
  the plugin while the binary stays wherever the user last installed it. A
  new plugin can meet an old binary (hence the `toggle` fallback), and an
  old plugin can meet a new binary (hence the `toggle` alias). Neither
  pairing may break `/ccbox`.
- **serde_json sorts object keys unless `preserve_order` is on.**
  Without the feature, `ccbox setup` would reorder every user's
  `settings.json`, and the corpus diff catches that.
- **The `\n\` continuation in Rust strings strips the next line's leading
  whitespace.** That is why `--help` has lost its indentation. Use
  `\x20`-escaped leading spaces or a raw string.
- **The `gh` token has no `workflow` scope.** Push workflow files over SSH,
  which is already the remote protocol, and don't switch `origin` to HTTPS.
- **The May design's runner and version facts are stale** (overrides 6 and
  9). Verify them before you build on them.
- **Comment guard.** A `PostToolUse` hook (`bash-comment-guard`) flags
  comments in the working tree. Treat its output as a review comment and fix
  what it flags in your own files.
- **Every Claude Code session re-renders every 5 s.** Anything added to the
  render path is paid for many times a minute on every machine.

## Rules

Your user-level rules (`~/.claude/CLAUDE.md`) hold throughout; the repo has no
`CLAUDE.md` or `AGENTS.md`. Two of them, restated because they are the ones
that slip:

- **Comments are expensive.** Add one only when it gives context the code
  can't. It is one line and says what the code does.
- **A negative result needs a positive control.** "The test passed" and "the
  test never ran" look the same without one. See *Positive controls* above.

## Run rules

This run is unattended. Repository rules (`AGENTS.md`, `CLAUDE.md`) hold
throughout; the rules below are the shape of an unattended run.

- **Decisions park; work continues.** Every choice that is the owner's — spend,
  licences, irreversible changes, scope cuts, dependencies — goes into
  `DECISIONS.md` at the worktree root as one section: the question, the options,
  your pick, and what you did meanwhile. Pick a sensible default, write it down,
  keep going. Where this document says "ask" or "show me", write the plan or
  the options to a file and proceed under your recommendation.
- **The run ends at open pull requests.** Its last action is your closing
  summary: what shipped, what is parked, what is open, and the lessons in
  `RETROSPECTIVE.md`. Merging is the owner's.
  An action your permission classifier refuses stays refused; record it in
  `DECISIONS.md` and continue. Another session cannot take it for you.
- **Commit as you go.** Small conventional commits, tests green before each.
  Nothing uncommitted at the end but `GOAL-*.md`, `STATE.md`, `DECISIONS.md`
  and `RETROSPECTIVE.md`.
- **Found, not fixed, is an issue.** Anything you notice and leave gets an issue
  with the measurement that shows it. A coverage gap is an issue too, linked
  from one line in the test, so the test file states what it guards.
- **Comments are one line and state what the code does.** Context the code
  cannot give, or nothing.
- **Wait cheaply.** Waiting for a wall-clock time is one background `until`
  loop that exits at the time; one notification, one turn.
- **State lives on disk.** After each item under *Do this* is done or parked,
  update `STATE.md` at the worktree root: the item in progress, what is
  verified and where its evidence is, the open pull requests. After a
  compaction, re-read this file and `STATE.md` before the next action.
- **Spend context on decisions, not output.** Reviews, verification runs and
  wide searches go to subagents that return a verdict and the path to the
  evidence. Test and build output goes to a file in the proof directory; read
  its tail or grep it for failures.
- **Lessons are written when they happen.** Whatever costs you time — a wrong
  claim, a flaky tool, a missing rule — gets a section in `RETROSPECTIVE.md`
  at the worktree root when you get past it: what happened, what it cost, how
  you got past it, and the line that would have prevented it.
- **Verify claims you inherit**, including this document's. A wrong claim has
  restatements; grep for them before correcting one.
