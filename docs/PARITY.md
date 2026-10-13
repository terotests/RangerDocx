# Feature parity: how RangerDocx is measured against WordCraft

RangerDocx is a Word-compatible editor written in Ranger (or Ranger-flavoured
Rust) on EVG/EVGUI. The comparison target is
[storytold/wordcraft](https://github.com/storytold/wordcraft) (Rust + egui,
alpha, itself measured against Word for Mac 16.113.4). WordCraft's code is not
copied; only its feature *lists* are used as the yardstick (see
[ATTRIBUTION.md](../ATTRIBUTION.md)).

The starting point is Ranger's `gallery/docx_viewer` (reader, layout, editing
MVP, no writer). It is scored as a third column so progress is visible from
day one.

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

## What the baseline shows

- docx_viewer already covers 20 of 502 catalog features (bold, italic,
  underline, font, size, colour, indents, undo/redo, select all; partial
  clipboard, table and chart insertion) and reads 14 of 55 OOXML groups.
- It writes nothing (C3 = 0, F2 = 0). A DOCX writer that preserves unknown
  parts is the first big step; Ranger's `gallery/ooxml` source-span writing
  (used by the pptx and xlsx editors) is the model to follow.
- Largest read gaps: footnotes, comments, track changes, fields, bookmarks,
  tab stops, vMerge/nested tables, floating images, first/even headers.
