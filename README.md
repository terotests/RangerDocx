# RangerDocx

A Word-compatible document editor written in [Ranger](https://github.com/terotests/Ranger)
(or Ranger-flavoured Rust), with its UI on EVG / EVGUI. Feature parity is
measured against [WordCraft](https://github.com/storytold/wordcraft).

> **Moved out of Ranger.** This repository is what was `gallery/docx_viewer`
> in [terotests/Ranger](https://github.com/terotests/Ranger) (master, commit
> 1b3126b), and it is developed here from now on. It builds inside a Ranger
> checkout: clone it to `gallery/docx_viewer` there (Ranger's compiler,
> `lib/evg`, `lib/image`, `gallery/ooxml`, `gallery/office`,
> `gallery/text_editor`, `gallery/datagrid` and the other modules it imports
> are found by relative path). Run `npm run deps` in Ranger first. The npm
> scripts are in [`ranger-scripts.json`](ranger-scripts.json); Ranger's
> `package.json` already has them under the same names.
>
> ```bash
> git clone https://github.com/terotests/RangerDocx <ranger>/gallery/docx_viewer
> cd <ranger> && npm run deps && npm run docx_viewer:test
> ```
>
> Imported from outside this repository by Ranger's `gallery/ooxml` tests and
> `gallery/mfiles`, which find it at the same path.

## Feature parity

- [docs/PARITY.md](docs/PARITY.md): the metrics, how each is measured, targets
- [docs/PARITY_REPORT.md](docs/PARITY_REPORT.md): current numbers (generated)
- `parity/*.tsv`: the data (catalog, OOXML, typing scenarios, metrics)

```bash
tools/parity/run.sh          # regenerate docs/PARITY_REPORT.md (JavaScript)
tools/parity/run.sh rust     # same program compiled to Rust
python3 tools/parity/import_wordcraft.py <wordcraft checkout>   # refresh WordCraft columns
```

Outside a Ranger checkout, set `RANGER=<ranger checkout>`.

## The EVGUI editor (shell/)

`shell/DocsApp.rgr` is the Word-style editor: the ribbon, menus and dialogs are
EVGUI controllers (the same pattern as EVGSheets' `SheetsApp`, and its
`RibbonCtl`), and the document area is the RangerDocx core painted into the
`dx-doc` element. Icons are Lucide (ISC), converted into `shell/DocsIcons.rgr`.

```bash
node scripts/build.mjs --test          # compile + run tests/DocsAppTest.rgr
node scripts/build.mjs --out dist      # build the site (web/docs + fonts)
```

The build finds Ranger via `--ranger`, `$RANGER_DIR` or `../Ranger`; EVGUI and
EVGSheets via `--evgui`/`--evgsheets`, `$EVGUI_DIR`/`$EVGSHEETS_DIR`, a sibling
`../EVGUI`/`../EVGSheets`, else a clone into `.deps/`.
GitHub Pages serves this editor at `/` and the older standalone editor at `/classic/`.
Query options: `?theme=dark`, `?ui=compact|viewer`, `?readOnly`, `?docx=<url>`.

## License

AGPL-3.0-or-later ([LICENSE](LICENSE)). The parity data derived from WordCraft
is MIT, see [ATTRIBUTION.md](ATTRIBUTION.md).

## The viewer and editor

WordprocessingML → normalized `RichDocument` → section-aware `DocxLayout` →
SoftCanvas / EVG. Aimed at a **credible multi-page business DOCX viewer**, not
scaffolding.

```text
DocxPackage
     ↓
WordMlParser
     ↓
WordStyleResolver / WordNumbering / relationships
     ↓
RichDocumentModel  (blocks: paragraph | table | pageBreak | sectionBreak)
     ↓
DocumentLayout → Page[]  (resolved geometry)
     ↓
EVG / SoftCanvas
```

EVG never sees `w:pStyle`, `w:numId`, or OOXML inheritance — only resolved
fonts, colors, and placed geometry.

Caret model (future edit): **`paragraphId + textOffset`**, not run indices.
Adjacent equivalent `w:r` are merged into format spans.

## What works

- **Styles**: `docDefaults` → `basedOn` (loop-guarded) → paragraph / character
  style → direct `pPr` / `rPr` → `ResolvedRunStyle` / paragraph metrics
- **Text**: font size, bold/italic/underline, color, alignment (incl. justify
  flag), left/right/first-line/hanging indent, spacing before/after, line
  spacing, wrap to content width
- **Numbering**: `numbering.xml` → `WordNumbering` → `ResolvedListMarker` via
  `%1`/`%2`/`%3` substitution (not baked into paragraph text)
- **Tables**: `w:tbl` / grid / rows / cells / `gridSpan` / fill / borders /
  cell padding; flow layout (not DataGrid)
- **Pagination**: `sectPr` page size & margins, portrait/landscape, explicit
  page breaks, section breaks, paragraph flow across pages, header/footer
  bands, multi-page gap in `presentAll`
- **Header / footer**: relationship parts, default text (architecture allows
  first/odd/even later)
- **Images**: DrawingML → JPEG (`JPEGDecoder`) and PNG (`DocxPng` → SoftCanvas
  `ImageBuffer`), extent / aspect, inline flow

## Fixtures

| File | Covers |
| --- | --- |
| `hello.docx` | Title + mixed runs |
| `styles_demo.docx` | Headings, align, color/size |
| `lists_demo.docx` | Bullets, decimal, multilevel |
| `images_demo.docx` | DrawingML JPEG |
| `05-table-basic.docx` | 3×3 table |
| `06-table-format.docx` | Cell fill + borders |
| `07-table-merged.docx` | `gridSpan` |
| `08-pages.docx` | Multi-page flow |
| `09-page-break.docx` | Explicit page break |
| `10-landscape-section.docx` | Landscape section |
| `11-mixed-sections.docx` | Different margins |
| `12-header-footer.docx` | Header/footer parts |
| `13-images-mixed.docx` | JPEG + PNG |
| `14-styles-indent.docx` | Indent / underline / justify |
| `20-business-report.docx` | Kitchen-sink ~4–7 page report |

Regenerate: `npm run docx_viewer:fixtures` (`ffmpeg` + Pillow).

## Oracles

| Layer | Command | Notes |
| --- | --- | --- |
| A. Ranger tests | `npm run docx_viewer:test` | Hard-fail |
| B. Semantic | `npm run docx_viewer:oracles` | python-docx ↔ `inspectJson` |
| C. Visual | `npm run docx_viewer:oracles:visual` | LibreOffice → PDF → PNG vs Ranger; skips if `soffice`/`pdftoppm` missing |

See `harness/ORACLES.md`.

## Running it in a browser, with no host and no PNG

The page is the word-processor frame (`src/DocxChrome.rgr`, turned on with
`DocxWeb.setRibbon(true)`): a title bar, a ribbon of grouped commands (Home,
Insert, Layout, References, Review, View, File), the navigation pane
(headings, pages, search results), both rulers, and a status bar with the word
count and a zoom slider. All of it is drawn by the engine into the same display
list as the pages. Commands that do not exist yet are drawn greyed, so the
ribbon doubles as the parity checklist. `npm run docx_viewer:chrome:test`
checks it. The compact strip is still the default for other hosts.

Every push to `main` builds this page and publishes it to GitHub Pages
(<https://terotests.github.io/RangerDocx/>, workflow `.github/workflows/pages.yml`).

```bash
npm run docx_viewer:web         # build gallery/docx_viewer/web/standalone/dist
npm run docx_viewer:web:serve   # …and serve it on :8002
npm run docx_viewer:web:test    # open it in headless Chrome and use it
```

Serve the **`dist/`** the build writes, not the source directory beside it:
`index.html` and `standalone.mjs` live in both, the compiled engine and the
WebGL module only in `dist/`. Getting that wrong used to leave a page that sat
at "starting…" with nothing but `DocxWeb is not defined` in the console; the
page now says which file is missing and which command writes it. The same is
true of the slide and spreadsheet pages, which are built the same way.

This viewer used to be the odd one out. The spreadsheet and the slide deck have
built `EVGDisplayList`s for a long time and let WebGL draw them; a document was
rasterized **on the server** and shipped to the browser as a PNG per page — a
machine with a GPU receiving a picture of a page it could have drawn.

The layout and the painting did not change. What changed is where the marks go:
[`DocxInk`](src/DocxInk.rgr) is one object with two modes, and every `rect`,
`text` and `image` the page painter makes either lands on the SoftCanvas or
becomes a draw command. A second painter would have meant two descriptions of
what a Word page looks like, drifting apart at their own pace; this way a
difference between the backends would have to be a difference in EVG.

```text
was                                     now
DocxLayout → SoftCanvas → PNG → HTTP    DocxLayout → EVGDisplayList → WebGL
```

A chart in the document is the clearest case: it already spoke display list, so
on this path its commands simply join the page's — nothing is rasterized at all.

The rest is the same recipe as the other two galleries: bytes instead of paths
for the fonts and the document (`DocxPackage.openBytes` over
`ZipReader.openBytes`), and the document's own image parts handed to the
renderer rather than served, since the package is closed by the time anything is
drawn.

The page tests itself (`?selftest=1`, read back through headless Chrome's
`--dump-dom`): it checks the context really is WebGL 2, that the page arrived as
text runs rather than one big picture, turns a page, types, and pastes a chart
copied out of the spreadsheet — then checks that the chart is drawn as geometry
and can still be re-typed here.

## Run

```bash
npm run docx_viewer:test
npm run docx_viewer:chart:test    # copy a chart in the grid, paste and edit it here
npm run docx_viewer:demo          # PNG snapshots under gallery/docx_viewer/
npm run docx_viewer:oracles
npm run docx_viewer:module
npm run docx_viewer:window        # http://127.0.0.1:8770/
```

## Saving

`DocxWriter` (`src/DocxWriter.rgr`, `DocxViewer.saveBytes()` / `saveFile()`)
writes the document back by splicing the `word/document.xml` it was read
from:

- a paragraph or table whose fingerprint (`src/DocxFingerprint.rgr`) still
  matches the one stamped when it was read is copied byte for byte;
- an edited one is regenerated from the model, keeping its `w:p` open tag,
  its `w:pPr` when the paragraph properties did not change (always its
  `w:sectPr`), and its inline anchors (tabs, pictures, field delimiters, note
  references) at their text offsets;
- a new one is generated, a deleted one is skipped, and everything between
  blocks (body-level bookmarks, `w:sdt` wrappers, the final `w:sectPr`) is
  copied through;
- every other part of the package is copied unchanged. A document built in
  memory gets a minimal package (content types, relationships, styles).

Checked by `npm run docx_viewer:write:test` (unchanged fixtures come back
byte for byte; edits, splits, merges, table edits, pictures, non-ASCII text
and a fresh document reopen correctly) and `npm run docx_viewer:roundtrip:check`
(python-docx compares original and saved files, LibreOffice converts every
saved file; also with every block forced through regeneration).

## Known limitations

- Not a full Word clone. Saving keeps unedited paragraphs, tables and parts
  byte for byte; an edited paragraph loses hyperlink, tracked-change and
  bookmark markup inside it, and run properties the model does not hold
  (see [Saving](#saving))
- Paste builds a plain table: no column widths from the source, cell borders,
  merged rows, or images
- SoftCanvas glyph metrics can clip the last letter(s) of long lines
- Per-span font *size* still paints at the visual line's size (layout wraps the
  paragraph at one size); only the face varies per run
- Selection is a single range: no multi-select, no column selection
- Tab does not insert tab characters (no tab-stop rendering)
- Images cannot be selected or resized yet: the caret can reach them, but there
  are no selection handles and no drag-to-resize
- Requesting a page past the end renders the last page (clamped); the host
  reports the page it actually drew so the UI cannot offer a phantom one
- Justify is flagged but not full glyph-distributed justification
- Table row vertical merge, nested tables, floating images: out of scope
- Header/footer: default only (no different first / odd-even yet)
- Fields (PAGE) are static text unless authored that way in the fixture
- Visual oracle requires LibreOffice + poppler; advisory unless `--strict-visual`
- Editing proof-of-concept (TextEditCore) not wired in this pass

## Editing (MVP)

`TextEditCore` types (`DocumentPosition` / `DocumentRange` / `DocEditOp`) live in
`gallery/text_editor/src/TextEditCore.rgr`. DOCX binds them via:

```text
DocxEditController → RichDocumentEdit → RichDocument (paragraph + spans)
                 ↘ DocxLayout hit-test / caret geometry
                 ↘ ClipboardTable (clipboard HTML / TSV → table)
```

Window host: toggle **Edit**, click a body paragraph, then use the keyboard
below. The same SoftCanvas layout drives caret placement.

v1 scope: body paragraphs only (not tables/headers yet).

## Keyboard

| Key | Does |
| --- | --- |
| ← → | One character; crosses into the previous / next paragraph at the ends |
| ↑ ↓ | One visual line, keeping the column; crosses page boundaries |
| PageUp / PageDown | One page of lines, with a line of overlap |
| Home / End | Start / end of the **visual** line (a wrapped paragraph behaves as it looks) |
| Ctrl+Home / Ctrl+End | Start / end of the document |
| Shift + any of the above | Extends the selection instead of moving |
| Tab / Shift+Tab | Next / previous table cell, selecting its contents; Tab in the last cell appends a row |
| Ctrl+A | Select the whole body |
| Ctrl+C / Ctrl+X | Copy / cut the selection to the OS clipboard |
| Ctrl+V | Paste (see [Paste](#paste)) |
| Ctrl+B | Bold the selection |
| Ctrl+Z / Ctrl+Y | Undo / redo |
| Enter / Backspace / Delete | Split, delete back, delete forward |

A bare arrow with a selection collapses it to the matching edge, as in Word.
Movement follows the caret onto its page, so ↓ off the bottom of page 1 turns
the view to page 2.

### Tables

Cell paragraphs are created with `addParagraphOnly`, so they are not body blocks
and do not appear in the linear caret order — Tab is the way across them.
`Tab` / `Shift+Tab` step to the next / previous cell and select its contents so
typing replaces them, wrapping across rows; `Tab` in the last cell appends a row
matching the last one, as Word does, and one `Ctrl+Z` removes it. Tab just
before or after a table steps into it. Tab does **not** insert a tab character:
tab stops are not rendered yet, so a literal tab would advance by the font's
tiny default rather than to a stop. (On macOS `Cmd+Tab` belongs to the OS and
never reaches the page; `Shift+Tab` is the "previous" binding.)

### Lists and images

Enter inside a list makes a **new list item**: `splitParagraph` carries the list
membership (`listNumId` / `listLevel` / `listFmt`) to the new paragraph, and
because `DocxLayout` renumbers from `listNumId` as it lays the list out, the new
item takes the next number and the ones after it shift down. Enter on an *empty*
list item ends the list instead of adding another empty bullet, so there is a
keyboard way out; one `Ctrl+Z` puts it back.

Clicking a picture places the caret in the paragraph that owns it — before it on
the left half, behind it on the right — so you can land next to an image and
press Enter for a new line. ↑ / ↓ stop on a picture when it is the only thing in
its paragraph.

### Selection across paragraphs

`DocumentRange` is anchor→focus in drag order; `orderedEnds` puts it in
document order using the paragraph's position among top-level body blocks, so a
backwards or multi-paragraph drag behaves like a forward one. Multi-paragraph
selections paint per visual line, copy with `\n` between paragraphs, delete by
trimming the ends and merging what is left, and take Ctrl+B across every
paragraph they touch.

### Measurement

`DocxTextMetrics` is the single authority for "how far along the line is offset
N?". The painter draws a line run by run, switching face per `TextSpan`, so the
caret, click hit-test and selection rectangles all measure the same way —
otherwise a click inside a bold run lands on the wrong character.

Face selection goes through `UITextRenderer.fontFamily`, not `rt.setFont`:
`measureWidth` / `wrap` / `getCachedLine` each call `applyFace()` first, which
resets the rasterizer from `fontFamily`, so poking `rt` directly is undone by
the very next call. Layout picks the face before wrapping too, so a bold
paragraph no longer wraps as if it were regular.

## Paste

`Ctrl+V` in edit mode reads the OS clipboard through the browser's native
`paste` event and inserts at the caret:

| Clipboard flavour | Result |
| --- | --- |
| `text/html` carrying `data-ranger-chart` | a live **chart** block — see below |
| `text/html` containing a `<table>` | a real `DocumentTable` block |
| `text/plain` with tabs | same, parsed as TSV |
| `text/plain` without tabs | text, newlines becoming paragraph splits |

That is the spreadsheet interop path: select a range in the
[DataGrid](../datagrid/README.md#clipboard) (or in Excel), press `Ctrl+C`, and
paste here to get a table. Per-cell background fill, bold, `colspan` and
horizontal alignment survive the trip; the table is sized to the section's text
column. Like Word, the paragraph is split at the caret and the table lands
between the halves, with the caret on the paragraph that follows it. One
`Ctrl+Z` removes the table and rejoins the paragraph.

`ClipboardTable` reads the HTML with a tolerant tag scanner rather than an XML
parser, because real clipboard markup is not well-formed XML — Excel ships a
`<style>` block, `<!--StartFragment-->` comments, unquoted and single-quoted
attributes and void `<col>` tags.

## Charts from the spreadsheet

Copy a chart in the [DataGrid](../datagrid/README.md#the-chart-as-a-document)
and paste it here. What arrives is **not a picture of a chart**: the clipboard's
HTML flavour carries the chart's whole Vela document — the Vega-Lite
specification, the numbers it was made of, and how it is presented — on the
`data-ranger-chart` attribute of the `<img>`. This editor reads that instead of
the bitmap and stores it as a `DocChart` block.

Nothing about the drawing is kept. The picture is recomputed on every paint by
the *same* renderer the spreadsheet uses — `GridChartView` → Vela → EVG — so a
chart in a document and a chart on a sheet are one implementation, not two that
resemble each other.

Which is what makes it editable here:

| Control | Does |
| --- | --- |
| Click the chart | Selects it (a frame is drawn around it) |
| **Type ▸** | Next chart type its own numbers can actually be drawn as |
| **Style ▸** | Next palette |
| **Legend** | Key on or off |
| **Title…** | Rename it |
| **Wider** | Grow the box; the chart re-lays out, it does not stretch |
| **Delete** | Remove it |

Every one of those decodes the chart's JSON, changes one field, regenerates the
specification from the carried cells through the spreadsheet's own generator,
and encodes it again. No control anywhere edits a specification by hand, so the
picture, the settings and the numbers cannot drift apart.

`npm run docx_viewer:chart:test` is the proof, and it is deliberately a
*cross-application* test: it copies a chart out of `GridApp`, pastes that exact
clipboard string into `DocxViewer`, checks the document holds the numbers rather
than pixels, checks the page gains ink where the chart is, and then edits the
chart and checks the specification followed.

`docx_chart_pasted.png` and `docx_chart_edited.png` (written by
`npm run docx_viewer:demo`, and tracked) are the same chart before and after
being changed to a smooth line on a slate palette — in the document, with
nothing but its own JSON in between.

Known limits: a chart cannot yet be exported back into a `.docx` (the writer
skips it with a warning). It should go out as a
`w:drawing` with the rendered PNG plus its JSON on the alt-text, which is how it
would survive a round trip through real Word.
