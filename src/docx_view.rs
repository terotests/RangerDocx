// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::docx_edit_controller::DocxEditController;
use crate::docx_ink::DocxInk;
use crate::docx_layout::DocxLayout;
use crate::docx_layout::LaidLine;
use crate::docx_layout::LaidLineKind;
use crate::docx_layout::LaidPage;
use crate::docx_text_metrics::DocxTextMetrics;
use crate::rgr::DxNative;
use crate::rich_document::DocChart;
use crate::rich_document::DocDrawing;
use crate::rich_document::DocImage;
use crate::rich_document::DocSection;
use crate::rich_document::Paragraph;
use crate::rich_document::RichDocument;
use crate::rich_document::TextSpan;
use crate::word_numbering::WordNumbering;
use crate::rgr::{ChartDoc, ChartDocCodec, DocumentRange, EVGColor, EVGDisplayList, EVGElement, EVGLayout, EVGUnit, GridChartView, GridImages, PNGEncoder, RasterBuffer, SoftCanvas, SoftPainter, UIContext, UITextRenderer};

/// ==============================================================================
/// DocxView - SoftCanvas paint of laid-out RichDocument pages.
/// ==============================================================================
/// The chart component: the same model, the same generator and the same Vela
/// renderer the spreadsheet uses. A chart pasted into a document is not a second
/// implementation of charts — it is the first one, running here.
pub struct DocxView {
    canvas: Rc<RefCell<SoftCanvas>>,
    ctx: Rc<RefCell<UIContext>>,
    /// Charts in the document, drawn by the spreadsheet's own renderer. The
    /// cache is keyed by chart id and box, so scrolling a page of charts costs
    /// nothing and editing one costs a single recompute.
    charts: Rc<RefCell<GridChartView>>,
    chartInk: Rc<RefCell<SoftPainter>>,
    /// Where this page's marks go — the canvas, or a display list. One painter,
    /// two backends; see DocxInk.
    pub ink: Rc<RefCell<DocxInk>>,
    /// Which chart the caret is on, or 0. Painted with a frame so the reader can
    /// see what an edit would change.
    pub selectedChartId: i64,
    /// …and which drawing, on the same terms.
    pub selectedDrawingId: i64,
    pub layout: Rc<RefCell<DocxLayout>>,
    pub pageIndex: i64,
    bg: Rc<RefCell<EVGColor>>,
    inkColor: Rc<RefCell<EVGColor>>,
    marginGuide: Rc<RefCell<EVGColor>>,
    pub pageGap: i64,
    pub docRef: Option<Rc<RefCell<RichDocument>>>,
    caretColor: Rc<RefCell<EVGColor>>,
    selColor: Rc<RefCell<EVGColor>>,
}

