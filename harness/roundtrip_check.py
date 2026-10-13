#!/usr/bin/env python3
"""Round-trip oracle: original fixture vs what DocxWriter saved.

    npm run docx_viewer:roundtrip            # writes bin/roundtrip/{same,regen}
    python3 gallery/docx_viewer/harness/roundtrip_check.py [--soffice]

F2 (semantic): python-docx dump of the original and of the saved file are
compared - paragraph texts and style names, effective bold/italic per run,
tables (rows and cell texts), section and picture counts.
F3 (opens elsewhere): with --soffice, LibreOffice converts every saved file to
PDF headlessly; a failed conversion fails the file.
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from docx import Document
except ImportError:
    print("SKIP: python-docx not installed (pip install python-docx)", file=sys.stderr)
    sys.exit(0)

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
FIX = ROOT / "fixtures"
OUT = ROOT / "bin" / "roundtrip"
A = "{http://schemas.openxmlformats.org/drawingml/2006/main}blip"


def effective(run, para, attr):
    """bold/italic as Word would show it: run, then its character style
    chain, then the paragraph style chain."""
    v = getattr(run.font, attr)
    if v is not None:
        return bool(v)
    st = run.style
    while st is not None:
        v = getattr(st.font, attr)
        if v is not None:
            return bool(v)
        st = st.base_style
    st = para.style
    while st is not None:
        v = getattr(st.font, attr)
        if v is not None:
            return bool(v)
        st = st.base_style
    return False


def para_dump(p):
    marks = []
    for r in p.runs:
        if r.text:
            marks.append((r.text, effective(r, p, "bold"), effective(r, p, "italic")))
    # Character-level, so how the text is cut into runs does not matter.
    chars = []
    for text, b, i in marks:
        chars.extend((ch, b, i) for ch in text)
    return {"text": p.text, "style": p.style.name if p.style is not None else "", "fmt": chars}


def dump(path: Path) -> dict:
    d = Document(str(path))
    return {
        "paragraphs": [para_dump(p) for p in d.paragraphs],
        "tables": [
            {"rows": len(t.rows), "cells": [[c.text for c in row.cells] for row in t.rows]}
            for t in d.tables
        ],
        "sections": len(d.sections),
        "images": len(d.element.body.findall(".//" + A)),
    }


def diff(a: dict, b: dict) -> list[str]:
    out = []
    for k in ("sections", "images"):
        if a[k] != b[k]:
            out.append(f"{k}: {a[k]} -> {b[k]}")
    if len(a["paragraphs"]) != len(b["paragraphs"]):
        out.append(f"paragraphs: {len(a['paragraphs'])} -> {len(b['paragraphs'])}")
    for i, (pa, pb) in enumerate(zip(a["paragraphs"], b["paragraphs"])):
        for k in ("text", "style", "fmt"):
            if pa[k] != pb[k]:
                out.append(f"paragraph {i} {k} differs: {str(pa[k])[:60]!r} -> {str(pb[k])[:60]!r}")
                break
    if a["tables"] != b["tables"]:
        out.append("tables differ")
    return out


def soffice_ok(path: Path, soffice: str) -> bool:
    with tempfile.TemporaryDirectory() as tmp:
        r = subprocess.run(
            [soffice, "--headless", "--convert-to", "pdf", "--outdir", tmp, str(path)],
            capture_output=True, timeout=120,
        )
        pdf = Path(tmp) / (path.stem + ".pdf")
        return r.returncode == 0 and pdf.exists() and pdf.stat().st_size > 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--soffice", action="store_true", help="also convert every saved file with LibreOffice")
    ap.add_argument("--json", help="write the results here")
    args = ap.parse_args()
    soffice = shutil.which("soffice") if args.soffice else None
    if args.soffice and not soffice:
        print("SKIP soffice: not installed", file=sys.stderr)
    results = {}
    for mode in ("same", "regen"):
        passed = opened = total = 0
        for src in sorted(FIX.glob("*.docx")):
            saved = OUT / mode / src.name
            if not saved.exists():
                continue
            # Some fixtures are malformed on purpose (an unescaped '&', a
            # mismatched tag) to test a tolerant reader. They have no
            # reference to compare against and are left out of the count.
            try:
                ref = dump(src)
            except Exception as e:
                try:
                    dump(saved)
                    fixed = "saved file is well-formed"
                except Exception:
                    fixed = "saved file is malformed the same way"
                print(f"  SKIP {mode}/{src.name}: original is not well-formed ({fixed})")
                continue
            total += 1
            try:
                problems = diff(ref, dump(saved))
            except Exception as e:  # a file python-docx cannot read is a failure
                problems = [f"python-docx could not read it: {e}"]
            ok = not problems
            passed += ok
            so = soffice_ok(saved, soffice) if soffice else None
            opened += bool(so)
            mark = "PASS" if ok else "FAIL"
            extra = "" if so is None else (" soffice ok" if so else " soffice FAILED")
            print(f"  {mark} {mode}/{src.name}{extra}")
            for p in problems[:5]:
                print(f"       {p}")
        results[mode] = {"files": total, "semantic_equal": passed, "soffice_opened": opened if soffice else None}
        print(f"{mode}: semantic equal {passed}/{total}" + (f", LibreOffice opened {opened}/{total}" if soffice else ""))
    if args.json:
        Path(args.json).write_text(json.dumps(results, indent=2))
    return 0 if all(r["semantic_equal"] == r["files"] for r in results.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
