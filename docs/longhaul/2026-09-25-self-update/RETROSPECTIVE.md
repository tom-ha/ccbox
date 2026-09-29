# RETROSPECTIVE — self-update run

Each section: what happened, what it cost, how I got past it, and the line
that would have prevented it.

## act on Docker Desktop: cargo downloads stall, artifact uploads never connect

- **What happened:** under `act`, `cargo build` failed with "spurious network error … transfer
  too slow" on every crate, although `curl` inside the same image fetched the same crate in
  0.3 s. Two causes stacked: act's default `--network host`, and cargo's HTTP/2 multiplexing.
  With `--network bridge` and `CARGO_HTTP_MULTIPLEXING=false` the build ran in 16–26 s. The
  artifact upload then timed out: act's artifact server listens on the host, and bridge
  containers can reach neither its LAN IP (host firewall) nor `host.docker.internal` (wrong bind).
- **What it cost:** about 25 minutes and three full act runs.
- **How I got past it:** isolated each layer with a one-line `docker run … curl` probe in the same
  image, then stopped using act for artifacts. act proves build, package and smoke test for
  both Linux targets; the real `pull_request` run proves upload, `SHA256SUMS` and all four targets.
- **The line that would have prevented it:** "On Docker Desktop, run act with
  `--network bridge --env CARGO_HTTP_MULTIPLEXING=false`, and don't expect `upload-artifact`
  to work; probe the container's network with curl before re-running a 5-minute job."

## A positive control that ran half its tests

- **What happened:** the checksum mutation's run filtered to the unit test and the
  integration test, but `cargo test` stops after the first test binary that fails, so the
  integration test (`a_checksum_mismatch_leaves_the_binary_alone`) never ran under the mutation.
  The record looked red, while one of the two guards was never exercised.
- **What it cost:** one re-run of the mutation script (about 5 minutes); caught by reading
  which test names appeared under "FAILED", not the exit code.
- **How I got past it:** `cargo test --no-fail-fast` in every mutation trial.
- **The line that would have prevented it:** "A positive control across several test binaries
  runs with `--no-fail-fast`, and the record names every test expected to go red."

## `set -e` is off inside a function called from `||`

- **What happened:** `install_prebuilt || exit 1` looked strict, but bash disables `set -e` for
  the whole function body when it runs as the left side of `||`. A failed `cp` into a read-only
  `CCBOX_BIN_DIR` fell through, and the script wired and reported the *old* binary as installed.
  My proof never tried a read-only bin dir; the round-1 reviewer did.
- **What it cost:** one blocking finding and a fix round.
- **How I got past it:** explicit `|| return 1` on every step, plus a proof case (read-only bin dir
  holding an older binary).
- **The line that would have prevented it:** "In a shell function called from `||`/`&&`/`if`, check
  every command's status explicitly; `set -e` does not apply there."

## A record that says something was done, when it isn't done yet

- **What happened:** D13 said "the PR's after-merge steps now say …", but I planned to update the PR
  body only at the end, so the live PR still had the old steps. The round-2 reviewer read both
  and rightly flagged it as blocking.
- **What it cost:** a blocking finding in round 2, which forced a third round.
- **How I got past it:** updated the PR body at once, and wrote later DECISIONS entries only
  after the change they describe had landed.
- **The line that would have prevented it:** "Write a decision record in the past tense only after
  the change it describes has landed everywhere it names."
