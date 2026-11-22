#!/usr/bin/env python3
"""26-review-notes.docx: tracked changes, comments, footnotes and endnotes,
bookmarks and fields, written by hand so each element is where Word puts it."""
from __future__ import annotations

import zipfile
from pathlib import Path

FIX = Path(__file__).resolve().parent
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

CT = '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>
<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>
<Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/>
</Types>'''

RELS = '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>'''

DOC_RELS = '''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="comments.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/>
</Relationships>'''

STYLES = f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles {W}>
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault>
<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="character" w:styleId="FootnoteReference"><w:name w:val="footnote reference"/><w:rPr><w:vertAlign w:val="superscript"/></w:rPr></w:style>
</w:styles>'''

A = 'w:author="Ann Editor" w:date="2026-11-01T10:00:00Z"'
B = 'w:author="Bo Reviewer" w:date="2026-11-02T11:30:00Z"'


def r(t: str, rpr: str = "") -> str:
    return f'<w:r>{rpr}<w:t xml:space="preserve">{t}</w:t></w:r>'


def dr(t: str) -> str:
    return f'<w:r><w:delText xml:space="preserve">{t}</w:delText></w:r>'


BODY = [
    # 0: tracked insertion and deletion
    f'<w:p>{r("The quick ")}<w:del w:id="1" {A}>{dr("brown ")}</w:del><w:ins w:id="2" {A}>{r("red ")}</w:ins>{r("fox jumps.")}</w:p>',
    # 1: a second author, and a deletion at the end
    f'<w:p>{r("Keep this")}<w:ins w:id="3" {B}>{r(" and add this")}</w:ins><w:del w:id="4" {B}>{dr(" but not this")}</w:del>{r(".")}</w:p>',
    # 2: a comment over a range
    f'<w:p>{r("A sentence with ")}<w:commentRangeStart w:id="0"/>{r("a remark")}<w:commentRangeEnd w:id="0"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="0"/></w:r>{r(" on it.")}</w:p>',
    # 3: a footnote and an endnote
    f'<w:p>{r("Noted here")}<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="1"/></w:r>{r(" and at the end")}<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteReference w:id="1"/></w:r>{r(".")}</w:p>',
    # 4: a bookmark
    f'<w:p>{r("Go ")}<w:bookmarkStart w:id="0" w:name="target"/>{r("here")}<w:bookmarkEnd w:id="0"/>{r(" later.")}</w:p>',
    # 5: a simple field and a complex one
    f'<w:p>{r("Page ")}<w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>{r(" of ")}<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> NUMPAGES </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{r("1")}<w:r><w:fldChar w:fldCharType="end"/></w:r>{r(".")}</w:p>',
    # 6: a link to the bookmark
    f'<w:p><w:hyperlink w:anchor="target">{r("Jump", "<w:rPr><w:color w:val=\"0563C1\"/><w:u w:val=\"single\"/></w:rPr>")}</w:hyperlink></w:p>',
    # 7: plain
    f'<w:p>{r("The end.")}</w:p>',
]

DOC = f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W}><w:body>{"".join(BODY)}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>'''

COMMENTS = f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:comments {W}><w:comment w:id="0" w:author="Bo Reviewer" w:date="2026-11-02T12:00:00Z" w:initials="BR"><w:p>{r("Is this right?")}</w:p></w:comment></w:comments>'''

SEP = '<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>'
FOOT = f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:footnotes {W}>{SEP}<w:footnote w:id="1"><w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteRef/></w:r>{r(" A footnote.")}</w:p></w:footnote></w:footnotes>'''
END = f'''<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:endnotes {W}>{SEP.replace("footnote", "endnote")}<w:endnote w:id="1"><w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteRef/></w:r>{r(" An endnote.")}</w:p></w:endnote></w:endnotes>'''


def main() -> None:
    out = FIX / "26-review-notes.docx"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CT)
        z.writestr("_rels/.rels", RELS)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/document.xml", DOC)
        z.writestr("word/styles.xml", STYLES)
        z.writestr("word/comments.xml", COMMENTS)
        z.writestr("word/footnotes.xml", FOOT)
        z.writestr("word/endnotes.xml", END)
    print(out)


if __name__ == "__main__":
    main()
