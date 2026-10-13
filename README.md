# RangerDocx

A Word-compatible document editor written in [Ranger](https://github.com/terotests/Ranger)
(or Ranger-flavoured Rust), with its UI on EVG / EVGUI. Feature parity is
measured against [WordCraft](https://github.com/storytold/wordcraft).

**Status:** step 1, parity metrics. Implementation starts from Ranger's
`gallery/docx_viewer`.

- [docs/PARITY.md](docs/PARITY.md): the metrics, how each is measured, targets
- [docs/PARITY_REPORT.md](docs/PARITY_REPORT.md): current numbers (generated)
- `parity/*.tsv`: the data (catalog, OOXML, typing scenarios, metrics)

## Build

Like [RangerPPTX](https://github.com/terotests/RangerPPTX), this repository is
built inside a Ranger checkout:

```bash
git clone https://github.com/terotests/RangerDocx <ranger>/gallery/docx
cd <ranger>/gallery/docx
tools/parity/run.sh          # regenerate docs/PARITY_REPORT.md (JavaScript)
tools/parity/run.sh rust     # same, compiled to Rust
```

Elsewhere, point `RANGER=<ranger checkout>` at Ranger.

Refresh the WordCraft side of the tables:

```bash
python3 tools/parity/import_wordcraft.py <wordcraft checkout>
```
