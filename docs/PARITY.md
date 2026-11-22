# Feature parity: how RangerDocx is measured against WordCraft

RangerDocx is a Word-compatible editor written in Ranger (or Ranger-flavoured
Rust) on EVG/EVGUI. The comparison target is
[storytold/wordcraft](https://github.com/storytold/wordcraft) (Rust + egui,
alpha, itself measured against Word for Mac 16.113.4). WordCraft's code is not
copied; only its feature *lists* are used as the yardstick (see
[ATTRIBUTION.md](../ATTRIBUTION.md)).

The code started as Ranger's `gallery/docx_viewer` (reader, layout, editing
MVP, no writer), moved here from Ranger commit 1b3126b. The "Ranger
docx_viewer" column is that snapshot, kept frozen as the baseline; the
RangerDocx column counts only what this repository's tests cover.

The numbers live in `parity/*.tsv`. `tools/parity/ParityReport.rgr` reads them
and writes [PARITY_REPORT.md](PARITY_REPORT.md); the same program compiles to
JavaScript and Rust and gives identical output.

## Rules

- **Tested or it does not count.** A RangerDocx row is scored only when its
  `test` column names a file that exists in this repository. The report lists
  rows that name a missing file under "Problems".
- Scoring: `yes`/`done`/`live` = 1, `partial`/`impl` = 0.5, `no` = 0.
- WordCraft columns are refreshed with `tools/parity/import_wordcraft.py`,
  which keeps our own columns intact.
- "unmeasured" is written as such. No estimate goes into a measured column.

## Metrics

