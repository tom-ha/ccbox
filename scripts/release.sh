#!/usr/bin/env bash
set -euo pipefail

err() { printf 'error: %s\n' "$*" >&2; exit 1; }

version="${1:-}"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || err "usage: scripts/release.sh X.Y.Z (got '${version}')"
tag="v$version"

cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

git diff --quiet && git diff --cached --quiet || err "working tree has uncommitted changes; commit or stash them first"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  err "tag $tag already exists"
fi
branch="$(git symbolic-ref --short -q HEAD)" || err "HEAD is detached; check out the branch to release from"

files=(CHANGELOG.md Cargo.toml Cargo.lock .claude-plugin/marketplace.json plugins/ccbox/.claude-plugin/plugin.json)
trap 'git checkout -q -- "${files[@]}"; err "release aborted; version files restored"' ERR

python3 - "$version" <<'PY'
import datetime, json, re, sys

version = sys.argv[1]

log = open("CHANGELOG.md").read()
m = re.search(r"^## \[Unreleased\][^\n]*\n(.*?)(?=^## \[|\Z)", log, re.S | re.M)
if not m or not m.group(1).strip():
    sys.exit("error: CHANGELOG.md has no entries under '## [Unreleased]'")
today = datetime.date.today().isoformat()
body = m.group(1).strip("\n")
log = log[: m.start()] + f"## [Unreleased]\n\n## [{version}] - {today}\n\n{body}\n\n" + log[m.end():].lstrip("\n")
open("CHANGELOG.md", "w").write(log.rstrip("\n") + "\n")

cargo = open("Cargo.toml").read().split("\n")
section = None
for i, line in enumerate(cargo):
    h = re.match(r"\s*\[([^\]]+)\]", line)
    if h:
        section = h.group(1).strip()
    elif section == "package" and re.match(r'\s*version\s*=', line):
        cargo[i] = re.sub(r'"[^"]*"', f'"{version}"', line, count=1)
        break
open("Cargo.toml", "w").write("\n".join(cargo))

def bump(path, edit):
    data = json.load(open(path))
    edit(data)
    open(path, "w").write(json.dumps(data, indent=2, ensure_ascii=False) + "\n")

def market(d):
    d["metadata"]["version"] = version
    for p in d["plugins"]:
        p["version"] = version

bump(".claude-plugin/marketplace.json", market)
bump("plugins/ccbox/.claude-plugin/plugin.json", lambda d: d.__setitem__("version", version))
PY

cargo update -p ccbox --quiet
cargo check --locked --quiet
scripts/check-versions.sh "$version"

git add "${files[@]}"
git commit -q -m "release: $tag"
trap - ERR
git tag -a "$tag" -m "ccbox $tag"

echo "==> committed 'release: $tag' and tagged $tag"
echo "==> publish it with: git push origin $branch $tag"
