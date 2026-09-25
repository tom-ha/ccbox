#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname -- "${BASH_SOURCE[0]}")/.."

python3 - "${1:-}" <<'PY'
import json, re, sys

expected = sys.argv[1]
fields = []

with open("Cargo.toml") as f:
    section = None
    for line in f:
        m = re.match(r"\s*\[([^\]]+)\]", line)
        if m:
            section = m.group(1).strip()
            continue
        m = re.match(r'\s*version\s*=\s*"([^"]*)"', line)
        if section == "package" and m:
            fields.append(("Cargo.toml [package].version", m.group(1)))
            break

with open(".claude-plugin/marketplace.json") as f:
    market = json.load(f)
fields.append((".claude-plugin/marketplace.json metadata.version", market.get("metadata", {}).get("version")))
for i, p in enumerate(market.get("plugins", [])):
    fields.append((f".claude-plugin/marketplace.json plugins[{i}].version", p.get("version")))

with open("plugins/ccbox/.claude-plugin/plugin.json") as f:
    fields.append(("plugins/ccbox/.claude-plugin/plugin.json version", json.load(f).get("version")))

want = expected or fields[0][1]
for name, value in fields:
    print(f"{name} = {value}")
bad = [(n, v) for n, v in fields if v != want]
if bad:
    source = f"expected {want} (from the tag)" if expected else f"expected {want} (from Cargo.toml)"
    print(f"error: version mismatch, {source}:", file=sys.stderr)
    for n, v in bad:
        print(f"  {n} = {v}", file=sys.stderr)
    sys.exit(1)
print(f"ok: every version field is {want}")
PY
