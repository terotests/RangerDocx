// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::docx_text_metrics::DocxTextMetrics;
use crate::rgr::DocBlock;
use crate::rgr::DocBlockKind;
use crate::rgr::DocChart;
use crate::rgr::DocDrawing;
use crate::rgr::DocImage;
use crate::rgr::DocSection;
use crate::rgr::DocTableCell;
use crate::rgr::DocTableRow;
use crate::rgr::DocumentTable;
use crate::rgr::DxNative;
use crate::rgr::Paragraph;
use crate::rgr::RichDocument;
use crate::rgr::TextSpan;
use crate::word_numbering::ListCounters;
use crate::word_numbering::NumLevel;
use crate::word_numbering::WordNumbering;
use crate::rgr::{UITextRenderer};

/// ==============================================================================
/// DocxLayout - section-aware pagination + tables for RichDocument.
/// ==============================================================================
/// Returns LaidPage[] with resolved geometry. EVG paints Placed* only — Word
/// pagination semantics stay here (not in SoftCanvas).
/// ==============================================================================
/// kind: 0=text 1=image 2=tableCellBg 3=tableBorder 4=headerText 5=footerText
///        6=chart (chartId names the DocChart; the picture is rendered at paint)
///        7=drawing (drawingId names the DocDrawing; laid out and drawn at paint)
/// ------------------------------------------------------------------------------
/// What a laid-out line draws.
///
/// A `DocBlock` has a `kind` too, and it is a DIFFERENT table over the same small
/// integers — 4 is a chart there and a header here. Both are read inside
/// `layoutDocument`, a few lines apart. See the note on `DocBlockKind`.
/// ------------------------------------------------------------------------------
#[derive(Clone)]
pub struct LaidLineKind {
}

impl LaidLineKind {
    pub fn new() -> Self {
        Self {
        }
    }
    /// A run of text. The great majority of lines.
    pub fn text() -> i64 {
        0
    }
    /// A picture, drawn from `imageIndex` within the line's paragraph.
    pub fn image() -> i64 {
        1
    }
    /// A filled rectangle — a table cell's shading.
    pub fn fill() -> i64 {
        2
    }
    /// A one-pixel rectangle standing in for a table border.
    pub fn border() -> i64 {
        3
    }
    /// Header text. Drawn muted, and not part of the body the caret walks.
    pub fn header() -> i64 {
        4
    }
    /// Footer text, on the same terms.
    pub fn footer() -> i64 {
        5
    }
    /// A chart, painted from the document's own JSON at paint time.
    pub fn chart() -> i64 {
        6
    }
    /// A drawing, laid out from its element tree at paint time.
    pub fn drawing() -> i64 {
        7
    }
    /// Is this a line of body text — something a click can land in and the caret
    /// can walk? A header and a footer are text and are drawn as text, and they
    /// are deliberately NOT this: the body is what the document editor edits.
    pub fn isBodyText(kind: i64) -> bool {
        kind == 0
    }
}

#[derive(Clone)]
pub struct LaidLine {
    pub paraIndex: i64,
    pub text: String,
    pub x: i64,
    pub y: i64,
    pub baseline: i64,
    pub height: i64,
    pub width: i64,
    /// Hit-test content box (full wrap column / cell inner), not just glyph bounds.
    /// x/width are painted text origin+advance; boxX/boxW decide which line owns a click
    /// when several lines share a Y band (table row cells).
    pub boxX: i64,
    pub boxW: i64,
    pub align: i64,
    /// The paragraph's direction, carried down so the painter does not have to
    /// go back to the model for it. -1 nobody said, 0 left to right, 1 right
    /// to left.
    pub bidi: i64,
    pub listPrefix: String,
    pub kind: i64,
    pub imageIndex: i64,
    /// Which DocChart this line draws, by id rather than by index — an edit may
    /// add or remove charts between layout and paint.
    pub chartId: i64,
    /// …and which DocDrawing, on the same terms.
    pub drawingId: i64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub fontSizePx: f64,
    /// the paragraph's family as laid out (`DocxTextMetrics.paragraphFamily`);
    /// the painter sets it before drawing the line
    pub fontFamily: String,
    pub colorHex: String,
    pub fillHex: String,
    pub borderColor: String,
    paragraphRef: Option<Rc<RefCell<Paragraph>>>,
    /// Offset range into paragraph.text covered by this visual line (excludes list prefix).
    pub docStart: i64,
    pub docEnd: i64,
}

impl LaidLine {
    pub fn new() -> Self {
        Self {
            paraIndex: 0,
            text: String::new(),
            x: 0,
            y: 0,
            baseline: 0,
            height: 0,
            width: 0,
            boxX: 0,
            boxW: 0,
            align: 0,
            bidi: -1,
            listPrefix: String::new(),
            kind: LaidLineKind::text(),
            imageIndex: 0,
            chartId: 0,
            drawingId: 0,
            bold: false,
            italic: false,
            underline: false,
            fontSizePx: 14.0,
            fontFamily: "Open Sans".to_string(),
            colorHex: "000000".to_string(),
            fillHex: String::new(),
            borderColor: "666666".to_string(),
            paragraphRef: None,
            docStart: 0,
            docEnd: 0,
        }
    }
}

#[derive(Clone)]
pub struct LaidPage {
    pub index: i64,
    pub lines: Vec<Rc<RefCell<LaidLine>>>,
    pub width: i64,
    pub height: i64,
    pub sectionIndex: i64,
    pub contentX: i64,
    pub contentY: i64,
    pub contentW: i64,
    pub contentH: i64,
}

impl LaidPage {
    pub fn new() -> Self {
        Self {
            index: 0,
            lines: Vec::new(),
            width: 0,
            height: 0,
            sectionIndex: 0,
            contentX: 0,
            contentY: 0,
            contentW: 0,
            contentH: 0,
        }
    }
}

pub struct DocxLayout {
    pub pages: Vec<Rc<RefCell<LaidPage>>>,
    tr: Option<Rc<RefCell<UITextRenderer>>>,
    pub contentW: i64,
    pub numbering: Rc<RefCell<WordNumbering>>,
    pub listCounters: Rc<RefCell<ListCounters>>,
    docRef: Option<Rc<RefCell<RichDocument>>>,
    /// Which revision of the document the pages currently in `pages` were laid
    /// out from. -1 means "nothing valid here": every path that puts a DIFFERENT
    /// document in front of this layout says so by calling `invalidate`, so the
    /// revision never has to be told apart from another document's.
    pub laidRevision: i64,
    pub curPage: Rc<RefCell<LaidPage>>,
    curSec: Rc<RefCell<DocSection>>,
    pub curSecIdx: i64,
    pub curMaxY: i64,
}

