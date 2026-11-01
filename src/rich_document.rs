// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::rgr::DxNative;
use crate::rgr::{EVGElement, ImageBuffer, SourceRef};

/// ==============================================================================
/// RichDocument - normalized Word-like document model (not OOXML run indices).
/// ==============================================================================
///
/// Caret / selection (future editor) should use paragraphId + textOffset.
/// Formatting lives in spans that can be re-split independently of Word runs.
///
/// Flow is a sequence of DocBlock (paragraph | table | pageBreak | sectionBreak).
/// EVG never sees OOXML IDs — only resolved geometry + styles from layout.
/// ==============================================================================
/// Run content the model has no text for — a picture, a tab, a field
/// delimiter, a note reference — kept as the XML it was read from and the text
/// offset it stood at. A paragraph written back unchanged copies its own XML
/// and never looks at these; one that was edited is regenerated from its
/// spans, and these go back in at their offsets (clamped to the new text).
#[derive(Clone)]
pub struct InlineAnchor {
    pub offset: i64,
    pub xml: String,
}

impl InlineAnchor {
    pub fn new() -> Self {
        Self {
            offset: 0,
            xml: String::new(),
        }
    }
}

/// The package a document was read from, kept whole so a save can copy every
/// part it does not rewrite. `mainXml` is word/document.xml exactly as the
/// parser read it: every SourceRef offset indexes this string.
#[derive(Clone)]
pub struct DocxSource {
    pub known: bool,
    pub mainPart: String,
    pub mainXml: String,
    /// The numbering part, "" when the package has none.
    pub numberingPart: String,
    pub parts: Vec<String>,
    pub bytes: HashMap<String,Vec<u8>>,
    /// Start and end of every body-level element the parser turned into a
    /// block, in document order. A save skips the ones whose block is gone.
    pub rangeStarts: Vec<i64>,
    pub rangeEnds: Vec<i64>,
}

impl DocxSource {
    pub fn new() -> Self {
        Self {
            known: false,
            mainPart: "word/document.xml".to_string(),
            mainXml: String::new(),
            numberingPart: String::new(),
            parts: Vec::new(),
            bytes: HashMap::new(),
            rangeStarts: Vec::new(),
            rangeEnds: Vec::new(),
        }
    }
    pub fn addRange(&mut self, from: i64, to: i64) {
        self.rangeStarts.push(from);
        self.rangeEnds.push(to);
    }
}

#[derive(Clone)]
pub struct TextSpan {
    pub start: i64,
    pub end: i64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub fontSizePt: f64,
    pub fontFamily: String,
    pub colorHex: String,
    pub hyperlink: String,
    pub strike: bool,
    /// 0 on the line, 1 superscript, 2 subscript (`w:vertAlign`).
    pub vertAlign: i64,
    /// A highlight colour by Word's name ("yellow", "green"...), "" for none.
    pub highlight: String,
    /// An equation in the line: its TeX source, which a writer turns into an
    /// Office Math run (`m:oMath`). The span's own text is the formula as one
    /// line of Unicode, which is what a painter without equations draws and
    /// what the plain text holds. Sliqtly's writer fills and writes it.
    pub math: String,
}

impl TextSpan {
    pub fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            bold: false,
            italic: false,
            underline: false,
            fontSizePt: 11.0,
            fontFamily: "Open Sans".to_string(),
            colorHex: "000000".to_string(),
            hyperlink: String::new(),
            strike: false,
            vertAlign: 0,
            highlight: String::new(),
            math: String::new(),
        }
    }
    pub fn create(start: i64, end: i64) -> Rc<RefCell<TextSpan>> {
        let mut s: Rc<RefCell<TextSpan>> = Rc::new(RefCell::new(TextSpan::new()));
        s.borrow_mut().start = start;
        s.borrow_mut().end = end;
        s
    }
    pub fn copy(&self) -> Rc<RefCell<TextSpan>> {
        let mut _tmp_1009 = self.start.clone();
        let mut s: Rc<RefCell<TextSpan>> = TextSpan::create(_tmp_1009, self.end.clone());
        s.borrow_mut().bold = self.bold;
        s.borrow_mut().italic = self.italic;
        s.borrow_mut().underline = self.underline;
        s.borrow_mut().fontSizePt = self.fontSizePt;
        s.borrow_mut().fontFamily = self.fontFamily.clone();
        s.borrow_mut().colorHex = self.colorHex.clone();
        s.borrow_mut().hyperlink = self.hyperlink.clone();
        s.borrow_mut().strike = self.strike;
        s.borrow_mut().vertAlign = self.vertAlign;
        s.borrow_mut().highlight = self.highlight.clone();
        s.borrow_mut().math = self.math.clone();
        s
    }
    pub fn sameFormat(&self, other: &Rc<RefCell<TextSpan>>) -> bool {
        if self.bold != other.borrow().bold {
            return false;
        }
        if self.italic != other.borrow().italic {
            return false;
        }
        if self.underline != other.borrow().underline {
            return false;
        }
        if self.fontSizePt != other.borrow().fontSizePt {
            return false;
        }
        if self.fontFamily != other.borrow().fontFamily {
            return false;
        }
        if self.colorHex != other.borrow().colorHex {
            return false;
        }
        if self.hyperlink != other.borrow().hyperlink {
            return false;
        }
        if self.strike != other.borrow().strike {
            return false;
        }
        if self.vertAlign != other.borrow().vertAlign {
            return false;
        }
        if self.highlight != other.borrow().highlight {
            return false;
        }
        // two formulas side by side stay two formulas
        if !self.math.is_empty() || !other.borrow().math.is_empty() {
            return false;
        }
        true
    }
}

