#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""fixtures/25-theme-fonts.docx: fonts named only through the theme.

Body text is Normal with <w:rFonts w:asciiTheme="minorHAnsi"/> and the theme's
minor Latin face is Cambria; the title's style points at majorHAnsi (Calibri).
A4 with 1.25" side margins, the page of the License Agreement documents on
research-clm, whose first paragraph Word (and M-Files' preview) breaks after
'("Effective' -- and Open Sans, about 15 % wider, broke before it.
tests/DocxTest.rgr "== theme fonts ==" checks those breaks.
"""
import os
import zipfile

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
R = 'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'

CT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/word/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
</Types>"""

RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/>
</Relationships>"""

THEME = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements>
<a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>
<a:minorFont><a:latin typeface="Cambria"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>
</a:themeElements></a:theme>"""

STYLES = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles {W}>
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia" w:hAnsiTheme="minorHAnsi" w:cstheme="minorBidi"/><w:sz w:val="22"/></w:rPr></w:rPrDefault>
<w:pPrDefault><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:pPr><w:jc w:val="center"/></w:pPr><w:rPr><w:rFonts w:asciiTheme="majorHAnsi" w:hAnsiTheme="majorHAnsi"/><w:b/><w:sz w:val="28"/></w:rPr></w:style>
</w:styles>"""

BODY = ('This License Agreement ("Agreement") is entered into as of September 1, 2026 '
        '("Effective Date"), by and between Company Inc., a corporation organized and existing '
        'under the laws of Finland, with its principal place of business at Keskuskatu 1, '
        'Tampere, Finland ("Licensor"), and CulinaryCraft, a company organized and existing '
        'under the laws of CN China, with its registered office at Nanjing Road 142, Shanghai, '
        'CN China ("Licensee").')


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")


DOC = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document {W} {R}><w:body>
<w:p><w:pPr><w:pStyle w:val="Title"/></w:pPr><w:r><w:t>LICENSE AGREEMENT</w:t></w:r></w:p>
<w:p><w:r><w:t xml:space="preserve">{esc(BODY)}</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1800" w:bottom="1440" w:left="1800" w:header="708" w:footer="708" w:gutter="0"/></w:sectPr>
</w:body></w:document>"""


def main():
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "25-theme-fonts.docx")
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as z:
        for name, data in (("[Content_Types].xml", CT), ("_rels/.rels", RELS),
                           ("word/_rels/document.xml.rels", DOC_RELS), ("word/document.xml", DOC),
                           ("word/styles.xml", STYLES), ("word/theme/theme1.xml", THEME)):
            zi = zipfile.ZipInfo(name, date_time=(2026, 11, 1, 0, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, data)
    print("wrote", out)


if __name__ == "__main__":
    main()
