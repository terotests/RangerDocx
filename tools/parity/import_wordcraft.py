#!/usr/bin/env python3
"""Refreshes parity/catalog.tsv and parity/typing.tsv from a WordCraft checkout.

    python3 tools/parity/import_wordcraft.py <path to storytold/wordcraft>

Only the WordCraft columns are replaced. The ranger_viewer, rangerdocx and
test columns already in the files are kept (matched by command id or by
section + expected text), so progress is never lost on a refresh. New
features arrive as `no`. Data source: WordCraft (MIT OR Apache-2.0), see
ATTRIBUTION.md.
"""
import os, re, sys

wc = sys.argv[1] if len(sys.argv) > 1 else "../wordcraft"
out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "parity")

def read_tsv(path):
    if not os.path.exists(path):
        return []
    lines = open(path, encoding="utf-8").read().splitlines()
    head = lines[0].split("\t")
    return [dict(zip(head, l.split("\t"))) for l in lines[1:] if l]

def write_tsv(path, head, rows):
    with open(path, "w", encoding="utf-8") as f:
        f.write("\t".join(head) + "\n")
        for r in rows:
            f.write("\t".join(r.get(h, "").replace("\t", " ") for h in head) + "\n")

# Catalog: crates/engine/src/catalog.rs, `Tab|Group|Feature|command id`
old = {r["command"]: r for r in read_tsv(os.path.join(out, "catalog.tsv"))}
missing = set()
for l in open(os.path.join(wc, "docs/parity-checklist.md"), encoding="utf-8"):
    m = re.search(r"\(`([^`]+)`\)", l)
    if l.startswith("- ") and m:
        missing.add(m.group(1))
src = open(os.path.join(wc, "crates/engine/src/catalog.rs"), encoding="utf-8").read()
body = src.split('pub const CATALOG: &str = "\\\n', 1)[1].split('";', 1)[0]
rows = []
for l in body.splitlines():
    l = l.rstrip("\\")
    if l.count("|") != 3:
        continue
    t, g, feat, cmd = l.split("|")
    prev = old.get(cmd, {})
    rows.append({"tab": t, "group": g, "feature": feat, "command": cmd,
                 "wordcraft": "missing" if cmd in missing else "live",
                 "ranger_viewer": prev.get("ranger_viewer", "no"),
                 "rangerdocx": prev.get("rangerdocx", "no"), "test": prev.get("test", "-")})
write_tsv(os.path.join(out, "catalog.tsv"),
          ["tab", "group", "feature", "command", "wordcraft", "ranger_viewer", "rangerdocx", "test"], rows)
print("catalog.tsv:", len(rows), "features,", len(missing), "missing in WordCraft")

# Typing scenarios: docs/typing-parity.md tables (Keys | result | Test) and
# bullet items that name a test.
oldt = {(r["section"], r["expected"]): r for r in read_tsv(os.path.join(out, "typing.tsv"))}
skip = ("Known gaps", "How to compare", "Revision history")
trows, sec, cur, bullets = [], None, None, []
for l in open(os.path.join(wc, "docs/typing-parity.md"), encoding="utf-8"):
    if l.startswith("## "):
        if cur: bullets.append((sec, cur)); cur = None
        sec = l[3:].strip(); continue
    if sec in skip:
        continue
    if l.startswith("| ") and not l.startswith("| Keys"):
        c = [x.strip() for x in l.strip().strip("|").split("|")]
        if len(c) == 3 and not set(c[0]) <= set("-: "):
            trows.append({"section": sec, "keys": c[0], "expected": c[1], "wordcraft_test": c[2]})
        continue
    if l.startswith("- "):
        if cur: bullets.append((sec, cur))
        cur = l[2:].strip()
    elif l.startswith("  ") and cur:
        cur += " " + l.strip()
    elif cur:
        bullets.append((sec, cur)); cur = None
if cur: bullets.append((sec, cur))
for sec, t in bullets:
    tests = [x for x in re.findall(r"`([a-z0-9_]+)`", t) if "_" in x]
    if not tests:
        continue
    d = re.sub(r"\s+", " ", t)
    d = d if len(d) <= 200 else d[:197] + "..."
    trows.append({"section": sec, "keys": "-", "expected": d,
                  "wordcraft_test": ", ".join("`" + x + "`" for x in tests)})
for r in trows:
    prev = oldt.get((r["section"], r["expected"]), {})
    r["rangerdocx"] = prev.get("rangerdocx", "no"); r["test"] = prev.get("test", "-")
write_tsv(os.path.join(out, "typing.tsv"),
          ["section", "keys", "expected", "wordcraft_test", "rangerdocx", "test"], trows)
print("typing.tsv:", len(trows), "scenarios")