pub struct Paragraph {
    pub id: i64,
    pub text: String,
    pub spans: Vec<Rc<RefCell<TextSpan>>>,
    pub align: i64,
    pub styleId: String,
    pub listKind: i64,
    pub listLevel: i64,
    pub listNumId: i64,
    pub listFmt: String,
    pub listText: String,
    pub spaceBeforePt: f64,
    pub spaceAfterPt: f64,
    pub lineSpacing: f64,
    pub indentLeftPt: f64,
    pub indentRightPt: f64,
    pub indentFirstLinePt: f64,
    pub images: Vec<Rc<RefCell<DocImage>>>,
    pub pageBreakBefore: bool,
    /// Which way the paragraph runs, from `w:bidi`: -1 nobody said, 0 left to
    /// right, 1 right to left. Three states rather than two because a style can
    /// say one thing and the paragraph the other, and "unset" has to be
    /// distinguishable from "explicitly left to right" for the style to be
    /// overridable at all.
    ///
    /// It decides two things a reader sees immediately: which margin an
    /// unaligned paragraph sits against, and — for a line that opens with a
    /// Latin brand name and is otherwise Arabic — which direction the line
    /// reads in, because the text alone would say the wrong one.
    pub bidi: i64,
    /// A heading (or paragraph) the markdown exporter placed, stamped with the
    /// source span of its INNER text — the same maps a slide title carries, so
    /// a retype in Word can patch document.md.
    pub mdSrcStart: i64,
    pub mdSrcEnd: i64,
    pub mdLastText: String,
    /// Where this paragraph was read from, and what it looked like then (see
    /// DocxWriter.fingerprint). Equal fingerprints mean the original XML is
    /// written back byte for byte.
    source: Rc<RefCell<SourceRef>>,
    pub srcFp: String,
    pub srcPFp: String,
    pub anchors: Vec<Rc<RefCell<InlineAnchor>>>,
}

impl Paragraph {
    pub fn new() -> Self {
        Self {
            id: 0,
            text: String::new(),
            spans: Vec::new(),
            align: 0,
            styleId: "Normal".to_string(),
            listKind: 0,
            listLevel: 0,
            listNumId: 0,
            listFmt: String::new(),
            listText: String::new(),
            spaceBeforePt: 0.0,
            spaceAfterPt: 8.0,
            lineSpacing: 1.15,
            indentLeftPt: 0.0,
            indentRightPt: 0.0,
            indentFirstLinePt: 0.0,
            images: Vec::new(),
            pageBreakBefore: false,
            bidi: -1,
            mdSrcStart: -1,
            mdSrcEnd: -1,
            mdLastText: String::new(),
            source: Rc::new(RefCell::new(SourceRef::new())),
            srcFp: String::new(),
            srcPFp: String::new(),
            anchors: Vec::new(),
        }
    }
    /// ALL of this paragraph's formatting onto another, which is what splitting
    /// one in half needs.
    ///
    /// A method rather than a list of assignments at the call site, for the
    /// reason the PowerPoint reader learned the same way: a hand-written list
    /// goes stale. `splitParagraph` copied thirteen fields and not `bidi`, so
    /// pressing Enter in an Arabic paragraph gave a left-to-right one — which
    /// then took the left margin, because in Word the direction IS the
    /// alignment when no `w:jc` was written.
    ///
    /// What is deliberately NOT copied: the text and its spans, which the split
    /// divides; the images, which belong where they were anchored; and
    /// `pageBreakBefore`, which belongs to the paragraph that starts the page
    /// and not to the one that continues it.
    pub fn copyFormatTo(&self, o: &Rc<RefCell<Paragraph>>) {
        o.borrow_mut().styleId = self.styleId.clone();
        o.borrow_mut().align = self.align;
        o.borrow_mut().bidi = self.bidi;
        o.borrow_mut().spaceBeforePt = self.spaceBeforePt;
        o.borrow_mut().spaceAfterPt = self.spaceAfterPt;
        o.borrow_mut().lineSpacing = self.lineSpacing;
        o.borrow_mut().indentLeftPt = self.indentLeftPt;
        o.borrow_mut().indentRightPt = self.indentRightPt;
        o.borrow_mut().indentFirstLinePt = self.indentFirstLinePt;
        o.borrow_mut().listKind = self.listKind;
        o.borrow_mut().listLevel = self.listLevel;
        o.borrow_mut().listNumId = self.listNumId;
        o.borrow_mut().listFmt = self.listFmt.clone();
        o.borrow_mut().listText = self.listText.clone();
    }
    pub fn appendText(&mut self, chunk: &str, span: &Rc<RefCell<TextSpan>>) {
        let a: i64 = DxNative::str_len(&self.text);
        self.text = format!("{}{}", self.text, chunk);
        let b: i64 = DxNative::str_len(&self.text);
        span.borrow_mut().start = a;
        span.borrow_mut().end = b;
        if !self.spans.is_empty() {
            let mut prev: Rc<RefCell<TextSpan>> = self.spans[((self.spans.len() as i64) - 1) as usize].clone();
            if prev.borrow().end == span.borrow().start && prev.borrow().sameFormat(span) {
                prev.borrow_mut().end = span.borrow().end;
                return;
            }
        }
        self.spans.push(span.borrow().copy());
    }
    pub fn plainText(&self) -> String {
        self.text.clone()
    }
}

