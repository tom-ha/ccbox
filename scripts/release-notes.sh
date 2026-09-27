#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

version="${1:?usage: scripts/release-notes.sh X.Y.Z}"

python3 - "$version" <<'PY'
import re, sys

version = sys.argv[1]
lines = open("CHANGELOG.md").read().splitlines()
start = None
for i, line in enumerate(lines):
    if re.match(r"^## \[" + re.escape(version) + r"\]", line):
        start = i + 1
        break
if start is None:
    sys.exit(f"error: CHANGELOG.md has no '## [{version}]' section")
end = next((j for j in range(start, len(lines)) if lines[j].startswith("## [")), len(lines))
body = "\n".join(lines[start:end]).strip()
if not body:
    sys.exit(f"error: CHANGELOG.md section '## [{version}]' is empty")
print(body)
PY
