# STATE — self-update run

PROOF=docs/longhaul/2026-09-25-self-update/proof

## Item in progress

None: items 0–7 are done. The run ended with PR #5 ready for review.

## Done (evidence in $PROOF; regenerate the local part with `$PROOF/run-all.sh`)

- 0 Setup: `ccbox-baseline`, `owner-files-before.txt` / `owner-files-after.txt` (diff empty).
- 1 Release scaffolding: `release-rehearsal.txt`.
- 2 Workflow: `actionlint.txt`, `act.txt`, `pr-checks.txt` (run 36193766197 at a3755f4, all green,
  publish skipped), `musl-runs.txt`, `version-gate.txt`.
- 3 CLI verbs: `cli.txt`.
- 4 setup + update: `setup-diff.txt`, `update-e2e.txt`, `update-check-github.txt`.
- 5 Check + chip: `chip-*.txt`, `chip-snapshot.json`, `nochip-*.txt`, `check-requests.log`, `disabled.txt`.
- 6 Install/uninstall/docs: `install.txt`, `uninstall.txt`.
- 7 Review: 3 rounds (round 3 clean), whole proof re-run at a3755f4; `mutations.txt`, `bench.txt`,
  `cargo-test.log` (501/0), `clippy.log` (21).

## Open pull requests

- #5 ready for review: https://github.com/tom-ha/ccbox/pull/5 (head a3755f4).

## Issues filed

#6 render time, #7 fmt/clippy, #8 skip a version, #9 tokyonight (fixed in #5), #10 stale-lock race,
#11 install.sh probe timeout.