/// Inline / floating picture from DrawingML (a:blip → media part).
pub struct DocImage {
    pub relId: String,
    pub partPath: String,
    /// The part's own bytes, kept beside the decoded pixels. The canvas path
    /// wants pixels; a display-list backend wants the file — a browser turns
    /// these into a texture itself, and the package is closed by then.
    pub bytes: Vec<u8>,
    pub widthEmu: i64,
    pub heightEmu: i64,
    pub widthPx: i64,
    pub heightPx: i64,
    /// Word's alt text, from `wp:docPr/@descr`, and the shape's name from
    /// `@name`. This is the ONLY thing a screen reader has to go on for a
    /// picture — the pixels tell it nothing — and it used to be dropped on the
    /// floor by the reader, so a document whose author had written proper alt
    /// text arrived here with none.
    pub altText: String,
    pub title: String,
    pixels: Rc<RefCell<ImageBuffer>>,
    pub ok: bool,
    pub format: String,
}

impl DocImage {
    pub fn new() -> Self {
        Self {
            relId: String::new(),
            partPath: String::new(),
            bytes: vec![0u8; 0],
            widthEmu: 0,
            heightEmu: 0,
            widthPx: 0,
            heightPx: 0,
            altText: String::new(),
            title: String::new(),
            pixels: Rc::new(RefCell::new(ImageBuffer::new())),
            ok: false,
            format: String::new(),
        }
    }
    pub fn emuToPx(emu: i64) -> i64 {
        // 914400 EMU = 1 inch; 96 CSS px/inch. Rounded rather than truncated,
        // and rounded away from zero on both sides — `to_int` floors, so a
        // negative offset would otherwise land a pixel out.
        let v: f64 = ((emu as f64) * 96.0) / 914400.0;
        if v < 0.0 {
            return 0 - (((0.0 - v) + 0.5).floor() as i64);
        }
        (v + 0.5).floor() as i64
    }
    /// What a reader should say about this picture. The author's alt text if
    /// there is any, the shape's name if that is all there is, and an honest
    /// "Image" if the document says nothing — which is itself information: a
    /// picture with no alt text is a documented accessibility failure of the
    /// DOCUMENT, and hiding it behind an invented description would only make
    /// it harder to find.
    pub fn accessibleName(&self) -> String {
        if !self.altText.is_empty() {
            return self.altText.clone();
        }
        if !self.title.is_empty() {
            return self.title.clone();
        }
        "Image".to_string()
    }
    pub fn hasAltText(&self) -> bool {
        if !self.altText.is_empty() {
            return true;
        }
        !self.title.is_empty()
    }
    pub fn syncPx(&mut self) {
        let _tmp_1015 = DocImage::emuToPx(self.widthEmu.clone());
        self.widthPx = _tmp_1015;
        let _tmp_1017 = DocImage::emuToPx(self.heightEmu.clone());
        self.heightPx = _tmp_1017;
        if self.widthPx < 1 {
            self.widthPx = 1;
        }
        if self.heightPx < 1 {
            self.heightPx = 1;
        }
    }
}

/// A chart in the document. Not a picture of one: `json` is the whole Vela
/// chart document — the specification, the numbers it was made of, and how it is
/// presented — so the chart can be re-rendered at any size and EDITED here, in
/// the word processor, long after the spreadsheet it came from was closed.
///
/// The pixels are never stored. A chart is its data, exactly as it is in the
/// grid; the picture is recomputed whenever the page is painted.
#[derive(Clone)]
pub struct DocChart {
    pub id: i64,
    pub json: String,
    /// Box on the page, in CSS pixels. A pasted chart arrives at the size it was
    /// on the sheet and can be made bigger or smaller here.
    pub widthPx: i64,
    pub heightPx: i64,
    /// What the last render made of it, for a host that wants to say so.
    pub note: String,
}

impl DocChart {
    pub fn new() -> Self {
        Self {
            id: 0,
            json: String::new(),
            widthPx: 380,
            heightPx: 260,
            note: String::new(),
        }
    }
}

/// A drawing in the document: a diagram, or anything else that arrives as
/// laid-out GEOMETRY rather than as a picture of it. Not a chart, because a
/// chart is its data and re-renders from a specification; and not a picture,
/// because nothing here is pixels. It is an element tree the engine can lay
/// out and turn into draw commands at any size — the same seam a diagram takes
/// onto a slide (`PptxFromEvg` reads the same list) and into a PDF.
///
/// `root` is laid out at `naturalW` x `naturalH`, the size it was made at,
/// and `widthPx` x `heightPx` is the box it is shown in; the painter scales the
/// commands between the two rather than laying the tree out again, because
/// the tree's children are placed absolutely and would not follow.
///
/// `notation` and `source` are provenance — "mermaid" and the fence's own text
/// — so what drew it can draw it again later. The same reason a slide carries
/// them in `p:cNvPr/a:extLst`.
pub struct DocDrawing {
    pub id: i64,
    pub name: String,
    pub altText: String,
    pub notation: String,
    pub source: String,
    pub widthPx: i64,
    pub heightPx: i64,
    pub naturalW: f64,
    pub naturalH: f64,
    root: Option<Rc<RefCell<EVGElement>>>,
    /// Why nothing could be drawn, when nothing could.
    pub note: String,
}

