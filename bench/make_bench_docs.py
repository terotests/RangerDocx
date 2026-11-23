#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Writes the two documents parity_bench.rgr measures (P1-P3 in parity/metrics.tsv):

  bin/bench/long-text.docx   ~188 pages of headed paragraphs (Calibri 11, Word defaults)
  bin/bench/long-table.docx  one 3-column table, ~1,096 pages of rows

  python3 gallery/docx_viewer/bench/make_bench_docs.py [textParas] [tableRows]

The counts are tuned so RangerDocx lays them out at 188 and 1,096 pages; the
bench prints the page count it got. These are synthetic: WordCraft's 188- and
1,096-page documents are not public, so the numbers compare like for like in
size only.
"""
import sys
import zipfile
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "bin" / "bench"

CT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"""
RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""
DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"""
STYLES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="240" w:after="120"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style></w:styles>"""
NS = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'
SECT = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
WORDS = ("layout measures paragraphs wraps text to the content width and paginates across "
         "section page sizes with margins from the section properties while the editor keeps "
         "every offset stable for the caret").split()


def sentence(i: int, n: int) -> str:
    return " ".join(WORDS[(i * 7 + k) % len(WORDS)] for k in range(n))


def write(name: str, body: str) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    doc = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS}><w:body>{body}{SECT}</w:body></w:document>'
    with zipfile.ZipFile(OUT / name, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT)
        z.writestr("_rels/.rels", RELS)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/styles.xml", STYLES)
        z.writestr("word/document.xml", doc)
    print(f"wrote {OUT / name}")


def long_text(paras: int) -> None:
    out = []
    for i in range(paras):
        if i % 12 == 0:
            out.append(f'<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Section {i // 12 + 1}</w:t></w:r></w:p>')
        else:
            out.append(f'<w:p><w:r><w:t xml:space="preserve">{sentence(i, 40)} </w:t></w:r>'
                       f'<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">{sentence(i + 3, 6)} </w:t></w:r>'
                       f'<w:r><w:t>{sentence(i + 5, 30)}.</w:t></w:r></w:p>')
    write("long-text.docx", "".join(out))


def long_table(rows: int) -> None:
    grid = '<w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="4860"/><w:gridCol w:w="3000"/></w:tblGrid>'
    pr = '<w:tblPr><w:tblW w:w="9360" w:type="dxa"/><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders></w:tblPr>'

    def cell(w: int, t: str) -> str:
        return f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>'
    out = [f"<w:tbl>{pr}{grid}"]
    for i in range(rows):
        out.append("<w:tr>" + cell(1500, str(i + 1)) + cell(4860, sentence(i, 5)) + cell(3000, f"{(i * 37) % 1000}.{i % 100:02d}") + "</w:tr>")
    out.append("</w:tbl><w:p/>")
    write("long-table.docx", "".join(out))


if __name__ == "__main__":
    long_text(int(sys.argv[1]) if len(sys.argv) > 1 else 1226)
    long_table(int(sys.argv[2]) if len(sys.argv) > 2 else 31770)