| id | What | Source of truth | WordCraft today | Beta target |
|---|---|---|---|---|
| C1 | Ribbon/menu features with a working, tested command | `parity/catalog.tsv` (WordCraft's 502-row catalog; same command ids, e.g. `format.bold`) | 487 | 300 |
| C2/C3 | OOXML feature groups read / written | `parity/ooxml.tsv` (55 groups, ECMA-376 elements named per row) | 52 / 51 | 40 / 35 |
| C4 | Typing-behaviour scenarios (keys → expected document) | `parity/typing.tsv` (42 rows from WordCraft's `typing-parity.md`) | 42 | 30 |
| F1 | Corpus files that open without error | corpus manifest | unmeasured | 99% |
| F2 | Semantic round trip (open → save → reopen, python-docx dump equal) | `harness/oracles/semantic.py` (from docx_viewer) | unmeasured | 90% |
| F3 | Saved files open in LibreOffice without error | headless `soffice` | unmeasured | 100% |
| L1 | Page count equals reference render | `harness/oracles/visual.py` | unmeasured | 80% |
| L2 | Line breaks equal reference render | pdftotext bbox, first word per line | unmeasured | 85% |
| L3 | Visual similarity (mean SSIM per page) | page PNGs at 96 dpi | unmeasured | 0.90 |
| P1 | Cold open + layout, 188-page document | `bench/` | 61 ms | 150 ms |
| P2 | Relayout after one keystroke, 188 pages | `bench/` | 1.4 ms | 8 ms |
| P3 | Scroll frame, 1,096-page table document | `bench/` | 4.5 ms | 16 ms |
| U1 | Ribbon tabs with at least one working feature | `parity/catalog.tsv` | 24 | 10 |
| U2 | Modal dialogs | count | 54 | 25 |
| U3 | Side panes (Navigation, Styles, Comments…) | count | 7 | 3 |
| R1 | Hostile inputs that crash or hang | `tests/malformed` | 0 | 0 |
| R2 | Fuzz iterations without a crash | `tests/fuzz` | none run | 100,000 |
| X1 | Other formats (PDF, RTF, ODT, HTML, MD, TXT, LaTeX, .doc) | count | 8 | 4 |
| A1 | Accessibility tree for the document canvas | yes/no | yes | yes |
| T1 | Automated tests | count | ~1,930 | 500 |

Beta targets are first proposals, chosen so the core loop (open a real .docx,
edit, save, reopen in Word and LibreOffice) is solid before breadth.

### Head-to-head on the same corpus

WordCraft has no real-world DOCX corpus either (its gap #1). F1–F3 and L1–L3
are run on **one shared corpus for both apps**: WordCraft through
`wordcraft-cli` (PDF export, open/save), RangerDocx through its own CLI.
The reference renderer is LibreOffice; Word PDFs are used when available but
stay local (Word output must not be committed). The corpus starts with
docx_viewer's 19 generated fixtures and grows with public-domain documents
listed in a manifest (URL + SHA-256), not committed binaries.

### Performance

P1–P3 use WordCraft's own published scenarios so the numbers are comparable.
Both apps run on the same machine, release builds, median of 5 runs.

## UI direction (U1–U3)

The reference is a Word-style Home ribbon (Clipboard, Font, Paragraph, Styles,
Editing groups), a Navigation pane with Headings / Pages / Results, rulers and
a centred page. RangerDocx keeps that structure so the catalog maps 1:1, and
builds it on EVGUI controllers (Tabs, Toolbar, Combobox, Popover, Dialog,
Tree, ScrollArea) with a more modern look: flatter ribbon with fewer
borders, rounded style gallery cards, a quieter navigation pane, and a
collapsible ribbon. Visual changes do not change command ids.

## Writing (C3, F2, F3)

A C3 row is `yes` when its elements survive a save even inside an edited
paragraph or table, and `partial` when they survive only while the paragraph
around them is not edited, or when part of the row is untested (strike,
complex-script properties) or not editable (headers and footers). Rows are
scored only for elements the fixtures actually contain; the fixtures are
simple, so footnotes, comments, fields and the like stay at 0 until fixtures
with them exist.

F2 and F3 currently run on the 19 docx_viewer fixtures, not on a real-world
corpus; two of them are malformed on purpose and have no reference to compare
against. The `regen` run forces every block through regeneration, the worst
case for an edited document.

## What the baseline shows

- The moved code covers 20 of 502 catalog features (bold, italic,
  underline, font, size, colour, indents, undo/redo, select all; partial
  clipboard, table and chart insertion) and reads 14 of 55 OOXML groups.
- It writes nothing (C3 = 0, F2 = 0). A DOCX writer that preserves unknown
  parts is the first big step; Ranger's `gallery/ooxml` source-span writing
  (used by the pptx and xlsx editors) is the model to follow.
- Largest read gaps: footnotes, comments, track changes, fields, bookmarks,
  tab stops, vMerge/nested tables, floating images, first/even headers.

## Re-measure 2026-11-22 (WordCraft 33d874c)

- WordCraft grew: catalog 502 → 522 rows (510 live), typing scenarios
  42 → 110, `#[test]` functions 1,930 → 4,802. The "View › Window" rows it
  lacked are now live; new tabs/rows: Read Mode, Mailings record navigation,
  Spike, underline colour, emphasis marks, multilevel list library.
- RangerDocx since 2026-10-16: .odt/.rtf import (e84b53a, X1 1 → 3),
  theme-font metric twins (91ed06d), sources converted to Ranger's Rust
  syntax (src/*.rs, 87db17b). No new catalog commands, so C1 stays 52
  (10.0% of 522, was 10.4% of 502).
- `parity/ooxml.tsv` (C2/C3) is not refreshed by the import script; its
  WordCraft columns are from 2026-10-13.
- C4 is still 0: there is no keystroke-script runner. That runner is the
  cheapest way to start scoring the 110 typing rows.

### Next steps, in order

1. Typing-scenario runner (tests/TypingTest) over the editor model; start
   with Lists, Undo, Editing keys (C4).
2. Home tab gaps (38 of 105): multilevel list, font/paragraph dialogs,
   link, styles pane.
3. Insert and Table Layout (2.5 of 47, 0 of 33): table editing commands
   on top of the existing table insert.
4. Read gaps (C2): footnotes, comments, track changes, fields, bookmarks.
5. Layout and performance metrics (L1–L3, P1–P3) on the shared corpus.

### Progress 2026-11-22

| Step | Commit | Result |
|---|---|---|
| 1. Typing runner | bcc914b | tests/TypingTest (156 checks). C4 0 → 28.5 of 110 (25 yes, 7 partial) |
| 2. Home tab | 0d18146 | tests/HomeTest (43 checks): keeps, outline level, hanging indent, direction, Heading 3, Sort, copy/paste formatting, Find Previous, Go To. C1 52 → 96 |
| 3. Table Layout | b0f91df | tests/TableTest (78 checks): insert/delete rows, columns, cells and table; select cell/row/column/table; merge, split cell, split table; column width, row height, distribute, AutoFit to window; cell alignment, shading, header rows; sort; convert to text and back. All undoable, saved and read back. C1 96 → 119.5, C2 14 → 14.5, C3 15 → 15.5 |
| 4. Read gaps | 36d65f3 | tests/ReviewTest (66 checks), fixture 26-review-notes.docx: tracked insertions and deletions read, drawn in the author's colour, accepted or rejected (one, all, next, previous) and written back as w:ins/w:del; comments read (comments.xml), their ranges highlighted, browsed, deleted and added (comments.xml written, made when missing); footnotes and endnotes read and laid out after the text; bookmarks inserted and reached with Go To; fields, footnote references, bookmarks, comment ranges and links kept through edits (anchors now follow the text). C1 119.5 → 134, C2 14.5 → 18.5, C3 15.5 → 20 |
| 5. Layout and performance metrics | (this commit) | harness/layout_metrics.py (L1–L3 against LibreOffice PDFs of the 19 fixtures it opens) and bench/parity_bench.rgr (P1–P3 on synthetic 188- and 1,096-page documents from bench/make_bench_docs.py). Fixed on the way: header and footer sit at the pgMar header/footer distance and the body starts at the top margin (was pushed below the header); tables break between rows and repeat their header rows (a long table ran off the page); paragraph wraps and line widths are cached per paragraph, so a keystroke re-measures one paragraph (P2 109 → 3.3 ms). L1 94.7%, L2 47.5%, L3 0.922, P1 250 ms, P2 3.3 ms, P3 6.4 ms. T1 1382 |

Left partial in step 3: merging across rows merges each row on its own
(vertical merge, `w:vMerge`, is not in the model); Distribute Rows uses the tallest set height,
not the laid-out one; AutoFit to contents and fixed width are not done.
Not started in step 3: Draw Table, Eraser, Quick Tables, table styles and
borders, cell margins, text direction, table direction, formula, properties.

Left partial in step 4: notes are laid out after the text, not at the foot
of their page, and their reference marks are not drawn; footnotes cannot be
inserted yet; fields show their stored result (no update, no field codes);
Track Changes recording while typing is not done (changes are only read,
accepted and rejected); a link split by Enter loses its link.

Step 5 notes: the reference is LibreOffice, not Word, and WordCraft was not
run on this corpus, so L1–L3 have no WordCraft value. The L2 misses are
mostly lines that fill the column to within about 0.5%: LibreOffice's text
is about 0.4% wider than Carlito's advance widths, so it breaks one word
earlier and the rest of the paragraph shifts. Which of the two Word agrees
with needs a Word-made PDF. P1–P3 use synthetic documents of the same page
counts as WordCraft's (theirs are not public), run under node on the JS
target. P1 (250 ms, target 150) is about half parsing and half the first
layout; the first layout of the 1,096-page table takes about 1.3 s. P2 still
rebuilds the whole page list after the cached wraps; a layout that resumes
from the edited page is the next step there.