impl DocDrawing {
    pub fn new() -> Self {
        Self {
            id: 0,
            name: "Drawing".to_string(),
            altText: String::new(),
            notation: String::new(),
            source: String::new(),
            widthPx: 320,
            heightPx: 200,
            naturalW: 0.0,
            naturalH: 0.0,
            root: None,
            note: String::new(),
        }
    }
    pub fn hasRoot(&self) -> bool {
        self.root.is_some()
    }
}

#[derive(Clone)]
pub struct DocSection {
    pub pageWTwips: i64,
    pub pageHTwips: i64,
    pub marginTopTwips: i64,
    pub marginRightTwips: i64,
    pub marginBottomTwips: i64,
    pub marginLeftTwips: i64,
    pub landscape: bool,
    pub headerRelId: String,
    pub footerRelId: String,
    pub headerParas: Vec<Rc<RefCell<Paragraph>>>,
    pub footerParas: Vec<Rc<RefCell<Paragraph>>>,
}

impl DocSection {
    pub fn new() -> Self {
        Self {
            pageWTwips: 12240,
            pageHTwips: 15840,
            marginTopTwips: 1440,
            marginRightTwips: 1440,
            marginBottomTwips: 1440,
            marginLeftTwips: 1440,
            landscape: false,
            headerRelId: String::new(),
            footerRelId: String::new(),
            headerParas: Vec::new(),
            footerParas: Vec::new(),
        }
    }
    pub fn pageWidthPx(&self) -> i64 {
        (((self.pageWTwips as f64) * 96.0) / 1440.0).floor() as i64
    }
    pub fn pageHeightPx(&self) -> i64 {
        (((self.pageHTwips as f64) * 96.0) / 1440.0).floor() as i64
    }
    pub fn marginLeftPx(&self) -> i64 {
        (((self.marginLeftTwips as f64) * 96.0) / 1440.0).floor() as i64
    }
    pub fn marginRightPx(&self) -> i64 {
        (((self.marginRightTwips as f64) * 96.0) / 1440.0).floor() as i64
    }
    pub fn marginTopPx(&self) -> i64 {
        (((self.marginTopTwips as f64) * 96.0) / 1440.0).floor() as i64
    }
    pub fn marginBottomPx(&self) -> i64 {
        (((self.marginBottomTwips as f64) * 96.0) / 1440.0).floor() as i64
    }
    pub fn contentWidthPx(&self) -> i64 {
        let w: i64 = (self.pageWidthPx() - self.marginLeftPx()) - self.marginRightPx();
        if w < 40 {
            return 40;
        }
        w
    }
}

/// --- Tables -----------------------------------------------------------------
#[derive(Clone)]
pub struct DocTableBorder {
    pub style: String,
    pub sz: i64,
    pub colorHex: String,
}

impl DocTableBorder {
    pub fn new() -> Self {
        Self {
            style: "single".to_string(),
            sz: 4,
            colorHex: "000000".to_string(),
        }
    }
}

pub struct DocTableCell {
    source: Rc<RefCell<SourceRef>>,
    pub gridSpan: i64,
    pub fillHex: String,
    pub paragraphs: Vec<Rc<RefCell<Paragraph>>>,
    pub vAlign: i64,
    pub borderTop: Rc<RefCell<DocTableBorder>>,
    pub borderLeft: Rc<RefCell<DocTableBorder>>,
    pub borderBottom: Rc<RefCell<DocTableBorder>>,
    pub borderRight: Rc<RefCell<DocTableBorder>>,
    pub padTopPx: i64,
    pub padRightPx: i64,
    pub padBottomPx: i64,
    pub padLeftPx: i64,
}

impl DocTableCell {
    pub fn new() -> Self {
        Self {
            source: Rc::new(RefCell::new(SourceRef::new())),
            gridSpan: 1,
            fillHex: String::new(),
            paragraphs: Vec::new(),
            vAlign: 0,
            borderTop: Rc::new(RefCell::new(DocTableBorder::new())),
            borderLeft: Rc::new(RefCell::new(DocTableBorder::new())),
            borderBottom: Rc::new(RefCell::new(DocTableBorder::new())),
            borderRight: Rc::new(RefCell::new(DocTableBorder::new())),
            padTopPx: 4,
            padRightPx: 6,
            padBottomPx: 4,
            padLeftPx: 6,
        }
    }
}

pub struct DocTableRow {
    source: Rc<RefCell<SourceRef>>,
    pub cells: Vec<Rc<RefCell<DocTableCell>>>,
    pub heightTwips: i64,
}

impl DocTableRow {
    pub fn new() -> Self {
        Self {
            source: Rc::new(RefCell::new(SourceRef::new())),
            cells: Vec::new(),
            heightTwips: 0,
        }
    }
}

pub struct DocumentTable {
    source: Rc<RefCell<SourceRef>>,
    pub srcFp: String,
    pub id: i64,
    pub colWidthsTwips: Vec<i64>,
    pub rows: Vec<Rc<RefCell<DocTableRow>>>,
    pub borderColor: String,
    pub cellPadPx: i64,
}