impl DocxLayout {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            tr: None,
            contentW: 500,
            numbering: Rc::new(RefCell::new(WordNumbering::new())),
            listCounters: Rc::new(RefCell::new(ListCounters::new())),
            docRef: None,
            laidRevision: 0 - 1,
            curPage: Rc::new(RefCell::new(LaidPage::new())),
            curSec: Rc::new(RefCell::new(DocSection::new())),
            curSecIdx: 0,
            curMaxY: 0,
        }
    }
    pub fn ptToPx(pt: f64) -> f64 {
        (pt * 96.0) / 72.0
    }
    /// 1440 twips to the inch, 96 px to the inch — so fifteen twips to the
    /// pixel, and every value that is not a multiple of fifteen lands between
    /// two. `to_int` FLOORS, which drops the fraction rather than rounding it,
    /// and floors a NEGATIVE outwards: an outdent of -100 twips came out a whole
    /// pixel further left than it should. Round the magnitude and put the sign
    /// back — the same fix this repository has now made in the JPEG quantizer,
    /// the path formatter and the pptx EMU writer.
    pub fn twipsToPx(tw: i64) -> i64 {
        let v: f64 = ((tw as f64) * 96.0) / 1440.0;
        if v < 0.0 {
            return 0 - (((0.0 - v) + 0.5).floor() as i64);
        }
        (v + 0.5).floor() as i64
    }
    /// Where a wrapped chunk really starts in the paragraph.
    ///
    /// `tr.wrap` breaks at a space and DROPS it, so line two does not begin
    /// where line one ended: it begins one character later. Advancing the cursor
    /// by the chunk's own length therefore names a range one short, and the
    /// error ACCUMULATES — by the fourth wrapped line of a paragraph the offsets
    /// were two characters behind the text they claim to describe.
    ///
    /// Everything that maps between pixels and offsets is built on this. The
    /// caret drew two characters to the right of where it belonged, a click
    /// landed on a different character than the one under the pointer, and text
    /// typed at the caret went in where the offsets said rather than where the
    /// caret was drawn — so a word appeared a couple of letters to the left of
    /// the cursor, in the middle of the previous one.
    ///
    /// Only WHITESPACE may be stepped over. If the next character is anything
    /// else the chunk is not a slice of this text at all, and moving the cursor
    /// further would invent an offset rather than find one.
    pub fn chunkStart(text: &str, cursor: i64, chunk: &str) -> i64 {
        if chunk.is_empty() {
            return cursor;
        }
        let n: i64 = DxNative::str_len(&text);
        let first: i64 = DxNative::char_at(&chunk, 0);
        let mut at: i64 = cursor;
        while at < n {
            let c: i64 = DxNative::char_at(&text, at);
            if c == first {
                return at;
            }
            if ((c != 32 && c != 9) && c != 10) && c != 13 {
                return at;
            }
            at += 1;
        }
        at
    }
    /// Lay the document out only when it has actually changed. Drawing one page
    /// used to lay out ALL of them, so the cost of a frame followed the length
    /// of the document — 3.5 ms at a hundred and twenty-five paragraphs, 25 ms
    /// at a thousand — and typing paid it twice, once to edit and once to draw.
    /// The document says when it has moved on (`revision`), which is a number,
    /// and a number is a dependency that cannot be got wrong.
    fn layoutIfNeeded(&mut self, doc: Rc<RefCell<RichDocument>>, text_renderer: Rc<RefCell<UITextRenderer>>, nums: Rc<RefCell<WordNumbering>>) {
        if self.laidRevision >= 0 {
            if self.laidRevision == doc.borrow().revision {
                self.tr = Some(text_renderer.clone());
                self.numbering = nums.clone();
                self.docRef = Some(doc.clone());
                return;
            }
        }
        self.layout(doc.clone(), text_renderer.clone(), nums.clone());
    }
    fn layout(&mut self, doc: Rc<RefCell<RichDocument>>, text_renderer: Rc<RefCell<UITextRenderer>>, nums: Rc<RefCell<WordNumbering>>) {
        self.tr = Some(text_renderer.clone());
        self.numbering = nums.clone();
        self.docRef = Some(doc.clone());
        self.laidRevision = doc.borrow().revision;
        self.pages.clear();
        self.listCounters = Rc::new(RefCell::new(ListCounters::new()));
        self.curSecIdx = 0;
        self.curSec = RichDocument::sectionAt(&doc, 0);
        let mut _tmp_1848 = self.curSec.clone();
        let _tmp_1850 = self.newPage(&_tmp_1848, self.curSecIdx.clone());
        self.curPage = _tmp_1850;
        let mut ml: i64 = DocSection::marginLeftPx(&self.curSec);
        let mut mt: i64 = DocSection::marginTopPx(&self.curSec);
        let mut mb: i64 = DocSection::marginBottomPx(&self.curSec);
        self.contentW = DocSection::contentWidthPx(&self.curSec);
        let mut y: i64 = mt;
        let mut _tmp_1856 = self.curSec.borrow().headerParas.clone();
        let mut header_h: i64 = self.measureHeaderFooter(&_tmp_1856, self.contentW.clone());
        let mut _tmp_1858 = self.curSec.borrow().footerParas.clone();
        let mut footer_h: i64 = self.measureHeaderFooter(&_tmp_1858, self.contentW.clone());
        if header_h > 0 {
            y = (mt + header_h) + 8;
        }
        self.curMaxY = (self.curPage.borrow().height - mb) - footer_h;
        if footer_h > 0 {
            self.curMaxY -= 8;
        }
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&doc, bi);
            if b.borrow().kind == DocBlockKind::pageBreak() {
                let mut _tmp_1861 = self.curPage.clone();
                self.paintChrome(&_tmp_1861, &self.curSec.clone());
                let mut _tmp_1863 = self.curSec.clone();
                let _tmp_1865 = self.newPage(&_tmp_1863, self.curSecIdx.clone());
                self.curPage = _tmp_1865;
                ml = DocSection::marginLeftPx(&self.curSec);
                mt = DocSection::marginTopPx(&self.curSec);
                mb = DocSection::marginBottomPx(&self.curSec);
                self.contentW = DocSection::contentWidthPx(&self.curSec);
                header_h = self.measureHeaderFooter(&self.curSec.borrow().headerParas,
                    self.contentW
                );
                footer_h = self.measureHeaderFooter(&self.curSec.borrow().footerParas,
                    self.contentW
                );
                y = mt;
                if header_h > 0 {
                    y = (mt + header_h) + 8;
                }
                self.curMaxY = (self.curPage.borrow().height - mb) - footer_h;
                if footer_h > 0 {
                    self.curMaxY -= 8;
                }
            }
            if b.borrow().kind == DocBlockKind::sectionBreak() {
                let mut _tmp_1869 = self.curPage.clone();
                self.paintChrome(&_tmp_1869, &self.curSec.clone());
                self.curSecIdx = b.borrow().sectionIndex;
                let _tmp_1873 = RichDocument::sectionAt(&doc, self.curSecIdx.clone());
                self.curSec = _tmp_1873;
                let mut _tmp_1874 = self.curSec.clone();
                let _tmp_1876 = self.newPage(&_tmp_1874, self.curSecIdx.clone());
                self.curPage = _tmp_1876;
                ml = DocSection::marginLeftPx(&self.curSec);
                mt = DocSection::marginTopPx(&self.curSec);
                mb = DocSection::marginBottomPx(&self.curSec);
                self.contentW = DocSection::contentWidthPx(&self.curSec);
                header_h = self.measureHeaderFooter(&self.curSec.borrow().headerParas,
                    self.contentW
                );
                footer_h = self.measureHeaderFooter(&self.curSec.borrow().footerParas,
                    self.contentW
                );
                y = mt;
                if header_h > 0 {
                    y = (mt + header_h) + 8;
                }
                self.curMaxY = (self.curPage.borrow().height - mb) - footer_h;
                if footer_h > 0 {
                    self.curMaxY -= 8;
                }
            }
            if b.borrow().kind == DocBlockKind::para() {
                let mut p: Rc<RefCell<Paragraph>> = b.borrow().paragraph.clone().clone();
                let need: i64 = self.estimateParaHeight(&p, self.contentW.clone());
                // A paragraph that starts a page and was not read with a break
                // block in front of it (one the editor broke) starts it here.
                let mut forced: bool = false;
                if p.borrow().pageBreakBefore && !self.curPage.borrow().lines.is_empty() {
                    forced = true;
                    if bi > 0 {
                        let mut prev_b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&doc, bi - 1);
                        if prev_b.borrow().kind == DocBlockKind::pageBreak() {
                            forced = false;
                        }
                    }
                }
                if y + need > self.curMaxY || forced {
                    let mut _tmp_1881 = self.curPage.clone();
                    self.paintChrome(&_tmp_1881, &self.curSec.clone());
                    let mut _tmp_1883 = self.curSec.clone();
                    let _tmp_1885 = self.newPage(&_tmp_1883, self.curSecIdx.clone());
                    self.curPage = _tmp_1885;
                    ml = DocSection::marginLeftPx(&self.curSec);
                    mt = DocSection::marginTopPx(&self.curSec);
                    mb = DocSection::marginBottomPx(&self.curSec);
                    header_h = self.measureHeaderFooter(&self.curSec.borrow().headerParas,
                        self.contentW
                    );
                    footer_h = self.measureHeaderFooter(&self.curSec.borrow().footerParas,
                        self.contentW
                    );
                    y = mt;
                    if header_h > 0 {
                        y = (mt + header_h) + 8;
                    }
                    self.curMaxY = (self.curPage.borrow().height - mb) - footer_h;
                    if footer_h > 0 {
                        self.curMaxY -= 8;
                    }
                }
                y = self.layoutParagraph(p.clone(), ml, y, self.contentW);
            }
            if b.borrow().kind == DocBlockKind::chart() {
                let mut ch: Rc<RefCell<DocChart>> = b.borrow().chart.clone().clone();
                let mut cw: i64 = ch.borrow().widthPx;
                if cw > self.contentW {
                    cw = self.contentW;
                }
                let chh: i64 = ch.borrow().heightPx;
                // A chart is not split across a page boundary: half a chart is
                // not a chart. It moves to the next page whole, as Word moves a
                // picture.
                if y + chh > self.curMaxY {
                    let mut _tmp_1887 = self.curPage.clone();
                    self.paintChrome(&_tmp_1887, &self.curSec.clone());
                    let mut _tmp_1889 = self.curSec.clone();
                    let _tmp_1891 = self.newPage(&_tmp_1889, self.curSecIdx.clone());
                    self.curPage = _tmp_1891;
                    ml = DocSection::marginLeftPx(&self.curSec);
                    mt = DocSection::marginTopPx(&self.curSec);
                    mb = DocSection::marginBottomPx(&self.curSec);
                    header_h = self.measureHeaderFooter(&self.curSec.borrow().headerParas,
                        self.contentW
                    );
                    footer_h = self.measureHeaderFooter(&self.curSec.borrow().footerParas,
                        self.contentW
                    );
                    y = mt;
                    if header_h > 0 {
                        y = (mt + header_h) + 8;
                    }
                    self.curMaxY = (self.curPage.borrow().height - mb) - footer_h;
                    if footer_h > 0 {
                        self.curMaxY -= 8;
                    }
                }
                let mut cl: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                cl.borrow_mut().kind = LaidLineKind::chart();
                cl.borrow_mut().chartId = ch.borrow().id;
                cl.borrow_mut().x = ml;
                cl.borrow_mut().y = y;
                cl.borrow_mut().width = cw;
                cl.borrow_mut().height = chh;
                cl.borrow_mut().boxX = ml;
                cl.borrow_mut().boxW = cw;
                self.curPage.borrow_mut().lines.push(cl.clone());
                y = (y + chh) + 10;
            }
            if b.borrow().kind == DocBlockKind::drawing() {
                let mut dr: Rc<RefCell<DocDrawing>> = b.borrow().drawing.clone().clone();
                let mut dw: i64 = dr.borrow().widthPx;
                let mut dh: i64 = dr.borrow().heightPx;
                // Narrower than the column: the whole drawing, at the width
                // there is. A chart re-renders at a narrower width; a drawing
                // is geometry made at one size, so its height follows.
                if dw > self.contentW {
                    if dw > 0 {
                        dh = (((dh as f64) * (self.contentW as f64)) / (dw as f64)).floor() as i64;
                    }
                    dw = self.contentW;
                }
                if dw < 1 {
                    dw = 1;
                }
                if dh < 1 {
                    dh = 1;
                }
                // Whole or not at all, as a chart is: half a diagram is not a
                // diagram.
                if y + dh > self.curMaxY {
                    let mut _tmp_1895 = self.curPage.clone();
                    self.paintChrome(&_tmp_1895, &self.curSec.clone());
                    let mut _tmp_1897 = self.curSec.clone();
                    let _tmp_1899 = self.newPage(&_tmp_1897, self.curSecIdx.clone());
                    self.curPage = _tmp_1899;
                    ml = DocSection::marginLeftPx(&self.curSec);
                    mt = DocSection::marginTopPx(&self.curSec);
                    mb = DocSection::marginBottomPx(&self.curSec);
                    header_h = self.measureHeaderFooter(&self.curSec.borrow().headerParas,
                        self.contentW
                    );
                    footer_h = self.measureHeaderFooter(&self.curSec.borrow().footerParas,
                        self.contentW
                    );
                    y = mt;
                    if header_h > 0 {
                        y = (mt + header_h) + 8;
                    }
                    self.curMaxY = (self.curPage.borrow().height - mb) - footer_h;
                    if footer_h > 0 {
                        self.curMaxY -= 8;
                    }
                }
                let mut dl: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                dl.borrow_mut().kind = LaidLineKind::drawing();
                dl.borrow_mut().drawingId = dr.borrow().id;
                dl.borrow_mut().x = ml;
                dl.borrow_mut().y = y;
                dl.borrow_mut().width = dw;
                dl.borrow_mut().height = dh;
                dl.borrow_mut().boxX = ml;
                dl.borrow_mut().boxW = dw;
                self.curPage.borrow_mut().lines.push(dl.clone());
                y = (y + dh) + 10;
            }
            if b.borrow().kind == DocBlockKind::table() {
                let mut tbl: Rc<RefCell<DocumentTable>> = b.borrow().table.clone().clone();
                let th: i64 = self.estimateTableHeight(&tbl, self.contentW.clone());
                if y + th > self.curMaxY {
                    let mut _tmp_1904 = self.curPage.clone();
                    self.paintChrome(&_tmp_1904, &self.curSec.clone());
                    let mut _tmp_1906 = self.curSec.clone();
                    let _tmp_1908 = self.newPage(&_tmp_1906, self.curSecIdx.clone());
                    self.curPage = _tmp_1908;
                    ml = DocSection::marginLeftPx(&self.curSec);
                    mt = DocSection::marginTopPx(&self.curSec);
                    mb = DocSection::marginBottomPx(&self.curSec);
                    header_h = self.measureHeaderFooter(&self.curSec.borrow().headerParas,
                        self.contentW
                    );
                    footer_h = self.measureHeaderFooter(&self.curSec.borrow().footerParas,
                        self.contentW
                    );
                    y = mt;
                    if header_h > 0 {
                        y = (mt + header_h) + 8;
                    }
                    self.curMaxY = (self.curPage.borrow().height - mb) - footer_h;
                    if footer_h > 0 {
                        self.curMaxY -= 8;
                    }
                }
                y = self.layoutTable(&tbl, ml, y, self.contentW);
            }
            bi += 1;
        }
        let mut _tmp_1910 = self.curPage.clone();
        self.paintChrome(&_tmp_1910, &self.curSec.clone());
    }
    fn newPage(&mut self, sec: &Rc<RefCell<DocSection>>, sec_idx: i64) -> Rc<RefCell<LaidPage>> {
        let mut page: Rc<RefCell<LaidPage>> = Rc::new(RefCell::new(LaidPage::new()));
        page.borrow_mut().width = DocSection::pageWidthPx(&sec);
        page.borrow_mut().height = DocSection::pageHeightPx(&sec);
        page.borrow_mut().index = self.pages.len() as i64;
        page.borrow_mut().sectionIndex = sec_idx;
        page.borrow_mut().contentX = DocSection::marginLeftPx(&sec);
        page.borrow_mut().contentY = DocSection::marginTopPx(&sec);
        page.borrow_mut().contentW = DocSection::contentWidthPx(&sec);
        page.borrow_mut().contentH = (DocSection::pageHeightPx(&sec) - DocSection::marginTopPx(&sec)) - DocSection::marginBottomPx(&sec);
        self.pages.push(page.clone());
        page
    }
    fn paintChrome(&self, page: &Rc<RefCell<LaidPage>>, sec: &Rc<RefCell<DocSection>>) {
        let ml: i64 = DocSection::marginLeftPx(&sec);
        let mt: i64 = DocSection::marginTopPx(&sec);
        let mb: i64 = DocSection::marginBottomPx(&sec);
        let cw: i64 = DocSection::contentWidthPx(&sec);
        // header
        let mut hy: i64 = mt;
        let mut hi: i64 = 0;
        while hi < (sec.borrow().headerParas.len() as i64) {
            let mut hp: Rc<RefCell<Paragraph>> = sec.borrow().headerParas[hi as usize].clone();
            hy = self.layoutChromePara(page,
                hp.clone(),
                ml,
                hy,
                cw,
                LaidLineKind::header()
            );
            hi += 1;
        }
        // footer
        let fh: i64 = self.measureHeaderFooter(&sec.borrow().footerParas.clone(), cw);
        let mut fy: i64 = (page.borrow().height - mb) - fh;
        let mut fi: i64 = 0;
        while fi < (sec.borrow().footerParas.len() as i64) {
            let mut fp: Rc<RefCell<Paragraph>> = sec.borrow().footerParas[fi as usize].clone();
            fy = self.layoutChromePara(page,
                fp.clone(),
                ml,
                fy,
                cw,
                LaidLineKind::footer()
            );
            fi += 1;
        }
    }
    fn layoutChromePara(&self, page: &Rc<RefCell<LaidPage>>, p: Rc<RefCell<Paragraph>>, x0: i64, mut y: i64, wrap_w: i64, kind: i64) -> i64 {
        let mut font_px: f64 = 11.0;
        if !p.borrow().spans.is_empty() {
            let mut s0: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
            font_px = DocxLayout::ptToPx(s0.borrow().fontSizePt);
        }
        if font_px < 8.0 {
            font_px = 8.0;
        }
        DocxTextMetrics::useParagraph(&self.tr.clone().unwrap(), &p);
        let mut lh: i64 = (UITextRenderer::lineHeight(&self.tr.as_ref().unwrap(), font_px) * p.borrow().lineSpacing).floor() as i64;
        if lh < 10 {
            lh = 10;
        }
        let mut wrapped: Vec<String> = UITextRenderer::wrap(
            &self.tr.as_ref().unwrap(),
            &p.borrow().text,
            font_px,
            (wrap_w as f64)
        );
        if wrapped.is_empty() {
            wrapped.push(p.borrow().text.clone());
        }
        let mut li: i64 = 0;
        while li < (wrapped.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
            line.borrow_mut().kind = kind;
            line.borrow_mut().paragraphRef = Some(p.clone());
            line.borrow_mut().text = wrapped[li as usize].clone();
            line.borrow_mut().height = lh;
            line.borrow_mut().y = y;
            line.borrow_mut().fontSizePx = font_px;
            line.borrow_mut().fontFamily = DocxTextMetrics::family(&self.tr.clone().unwrap());
            line.borrow_mut().align = p.borrow().align;
            line.borrow_mut().bidi = p.borrow().bidi;
            if !p.borrow().spans.is_empty() {
                let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
                line.borrow_mut().bold = sp.borrow().bold;
                line.borrow_mut().italic = sp.borrow().italic;
                line.borrow_mut().underline = sp.borrow().underline;
                line.borrow_mut().colorHex = sp.borrow().colorHex.clone();
            }
            let mut _tmp_1928 = line.borrow().text.clone();
            let tw: i64 = (UITextRenderer::measureWidth(
                &self.tr.as_ref().unwrap(),
                &_tmp_1928,
                font_px
            )).floor() as i64;
            let mut x: i64 = x0;
            if p.borrow().align == 1 {
                x = x0 + (((wrap_w - tw) as f64 / 2.0).floor() as i64);
            }
            if p.borrow().align == 2 {
                x = x0 + (wrap_w - tw);
            }
            line.borrow_mut().x = x;
            line.borrow_mut().width = tw;
            self.curPage.borrow_mut().lines.push(line.clone());
            y += lh;
            li += 1;
        }
        y
    }
    fn measureHeaderFooter(&self, paras: &Vec<Rc<RefCell<Paragraph>>>, wrap_w: i64) -> i64 {
        let mut h: i64 = 0;
        let mut i: i64 = 0;
        while i < (paras.len() as i64) {
            let mut p: Rc<RefCell<Paragraph>> = paras[i as usize].clone();
            h += self.estimateParaHeight(&p, wrap_w);
            i += 1;
        }
        h
    }
    fn estimateParaHeight(&self, p: &Rc<RefCell<Paragraph>>, wrap_w: i64) -> i64 {
        let before: i64 = (DocxLayout::ptToPx(p.borrow().spaceBeforePt.clone())).floor() as i64;
        let after: i64 = (DocxLayout::ptToPx(p.borrow().spaceAfterPt.clone())).floor() as i64;
        let mut font_px: f64 = 14.0;
        if !p.borrow().spans.is_empty() {
            let mut s0: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
            font_px = DocxLayout::ptToPx(s0.borrow().fontSizePt);
        }
        if font_px < 8.0 {
            font_px = 8.0;
        }
        DocxTextMetrics::useParagraph(&self.tr.clone().unwrap(), p);
        let mut lh: i64 = (UITextRenderer::lineHeight(&self.tr.as_ref().unwrap(), font_px) * p.borrow().lineSpacing).floor() as i64;
        if lh < 12 {
            lh = 12;
        }
        let indent: i64 = (DocxLayout::ptToPx(p.borrow().indentLeftPt.clone())).floor() as i64;
        let mut wrap: f64 = (wrap_w - indent) as f64;
        if wrap < 40.0 {
            wrap = 40.0;
        }
        let mut lines: i64 = 1;
        if !p.borrow().text.is_empty() {
            let mut wrapped: Vec<String> = UITextRenderer::wrap(
                &self.tr.as_ref().unwrap(),
                &p.borrow().text,
                font_px,
                wrap
            );
            lines = wrapped.len() as i64;
            if lines < 1 {
                lines = 1;
            }
        }
        let mut img_h: i64 = 0;
        let mut ii: i64 = 0;
        while ii < (p.borrow().images.len() as i64) {
            let mut dimg: Rc<RefCell<DocImage>> = p.borrow().images[ii as usize].clone();
            let mut ih: i64 = dimg.borrow().heightPx;
            let iw: i64 = dimg.borrow().widthPx;
            if iw > wrap_w {
                let scale: f64 = (wrap_w as f64) / (iw as f64);
                ih = ((ih as f64) * scale).floor() as i64;
            }
            img_h = (img_h + ih) + 8;
            ii += 1;
        }
        ((before + after) + lines * lh) + img_h
    }
    fn estimateTableHeight(&self, tbl: &Rc<RefCell<DocumentTable>>, wrap_w: i64) -> i64 {
        let mut h: i64 = 8;
        let mut ri: i64 = 0;
        while ri < (tbl.borrow().rows.len() as i64) {
            let mut row: Rc<RefCell<DocTableRow>> = tbl.borrow().rows[ri as usize].clone();
            let mut row_h: i64 = 28;
            let mut ci: i64 = 0;
            while ci < (row.borrow().cells.len() as i64) {
                let mut cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                let mut ch: i64 = (cell.borrow().padTopPx + cell.borrow().padBottomPx) + 4;
                let mut pi: i64 = 0;
                while pi < (cell.borrow().paragraphs.len() as i64) {
                    let mut cp: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
                    ch += self.estimateParaHeight(&cp, 120);
                    pi += 1;
                }
                if ch > row_h {
                    row_h = ch;
                }
                ci += 1;
            }
            h += row_h;
            ri += 1;
        }
        h
    }
    pub fn breakPage(&mut self, y_unused: i64) -> i64 {
        let mut _tmp_1933 = self.curPage.clone();
        self.paintChrome(&_tmp_1933, &self.curSec.clone());
        let mut _tmp_1935 = self.curSec.clone();
        let _tmp_1937 = self.newPage(&_tmp_1935, self.curSecIdx.clone());
        self.curPage = _tmp_1937;
        self.contentW = DocSection::contentWidthPx(&self.curSec);
        let mut _tmp_1940 = self.curSec.borrow().headerParas.clone();
        let header_h: i64 = self.measureHeaderFooter(&_tmp_1940, self.contentW.clone());
        let mut _tmp_1942 = self.curSec.borrow().footerParas.clone();
        let footer_h: i64 = self.measureHeaderFooter(&_tmp_1942, self.contentW.clone());
        let mut y: i64 = DocSection::marginTopPx(&self.curSec);
        if header_h > 0 {
            y = (y + header_h) + 8;
        }
        self.curMaxY = (self.curPage.borrow().height - DocSection::marginBottomPx(&self.curSec)) - footer_h;
        if footer_h > 0 {
            self.curMaxY -= 8;
        }
        y
    }
    fn layoutParagraph(&mut self, p: Rc<RefCell<Paragraph>>, mut ml: i64, mut y: i64, content_w: i64) -> i64 {
        let before: i64 = (DocxLayout::ptToPx(p.borrow().spaceBeforePt.clone())).floor() as i64;
        let after: i64 = (DocxLayout::ptToPx(p.borrow().spaceAfterPt.clone())).floor() as i64;
        y += before;
        let mut prefix: String = String::new();
        let indent: i64 = (DocxLayout::ptToPx(p.borrow().indentLeftPt.clone())).floor() as i64;
        let first_extra: i64 = (DocxLayout::ptToPx(p.borrow().indentFirstLinePt.clone())).floor() as i64;
        if p.borrow().listNumId > 0 {
            let mut _tmp_1952 = p.borrow().listNumId.clone();
            let mut lv: Rc<RefCell<NumLevel>> = self.numbering.borrow().levelFor(_tmp_1952, p.borrow().listLevel.clone());
            let mut _tmp_1955 = self.listCounters.clone();
            let mut _tmp_1956 = p.borrow().listNumId.clone();
            let mut _tmp_1957 = p.borrow().listLevel.clone();
            let mut _tmp_1958 = lv.borrow().start.clone();
            self.numbering.clone().borrow().bumpCounter(&_tmp_1955, _tmp_1956, _tmp_1957, _tmp_1958);
            prefix = self.numbering.borrow().formatMarker(p.borrow().listNumId, p.borrow().listLevel, &self.listCounters);
        }
        let mut font_px: f64 = 14.0;
        let mut bold: bool = false;
        let mut italic: bool = false;
        let mut underline: bool = false;
        let mut color_hex: String = "000000".to_string();
        if !p.borrow().spans.is_empty() {
            let mut s0: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
            font_px = DocxLayout::ptToPx(s0.borrow().fontSizePt);
            bold = s0.borrow().bold;
            italic = s0.borrow().italic;
            underline = s0.borrow().underline;
            color_hex = s0.borrow().colorHex.clone();
        }
        if font_px < 8.0 {
            font_px = 8.0;
        }
        // Wrapping and line widths must use the same face the painter will use
        // for this paragraph's primary run, or a bold paragraph wraps as if it
        // were regular and every line measures short. The slant counts for the
        // same reason: an italic face is not the width of its upright one.
        DocxTextMetrics::useParagraph(&self.tr.clone().unwrap(), &p);
        DocxTextMetrics::applyStyle(&self.tr.clone().unwrap(), bold, italic);
        let para_fam: String = DocxTextMetrics::family(&self.tr.clone().unwrap());
        let mut lh: i64 = (UITextRenderer::lineHeight(&self.tr.as_ref().unwrap(), font_px) * p.borrow().lineSpacing).floor() as i64;
        if lh < 12 {
            lh = 12;
        }
        if !p.borrow().text.is_empty() {
            let mut prefix_w: i64 = 0;
            if !prefix.is_empty() {
                prefix_w = (UITextRenderer::measureWidth(
                    &self.tr.as_ref().unwrap(),
                    &prefix,
                    font_px
                )).floor() as i64;
            }
            let mut wrap_w: f64 = ((content_w - indent) - prefix_w) as f64;
            if wrap_w < 40.0 {
                wrap_w = 40.0;
            }
            // Wrap body text only so doc offsets stay accurate for editing hit-test.
            let mut wrapped: Vec<String> = UITextRenderer::wrap(
                &self.tr.as_ref().unwrap(),
                &p.borrow().text,
                font_px,
                wrap_w
            );
            if wrapped.is_empty() {
                wrapped.push(String::new().to_string());
            }
            let mut cursor: i64 = 0;
            let mut li: i64 = 0;
            while li < (wrapped.len() as i64) {
                if y + lh > self.curMaxY {
                    y = self.breakPage(y);
                    ml = DocSection::marginLeftPx(&self.curSec);
                    // the new page's header and footer set their own face
                    DocxTextMetrics::useFamily(&self.tr.clone().unwrap(), &para_fam);
                    DocxTextMetrics::applyStyle(&self.tr.clone().unwrap(), bold, italic);
                }
                let chunk: String = wrapped[li as usize].clone();
                let mut line: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                line.borrow_mut().kind = LaidLineKind::text();
                line.borrow_mut().paragraphRef = Some(p.clone());
                line.borrow_mut().paraIndex = p.borrow().id;
                cursor = DocxLayout::chunkStart(&p.borrow().text, cursor, &chunk);
                line.borrow_mut().docStart = cursor;
                line.borrow_mut().docEnd = cursor + (DxNative::str_len(&chunk));
                cursor = line.borrow().docEnd;
                if li == 0 && !prefix.is_empty() {
                    line.borrow_mut().text = format!("{}{}", prefix, chunk);
                    line.borrow_mut().listPrefix = prefix.clone();
                } else {
                    line.borrow_mut().text = chunk.clone();
                    line.borrow_mut().listPrefix = String::new();
                }
                line.borrow_mut().height = lh;
                line.borrow_mut().y = y;
                line.borrow_mut().baseline = y + ((font_px * 0.8).floor() as i64);
                line.borrow_mut().align = p.borrow().align;
                line.borrow_mut().bidi = p.borrow().bidi;
                line.borrow_mut().bold = bold;
                line.borrow_mut().underline = underline;
                line.borrow_mut().fontSizePx = font_px;
                line.borrow_mut().fontFamily = para_fam.clone();
                line.borrow_mut().colorHex = color_hex.clone();
                if !p.borrow().spans.is_empty() {
                    let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
                    line.borrow_mut().bold = sp.borrow().bold;
                    line.borrow_mut().italic = sp.borrow().italic;
                    line.borrow_mut().underline = sp.borrow().underline;
                    line.borrow_mut().colorHex = sp.borrow().colorHex.clone();
                    let _tmp_1970 = DocxLayout::ptToPx(sp.borrow().fontSizePt.clone());
                    line.borrow_mut().fontSizePx = _tmp_1970;
                }
                let mut _tmp_1972 = line.borrow().text.clone();
                let tw: i64 = (UITextRenderer::measureWidth(
                    &self.tr.as_ref().unwrap(),
                    &_tmp_1972,
                    font_px
                )).floor() as i64;
                let mut x0: i64 = ml + indent;
                if li == 0 {
                    x0 += first_extra;
                }
                // A list item's later lines line up with its text, not its
                // number, the way a hanging indent does in Word.
                if li > 0 && prefix_w > 0 {
                    x0 += prefix_w;
                }
                if p.borrow().align == 1 {
                    x0 = ml + ((((content_w - indent) - tw) as f64 / 2.0).floor() as i64);
                }
                if p.borrow().align == 2 {
                    x0 = ml + ((content_w - indent) - tw);
                }
                if x0 < ml {
                    x0 = ml;
                }
                line.borrow_mut().x = x0;
                line.borrow_mut().width = tw;
                line.borrow_mut().boxX = ml + indent;
                line.borrow_mut().boxW = content_w - indent;
                if line.borrow().boxW < 4 {
                    line.borrow_mut().boxW = 4;
                }
                self.curPage.borrow_mut().lines.push(line.clone());
                y += lh;
                li += 1;
            }
        } else {
            // Empty paragraph still occupies a line so it can receive the caret.
            if y + lh > self.curMaxY {
                y = self.breakPage(y);
                ml = DocSection::marginLeftPx(&self.curSec);
            }
            let mut empty_line: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
            empty_line.borrow_mut().kind = LaidLineKind::text();
            empty_line.borrow_mut().paragraphRef = Some(p.clone());
            empty_line.borrow_mut().paraIndex = p.borrow().id;
            empty_line.borrow_mut().docStart = 0;
            empty_line.borrow_mut().docEnd = 0;
            empty_line.borrow_mut().text = String::new();
            empty_line.borrow_mut().height = lh;
            empty_line.borrow_mut().y = y;
            empty_line.borrow_mut().baseline = y + ((font_px * 0.8).floor() as i64);
            empty_line.borrow_mut().align = p.borrow().align;
            empty_line.borrow_mut().bidi = p.borrow().bidi;
            empty_line.borrow_mut().fontSizePx = font_px;
            empty_line.borrow_mut().fontFamily = para_fam.clone();
            empty_line.borrow_mut().x = ml + indent;
            empty_line.borrow_mut().width = 4;
            // An empty list item still shows its bullet or number.
            if !prefix.is_empty() {
                empty_line.borrow_mut().text = prefix.clone();
                empty_line.borrow_mut().listPrefix = prefix.clone();
                let _tmp_1978 = (UITextRenderer::measureWidth(
                    &self.tr.as_ref().unwrap(),
                    &prefix,
                    font_px
                )).floor() as i64;
                empty_line.borrow_mut().width = _tmp_1978;
            }
            empty_line.borrow_mut().boxX = ml + indent;
            empty_line.borrow_mut().boxW = content_w - indent;
            if empty_line.borrow().boxW < 4 {
                empty_line.borrow_mut().boxW = 4;
            }
            self.curPage.borrow_mut().lines.push(empty_line.clone());
            y += lh;
            if p.borrow().images.is_empty() {
                // already advanced
            }
        }
        let mut ii: i64 = 0;
        while ii < (p.borrow().images.len() as i64) {
            let mut dimg: Rc<RefCell<DocImage>> = p.borrow().images[ii as usize].clone();
            let mut iw: i64 = dimg.borrow().widthPx;
            let mut ih: i64 = dimg.borrow().heightPx;
            if iw > content_w {
                let scale: f64 = (content_w as f64) / (iw as f64);
                iw = content_w;
                ih = ((ih as f64) * scale).floor() as i64;
            }
            if y + ih > self.curMaxY {
                y = self.breakPage(y);
                ml = DocSection::marginLeftPx(&self.curSec);
            }
            let mut iline: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
            iline.borrow_mut().kind = LaidLineKind::image();
            iline.borrow_mut().paragraphRef = Some(p.clone());
            iline.borrow_mut().paraIndex = p.borrow().id;
            iline.borrow_mut().imageIndex = ii;
            iline.borrow_mut().x = ml;
            iline.borrow_mut().y = y;
            iline.borrow_mut().height = ih;
            iline.borrow_mut().width = iw;
            iline.borrow_mut().align = p.borrow().align;
            iline.borrow_mut().bidi = p.borrow().bidi;
            if p.borrow().align == 1 {
                iline.borrow_mut().x = ml + (((content_w - iw) as f64 / 2.0).floor() as i64);
            }
            if p.borrow().align == 2 {
                iline.borrow_mut().x = ml + (content_w - iw);
            }
            self.curPage.borrow_mut().lines.push(iline.clone());
            y = (y + ih) + 8;
            ii += 1;
        }
        y += after;
        y
    }
    fn layoutTable(&self, tbl: &Rc<RefCell<DocumentTable>>, ml: i64, mut y: i64, content_w: i64) -> i64 {
        let cols: i64 = tbl.borrow().colWidthsTwips.len() as i64;
        if cols < 1 {
            return y + 4;
        }
        let mut total_tw: i64 = 0;
        let mut ci0: i64 = 0;
        while ci0 < cols {
            total_tw += tbl.borrow().colWidthsTwips[ci0 as usize];
            ci0 += 1;
        }
        if total_tw < 1 {
            total_tw = 1;
        }
        let mut col_px: Vec<i64> = Vec::new();
        let mut ci1: i64 = 0;
        while ci1 < cols {
            let tw: i64 = tbl.borrow().colWidthsTwips[ci1 as usize];
            let mut px: i64 = (((tw as f64) * (content_w as f64)) / (total_tw as f64)).floor() as i64;
            if px < 24 {
                px = 24;
            }
            col_px.push(px);
            ci1 += 1;
        }
        let mut ri: i64 = 0;
        while ri < (tbl.borrow().rows.len() as i64) {
            let mut row: Rc<RefCell<DocTableRow>> = tbl.borrow().rows[ri as usize].clone();
            // Measure row height
            let mut row_h: i64 = 24;
            let mut cell_heights: Vec<i64> = Vec::new();
            let mut c: i64 = 0;
            let mut grid_col: i64 = 0;
            while c < (row.borrow().cells.len() as i64) {
                let mut cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[c as usize].clone();
                let mut span: i64 = cell.borrow().gridSpan;
                if span < 1 {
                    span = 1;
                }
                let mut cw: i64 = 0;
                let mut s: i64 = 0;
                while s < span && grid_col + s < (col_px.len() as i64) {
                    cw += col_px[(grid_col + s) as usize];
                    s += 1;
                }
                let mut inner_w: i64 = (cw - cell.borrow().padLeftPx) - cell.borrow().padRightPx;
                if inner_w < 20 {
                    inner_w = 20;
                }
                let mut ch: i64 = cell.borrow().padTopPx + cell.borrow().padBottomPx;
                let mut pi: i64 = 0;
                while pi < (cell.borrow().paragraphs.len() as i64) {
                    let mut cp: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
                    ch += self.estimateParaHeight(&cp, inner_w);
                    pi += 1;
                }
                if ch < 24 {
                    ch = 24;
                }
                cell_heights.push(ch);
                if ch > row_h {
                    row_h = ch;
                }
                grid_col += span;
                c += 1;
            }
            // Draw cells
            let mut x: i64 = ml;
            grid_col = 0;
            c = 0;
            while c < (row.borrow().cells.len() as i64) {
                let mut cell2: Rc<RefCell<DocTableCell>> = row.borrow().cells[c as usize].clone();
                let mut span2: i64 = cell2.borrow().gridSpan;
                if span2 < 1 {
                    span2 = 1;
                }
                let mut cw2: i64 = 0;
                let mut s2: i64 = 0;
                while s2 < span2 && grid_col + s2 < (col_px.len() as i64) {
                    cw2 += col_px[(grid_col + s2) as usize];
                    s2 += 1;
                }
                // background
                if (DxNative::str_len(&cell2.borrow().fillHex)) == 6 {
                    let mut bg: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                    bg.borrow_mut().kind = LaidLineKind::fill();
                    bg.borrow_mut().x = x;
                    bg.borrow_mut().y = y;
                    bg.borrow_mut().width = cw2;
                    bg.borrow_mut().height = row_h;
                    bg.borrow_mut().fillHex = cell2.borrow().fillHex.clone();
                    self.curPage.borrow_mut().lines.push(bg.clone());
                }
                // borders
                let mut bcol: String = cell2.borrow().borderTop.borrow().colorHex.clone();
                if (DxNative::str_len(&bcol)) != 6 {
                    bcol = tbl.borrow().borderColor.clone();
                }
                if cell2.borrow().borderTop.borrow().style != "nil" {
                    if cell2.borrow().borderTop.borrow().style != "none" {
                        let mut bt: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                        bt.borrow_mut().kind = LaidLineKind::border();
                        bt.borrow_mut().x = x;
                        bt.borrow_mut().y = y;
                        bt.borrow_mut().width = cw2;
                        bt.borrow_mut().height = 1;
                        bt.borrow_mut().borderColor = bcol.clone();
                        self.curPage.borrow_mut().lines.push(bt.clone());
                    }
                }
                if cell2.borrow().borderBottom.borrow().style != "nil" {
                    if cell2.borrow().borderBottom.borrow().style != "none" {
                        let mut bb: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                        bb.borrow_mut().kind = LaidLineKind::border();
                        bb.borrow_mut().x = x;
                        bb.borrow_mut().y = (y + row_h) - 1;
                        bb.borrow_mut().width = cw2;
                        bb.borrow_mut().height = 1;
                        bb.borrow_mut().borderColor = bcol.clone();
                        self.curPage.borrow_mut().lines.push(bb.clone());
                    }
                }
                if cell2.borrow().borderLeft.borrow().style != "nil" {
                    if cell2.borrow().borderLeft.borrow().style != "none" {
                        let mut bl: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                        bl.borrow_mut().kind = LaidLineKind::border();
                        bl.borrow_mut().x = x;
                        bl.borrow_mut().y = y;
                        bl.borrow_mut().width = 1;
                        bl.borrow_mut().height = row_h;
                        bl.borrow_mut().borderColor = bcol.clone();
                        self.curPage.borrow_mut().lines.push(bl.clone());
                    }
                }
                if cell2.borrow().borderRight.borrow().style != "nil" {
                    if cell2.borrow().borderRight.borrow().style != "none" {
                        let mut br: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
                        br.borrow_mut().kind = LaidLineKind::border();
                        br.borrow_mut().x = (x + cw2) - 1;
                        br.borrow_mut().y = y;
                        br.borrow_mut().width = 1;
                        br.borrow_mut().height = row_h;
                        br.borrow_mut().borderColor = bcol.clone();
                        self.curPage.borrow_mut().lines.push(br.clone());
                    }
                }
                // cell paragraphs
                let mut cy: i64 = y + cell2.borrow().padTopPx;
                let mut inner_w2: i64 = (cw2 - cell2.borrow().padLeftPx) - cell2.borrow().padRightPx;
                if inner_w2 < 20 {
                    inner_w2 = 20;
                }
                let mut pi2: i64 = 0;
                while pi2 < (cell2.borrow().paragraphs.len() as i64) {
                    let mut cp2: Rc<RefCell<Paragraph>> = cell2.borrow().paragraphs[pi2 as usize].clone();
                    cy = self.layoutCellPara(cp2.clone(),
                        x + cell2.borrow().padLeftPx,
                        cy,
                        inner_w2
                    );
                    pi2 += 1;
                }
                x += cw2;
                grid_col += span2;
                c += 1;
            }
            y += row_h;
            ri += 1;
        }
        y + 10
    }
    fn layoutCellPara(&self, p: Rc<RefCell<Paragraph>>, x0: i64, mut y: i64, wrap_w: i64) -> i64 {
        let mut font_px: f64 = 12.0;
        if !p.borrow().spans.is_empty() {
            let mut s0: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
            font_px = DocxLayout::ptToPx(s0.borrow().fontSizePt);
        }
        if font_px < 8.0 {
            font_px = 8.0;
        }
        DocxTextMetrics::useParagraph(&self.tr.clone().unwrap(), &p);
        let mut lh: i64 = (UITextRenderer::lineHeight(&self.tr.as_ref().unwrap(), font_px)).floor() as i64;
        if lh < 12 {
            lh = 12;
        }
        let mut wrapped: Vec<String> = UITextRenderer::wrap(
            &self.tr.as_ref().unwrap(),
            &p.borrow().text,
            font_px,
            (wrap_w as f64)
        );
        if wrapped.is_empty() {
            wrapped.push(String::new().to_string());
        }
        let mut cursor: i64 = 0;
        let mut li: i64 = 0;
        while li < (wrapped.len() as i64) {
            let chunk: String = wrapped[li as usize].clone();
            let mut line: Rc<RefCell<LaidLine>> = Rc::new(RefCell::new(LaidLine::new()));
            line.borrow_mut().kind = LaidLineKind::text();
            line.borrow_mut().paragraphRef = Some(p.clone());
            line.borrow_mut().paraIndex = p.borrow().id;
            cursor = DocxLayout::chunkStart(&p.borrow().text, cursor, &chunk);
            line.borrow_mut().docStart = cursor;
            line.borrow_mut().docEnd = cursor + (DxNative::str_len(&chunk));
            cursor = line.borrow().docEnd;
            line.borrow_mut().text = chunk.clone();
            line.borrow_mut().height = lh;
            line.borrow_mut().y = y;
            line.borrow_mut().fontSizePx = font_px;
            line.borrow_mut().fontFamily = DocxTextMetrics::family(&self.tr.clone().unwrap());
            line.borrow_mut().align = p.borrow().align;
            line.borrow_mut().bidi = p.borrow().bidi;
            if !p.borrow().spans.is_empty() {
                let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
                line.borrow_mut().bold = sp.borrow().bold;
                line.borrow_mut().italic = sp.borrow().italic;
                line.borrow_mut().underline = sp.borrow().underline;
                line.borrow_mut().colorHex = sp.borrow().colorHex.clone();
            }
            let mut _tmp_2000 = line.borrow().text.clone();
            let tw: i64 = (UITextRenderer::measureWidth(
                &self.tr.as_ref().unwrap(),
                &_tmp_2000,
                font_px
            )).floor() as i64;
            let mut x: i64 = x0;
            if p.borrow().align == 1 {
                x = x0 + (((wrap_w - tw) as f64 / 2.0).floor() as i64);
            }
            if p.borrow().align == 2 {
                x = x0 + (wrap_w - tw);
            }
            line.borrow_mut().x = x;
            line.borrow_mut().width = tw;
            line.borrow_mut().boxX = x0;
            line.borrow_mut().boxW = wrap_w;
            if line.borrow().boxW < 4 {
                line.borrow_mut().boxW = 4;
            }
            self.curPage.borrow_mut().lines.push(line.clone());
            y += lh;
            li += 1;
        }
        y + 2
    }
    /// A different document is in front of this layout, so nothing it holds is
    /// about the document it is now being asked about.
    pub fn invalidate(&mut self) {
        self.laidRevision = 0 - 1;
    }
    pub fn pageCount(&self) -> i64 {
        self.pages.len() as i64
    }
    pub fn pageAt(&self, i: i64) -> Rc<RefCell<LaidPage>> {
        self.pages[i as usize].clone()
    }
}
