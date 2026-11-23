# Writes 27-cell-fit.docx for tests/LayoutFitTest.rgr: a table whose second
# cell holds five paragraphs at single line spacing (the row must grow to hold
# them), w:br breaks in the next row, then an empty centred and an empty
# right-aligned paragraph.
import os
import zipfile
W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'
def p(text, bold=False, font=None, jc=None):
    ppr = f'<w:pPr><w:jc w:val="{jc}"/></w:pPr>' if jc else ''
    rpr = ''
    if bold or font:
        rpr = '<w:rPr>' + (f'<w:rFonts w:ascii="{font}" w:hAnsi="{font}"/>' if font else '') + ('<w:b/>' if bold else '') + '</w:rPr>'
    run = f'<w:r>{rpr}<w:t xml:space="preserve">{text}</w:t></w:r>' if text else ''
    return f'<w:p>{ppr}{run}</w:p>'
def cell(content, w, va=None):
    vpr = f'<w:vAlign w:val="{va}"/>' if va else ''
    return f'<w:tc><w:tcPr><w:tcW w:w="{w}" w:type="dxa"/>{vpr}</w:tcPr>{content}</w:tc>'
emp2 = ''.join(p(t, font='Cambria') for t in ['Nordic Solutions Oy (Business ID: 1234567-8)', 'Mannerheimintie 10', '00100 Helsinki', 'Finland', '"Employer"'])
emp3 = ('<w:p><w:r><w:t xml:space="preserve">[Employee] (Personal ID: )</w:t></w:r><w:r><w:br/></w:r>'
        '<w:r><w:t>[EmployeeAddress]</w:t></w:r><w:r><w:br/><w:t>"Employee"</w:t></w:r></w:p>')
tbl = ('<w:tbl><w:tblPr><w:tblW w:w="9000" w:type="dxa"/><w:tblBorders>'
       + ''.join(f'<w:{s} w:val="single" w:sz="4" w:space="0" w:color="000000"/>' for s in ['top', 'left', 'bottom', 'right', 'insideH', 'insideV'])
       + '</w:tblBorders><w:tblCellMar><w:top w:w="57" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="57" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr>'
       '<w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="6000"/></w:tblGrid>'
       f'<w:tr>{cell(p("Employer", True), 3000, "center")}{cell(emp2, 6000)}</w:tr>'
       f'<w:tr>{cell(p("Employee", True), 3000, "center")}{cell(emp3, 6000)}</w:tr></w:tbl>')
body = p('1. Parties', True) + tbl + p('Centred and right-aligned empty lines follow.') + p('', jc='center') + p('', jc='right') + p('The end.')
doc = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body></w:document>'
styles = f'<?xml version="1.0" encoding="UTF-8"?><w:styles {W}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults></w:styles>'
ct = '<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'
rels = '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>'
drels = '<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>'
with zipfile.ZipFile(os.path.join(os.path.dirname(os.path.abspath(__file__)), '27-cell-fit.docx'), 'w', zipfile.ZIP_DEFLATED) as z:
    z.writestr('[Content_Types].xml', ct)
    z.writestr('_rels/.rels', rels)
    z.writestr('word/_rels/document.xml.rels', drels)
    z.writestr('word/styles.xml', styles)
    z.writestr('word/document.xml', doc)