impl DocumentTable {
    pub fn new() -> Self {
        Self {
            source: Rc::new(RefCell::new(SourceRef::new())),
            srcFp: String::new(),
            id: 0,
            colWidthsTwips: Vec::new(),
            rows: Vec::new(),
            borderColor: "666666".to_string(),
            cellPadPx: 6,
        }
    }
    pub fn columnCount(&self) -> i64 {
        self.colWidthsTwips.len() as i64
    }
    pub fn rowCount(&self) -> i64 {
        self.rows.len() as i64
    }
}

/// kind: 0=paragraph 1=table 2=pageBreak 3=sectionBreak
/// ------------------------------------------------------------------------------
/// What a body block is.
///
/// These numbers used to be written bare at both ends. That is the shape that
/// produced four separate bugs in the document editor's op kinds — a number the
/// producer wrote and the consumer had never been taught — and here it is worse
/// than it looks, because a laid-out line has a `kind` too and it means
/// something else entirely:
///
///     DocBlock.kind    0 paragraph  1 table  2 page break  3 section break  4 chart
///     LaidLine.kind    0 text  1 image  2 fill  3 border  4 header  5 footer  6 chart
///
/// Both are read in `DocxLayout.layoutDocument`, eight lines apart: `b.kind == 4`
/// is a chart and `cl.kind = 6` is the line drawn for it. Getting those the wrong
/// way round is one character, it compiles, and a chart silently becomes page
/// furniture. Naming them is what stops the two tables from being confusable.
/// ------------------------------------------------------------------------------
#[derive(Clone)]
pub struct DocBlockKind {
}

impl DocBlockKind {
    pub fn new() -> Self {
        Self {
        }
    }
    /// A paragraph of the body text.
    pub fn para() -> i64 {
        0
    }
    /// A table.
    pub fn table() -> i64 {
        1
    }
    /// An explicit page break: the same section continues on a new page.
    pub fn pageBreak() -> i64 {
        2
    }
    /// A section break: `sectionIndex` says which section starts here, so the
    /// margins, the header and the footer all change with it.
    pub fn sectionBreak() -> i64 {
        3
    }
    /// A chart, laid out whole — it is never split across a page boundary.
    pub fn chart() -> i64 {
        4
    }
    /// A drawing, on the same terms as a chart: whole, never split.
    pub fn drawing() -> i64 {
        5
    }
}

#[derive(Clone)]
pub struct DocBlock {
    pub kind: i64,
    pub paragraph: Rc<RefCell<Paragraph>>,
    pub table: Rc<RefCell<DocumentTable>>,
    pub chart: Rc<RefCell<DocChart>>,
    pub drawing: Rc<RefCell<DocDrawing>>,
    pub sectionIndex: i64,
    /// A page or section break the parser derived from a paragraph's own XML
    /// (a `w:br w:type="page"`, a `w:sectPr` in its pPr). That XML is written
    /// with the paragraph, so the block itself writes nothing.
    pub synthetic: bool,
}

impl DocBlock {
    pub fn new() -> Self {
        Self {
            kind: DocBlockKind::para(),
            paragraph: Rc::new(RefCell::new(Paragraph::new())),
            table: Rc::new(RefCell::new(DocumentTable::new())),
            chart: Rc::new(RefCell::new(DocChart::new())),
            drawing: Rc::new(RefCell::new(DocDrawing::new())),
            sectionIndex: 0,
            synthetic: false,
        }
    }
    pub fn chartBlock(c: Rc<RefCell<DocChart>>) -> Rc<RefCell<DocBlock>> {
        let mut b: Rc<RefCell<DocBlock>> = Rc::new(RefCell::new(DocBlock::new()));
        b.borrow_mut().kind = DocBlockKind::chart();
        b.borrow_mut().chart = c.clone();
        b
    }
    pub fn drawingBlock(d: Rc<RefCell<DocDrawing>>) -> Rc<RefCell<DocBlock>> {
        let mut b: Rc<RefCell<DocBlock>> = Rc::new(RefCell::new(DocBlock::new()));
        b.borrow_mut().kind = DocBlockKind::drawing();
        b.borrow_mut().drawing = d.clone();
        b
    }
    pub fn para(p: Rc<RefCell<Paragraph>>) -> Rc<RefCell<DocBlock>> {
        let mut b: Rc<RefCell<DocBlock>> = Rc::new(RefCell::new(DocBlock::new()));
        b.borrow_mut().kind = DocBlockKind::para();
        b.borrow_mut().paragraph = p.clone();
        b
    }
    pub fn tbl(t: Rc<RefCell<DocumentTable>>) -> Rc<RefCell<DocBlock>> {
        let mut b: Rc<RefCell<DocBlock>> = Rc::new(RefCell::new(DocBlock::new()));
        b.borrow_mut().kind = DocBlockKind::table();
        b.borrow_mut().table = t.clone();
        b
    }
    pub fn pageBreak() -> Rc<RefCell<DocBlock>> {
        let mut b: Rc<RefCell<DocBlock>> = Rc::new(RefCell::new(DocBlock::new()));
        b.borrow_mut().kind = DocBlockKind::pageBreak();
        b
    }
    pub fn sectionBreak(sec_idx: i64) -> Rc<RefCell<DocBlock>> {
        let mut b: Rc<RefCell<DocBlock>> = Rc::new(RefCell::new(DocBlock::new()));
        b.borrow_mut().kind = DocBlockKind::sectionBreak();
        b.borrow_mut().sectionIndex = sec_idx;
        b
    }
}