impl DocxView {
    pub fn new() -> Self {
        Self {
            canvas: Rc::new(RefCell::new(SoftCanvas::new())),
            ctx: Rc::new(RefCell::new(UIContext::new())),
            charts: Rc::new(RefCell::new(GridChartView::new())),
            chartInk: Rc::new(RefCell::new(SoftPainter::new())),
            ink: Rc::new(RefCell::new(DocxInk::new())),
            selectedChartId: 0,
            selectedDrawingId: 0,
            layout: Rc::new(RefCell::new(DocxLayout::new())),
            pageIndex: 0,
            bg: EVGColor::rgb(248, 248, 250),
            inkColor: EVGColor::rgb(20, 24, 32),
            marginGuide: EVGColor::rgb(220, 224, 232),
            pageGap: 24,
            docRef: None,
            caretColor: EVGColor::rgb(20, 90, 200),
            selColor: EVGColor::rgb(120, 170, 255),
        }
    }
    /// The four corners of a drawing's selection frame, in the page's own
    /// pixels: 1 top-left, 2 top-right, 3 bottom-left, 4 bottom-right. The
    /// frame sits three pixels outside the drawing, so the handles do too.
    pub fn handleX(line: &Rc<RefCell<LaidLine>>, corner: i64) -> i64 {
        if corner == 2 {
            return (line.borrow().x + line.borrow().width) + 3;
        }
        if corner == 4 {
            return (line.borrow().x + line.borrow().width) + 3;
        }
        return line.borrow().x - 3;
    }
    pub fn handleY(line: &Rc<RefCell<LaidLine>>, corner: i64) -> i64 {
        if corner == 3 {
            return (line.borrow().y + line.borrow().height) + 3;
        }
        if corner == 4 {
            return (line.borrow().y + line.borrow().height) + 3;
        }
        return line.borrow().y - 3;
    }
    /// Word's highlight colours, by the names `w:highlight` uses.
    fn highlightColor(name: &str) -> Rc<RefCell<EVGColor>> {
        if name == "yellow" {
            return EVGColor::rgb(255, 255, 0);
        }
        if name == "green" {
            return EVGColor::rgb(0, 255, 0);
        }
        if name == "cyan" {
            return EVGColor::rgb(0, 255, 255);
        }
        if name == "magenta" {
            return EVGColor::rgb(255, 0, 255);
        }
        if name == "blue" {
            return EVGColor::rgb(0, 0, 255);
        }
        if name == "red" {
            return EVGColor::rgb(255, 0, 0);
        }
        if name == "darkBlue" {
            return EVGColor::rgb(0, 0, 139);
        }
        if name == "darkCyan" {
            return EVGColor::rgb(0, 139, 139);
        }
        if name == "darkGreen" {
            return EVGColor::rgb(0, 100, 0);
        }
        if name == "darkMagenta" {
            return EVGColor::rgb(139, 0, 139);
        }
        if name == "darkRed" {
            return EVGColor::rgb(139, 0, 0);
        }
        if name == "darkYellow" {
            return EVGColor::rgb(128, 128, 0);
        }
        if name == "darkGray" {
            return EVGColor::rgb(169, 169, 169);
        }
        if name == "lightGray" {
            return EVGColor::rgb(211, 211, 211);
        }
        if name == "black" {
            return EVGColor::rgb(0, 0, 0);
        }
        EVGColor::rgb(255, 255, 0)
    }
    pub fn init(&self, font_dir: &str) {
        UITextRenderer::addFontDir(&self.ctx.borrow().tr.clone(), font_dir);
        let _tmp_6771 = self.ctx.borrow().tr.clone();
        UITextRenderer::loadFont(
            &_tmp_6771,
            "Open Sans".to_string(),
            "Open_Sans/OpenSans-Regular.ttf"
        );
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Open_Sans/OpenSans-Bold.ttf");
        // The slanted faces. Nothing loaded them before, because nothing could
        // ask for them: the painter and the metrics took `bold` and no more, so
        // every italic run in every document was drawn upright and Word's
        // italic button did nothing anyone could see. `OfficeFont` walks back
        // to the upright face when a family has no italic, so a document naming
        // something else still draws — but Open Sans has both, and now they are
        // here to be drawn with.
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Open_Sans/OpenSans-Italic.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Open_Sans/OpenSans-BoldItalic.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Cinzel/Cinzel-Regular.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Josefin_Sans/JosefinSans-Regular.ttf");
        // The families the shared alias map sends documents to. Without them
        // `OfficeFont` walks back to Open Sans and a Times New Roman paragraph
        // draws sans-serif — which is what this viewer did while the
        // spreadsheet, loading the same map AND these faces, drew it serif.
        // Sharing the map is only half the claim; the faces are the other half.
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Noto_Sans/NotoSans-Regular.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Noto_Sans/NotoSans-Bold.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Noto_Sans/NotoSans-Italic.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Noto_Sans/NotoSans-BoldItalic.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Droid_Serif/DroidSerif.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Droid_Serif/DroidSerif-Bold.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Droid_Serif/DroidSerif-Italic.ttf");
        // Word's default faces, Cambria and Calibri, as their metric-compatible
        // open twins (SIL OFL, this repository's fonts/): the same advance
        // widths, so lines break where Word breaks them. A host whose font
        // directory has no Caladea/ or Carlito/ draws those in Open Sans.
        let __n_si = vec!["Regular".to_string(), "Bold".to_string(), "Italic".to_string(), "BoldItalic".to_string()].len() as i64;
        for si in 0..__n_si {
            let mut st = vec!["Regular".to_string(), "Bold".to_string(), "Italic".to_string(), "BoldItalic".to_string()][si as usize].clone();
            UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), &format!("Caladea/Caladea-{}.ttf", st));
            UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), &format!("Carlito/Carlito-{}.ttf", st));
        }
        // The glyphs a document reaches for that a text face does not carry.
        // Word writes its bullet levels as ● ○ ▪ — geometric shapes, not
        // letters — so every list drew a column of empty boxes; and an emoji in
        // running text drew one too. `FontManager` has had per-codepoint
        // fallback all along, and these two faces are what give it something to
        // fall back TO. Neither becomes the face anything is drawn in.
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Noto_Sans/NotoSans-Regular.ttf");
        // Arabic. Nothing loaded here had a single Arabic glyph in it, so a
        // paragraph in Arabic drew a row of .notdef boxes — and it would have
        // gone on doing that however correct the bidirectional algorithm and
        // the shaper above it were. El Messiri carries the base block AND the
        // joining forms the shaper emits, which is why it is the one chosen:
        // without a GSUB shaper here, a face with only the base block would
        // still have drawn every letter unjoined.
        //
        // It joins the fallback pool per CODEPOINT, like the emoji and the
        // bullets, so it never becomes the face Latin text is drawn in — a
        // mixed line takes its Latin from the document's own face and its
        // Arabic from here, which is what a reader expects and what Word does.
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "El_Messiri/ElMessiri-Regular.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "El_Messiri/ElMessiri-Bold.ttf");
        UITextRenderer::addFace(&self.ctx.borrow().tr.clone(), "Noto_Emoji/NotoEmoji-Regular.ttf");
    }
    pub fn present(&mut self, doc: Rc<RefCell<RichDocument>>, mut page: i64, nums: Rc<RefCell<WordNumbering>>) {
        self.docRef = Some(doc.clone());
        let mut _tmp_6791 = self.ctx.borrow().tr.clone();
        self.layout.clone().borrow_mut().layoutIfNeeded(&doc, &_tmp_6791, nums.clone());
        if page < 0 {
            page = 0;
        }
        if page >= self.layout.borrow().pageCount() {
            page = self.layout.borrow().pageCount() - 1;
        }
        if page < 0 {
            page = 0;
        }
        self.pageIndex = page;
        let mut pg: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(self.pageIndex.clone());
        let mut _tmp_6797 = pg.borrow().width.clone();
        let mut _tmp_6798 = pg.borrow().height.clone();
        SoftCanvas::init(&self.canvas.clone(), _tmp_6797, _tmp_6798);
        let mut _tmp_6801 = self.canvas.clone();
        UIContext::attach(&self.ctx.clone(), _tmp_6801);
        let mut _tmp_6804 = self.ctx.clone();
        self.ink.clone().borrow_mut().canvasMode(_tmp_6804);
        let __arg_0 = EVGColor::red(&self.bg);
        let __arg_1 = EVGColor::green(&self.bg);
        SoftCanvas::clear(&self.canvas, __arg_0, __arg_1, EVGColor::blue(&self.bg));
        let mut sec: Rc<RefCell<DocSection>> = doc.borrow().sectionAt(pg.borrow().sectionIndex.clone());
        let ml: i64 = sec.borrow().marginLeftPx();
        let mt: i64 = sec.borrow().marginTopPx();
        let mr: i64 = sec.borrow().marginRightPx();
        let mb: i64 = sec.borrow().marginBottomPx();
        let mut _tmp_6811 = self.marginGuide.clone();
        let __arg_0 = (pg.borrow().width - ml) - mr;
        let __arg_1 = (pg.borrow().height - mt) - mb;
        self.ink.clone().borrow().strokeRect(ml, mt, __arg_0, __arg_1, 1, &_tmp_6811);
        self.paintPage(&pg);
    }
    pub fn presentWithEdit(&mut self, doc: Rc<RefCell<RichDocument>>, mut page: i64, nums: Rc<RefCell<WordNumbering>>, edit: &Rc<RefCell<DocxEditController>>) {
        self.docRef = Some(doc.clone());
        let mut _tmp_6814 = self.ctx.borrow().tr.clone();
        self.layout.clone().borrow_mut().layoutIfNeeded(&doc, &_tmp_6814, nums.clone());
        if page < 0 {
            page = 0;
        }
        if page >= self.layout.borrow().pageCount() {
            page = self.layout.borrow().pageCount() - 1;
        }
        if page < 0 {
            page = 0;
        }
        self.pageIndex = page;
        let mut pg: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(self.pageIndex.clone());
        let mut _tmp_6820 = pg.borrow().width.clone();
        let mut _tmp_6821 = pg.borrow().height.clone();
        SoftCanvas::init(&self.canvas.clone(), _tmp_6820, _tmp_6821);
        let mut _tmp_6824 = self.canvas.clone();
        UIContext::attach(&self.ctx.clone(), _tmp_6824);
        let mut _tmp_6827 = self.ctx.clone();
        self.ink.clone().borrow_mut().canvasMode(_tmp_6827);
        let __arg_0 = EVGColor::red(&self.bg);
        let __arg_1 = EVGColor::green(&self.bg);
        SoftCanvas::clear(&self.canvas, __arg_0, __arg_1, EVGColor::blue(&self.bg));
        let mut sec: Rc<RefCell<DocSection>> = doc.borrow().sectionAt(pg.borrow().sectionIndex.clone());
        let ml: i64 = sec.borrow().marginLeftPx();
        let mt: i64 = sec.borrow().marginTopPx();
        let mr: i64 = sec.borrow().marginRightPx();
        let mb: i64 = sec.borrow().marginBottomPx();
        let mut _tmp_6834 = self.marginGuide.clone();
        let __arg_0 = (pg.borrow().width - ml) - mr;
        let __arg_1 = (pg.borrow().height - mt) - mb;
        self.ink.clone().borrow().strokeRect(ml, mt, __arg_0, __arg_1, 1, &_tmp_6834);
        if edit.borrow().isActive() {
            self.paintSelection(edit);
        }
        self.paintPage(&pg);
        if edit.borrow().isActive() {
            self.paintCaret(edit);
        }
    }
    /// The same page, as an EVGDisplayList. Identical layout and identical
    /// painting — only the sink differs — so what a browser draws with WebGL is
    /// what the SoftCanvas path would have rasterized, rather than a second
    /// opinion about it.
    fn buildDisplayList(&mut self, doc: Rc<RefCell<RichDocument>>, mut page: i64, nums: Rc<RefCell<WordNumbering>>, edit: &Rc<RefCell<DocxEditController>>, editing: bool) -> Rc<RefCell<EVGDisplayList>> {
        self.docRef = Some(doc.clone());
        let mut _tmp_6837 = self.ctx.borrow().tr.clone();
        self.layout.clone().borrow_mut().layoutIfNeeded(&doc, &_tmp_6837, nums.clone());
        if page < 0 {
            page = 0;
        }
        if page >= self.layout.borrow().pageCount() {
            page = self.layout.borrow().pageCount() - 1;
        }
        if page < 0 {
            page = 0;
        }
        self.pageIndex = page;
        let mut out: Rc<RefCell<EVGDisplayList>> = Rc::new(RefCell::new(EVGDisplayList::new()));
        let mut pg: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(self.pageIndex.clone());
        let mut _tmp_6843 = self.ctx.clone();
        self.ink.clone().borrow_mut().listMode(_tmp_6843, out.clone());
        // The page itself: the paper, then the margin guide, then the content.
        let mut _tmp_6846 = pg.borrow().width.clone();
        let mut _tmp_6847 = pg.borrow().height.clone();
        let mut _tmp_6848 = self.bg.clone();
        self.ink.clone().borrow().rect(0, 0, _tmp_6846, _tmp_6847, &_tmp_6848);
        let mut sec: Rc<RefCell<DocSection>> = doc.borrow().sectionAt(pg.borrow().sectionIndex.clone());
        let ml: i64 = sec.borrow().marginLeftPx();
        let mt: i64 = sec.borrow().marginTopPx();
        let mr: i64 = sec.borrow().marginRightPx();
        let mb: i64 = sec.borrow().marginBottomPx();
        let mut _tmp_6852 = self.marginGuide.clone();
        let __arg_0 = (pg.borrow().width - ml) - mr;
        let __arg_1 = (pg.borrow().height - mt) - mb;
        self.ink.clone().borrow().strokeRect(ml, mt, __arg_0, __arg_1, 1, &_tmp_6852);
        if editing {
            if edit.borrow().isActive() {
                self.paintSelection(edit);
            }
        }
        self.paintPageAt(&pg, 0, 0);
        if editing {
            if edit.borrow().isActive() {
                self.paintCaret(edit);
            }
        }
        // Leave the sink where the rest of the viewer expects it.
        let mut _tmp_6855 = self.ctx.clone();
        self.ink.clone().borrow_mut().canvasMode(_tmp_6855);
        out
    }
    pub fn sceneWidth(&self) -> i64 {
        let mut pg: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(self.pageIndex.clone());
        return pg.borrow().width;
    }
    pub fn sceneHeight(&self) -> i64 {
        let mut pg: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(self.pageIndex.clone());
        return pg.borrow().height;
    }
    pub fn paintCaret(&self, edit: &Rc<RefCell<DocxEditController>>) {
        let mut _tmp_6861 = self.layout.clone();
        let cx: i64 = edit.borrow().caretPixelX(&_tmp_6861, self.pageIndex.clone());
        let mut _tmp_6863 = self.layout.clone();
        let cy: i64 = edit.borrow().caretPixelY(&_tmp_6863, self.pageIndex.clone());
        let mut _tmp_6865 = self.layout.clone();
        let ch: i64 = edit.borrow().caretHeight(&_tmp_6865, self.pageIndex.clone());
        if cx < 0 || cy < 0 {
            return;
        }
        let mut _tmp_6868 = self.caretColor.clone();
        self.ink.clone().borrow().rect(cx, cy, 2, ch, &_tmp_6868);
    }
    pub fn paintSelection(&self, edit: &Rc<RefCell<DocxEditController>>) {
        let mut sel: Rc<RefCell<DocumentRange>> = edit.borrow().selection();
        if DocumentRange::isCollapsed(&sel) {
            return;
        }
        let mut ends: Vec<i64> = edit.borrow().orderedEndList();
        let s_pid: i64 = ends[0];
        let s_off: i64 = ends[1];
        let e_pid: i64 = ends[2];
        let e_off: i64 = ends[3];
        let s_order: i64 = (edit.borrow().adapter).borrow().bodyOrderOf(s_pid);
        let e_order: i64 = (edit.borrow().adapter).borrow().bodyOrderOf(e_pid);
        if s_order < 0 || e_order < 0 {
            return;
        }
        let mut pg: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(self.pageIndex.clone());
        let mut i: i64 = 0;
        while i < (pg.borrow().lines.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = pg.borrow().lines[i as usize].clone();
            if LaidLineKind::isBodyText(line.borrow().kind.clone()) {
                if line.borrow().paraIndex > 0 {
                    let order: i64 = (edit.borrow().adapter).borrow().bodyOrderOf(line.borrow().paraIndex.clone());
                    if (order >= s_order && order <= e_order) {
                        // Clip the selection to what this visual line covers.
                        let ls: i64 = line.borrow().docStart;
                        let le: i64 = line.borrow().docEnd;
                        let mut s: i64 = ls;
                        let mut e: i64 = le;
                        if order == s_order {
                            if s_off > s {
                                s = s_off;
                            }
                        }
                        if order == e_order {
                            if e_off < e {
                                e = e_off;
                            }
                        }
                        if s < ls {
                            s = ls;
                        }
                        if e > le {
                            e = le;
                        }
                        let mut p: Rc<RefCell<Paragraph>> = line.borrow().paragraphRef.clone().unwrap();
                        let font_px: f64 = line.borrow().fontSizePx;
                        let mut x0: i64 = line.borrow().x;
                        DocxTextMetrics::useParagraph(&self.ctx.borrow().tr.clone(), &p);
                        if !line.borrow().listPrefix.is_empty() {
                            if DxNative::index_of(&line.borrow().text, &line.borrow().listPrefix) == 0 {
                                x0 += DocxTextMetrics::measurePlain(
                                    &self.ctx.borrow().tr,
                                    &line.borrow().listPrefix,
                                    font_px,
                                    line.borrow().bold
                                );
                            }
                        }
                        if s < e {
                            let x1: i64 = x0 + DocxTextMetrics::measureRange(&self.ctx.borrow().tr.clone(), &p, font_px, ls, s);
                            let x2: i64 = x0 + DocxTextMetrics::measureRange(&self.ctx.borrow().tr.clone(), &p, font_px, ls, e);
                            let mut w: i64 = x2 - x1;
                            if w < 2 {
                                w = 2;
                            }
                            // A paragraph swallowed whole shows its newline too.
                            if order < e_order {
                                if e == (DxNative::str_len(&p.borrow().text)) {
                                    w += 6;
                                }
                            }
                            let mut _tmp_6878 = line.borrow().y.clone();
                            let mut _tmp_6879 = line.borrow().height.clone();
                            let mut _tmp_6880 = self.selColor.clone();
                            self.ink.clone().borrow().rect(x1, _tmp_6878, w, _tmp_6879, &_tmp_6880);
                        } else {
                            // Empty paragraph inside the selection still shows.
                            let mut empty_inside: bool = false;
                            if ls == le && line.borrow().text.is_empty() {
                                if order > s_order && order < e_order {
                                    empty_inside = true;
                                }
                                if order == s_order && order != e_order {
                                    empty_inside = true;
                                }
                                if order == e_order && order != s_order {
                                    empty_inside = false;
                                }
                                if (s_pid == e_pid && s_off <= ls) && e_off >= ls {
                                    empty_inside = true;
                                }
                            }
                            if empty_inside {
                                let mut _tmp_6883 = line.borrow().x.clone();
                                let mut _tmp_6884 = line.borrow().y.clone();
                                let mut _tmp_6885 = line.borrow().height.clone();
                                let mut _tmp_6886 = self.selColor.clone();
                                self.ink.clone().borrow().rect(_tmp_6883, _tmp_6884, 6, _tmp_6885, &_tmp_6886);
                            }
                        }
                    }
                }
            }
            i += 1;
        }
    }
    pub fn presentAll(&mut self, doc: Rc<RefCell<RichDocument>>, nums: Rc<RefCell<WordNumbering>>, max_pages: i64) {
        self.docRef = Some(doc.clone());
        let mut _tmp_6889 = self.ctx.borrow().tr.clone();
        self.layout.clone().borrow_mut().layoutIfNeeded(&doc, &_tmp_6889, nums.clone());
        let mut n: i64 = self.layout.borrow().pageCount();
        if n < 1 {
            SoftCanvas::init(&self.canvas.clone(), 100, 100);
            return;
        }
        if max_pages > 0 && n > max_pages {
            n = max_pages;
        }
        let mut max_w: i64 = 0;
        let mut total_h: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let mut pg: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(i);
            if pg.borrow().width > max_w {
                max_w = pg.borrow().width;
            }
            total_h += pg.borrow().height;
            if i < n - 1 {
                total_h += self.pageGap;
            }
            i += 1;
        }
        SoftCanvas::init(&self.canvas.clone(), max_w, total_h);
        let mut _tmp_6898 = self.canvas.clone();
        UIContext::attach(&self.ctx.clone(), _tmp_6898);
        let mut _tmp_6901 = self.ctx.clone();
        self.ink.clone().borrow_mut().canvasMode(_tmp_6901);
        SoftCanvas::clear(&self.canvas, 210, 214, 220);
        let mut y_off: i64 = 0;
        i = 0;
        while i < n {
            let mut pg2: Rc<RefCell<LaidPage>> = self.layout.borrow().pageAt(i);
            // page shadow card
            let mut _tmp_6905 = pg2.borrow().width.clone();
            let mut _tmp_6906 = pg2.borrow().height.clone();
            let mut _tmp_6907 = self.bg.clone();
            self.ink.clone().borrow().rect(0, y_off, _tmp_6905, _tmp_6906, &_tmp_6907);
            // temporarily paint into offset by adjusting line y — copy approach:
            self.paintPageAt(&pg2, 0, y_off);
            y_off = (y_off + pg2.borrow().height) + self.pageGap;
            i += 1;
        }
    }
    pub fn paintPage(&self, pg: &Rc<RefCell<LaidPage>>) {
        self.paintPageAt(pg, 0, 0);
    }
    pub fn paintPageAt(&self, pg: &Rc<RefCell<LaidPage>>, ox: i64, oy: i64) {
        let mut i: i64 = 0;
        while i < (pg.borrow().lines.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = pg.borrow().lines[i as usize].clone();
            let lx: i64 = line.borrow().x + ox;
            let ly: i64 = line.borrow().y + oy;
            if line.borrow().kind == LaidLineKind::fill() {
                let mut _tmp_6910 = line.borrow().width.clone();
                let mut _tmp_6911 = line.borrow().height.clone();
                let mut _tmp_6913 = self.hexColor(&line.borrow().fillHex.clone());
                self.ink.clone().borrow().rect(lx, ly, _tmp_6910, _tmp_6911, &_tmp_6913);
            }
            if line.borrow().kind == LaidLineKind::border() {
                let mut _tmp_6917 = line.borrow().width.clone();
                let mut _tmp_6918 = line.borrow().height.clone();
                let mut _tmp_6920 = self.hexColor(&line.borrow().borderColor.clone());
                self.ink.clone().borrow().rect(lx, ly, _tmp_6917, _tmp_6918, &_tmp_6920);
            }
            if line.borrow().kind == LaidLineKind::image() {
                let mut p: Rc<RefCell<Paragraph>> = line.borrow().paragraphRef.clone().unwrap();
                if line.borrow().imageIndex < (p.borrow().images.len() as i64) {
                    let mut dimg: Rc<RefCell<DocImage>> = p.borrow().images[(line.borrow().imageIndex) as usize].clone();
                    if self.ink.borrow().toList {
                        // A display-list backend fetches the part itself; the
                        // decoded pixels here are the canvas path's business.
                        let mut _tmp_6924 = dimg.borrow().partPath.clone();
                        let mut _tmp_6925 = line.borrow().width.clone();
                        let mut _tmp_6926 = line.borrow().height.clone();
                        self.ink.clone().borrow().image(_tmp_6924, lx, ly, _tmp_6925, _tmp_6926);
                    } else {
                        if dimg.borrow().ok {
                            if dimg.borrow()
                                .pixels.borrow()
                                .width == line.borrow()
                                .width && dimg.borrow()
                                .pixels.borrow()
                                .height == line.borrow()
                                .height {
                                let mut _tmp_6929 = dimg.borrow().pixels.clone();
                                SoftCanvas::blitImage(&self.canvas.clone(), &_tmp_6929, lx, ly);
                            } else {
                                let mut _tmp_6932 = dimg.borrow().pixels.clone();
                                let mut _tmp_6933 = dimg.borrow().pixels.borrow().width.clone();
                                let mut _tmp_6934 = dimg.borrow().pixels.borrow().height.clone();
                                let mut _tmp_6935 = line.borrow().width.clone();
                                let mut _tmp_6936 = line.borrow().height.clone();
                                let _tmp_6937 = self.canvas.clone();
                                SoftCanvas::blitImageRectScaled(
                                    &_tmp_6937,
                                    &_tmp_6932,
                                    0,
                                    0,
                                    _tmp_6933,
                                    _tmp_6934,
                                    lx,
                                    ly,
                                    _tmp_6935,
                                    _tmp_6936
                                );
                            }
                        } else {
                            let mut _tmp_6939 = line.borrow().width.clone();
                            let mut _tmp_6940 = line.borrow().height.clone();
                            let __arg_0 = EVGColor::rgb(200, 210, 220).clone();
                            self.ink.clone().borrow().rect(lx, ly, _tmp_6939, _tmp_6940, &__arg_0);
                            let mut _tmp_6943 = self.inkColor.clone();
                            self.ink.clone().borrow().text("[image]", lx, ly + 4, 12.0, &_tmp_6943, false);
                        }
                    }
                }
            }
            if line.borrow().kind == LaidLineKind::chart() {
                self.paintChartLine(&line, lx, ly);
            }
            if line.borrow().kind == LaidLineKind::drawing() {
                self.paintDrawingLine(&line, lx, ly);
            }
            if (line.borrow().kind == LaidLineKind::text() || line.borrow().kind == LaidLineKind::header()) || line.borrow().kind == LaidLineKind::footer() {
                self.paintStyledLine(&line, lx, ly);
            }
            i += 1;
        }
    }
    /// A chart, drawn where the layout put it. The document carries the chart's
    /// JSON; this decodes it, hands the cells and the settings to the same
    /// generator the grid uses, and paints the resulting draw commands onto this
    /// page's canvas. Nothing about the picture was stored — it is made here,
    /// which is why an edit shows up immediately.
    pub fn paintChartLine(&self, line: &Rc<RefCell<LaidLine>>, lx: i64, ly: i64) {
        let mut ch: Rc<RefCell<DocChart>> = (self.docRef.as_ref().unwrap()).borrow().chartById(line.borrow().chartId.clone());
        if ch.borrow().json.is_empty() {
            return;
        }
        let mut doc: Rc<RefCell<ChartDoc>> = ChartDocCodec::decode(&ch.borrow().json.clone());
        if !doc.borrow().ok {
            ch.borrow_mut().note = doc.borrow().note.clone();
            let mut _tmp_6950 = line.borrow().width.clone();
            let mut _tmp_6951 = line.borrow().height.clone();
            let __arg_0 = EVGColor::rgb(240, 242, 246).clone();
            self.ink.clone().borrow().rect(lx, ly, _tmp_6950, _tmp_6951, &__arg_0);
            let mut _tmp_6954 = self.inkColor.clone();
            let __arg_0 = format!("[chart: {}]", doc.borrow().note).clone();
            self.ink.clone().borrow().text(&__arg_0, lx + 8, ly + 8, 12.0, &_tmp_6954, false);
            return;
        }
        // The chart's id is the render cache's key, so two charts in one
        // document do not evict each other.
        doc.borrow_mut().chart.borrow_mut().id = ch.borrow().id;
        let mut dl: Rc<RefCell<EVGDisplayList>> = Rc::new(RefCell::new(EVGDisplayList::new()));
        let mut _tmp_6958 = Rc::clone(&doc.borrow().cells);
        let mut _tmp_6959 = doc.borrow().chart.clone();
        let mut _tmp_6960 = self.ctx.borrow().tr.clone();
        let mut _tmp_6961 = line.borrow().width.clone();
        let mut _tmp_6962 = line.borrow().height.clone();
        let _tmp_6963 = self.charts.clone();
        GridChartView::paint(
            &_tmp_6963,
            &dl,
            &_tmp_6958,
            &_tmp_6959,
            &_tmp_6960,
            lx,
            ly,
            _tmp_6961,
            _tmp_6962
        );
        if self.ink.borrow().toList {
            // The chart already speaks display list, so on that path its
            // commands simply join the page's — no rasterizing at all.
            self.ink.clone().borrow().append(&dl);
        } else {
            let mut _tmp_6967 = self.canvas.clone();
            let mut _tmp_6968 = self.ctx.clone();
            let _tmp_6969 = SoftCanvas::getWidth(&self.canvas);
            let _tmp_6970 = SoftCanvas::getHeight(&self.canvas);
            let _tmp_6973 = self.chartInk.clone();
            SoftPainter::attach(
                &_tmp_6973,
                _tmp_6967,
                _tmp_6968,
                Rc::new(RefCell::new(GridImages::new())),
                _tmp_6969,
                _tmp_6970
            );
            SoftPainter::draw(&self.chartInk.clone(), &dl);
        }
        ch.borrow_mut().note = String::new();
        if self.selectedChartId == ch.borrow().id {
            // The selection frame is chrome, not part of the chart — it is
            // drawn after, so it never ends up in an exported picture.
            let mut mark: Rc<RefCell<EVGColor>> = EVGColor::rgb(40, 100, 210);
            let __arg_0 = line.borrow().width + 6;
            let __arg_1 = line.borrow().height + 6;
            self.ink.clone().borrow().strokeRect(lx - 3, ly - 3, __arg_0, __arg_1, 2, &mark);
        }
    }
    /// A drawing, drawn where the layout put it. The document carries an
    /// element tree made at one size; this lays it out at that size, turns it
    /// into draw commands, and scales and moves the commands into the box the
    /// layout gave it. Scaling the LIST rather than laying the tree out at the
    /// new size, because a diagram's children are placed absolutely and a tree
    /// laid out narrower simply overflows.
    ///
    /// The same road a chart takes: on the display-list path the commands join
    /// the page's, on the canvas path the shared painter rasterizes them.
    pub fn paintDrawingLine(&self, line: &Rc<RefCell<LaidLine>>, lx: i64, ly: i64) {
        let mut dr: Rc<RefCell<DocDrawing>> = (self.docRef.as_ref().unwrap()).borrow().drawingById(line.borrow().drawingId.clone());
        if false == dr.borrow().hasRoot() {
            let mut _tmp_6981 = line.borrow().width.clone();
            let mut _tmp_6982 = line.borrow().height.clone();
            let __arg_0 = EVGColor::rgb(240, 242, 246).clone();
            self.ink.clone().borrow().rect(lx, ly, _tmp_6981, _tmp_6982, &__arg_0);
            let mut why: String = dr.borrow().note.clone();
            if why.is_empty() {
                why = "nothing to draw".to_string();
            }
            let mut _tmp_6985 = self.inkColor.clone();
            let __arg_0 = format!("[{}: {}]", dr.borrow().name, why).clone();
            self.ink.clone().borrow().text(&__arg_0, lx + 8, ly + 8, 12.0, &_tmp_6985, false);
            return;
        }
        let mut root: Rc<RefCell<EVGElement>> = dr.borrow().root.clone().unwrap();
        let mut nw: f64 = dr.borrow().naturalW;
        let mut nh: f64 = dr.borrow().naturalH;
        if nw <= 0.0 {
            nw = line.borrow().width as f64;
        }
        if nh <= 0.0 {
            nh = line.borrow().height as f64;
        }
        // At the origin, at its own size. The tree is shared with whoever
        // else draws it — a preview places it on its page, a slide at its
        // box — so where it sits is stated here rather than assumed.
        root.borrow_mut().position = "absolute".to_string();
        root.borrow_mut().left = Some(EVGUnit::px(0.0));
        root.borrow_mut().top = Some(EVGUnit::px(0.0));
        root.borrow_mut().width = Some(EVGUnit::px(nw));
        root.borrow_mut().height = Some(EVGUnit::px(nh));
        root.borrow_mut().backgroundColor = Some(EVGColor::noColor());
        let mut le: Rc<RefCell<EVGLayout>> = Rc::new(RefCell::new(EVGLayout::new()));
        EVGLayout::setPageSize(&le, nw, nh);
        EVGLayout::layout(&le, root.clone());
        let mut dl: Rc<RefCell<EVGDisplayList>> = Rc::new(RefCell::new(EVGDisplayList::new()));
        EVGDisplayList::build(&dl, root.clone());
        let s: f64 = (line.borrow().width as f64) / nw;
        EVGDisplayList::scaleBy(&dl, s);
        EVGDisplayList::offsetBy(&dl, lx as f64, ly as f64);
        if self.ink.borrow().toList {
            self.ink.clone().borrow().append(&dl);
        } else {
            let mut _tmp_6995 = self.canvas.clone();
            let mut _tmp_6996 = self.ctx.clone();
            let _tmp_6997 = SoftCanvas::getWidth(&self.canvas);
            let _tmp_6998 = SoftCanvas::getHeight(&self.canvas);
            let _tmp_7001 = self.chartInk.clone();
            SoftPainter::attach(
                &_tmp_7001,
                _tmp_6995,
                _tmp_6996,
                Rc::new(RefCell::new(GridImages::new())),
                _tmp_6997,
                _tmp_6998
            );
            SoftPainter::draw(&self.chartInk.clone(), &dl);
        }
        if self.selectedDrawingId == dr.borrow().id {
            // The selection frame is chrome, drawn after the drawing so it is
            // never in an exported picture — the same frame a chart gets.
            let mut mark: Rc<RefCell<EVGColor>> = EVGColor::rgb(40, 100, 210);
            let __arg_0 = line.borrow().width + 6;
            let __arg_1 = line.borrow().height + 6;
            self.ink.clone().borrow().strokeRect(lx - 3, ly - 3, __arg_0, __arg_1, 2, &mark);
            // And a handle on each corner, which is what says the frame can
            // be dragged: a frame alone says "selected" and nothing more,
            // and a reader who wanted the drawing smaller had no way to ask.
            // `DocxViewer.drawingHandleAt` uses the same four corners.
            let mut k: i64 = 1;
            while k <= 4 {
                let hx: i64 = DocxView::handleX(line, k);
                let hy: i64 = DocxView::handleY(line, k);
                let __arg_0 = EVGColor::rgb(255, 255, 255).clone();
                self.ink.clone().borrow().rect(hx - 5, hy - 5, 10, 10, &__arg_0);
                self.ink.clone().borrow().rect(hx - 4, hy - 4, 8, 8, &mark);
                k += 1;
            }
        }
    }
    /// Paint a visual line using paragraph TextSpan runs (bold/color/underline).
    /// Layout still wraps with the line's primary size; run styles drive glyphs.
    pub fn paintStyledLine(&self, line: &Rc<RefCell<LaidLine>>, lx: i64, ly: i64) {
        // The paragraph's direction, before anything is drawn. Every ink call
        // below is a piece of the same paragraph, so this is set once here
        // rather than passed down through each of them.
        self.ink.borrow_mut().dir = line.borrow().bidi;
        // the family the layout wrapped this line in
        let mut _tmp_7011 = self.ctx.borrow().tr.clone();
        DocxTextMetrics::useFamily(&_tmp_7011, &line.borrow().fontFamily.clone());
        let mut size: f64 = line.borrow().fontSizePx;
        if size < 8.0 {
            size = 8.0;
        }
        let mut p: Rc<RefCell<Paragraph>> = line.borrow().paragraphRef.clone().unwrap();
        let mut mute_chrome: bool = false;
        if line.borrow()
            .kind == LaidLineKind::header() || line.borrow()
            .kind == LaidLineKind::footer() {
            mute_chrome = true;
        }
        if p.borrow().id <= 0 || p.borrow().spans.is_empty() {
            let mut col0: Rc<RefCell<EVGColor>> = self.hexColor(&line.borrow().colorHex.clone());
            if mute_chrome {
                col0 = EVGColor::rgb(80, 90, 110);
            }
            let mut _tmp_7016 = line.borrow().text.clone();
            let mut _tmp_7017 = line.borrow().bold.clone();
            let mut _tmp_7018 = line.borrow().italic.clone();
            self.ink.clone().borrow().textStyled(&_tmp_7016, lx, ly, size, &col0, _tmp_7017, _tmp_7018);
            if line.borrow().underline {
                let uy0: i64 = (ly + (size.floor() as i64)) + 1;
                let mut _tmp_7021 = line.borrow().width.clone();
                self.ink.clone().borrow().rect(lx, uy0, _tmp_7021, 1, &col0);
            }
            return;
        }
        let mut display: String = line.borrow().text.clone();
        let mut x: i64 = lx;
        let mut prefix_len: i64 = 0;
        if !line.borrow().listPrefix.is_empty() {
            if DxNative::index_of(&display, &line.borrow().listPrefix) == 0 {
                prefix_len = DxNative::str_len(&line.borrow().listPrefix);
            }
        }
        if prefix_len > 0 {
            let pref: String = DxNative::substr(&display, 0, prefix_len);
            let mut col_p: Rc<RefCell<EVGColor>> = self.hexColor(&line.borrow().colorHex.clone());
            if mute_chrome {
                col_p = EVGColor::rgb(80, 90, 110);
            }
            let mut _tmp_7026 = line.borrow().bold.clone();
            self.ink.clone().borrow().text(&pref, x, ly, size, &col_p, _tmp_7026);
            x += (UITextRenderer::measureWidth(
                &self.ctx.borrow().tr,
                &pref,
                size
            )).floor() as i64;
            display = DxNative::substr(&display, prefix_len, (DxNative::str_len(&display)));
        }
        let doc0: i64 = line.borrow().docStart;
        let mut doc1: i64 = line.borrow().docEnd;
        if doc1 < doc0 {
            doc1 = doc0;
        }
        // Empty line still needs a hit target; nothing to paint.
        if display.is_empty() {
            return;
        }
        // If doc offsets were never set, paint whole line with first span.
        if ((doc0 == 0 && doc1 == 0) && !p.borrow().text.is_empty()) && !display.is_empty() {
            if DxNative::index_of(&p.borrow().text, &display) < 0 {
                let mut s0: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
                self.paintTextRun(&display, x, ly, size, &s0, mute_chrome);
                return;
            }
        }
        let mut si: i64 = 0;
        while si < (p.borrow().spans.len() as i64) {
            let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[si as usize].clone();
            let mut a: i64 = sp.borrow().start;
            let mut b: i64 = sp.borrow().end;
            if a < doc0 {
                a = doc0;
            }
            if b > doc1 {
                b = doc1;
            }
            if a < b {
                let mut local0: i64 = a - doc0;
                let mut local1: i64 = b - doc0;
                if local0 < 0 {
                    local0 = 0;
                }
                if local1 > (DxNative::str_len(&display)) {
                    local1 = DxNative::str_len(&display);
                }
                if local0 < local1 {
                    let chunk: String = DxNative::substr(&display, local0, local1);
                    let run_w: i64 = self.paintTextRun(&chunk,
                        x,
                        ly,
                        size,
                        &sp,
                        mute_chrome
                    );
                    x += run_w;
                }
            }
            si += 1;
        }
    }
    /// Single face policy, shared with DocxTextMetrics so what is painted and
    /// what the caret / selection measure can never disagree.
    pub fn setPaintFace(&self, bold: bool) {
        DocxTextMetrics::applyFace(&self.ctx.borrow().tr.clone(), bold);
    }
    pub fn setPaintStyle(&self, bold: bool, italic: bool) {
        DocxTextMetrics::applyStyle(&self.ctx.borrow().tr.clone(), bold, italic);
    }
    pub fn paintTextRun(&self, chunk: &str, x: i64, y: i64, size: f64, sp: &Rc<RefCell<TextSpan>>, mute_chrome: bool) -> i64 {
        let mut col: Rc<RefCell<EVGColor>> = self.hexColor(&sp.borrow().colorHex.clone());
        if mute_chrome {
            col = EVGColor::rgb(80, 90, 110);
        }
        let mut run_size: f64 = size;
        if run_size < 8.0 {
            run_size = 8.0;
        }
        // Both the slant and the weight, and the SAME pair to the measure — the
        // painter and the metrics drifting apart is how a caret ends up beside
        // the glyphs rather than between them.
        // The advance is the full-size width whatever is drawn: the layout and
        // the caret measure a line at one size, and a superscript drawn
        // smaller still occupies the room they gave it.
        let mut _tmp_7032 = self.ctx.borrow().tr.clone();
        let mut _tmp_7033 = sp.borrow().bold.clone();
        let w: i64 = DocxTextMetrics::measureStyled(&_tmp_7032, chunk, run_size, _tmp_7033, sp.borrow().italic.clone());
        if !sp.borrow().highlight.is_empty() {
            let __arg_0 = DocxView::highlightColor(&sp.borrow().highlight.clone()).clone();
            self.ink.clone().borrow().rect(x, y - 1, w, ((run_size * 1.25).floor() as i64) + 2, &__arg_0);
        }
        let mut draw_size: f64 = run_size;
        let mut draw_y: i64 = y;
        if sp.borrow().vertAlign == 1 {
            draw_size = run_size * 0.66;
            draw_y = y - ((run_size * 0.05).floor() as i64);
        }
        if sp.borrow().vertAlign == 2 {
            draw_size = run_size * 0.66;
            draw_y = y + ((run_size * 0.45).floor() as i64);
        }
        let mut _tmp_7039 = sp.borrow().bold.clone();
        let mut _tmp_7040 = sp.borrow().italic.clone();
        self.ink.clone().borrow().textStyled(chunk, x, draw_y, draw_size, &col, _tmp_7039, _tmp_7040);
        if sp.borrow().underline {
            let uy: i64 = (y + (run_size.floor() as i64)) + 1;
            self.ink.clone().borrow().rect(x, uy, w, 1, &col);
        }
        if sp.borrow().strike {
            let mut sw: i64 = w;
            if sp.borrow().vertAlign != 0 {
                sw = DocxTextMetrics::measureStyled(
                    &self.ctx.borrow().tr,
                    chunk,
                    draw_size,
                    sp.borrow().bold,
                    sp.borrow().italic
                );
            }
            let sy: i64 = draw_y + ((draw_size * 0.62).floor() as i64);
            self.ink.clone().borrow().rect(x, sy, sw, 1, &col);
        }
        w
    }
    fn hexColor(&self, hex: &str) -> Rc<RefCell<EVGColor>> {
        if (DxNative::str_len(&hex)) != 6 {
            return self.inkColor.clone();
        }
        let r: i64 = self.hexByte(&DxNative::substr(&hex, 0, 2));
        let g: i64 = self.hexByte(&DxNative::substr(&hex, 2, 4));
        let b: i64 = self.hexByte(&DxNative::substr(&hex, 4, 6));
        EVGColor::rgb(r, g, b)
    }
    pub fn hexByte(&self, hh: &str) -> i64 {
        let hi: i64 = self.hexNibble(DxNative::char_at(&hh, 0));
        let lo: i64 = self.hexNibble(DxNative::char_at(&hh, 1));
        hi * 16 + lo
    }
    pub fn hexNibble(&self, ch: i64) -> i64 {
        if (ch >= 48 && ch <= 57) {
            return ch - 48;
        }
        if (ch >= 65 && ch <= 70) {
            return ch - 55;
        }
        if (ch >= 97 && ch <= 102) {
            return ch - 87;
        }
        0
    }
    pub fn raw(&self) -> Vec<u8> {
        return SoftCanvas::raw(&self.canvas);
    }
    pub fn width(&self) -> i64 {
        self.canvas.borrow().w
    }
    pub fn height(&self) -> i64 {
        self.canvas.borrow().h
    }
    pub fn writePng(&self, dir: &str, file: &str) {
        let mut rb: Rc<RefCell<RasterBuffer>> = Rc::new(RefCell::new(RasterBuffer::new()));
        rb.borrow_mut().width = self.width();
        rb.borrow_mut().height = self.height();
        rb.borrow_mut().pixels = self.raw();
        let mut png: Rc<RefCell<PNGEncoder>> = Rc::new(RefCell::new(PNGEncoder::new()));
        PNGEncoder::encode(&png, &rb, dir, file);
    }
    pub fn pngBytes(&self) -> Vec<u8> {
        let mut rb: Rc<RefCell<RasterBuffer>> = Rc::new(RefCell::new(RasterBuffer::new()));
        rb.borrow_mut().width = self.width();
        rb.borrow_mut().height = self.height();
        rb.borrow_mut().pixels = self.raw();
        let mut png: Rc<RefCell<PNGEncoder>> = Rc::new(RefCell::new(PNGEncoder::new()));
        return PNGEncoder::encodeToBuffer(&png, &rb);
    }
}
