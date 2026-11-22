#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Layout metrics L1-L3 (parity/metrics.tsv) against a LibreOffice reference.

For every fixtures/*.docx:
  reference  soffice -> PDF; pdftoppm -> PNG at 96 dpi; pdftotext -bbox-layout -> lines
  RangerDocx bin/docx_oracle_dump.js -> page PNGs + lines.tsv (DocxViewer.layoutLinesText)

  L1  share of documents whose page count equals the reference
  L2  share of reference lines whose line break RangerDocx reproduces: the
      documents' line lists (first word + last word of each line, in reading
      order) are aligned with difflib, and matched lines / reference lines is
      the score. Page boundaries are ignored, L1 covers those.
  L3  mean SSIM (grayscale, 8x8 box window) over the pages both renders have,
      averaged per document, then over the corpus

Run from the Ranger root after the dump tool is compiled (run_oracles.mjs does that):
  python3 gallery/docx_viewer/harness/layout_metrics.py [--json out.json]
"""
from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
DOCX = HERE.parent
ROOT = DOCX.parent.parent
OUT = HERE / "out" / "layout"
DPI = 96
FONTS: Path


def reference(docx: Path, out: Path) -> tuple[list[Path], list[list[str]]]:
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="docx-lm-") as tmp:
        t = Path(tmp)
        prof = t / "profile"
        prof.mkdir()
        subprocess.run(
            ["soffice", f"-env:UserInstallation=file://{prof}", "--headless", "--norestore",
             "--convert-to", "pdf", "--outdir", str(t), str(docx)],
            capture_output=True, check=True)
        pdfs = list(t.glob("*.pdf"))
        if not pdfs:
            raise RuntimeError("LibreOffice could not load it")
        pdf = pdfs[0]
        subprocess.run(["pdftoppm", "-png", "-r", str(DPI), str(pdf), str(t / "page")],
                       capture_output=True, check=True)
        for old in out.glob("ref-*.png"):
            old.unlink()
        pngs = []
        for i, p in enumerate(sorted(t.glob("page*.png"))):
            dest = out / f"ref-{i:03d}.png"
            shutil.copy2(p, dest)
            pngs.append(dest)
        html = subprocess.run(["pdftotext", "-bbox-layout", str(pdf), "-"],
                              capture_output=True, text=True, check=True).stdout
    return pngs, ref_lines(html)


def ref_lines(html: str) -> list[list[str]]:
    """Lines per page from pdftotext -bbox-layout, as lists of words, top to bottom."""
    pages = []
    for page in re.findall(r"<page .*?</page>", html, re.S):
        lines = []
        for line in re.findall(r"<line [^>]*yMin=\"([\d.]+)\"[^>]*>(.*?)</line>", page, re.S):
            words = re.findall(r"<word [^>]*xMin=\"([\d.]+)\"[^>]*>(.*?)</word>", line[1], re.S)
            if words:
                x0 = float(words[0][0])
                lines.append((float(line[0]), x0, [unescape(w[1]) for w in words]))
        lines.sort(key=lambda l: (round(l[0] / 4), l[1]))
        pages.append([l[2] for l in lines])
    return pages


def unescape(s: str) -> str:
    return s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", '"').replace("&#39;", "'")


def font_dir() -> Path:
    """The faces a host gives the viewer: the shared set plus this repository's
    fonts/ (Carlito, Caladea), which gallery/pdf_writer/assets/fonts lacks."""
    d = OUT / "fonts"
    d.mkdir(parents=True, exist_ok=True)
    for src in [ROOT / "gallery/pdf_writer/assets/fonts", DOCX / "fonts"]:
        for sub in src.iterdir():
            link = d / sub.name
            if sub.is_dir() and not link.exists():
                link.symlink_to(sub.resolve())
    return d


def ranger(docx: Path, out: Path) -> tuple[list[Path], list[list[str]]]:
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("page-*.png"):
        old.unlink()
    env = dict(os.environ, RANGER_LIB="./compiler/Lang.rgr:./lib/stdops.rgr")
    subprocess.run(["node", str(DOCX / "bin/docx_oracle_dump.js"), str(docx), str(out), str(FONTS)],
                   cwd=ROOT, env=env, capture_output=True, check=True)
    pngs = sorted(out.glob("page-*.png"))
    by_page: dict[int, list] = {}
    for row in (out / "lines.tsv").read_text(encoding="utf-8").splitlines():
        parts = row.split("\t", 4)
        if len(parts) < 5:
            continue
        page, _kind, y, x, text = int(parts[0]), parts[1], int(parts[2]), int(parts[3]), parts[4]
        words = text.split()
        if words:
            by_page.setdefault(page, []).append((y, x, words))
    lines = []
    for i in range(len(pngs)):
        rows = sorted(by_page.get(i, []), key=lambda l: (round(l[0] / 4), l[1]))
        lines.append([r[2] for r in rows])
    return pngs, lines


def key(words: list[str]) -> str:
    """A line is identified by its first and last word: equal keys = the same break."""
    norm = [re.sub(r"\W+", "", w).lower() for w in words]
    norm = [w for w in norm if w] or [""]
    return norm[0] + "|" + norm[-1]


def line_score(ref: list[list[list[str]]], got: list[list[list[str]]]) -> tuple[int, int]:
    a = [key(l) for page in ref for l in page]
    b = [key(l) for page in got for l in page]
    if not a:
        return 0, 0
    sm = difflib.SequenceMatcher(None, a, b, autojunk=False)
    return sum(m.size for m in sm.get_matching_blocks()), len(a)


def gray(p: Path) -> np.ndarray:
    return np.asarray(Image.open(p).convert("L"), dtype=np.float64)


def box_mean(x: np.ndarray, k: int) -> np.ndarray:
    c = np.pad(x, ((1, 0), (1, 0))).cumsum(0).cumsum(1)
    s = c[k:, k:] - c[:-k, k:] - c[k:, :-k] + c[:-k, :-k]
    return s / (k * k)


def ssim(a: np.ndarray, b: np.ndarray, k: int = 8) -> float:
    if a.shape != b.shape:
        h, w = a.shape
        b = np.asarray(Image.fromarray(b.astype(np.uint8)).resize((w, h)), dtype=np.float64)
    c1, c2 = (0.01 * 255) ** 2, (0.03 * 255) ** 2
    ma, mb = box_mean(a, k), box_mean(b, k)
    va = box_mean(a * a, k) - ma * ma
    vb = box_mean(b * b, k) - mb * mb
    cov = box_mean(a * b, k) - ma * mb
    m = ((2 * ma * mb + c1) * (2 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))
    return float(m.mean())


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--json")
    ap.add_argument("--fixture")
    args = ap.parse_args()
    if not shutil.which("soffice") or not shutil.which("pdftotext"):
        print("SKIP: need soffice and poppler-utils")
        return 0
    if not (DOCX / "bin/docx_oracle_dump.js").exists():
        print("bin/docx_oracle_dump.js missing: run harness/run_oracles.mjs --skip-visual first")
        return 1
    global FONTS
    FONTS = font_dir()
    rows = []
    skipped = []
    for docx in sorted((DOCX / "fixtures").glob("*.docx")):
        if args.fixture and docx.name != args.fixture:
            continue
        case = OUT / docx.stem
        try:
            ref_png, ref_ln = reference(docx, case)
        except RuntimeError as e:
            skipped.append(docx.name)
            print(f"{docx.name:28s} skipped: {e}")
            continue
        got_png, got_ln = ranger(docx, case)
        matched, total = line_score(ref_ln, got_ln)
        n = min(len(ref_png), len(got_png))
        sims = [ssim(gray(ref_png[i]), gray(got_png[i])) for i in range(n)]
        row = {
            "file": docx.name,
            "refPages": len(ref_png),
            "pages": len(got_png),
            "linesMatched": matched,
            "refLines": total,
            "ssim": round(sum(sims) / len(sims), 4) if sims else 0.0,
        }
        rows.append(row)
        print(f"{docx.name:28s} pages {row['pages']:3d}/{row['refPages']:<3d} "
              f"lines {matched:4d}/{total:<4d} ssim {row['ssim']:.3f}")
    if not rows:
        return 1
    l1 = 100.0 * sum(r["pages"] == r["refPages"] for r in rows) / len(rows)
    tot = sum(r["refLines"] for r in rows)
    l2 = 100.0 * sum(r["linesMatched"] for r in rows) / tot if tot else 0.0
    l3 = sum(r["ssim"] for r in rows) / len(rows)
    print(f"\nL1 page count equal: {l1:.1f}% of {len(rows)} documents")
    print(f"L2 line breaks equal: {l2:.1f}% of {tot} reference lines")
    print(f"L3 mean SSIM: {l3:.3f}")
    if skipped:
        print("no reference render: " + ", ".join(skipped))
    if args.json:
        Path(args.json).write_text(json.dumps({"L1": l1, "L2": l2, "L3": l3, "rows": rows, "skipped": skipped}, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
