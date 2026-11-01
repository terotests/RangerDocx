// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::docx_text_metrics::DocxTextMetrics;
use crate::rgr::{EVGColor, EVGDisplayList, EVGDrawCmd, OfficeText, UIContext, UITextRenderer};

/// ==============================================================================
/// DocxInk.rgr — where a page's marks go: a canvas, or a display list.
/// ==============================================================================
///
/// The viewer painted straight onto a SoftCanvas, and the web host answered with
/// a PNG of it. That works and it is slow in the least interesting way: the
/// server rasterizes a page, encodes it, ships it, and the browser — which has a
/// GPU — decodes a picture it could have drawn itself. The spreadsheet and the
/// slide viewer have not done that for a long time; they build an
/// `EVGDisplayList` and let WebGL draw it.
///
/// The obvious way to add a display list here would have been a second painter
/// beside the first, and then two descriptions of what a Word page looks like,
/// drifting apart at their own pace. So instead the marks go through this: one
/// object with two modes. Every `rect`, `text` and `image` the page painter makes
/// either lands on the canvas or becomes a draw command, and nothing else in the
/// painter changes.
///
/// Text carries its face with it. A display list is measured and drawn by
/// somebody else later, so "bold" has to be on the command rather than implied
/// by whichever face happened to be selected when it was written.
/// ==============================================================================
pub struct DocxInk {
    ctx: Rc<RefCell<UIContext>>,
    dl: Rc<RefCell<EVGDisplayList>>,
    /// false: paint onto the canvas the context is attached to.
    /// true:  append to the display list instead.
    pub toList: bool,
    pub fontFamily: String,
    /// Which way the paragraph being painted runs: -1 work it out from the
    /// text, 0 left to right, 1 right to left. It is state on the ink rather
    /// than an argument on every call for the same reason `fontFamily` is —
    /// it belongs to the paragraph, not to the run, and threading it through
    /// every `text`, `textStyled` and prefix call would be the same value
    /// written eight times.
    ///
    /// It matters where the text alone would answer wrongly: a line that opens
    /// with a Latin brand name and is otherwise Arabic reads left to right by
    /// the algorithm's own rule, and right to left because the author said so.
    pub dir: i64,
}