#[derive(Clone)]
pub struct RichDocument {
    pub title: String,
    pub blocks: Vec<Rc<RefCell<DocBlock>>>,
    pub paragraphs: Vec<Rc<RefCell<Paragraph>>>,
    pub tables: Vec<Rc<RefCell<DocumentTable>>>,
    pub sections: Vec<Rc<RefCell<DocSection>>>,
    pub section: Rc<RefCell<DocSection>>,
    pub charts: Vec<Rc<RefCell<DocChart>>>,
    pub drawings: Vec<Rc<RefCell<DocDrawing>>>,
    pub nextParaId: i64,
    pub nextTableId: i64,
    pub nextChartId: i64,
    pub nextDrawingId: i64,
    /// Bumped by every edit, and never reused, so anything that keeps a picture
    /// of this document — a laid-out page, a thumbnail, a cached measurement —
    /// can tell whether it is looking at the same document it built from. The
    /// pptx side arrived at the same thing for the same reason: laying a
    /// five-hundred-paragraph document out again to draw one page of it is the
    /// cost that follows the document rather than the frame.
    pub revision: i64,
    pub source: Rc<RefCell<DocxSource>>,
}

impl RichDocument {
    pub fn new() -> Self {
        let mut me = Self {
            title: String::new(),
            blocks: Vec::new(),
            paragraphs: Vec::new(),
            tables: Vec::new(),
            sections: Vec::new(),
            section: Rc::new(RefCell::new(DocSection::new())),
            charts: Vec::new(),
            drawings: Vec::new(),
            nextParaId: 1,
            nextTableId: 1,
            nextChartId: 1,
            nextDrawingId: 1,
            revision: 0,
            source: Rc::new(RefCell::new(DocxSource::new())),
        };
        me.sections.push(me.section.clone());
        me
    }
    /// Say that something in here changed. Every edit path goes through this,
    /// and anything that keeps a derived picture watches it.
    pub fn touch(&mut self) {
        self.revision += 1;
    }
    pub fn ensureSection(&mut self, idx: i64) -> Rc<RefCell<DocSection>> {
        while (self.sections.len() as i64) <= idx {
            self.sections.push(Rc::new(RefCell::new(DocSection::new())));
        }
        self.sections[idx as usize].clone()
    }
    pub fn addParagraph(&mut self) -> Rc<RefCell<Paragraph>> {
        let mut p: Rc<RefCell<Paragraph>> = Rc::new(RefCell::new(Paragraph::new()));
        p.borrow_mut().id = self.nextParaId;
        self.nextParaId += 1;
        self.paragraphs.push(p.clone());
        self.blocks.push(DocBlock::para(p.clone()));
        p
    }
    pub fn addParagraphOnly(&mut self) -> Rc<RefCell<Paragraph>> {
        // Paragraph not pushed as a top-level block (e.g. table cell / header).
        let mut p: Rc<RefCell<Paragraph>> = Rc::new(RefCell::new(Paragraph::new()));
        p.borrow_mut().id = self.nextParaId;
        self.nextParaId += 1;
        self.paragraphs.push(p.clone());
        p
    }
    /// The same, but keeping an id a paragraph already had.
    ///
    /// This is for REDO. Undo of an Enter merges two paragraphs and the second
    /// one's id goes with it; redoing the Enter used to mint a fresh id, and
    /// every later op in the same action still named the old one — so redoing a
    /// paste of three lines put the first line back and then wrote the rest
    /// nowhere. Handing the id back makes the redone paragraph the SAME
    /// paragraph as far as the rest of the action is concerned.
    pub fn addParagraphWithId(&mut self, id: i64) -> Rc<RefCell<Paragraph>> {
        if id <= 0 {
            return self.addParagraphOnly();
        }
        let mut p: Rc<RefCell<Paragraph>> = Rc::new(RefCell::new(Paragraph::new()));
        p.borrow_mut().id = id;
        if id >= self.nextParaId {
            self.nextParaId = id + 1;
        }
        self.paragraphs.push(p.clone());
        p
    }
    pub fn addTable(&mut self) -> Rc<RefCell<DocumentTable>> {
        let mut t: Rc<RefCell<DocumentTable>> = Rc::new(RefCell::new(DocumentTable::new()));
        t.borrow_mut().id = self.nextTableId;
        self.nextTableId += 1;
        self.tables.push(t.clone());
        self.blocks.push(DocBlock::tbl(t.clone()));
        t
    }
    pub fn addTableOnly(&mut self) -> Rc<RefCell<DocumentTable>> {
        // Table not appended as a block — the caller splices the block itself
        // (paste inserts one in the middle of the document, not at the end).
        let mut t: Rc<RefCell<DocumentTable>> = Rc::new(RefCell::new(DocumentTable::new()));
        t.borrow_mut().id = self.nextTableId;
        self.nextTableId += 1;
        self.tables.push(t.clone());
        t
    }
    /// A chart, appended. `nextChartId` hands out ids that are never reused, so
    /// a selection cannot come to mean a different chart after an edit.
    pub fn addChart(&mut self, json: String) -> Rc<RefCell<DocChart>> {
        let mut c: Rc<RefCell<DocChart>> = Rc::new(RefCell::new(DocChart::new()));
        c.borrow_mut().id = self.nextChartId;
        self.nextChartId += 1;
        c.borrow_mut().json = json.clone();
        self.charts.push(c.clone());
        self.blocks.push(DocBlock::chartBlock(c.clone()));
        c
    }
    pub fn chartCount(&self) -> i64 {
        self.charts.len() as i64
    }
    pub fn chartAt(&self, i: i64) -> Rc<RefCell<DocChart>> {
        if i < 0 {
            return Rc::new(RefCell::new(DocChart::new()));
        }
        if i >= (self.charts.len() as i64) {
            return Rc::new(RefCell::new(DocChart::new()));
        }
        self.charts[i as usize].clone()
    }
    pub fn chartById(&self, id: i64) -> Rc<RefCell<DocChart>> {
        let mut i: i64 = 0;
        while i < (self.charts.len() as i64) {
            let mut c: Rc<RefCell<DocChart>> = self.charts[i as usize].clone();
            if c.borrow().id == id {
                return c;
            }
            i += 1;
        }
        Rc::new(RefCell::new(DocChart::new()))
    }
    /// A drawing, appended. Ids come from their own counter and are never
    /// reused, for the reason a chart's are not.
    pub fn addDrawing(&mut self) -> Rc<RefCell<DocDrawing>> {
        let mut d: Rc<RefCell<DocDrawing>> = Rc::new(RefCell::new(DocDrawing::new()));
        d.borrow_mut().id = self.nextDrawingId;
        self.nextDrawingId += 1;
        self.drawings.push(d.clone());
        self.blocks.push(DocBlock::drawingBlock(d.clone()));
        d
    }
    pub fn drawingCount(&self) -> i64 {
        self.drawings.len() as i64
    }
    pub fn drawingById(&self, id: i64) -> Rc<RefCell<DocDrawing>> {
        let mut i: i64 = 0;
        while i < (self.drawings.len() as i64) {
            let mut d: Rc<RefCell<DocDrawing>> = self.drawings[i as usize].clone();
            if d.borrow().id == id {
                return d;
            }
            i += 1;
        }
        Rc::new(RefCell::new(DocDrawing::new()))
    }
    pub fn addPageBreak(&mut self) {
        self.blocks.push(DocBlock::pageBreak());
    }
    pub fn addSectionBreak(&mut self) -> i64 {
        let idx: i64 = self.sections.len() as i64;
        let mut sec: Rc<RefCell<DocSection>> = Rc::new(RefCell::new(DocSection::new()));
        self.sections.push(sec.clone());
        self.blocks.push(DocBlock::sectionBreak(idx));
        self.section = sec.clone();
        idx
    }
    pub fn paragraphCount(&self) -> i64 {
        self.paragraphs.len() as i64
    }
    pub fn paragraphAt(&self, i: i64) -> Rc<RefCell<Paragraph>> {
        self.paragraphs[i as usize].clone()
    }
    pub fn blockCount(&self) -> i64 {
        self.blocks.len() as i64
    }
    pub fn blockAt(&self, i: i64) -> Rc<RefCell<DocBlock>> {
        self.blocks[i as usize].clone()
    }
    pub fn sectionCount(&self) -> i64 {
        self.sections.len() as i64
    }
    pub fn sectionAt(&self, i: i64) -> Rc<RefCell<DocSection>> {
        self.sections[i as usize].clone()
    }
    pub fn plainText(&self) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < (self.blocks.len() as i64) {
            let mut b: Rc<RefCell<DocBlock>> = self.blocks[i as usize].clone();
            if b.borrow().kind == 0 {
                if !out.is_empty() {
                    out = format!("{}\n", out);
                }
                out = format!("{}{}", out, b.borrow().paragraph.borrow().text);
            }
            if b.borrow().kind == 1 {
                let mut t: Rc<RefCell<DocumentTable>> = b.borrow().table.clone().clone();
                let mut ri: i64 = 0;
                while ri < (t.borrow().rows.len() as i64) {
                    let mut row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
                    let mut ci: i64 = 0;
                    while ci < (row.borrow().cells.len() as i64) {
                        let mut cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                        let mut pi: i64 = 0;
                        while pi < (cell.borrow().paragraphs.len() as i64) {
                            let mut cp: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
                            if !out.is_empty() {
                                out = format!("{}\n", out);
                            }
                            out = format!("{}{}", out, cp.borrow().text);
                            pi += 1;
                        }
                        ci += 1;
                    }
                    ri += 1;
                }
            }
            i += 1;
        }
        out
    }
    /// Compact JSON for semantic oracle (supported subset).
    pub fn inspectJson(&self) -> String {
        let mut out: String = "{\"title\":\"".to_string();
        out = format!("{}{}\",", out, self.jsonEsc(&self.title));
        out = format!("{}\"sectionCount\":{},", out, self.sections.len() as i64);
        out = format!("{}\"paragraphCount\":{},", out, self.paragraphs.len() as i64);
        out = format!("{}\"tableCount\":{},", out, self.tables.len() as i64);
        out = format!("{}\"blockCount\":{},", out, self.blocks.len() as i64);
        out = format!("{}\"sections\":[", out);
        let mut si: i64 = 0;
        while si < (self.sections.len() as i64) {
            if si > 0 {
                out = format!("{},", out);
            }
            let mut sec: Rc<RefCell<DocSection>> = self.sections[si as usize].clone();
            out = format!("{}{}{}", out, "{\"w\":".to_string(), sec.borrow().pageWTwips);
            out = format!("{},\"h\":{}", out, sec.borrow().pageHTwips);
            out = format!("{},\"mt\":{}", out, sec.borrow().marginTopTwips);
            out = format!("{},\"mr\":{}", out, sec.borrow().marginRightTwips);
            out = format!("{},\"mb\":{}", out, sec.borrow().marginBottomTwips);
            out = format!("{},\"ml\":{}", out, sec.borrow().marginLeftTwips);
            out = format!("{},\"landscape\":{}", out, self.jsonBool(sec.borrow().landscape));
            out = format!("{},\"headerParas\":{}", out, sec.borrow().headerParas.len() as i64);
            out = format!("{},\"footerParas\":{}", out, sec.borrow().footerParas.len() as i64);
            out = format!("{}{}", out, "}".to_string());
            si += 1;
        }
        out = format!("{}],\"paragraphs\":[", out);
        let mut pi: i64 = 0;
        while pi < (self.paragraphs.len() as i64) {
            if pi > 0 {
                out = format!("{},", out);
            }
            let mut p: Rc<RefCell<Paragraph>> = self.paragraphs[pi as usize].clone();
            out = format!("{}{}{}", out, "{\"id\":".to_string(), p.borrow().id);
            out = format!("{},\"text\":\"{}\"", out, self.jsonEsc(&p.borrow().text));
            out = format!("{},\"style\":\"{}\"", out, self.jsonEsc(&p.borrow().styleId));
            out = format!("{},\"align\":{}", out, p.borrow().align);
            out = format!("{},\"bold\":{}", out, self.jsonBool(self.paraHasBold(&p)));
            out = format!("{},\"italic\":{}", out, self.jsonBool(self.paraHasItalic(&p)));
            out = format!("{},\"underline\":{}", out, self.jsonBool(self.paraHasUnderline(&p)));
            out = format!("{},\"listKind\":{}", out, p.borrow().listKind);
            out = format!("{},\"listLevel\":{}", out, p.borrow().listLevel);
            out = format!("{},\"listNumId\":{}", out, p.borrow().listNumId);
            out = format!("{},\"indentLeftPt\":{}", out, p.borrow().indentLeftPt.floor() as i64);
            out = format!("{},\"images\":{}", out, p.borrow().images.len() as i64);
            out = format!("{}{}", out, "}".to_string());
            pi += 1;
        }
        out = format!("{}],\"tables\":[", out);
        let mut ti: i64 = 0;
        while ti < (self.tables.len() as i64) {
            if ti > 0 {
                out = format!("{},", out);
            }
            let mut t: Rc<RefCell<DocumentTable>> = self.tables[ti as usize].clone();
            out = format!("{}{}{}", out, "{\"cols\":".to_string(), t.borrow().colWidthsTwips.len() as i64);
            out = format!("{},\"rows\":{}", out, t.borrow().rows.len() as i64);
            out = format!("{},\"cells\":[", out);
            let mut ri: i64 = 0;
            let mut first_cell: bool = true;
            while ri < (t.borrow().rows.len() as i64) {
                let mut row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
                let mut ci: i64 = 0;
                while ci < (row.borrow().cells.len() as i64) {
                    if first_cell {
                        first_cell = false;
                    } else {
                        out = format!("{},", out);
                    }
                    let mut cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                    let mut ctext: String = String::new();
                    if !cell.borrow().paragraphs.is_empty() {
                        let mut cp0: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[0].clone();
                        ctext = cp0.borrow().text.clone();
                    }
                    out = format!("{}{}{}", out, "{\"span\":".to_string(), cell.borrow().gridSpan);
                    out = format!("{},\"fill\":\"{}\"", out, self.jsonEsc(&cell.borrow().fillHex));
                    out = format!("{},\"text\":\"{}{}", out, self.jsonEsc(&ctext), "\"}".to_string());
                    ci += 1;
                }
                ri += 1;
            }
            out = format!("{}{}", out, "]}".to_string());
            ti += 1;
        }
        out = format!("{}{}", out, "]}".to_string());
        out
    }
    pub fn paraHasBold(&self, p: &Rc<RefCell<Paragraph>>) -> bool {
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            if p.borrow().spans[i as usize].clone().borrow().bold {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn paraHasItalic(&self, p: &Rc<RefCell<Paragraph>>) -> bool {
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            if p.borrow().spans[i as usize].clone().borrow().italic {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn paraHasUnderline(&self, p: &Rc<RefCell<Paragraph>>) -> bool {
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            if p.borrow().spans[i as usize].clone().borrow().underline {
                return true;
            }
            i += 1;
        }
        false
    }
    pub fn jsonBool(&self, v: bool) -> String {
        if v {
            return "true".to_string();
        }
        "false".to_string()
    }
    pub fn jsonEsc(&self, s: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&s);
        while i < n {
            let ch: i64 = DxNative::char_at(&s, i);
            if ch == 92 {
                out = format!("{}\\\\", out);
            } else {
                if ch == 34 {
                    out = format!("{}\\\"", out);
                } else {
                    if ch == 10 {
                        out = format!("{}\\n", out);
                    } else {
                        if ch == 13 {
                            out = format!("{}\\r", out);
                        } else {
                            if ch == 9 {
                                out = format!("{}\\t", out);
                            } else {
                                out = format!("{}{}", out, DxNative::substr(&s, i, i + 1));
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        out
    }
}