impl DocxInk {
    pub fn new() -> Self {
        Self {
            ctx: Rc::new(RefCell::new(UIContext::new())),
            dl: Rc::new(RefCell::new(EVGDisplayList::new())),
            toList: false,
            fontFamily: "Open Sans".to_string(),
            dir: -1,
        }
    }
    fn canvasMode(&mut self, c: Rc<RefCell<UIContext>>) {
        self.ctx = c.clone();
        self.toList = false;
    }
    fn listMode(&mut self, c: Rc<RefCell<UIContext>>, out: Rc<RefCell<EVGDisplayList>>) {
        self.ctx = c.clone();
        self.dl = out.clone();
        self.toList = true;
    }
    fn rect(&self, x: i64, y: i64, w: i64, h: i64, col: &Rc<RefCell<EVGColor>>) {
        if w <= 0 {
            return;
        }
        if h <= 0 {
            return;
        }
        if !self.toList {
            UIContext::fillRect(&self.ctx.clone(), x, y, w, h, col);
            return;
        }
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 0;
        c.borrow_mut().x = x as f64;
        c.borrow_mut().y = y as f64;
        c.borrow_mut().w = w as f64;
        c.borrow_mut().h = h as f64;
        c.borrow_mut().r = EVGColor::red(&col);
        c.borrow_mut().g = EVGColor::green(&col);
        c.borrow_mut().b = EVGColor::blue(&col);
        c.borrow_mut().a = EVGColor::alpha(&col);
        self.dl.borrow_mut().cmds.push(c.clone());
    }
    /// An outline. The display list has a BORDER command for exactly this, so a
    /// page frame is one command rather than four rectangles.
    fn strokeRect(&self, x: i64, y: i64, w: i64, h: i64, thickness: i64, col: &Rc<RefCell<EVGColor>>) {
        if !self.toList {
            UIContext::strokeRect(&self.ctx.clone(), x, y, w, h, thickness, col);
            return;
        }
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 1;
        c.borrow_mut().x = x as f64;
        c.borrow_mut().y = y as f64;
        c.borrow_mut().w = w as f64;
        c.borrow_mut().h = h as f64;
        c.borrow_mut().thickness = thickness as f64;
        c.borrow_mut().r = EVGColor::red(&col);
        c.borrow_mut().g = EVGColor::green(&col);
        c.borrow_mut().b = EVGColor::blue(&col);
        c.borrow_mut().a = EVGColor::alpha(&col);
        self.dl.borrow_mut().cmds.push(c.clone());
    }
    /// A run of text at its own size and weight. The face is applied before
    /// measuring either way, so the width recorded on the command is the width
    /// the same renderer would paint.
    fn text(&self, s: &str, x: i64, y: i64, size: f64, col: &Rc<RefCell<EVGColor>>, bold: bool) {
        self.textStyled(s, x, y, size, col, bold, false);
    }
    /// The slant reaches the face here. It used to stop at the model: a span
    /// carried `italic`, the toolbar toggled it, Ctrl+I set it — and this took
    /// only `bold`, so nothing italic in any document was ever drawn slanted.
    /// This is the one place a run of Word text becomes marks, on either path,
    /// which makes it the one place the bidirectional algorithm and the Arabic
    /// shaper have to run. Before this, a right-to-left document was drawn in
    /// the order it was stored — every Arabic and Hebrew line reversed, and
    /// every Arabic letter in the isolated shape it has when it stands alone.
    /// That is not a degraded rendering of the document, it is different text.
    ///
    /// `forDisplay` is applied HERE and nowhere below: the canvas painter and
    /// the display-list backends both draw whatever string reaches them, so
    /// doing it twice would reverse the line back again.
    fn textStyled(&self, s: &str, x: i64, y: i64, size: f64, col: &Rc<RefCell<EVGColor>>, bold: bool, italic: bool) {
        if s.is_empty() {
            return;
        }
        let shown: String = OfficeText::forDisplayDir(s, self.dir.clone());
        DocxTextMetrics::applyStyle(&self.ctx.borrow().tr.clone(), bold, italic);
        if !self.toList {
            UIContext::text(&self.ctx.clone(), &shown, x, y, size, col);
            return;
        }
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 3;
        c.borrow_mut().x = x as f64;
        c.borrow_mut().y = y as f64;
        let _tmp_2394 = UITextRenderer::measureWidth(
            &self.ctx.borrow().tr,
            &shown,
            size
        );
        c.borrow_mut().w = _tmp_2394;
        c.borrow_mut().h = UITextRenderer::lineHeight(&self.ctx.borrow().tr, size);
        c.borrow_mut().text = shown.clone();
        // the family the run was measured in, so a backend that draws by
        // family name (the browser) draws the face the widths came from
        let _tmp_2397 = DocxTextMetrics::family(&self.ctx.borrow().tr.clone());
        c.borrow_mut().fontFamily = _tmp_2397;
        c.borrow_mut().fontSize = size;
        if bold {
            c.borrow_mut().fontWeight = "bold".to_string();
        }
        // How a display list says "slanted" — `EVGDisplayList.addText` writes
        // the same field, and the JSON it serialises to carries it as
        // `"italic":true`. A backend that only ever saw the weight drew every
        // italic run upright, which is the browser half of the same bug.
        if italic {
            c.borrow_mut().textAlign = "italic".to_string();
        }
        c.borrow_mut().r = EVGColor::red(&col);
        c.borrow_mut().g = EVGColor::green(&col);
        c.borrow_mut().b = EVGColor::blue(&col);
        c.borrow_mut().a = EVGColor::alpha(&col);
        self.dl.borrow_mut().cmds.push(c.clone());
    }
    /// A picture, named by its package part. The canvas has the decoded pixels;
    /// a display-list backend fetches the part itself, which is the same contract
    /// the spreadsheet's images use.
    pub fn image(&self, src: String, x: i64, y: i64, w: i64, h: i64) {
        if !self.toList {
            return;
        }
        let mut c: Rc<RefCell<EVGDrawCmd>> = Rc::new(RefCell::new(EVGDrawCmd::new()));
        c.borrow_mut().kind = 2;
        c.borrow_mut().x = x as f64;
        c.borrow_mut().y = y as f64;
        c.borrow_mut().w = w as f64;
        c.borrow_mut().h = h as f64;
        c.borrow_mut().src = src.clone();
        c.borrow_mut().a = 1.0;
        self.dl.borrow_mut().cmds.push(c.clone());
    }
    /// Commands somebody else already made — a chart draws itself into a list of
    /// its own, and in list mode those commands simply join this page's.
    fn append(&self, other: &Rc<RefCell<EVGDisplayList>>) {
        let mut i: i64 = 0;
        while i < (other.borrow().cmds.len() as i64) {
            self.dl.borrow_mut().cmds.push(other.borrow().cmds[i as usize].clone());
            i += 1;
        }
    }
}
