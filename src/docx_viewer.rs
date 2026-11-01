// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::clipboard_table::ClipboardTable;
use crate::docx_edit_controller::DocxEditController;
use crate::docx_layout::LaidLine;
use crate::docx_layout::LaidLineKind;
use crate::docx_layout::LaidPage;
use crate::docx_package::DocxPackage;
use crate::docx_view::DocxView;
use crate::docx_writer::DocxWriter;
use crate::rich_document::DocChart;
use crate::rich_document::DocDrawing;
use crate::rich_document::DocImage;
use crate::rich_document::Paragraph;
use crate::rich_document::RichDocument;
use crate::word_ml_parser::WordMlParser;
use crate::word_numbering::WordNumbering;
use crate::rgr::{ChartDoc, ChartDocCodec, DocumentPosition, DocumentRange};

/// ==============================================================================
/// DocxViewer - load .docx → RichDocument → SoftCanvas pages (+ optional edit).
/// ==============================================================================
#[derive(Clone)]
pub struct DocxViewer {
    pub parser: Rc<RefCell<WordMlParser>>,
    pub view: Rc<RefCell<DocxView>>,
    pub doc: Rc<RefCell<RichDocument>>,
    pub numbering: Rc<RefCell<WordNumbering>>,
    pub edit: Rc<RefCell<DocxEditController>>,
    pub lastError: String,
    pub currentPage: i64,
    /// How far down the current page the window is looking, in scene pixels.
    ///
    /// The viewer had no such thing: a scroll gesture only ever TURNED PAGES,
    /// so a document could not be read down. A trackpad flick jumped a whole
    /// page and everything between the two positions went past unseen, which is
    /// not slow scrolling — it is no scrolling at all.
    pub scrollY: i64,
    /// How tall the window is that is looking at the page. The host sets it;
    /// without it there is nothing to say how far down a page can be scrolled
    /// before the bottom of the paper is on screen.
    pub viewportH: i64,
    /// The document as a .docx. A document that was opened from a file keeps
    /// every part and every paragraph it did not change exactly as it was read
    /// (DocxWriter). `saveWarnings` lists what could not be written.
    pub saveWarnings: Vec<String>,
    /// --- resizing a drawing by its corners ------------------------------------
    /// The frame the selected drawing gets has a handle on each corner, and a
    /// press on one starts a drag that sets the drawing's width — the height
    /// follows, because a diagram stretched out of proportion is a different
    /// diagram. `drawingHandleAt` says which corner, if any, is under a point;
    /// the app routes the press, the moves and the release here.
    pub dragDrawingId: i64,
    pub dragCorner: i64,
    pub dragStartW: i64,
    pub dragStartH: i64,
    pub dragStartX: i64,
    pub dragStartY: i64,
}

impl DocxViewer {
    pub fn new() -> Self {
        Self {
            parser: Rc::new(RefCell::new(WordMlParser::new())),
            view: Rc::new(RefCell::new(DocxView::new())),
            doc: Rc::new(RefCell::new(RichDocument::new())),
            numbering: Rc::new(RefCell::new(WordNumbering::new())),
            edit: Rc::new(RefCell::new(DocxEditController::new())),
            lastError: String::new(),
            currentPage: 0,
            scrollY: 0,
            viewportH: 0,
            saveWarnings: Vec::new(),
            dragDrawingId: 0,
            dragCorner: 0,
            dragStartW: 0,
            dragStartH: 0,
            dragStartX: 0,
            dragStartY: 0,
        }
    }
    /// Which corner handle of the selected drawing's frame is under the point:
    /// 1 top-left, 2 top-right, 3 bottom-left, 4 bottom-right, 0 for none. A
    /// handle is eight pixels square and takes a press from two more around
    /// it, which is what makes the corner of a drawing something a pointer
    /// can find.
    /// How far apart two integers are. Ranger has `fabs` for doubles and
    /// nothing for ints, and a hit test on pixels wants ints.
    pub fn away(a: i64, b: i64) -> i64 {
        if a < b {
            return b - a;
        }
        a - b
    }
    pub fn init(&self, font_dir: &str) {
        self.view.clone().borrow().init(font_dir);
    }
    pub fn openFile(&mut self, dir: &str, file: String) -> bool {
        self.lastError = String::new();
        let mut pkg: Rc<RefCell<DocxPackage>> = Rc::new(RefCell::new(DocxPackage::new()));
        if !pkg.borrow_mut().open(dir.to_string(), file.clone()) {
            self.lastError = format!("failed to open {}", file);
            return false;
        }
        if !pkg.borrow().hasPart("word/document.xml") {
            self.lastError = "missing word/document.xml".to_string();
            pkg.borrow_mut().close();
            return false;
        }
        self.doc = self.parser.borrow_mut().parsePackage(pkg.clone());
        self.numbering = self.parser.borrow().numbering.clone();
        pkg.borrow_mut().close();
        if self.doc.borrow().title.is_empty() {
            self.doc.borrow_mut().title = file.clone();
        }
        let mut _tmp_6978 = self.doc.clone();
        self.edit.clone().borrow_mut().bindDocument(_tmp_6978);
        self.edit.borrow_mut().numbering = self.numbering.clone();
        // A different document: nothing the layout holds is about this one.
        self.view.borrow().layout.clone().borrow_mut().invalidate();
        self.currentPage = 0;
        true
    }
    /// A document the caller already has, rather than one read from a package.
    /// The bench builds one in memory, and so does anything that wants to drive
    /// this viewer without a file behind it.
    pub fn attachDocument(&mut self, d: Rc<RefCell<RichDocument>>) {
        self.doc = d.clone();
        self.numbering = Rc::new(RefCell::new(WordNumbering::new()));
        if self.doc.borrow().title.is_empty() {
            self.doc.borrow_mut().title = "untitled".to_string();
        }
        let mut _tmp_6984 = self.doc.clone();
        self.edit.clone().borrow_mut().bindDocument(_tmp_6984);
        self.edit.borrow_mut().numbering = self.numbering.clone();
        // A different document: nothing the layout holds is about this one.
        self.view.borrow().layout.clone().borrow_mut().invalidate();
        self.currentPage = 0;
        self.renderPage(0);
    }
    pub fn saveBytes(&mut self) -> Vec<u8> {
        let mut w: Rc<RefCell<DocxWriter>> = Rc::new(RefCell::new(DocxWriter::new()));
        w.borrow_mut().resolver = self.parser.borrow().resolver.clone();
        w.borrow_mut().numbering = self.numbering.clone();
        let mut b: Vec<u8> = w.borrow_mut().toBuffer(&self.doc.clone());
        self.saveWarnings = w.borrow().warnings.clone();
        b
    }
    pub fn saveFile(&mut self, dir: &str, file: &str) {
        let mut w: Rc<RefCell<DocxWriter>> = Rc::new(RefCell::new(DocxWriter::new()));
        w.borrow_mut().resolver = self.parser.borrow().resolver.clone();
        w.borrow_mut().numbering = self.numbering.clone();
        w.borrow_mut().save(&self.doc.clone(), dir, file);
        self.saveWarnings = w.borrow().warnings.clone();
    }
    /// The same, from bytes. `openFile` is this with a file read in front of it.
    pub fn openBytes(&mut self, data: &[u8], name: String) -> bool {
        self.lastError = String::new();
        let mut pkg: Rc<RefCell<DocxPackage>> = Rc::new(RefCell::new(DocxPackage::new()));
        if !pkg.borrow_mut().openBytes(data, name.clone()) {
            self.lastError = format!("failed to open {}", name);
            return false;
        }
        if !pkg.borrow().hasPart("word/document.xml") {
            self.lastError = "missing word/document.xml".to_string();
            pkg.borrow_mut().close();
            return false;
        }
        self.doc = self.parser.borrow_mut().parsePackage(pkg.clone());
        self.numbering = self.parser.borrow().numbering.clone();
        pkg.borrow_mut().close();
        if self.doc.borrow().title.is_empty() {
            self.doc.borrow_mut().title = name.clone();
        }
        let mut _tmp_6998 = self.doc.clone();
        self.edit.clone().borrow_mut().bindDocument(_tmp_6998);
        self.edit.borrow_mut().numbering = self.numbering.clone();
        // A different document: nothing the layout holds is about this one.
        self.view.borrow().layout.clone().borrow_mut().invalidate();
        self.currentPage = 0;
        true
    }
    /// A document another format's reader built (DocxFormats: .odt, .rtf). It
    /// has no .docx package behind it, so a save writes a fresh one; the
    /// parser's style table is reset so that nothing from a previously opened
    /// file leaks into it.
    pub fn adoptForeign(&mut self, d: Rc<RefCell<RichDocument>>, nb: Rc<RefCell<WordNumbering>>, name: String) {
        self.parser = Rc::new(RefCell::new(WordMlParser::new()));
        self.doc = d.clone();
        self.numbering = nb.clone();
        if self.doc.borrow().title.is_empty() {
            self.doc.borrow_mut().title = name.clone();
        }
        let mut _tmp_7004 = self.doc.clone();
        self.edit.clone().borrow_mut().bindDocument(_tmp_7004);
        self.edit.borrow_mut().numbering = self.numbering.clone();
        self.view.borrow().layout.clone().borrow_mut().invalidate();
        self.currentPage = 0;
    }
    /// The page as an EVGDisplayList, for a backend that draws rather than
    /// receives a picture. Same layout and same painting as `renderPage`.
    fn pageDisplayList(&mut self, page: i64) -> Rc<RefCell<EVGDisplayList>> {
        let mut _tmp_7009 = self.doc.clone();
        let mut _tmp_7010 = self.numbering.clone();
        let mut _tmp_7011 = self.edit.clone();
        let _tmp_7012 = self.edit.borrow().isEnabled();
        let mut dl: Rc<RefCell<EVGDisplayList>> = self.view.borrow_mut().buildDisplayList(&_tmp_7009, page, _tmp_7010, &_tmp_7011, _tmp_7012);
        self.currentPage = self.view.borrow().pageIndex;
        dl
    }
    pub fn sceneJson(&mut self, page: i64) -> String {
        let mut dl: Rc<RefCell<EVGDisplayList>> = self.pageDisplayList(page);
        let body: String = EVGDisplayList::to_json(&dl);
        return format!("{}{},\"height\":{},\"list\":{}{}", "{\"width\":".to_string(), EVGDisplayList::num((self.view.borrow().sceneWidth()) as f64), EVGDisplayList::num((self.view.borrow().sceneHeight()) as f64), body, "}".to_string());
    }
    /// Every image part this document carries, and their bytes — a host with no
    /// file server hands them to the renderer itself.
    pub fn imagePartsJson(&self) -> String {
        let mut names: Vec<String> = Vec::new();
        let mut i: i64 = 0;
        while i < self.doc.borrow().paragraphCount() {
            let mut p: Rc<RefCell<Paragraph>> = self.doc.borrow().paragraphAt(i);
            let mut j: i64 = 0;
            while j < (p.borrow().images.len() as i64) {
                let mut im: Rc<RefCell<DocImage>> = p.borrow().images[j as usize].clone();
                if !im.borrow().bytes.is_empty() {
                    let mut seen: bool = false;
                    let mut k: i64 = 0;
                    while k < (names.len() as i64) {
                        if names[k as usize].clone() == im.borrow().partPath {
                            seen = true;
                        }
                        k += 1;
                    }
                    if !seen {
                        names.push(im.borrow().partPath.clone());
                    }
                }
                j += 1;
            }
            i += 1;
        }
        let mut out: String = "[".to_string();
        let mut n: i64 = 0;
        while n < (names.len() as i64) {
            if n > 0 {
                out = format!("{},", out);
            }
            out = format!("{}\"{}\"", out, names[n as usize].clone());
            n += 1;
        }
        format!("{}]", out)
    }
    pub fn imageBytes(&self, part: &str) -> Vec<u8> {
        let mut i: i64 = 0;
        while i < self.doc.borrow().paragraphCount() {
            let mut p: Rc<RefCell<Paragraph>> = self.doc.borrow().paragraphAt(i);
            let mut j: i64 = 0;
            while j < (p.borrow().images.len() as i64) {
                let mut im: Rc<RefCell<DocImage>> = p.borrow().images[j as usize].clone();
                if im.borrow().partPath == part {
                    return im.borrow().bytes.clone();
                }
                j += 1;
            }
            i += 1;
        }
        vec![0u8; 0]
    }
    pub fn document(&self) -> Rc<RefCell<RichDocument>> {
        self.doc.clone()
    }
    pub fn editor(&self) -> Rc<RefCell<DocxEditController>> {
        self.edit.clone()
    }
    pub fn setEditMode(&self, on: bool) {
        self.edit.clone().borrow_mut().setEnabled(on);
    }
    pub fn editMode(&self) -> bool {
        return self.edit.borrow().isEnabled();
    }
    /// present() silently clamps an out-of-range page to the last one. Adopting
    /// the page it actually drew keeps currentPage from ever naming a page that
    /// does not exist — otherwise page N and a phantom page N+1 both report
    /// success while showing the same content.
    pub fn renderPage(&mut self, page: i64) {
        if self.edit.borrow().isEnabled() {
            let mut _tmp_7024 = self.doc.clone();
            let mut _tmp_7025 = self.numbering.clone();
            let mut _tmp_7026 = self.edit.clone();
            self.view.clone().borrow_mut().presentWithEdit(&_tmp_7024, page, _tmp_7025, &_tmp_7026);
        } else {
            let mut _tmp_7029 = self.doc.clone();
            let mut _tmp_7030 = self.numbering.clone();
            self.view.clone().borrow_mut().present(&_tmp_7029, page, _tmp_7030);
        }
        self.currentPage = self.view.borrow().pageIndex;
    }
    pub fn renderAll(&self, max_pages: i64) {
        let mut _tmp_7034 = self.doc.clone();
        let mut _tmp_7035 = self.numbering.clone();
        self.view.clone().borrow_mut().presentAll(&_tmp_7034, _tmp_7035, max_pages);
    }
    pub fn pageCount(&self) -> i64 {
        return (self.view.borrow().layout).borrow().pageCount();
    }
    pub fn pageWidth(&self) -> i64 {
        return self.view.borrow().width();
    }
    pub fn pageHeight(&self) -> i64 {
        return self.view.borrow().height();
    }
    pub fn writePng(&self, dir: &str, file: &str) {
        self.view.clone().borrow().writePng(dir, file);
    }
    pub fn pngBytes(&self) -> Vec<u8> {
        return self.view.borrow().pngBytes();
    }
    pub fn plainText(&self) -> String {
        return self.doc.borrow().plainText();
    }
    pub fn inspectJson(&self) -> String {
        return self.doc.borrow().inspectJson();
    }
    /// --- editing surface for window host ---
    pub fn clickAt(&mut self, x: i64, y: i64, extend_selection: bool) {
        if !self.edit.borrow().isEnabled() {
            return;
        }
        self.renderPage(self.currentPage.clone());
        let mut _tmp_7047 = self.view.borrow().layout.clone();
        let mut pos: Rc<RefCell<DocumentPosition>> = self.edit.borrow().hitTest(&_tmp_7047, self.currentPage.clone(), x, y);
        if pos.borrow().paragraphId <= 0 {
            return;
        }
        if extend_selection {
            if self.edit.borrow().isActive() {
                // Move the FOCUS and keep the anchor, whichever paragraph the
                // point landed in. This used to give up when the two ends were
                // in different paragraphs and collapse to a caret instead —
                // which is a selection you cannot make across a paragraph
                // break, by shift-clicking or by dragging, even though
                // `orderedEnds` and every edit below it handle a multi-
                // paragraph range perfectly well.
                let mut _tmp_7051 = pos.borrow().paragraphId.clone();
                let mut _tmp_7052 = pos.borrow().textOffset.clone();
                self.edit.clone().borrow_mut().extendTo(_tmp_7051, _tmp_7052);
            } else {
                let mut _tmp_7055 = pos.borrow().paragraphId.clone();
                let mut _tmp_7056 = pos.borrow().textOffset.clone();
                self.edit.clone().borrow_mut().setCaret(_tmp_7055, _tmp_7056);
            }
        } else {
            let mut _tmp_7059 = pos.borrow().paragraphId.clone();
            let mut _tmp_7060 = pos.borrow().textOffset.clone();
            self.edit.clone().borrow_mut().setCaret(_tmp_7059, _tmp_7060);
        }
        // The toolbar describes the run the caret is in, so the caret has to
        // pick that up when it moves somewhere new.
        self.edit.clone().borrow_mut().adoptRunAtCaret();
        self.renderPage(self.currentPage.clone());
    }
    /// The pointer moving with the button held: the same thing as a shift-click,
    /// and the reason a drag paints a selection rather than only ever moving the
    /// caret. Nothing used to call this, because the host never said the button
    /// was still down and the app never asked.
    pub fn dragTo(&mut self, x: i64, y: i64) {
        if !self.edit.borrow().isEnabled() {
            return;
        }
        if !self.edit.borrow().isActive() {
            return;
        }
        self.renderPage(self.currentPage.clone());
        let mut _tmp_7069 = self.view.borrow().layout.clone();
        let mut pos: Rc<RefCell<DocumentPosition>> = self.edit.borrow().hitTest(&_tmp_7069, self.currentPage.clone(), x, y);
        if pos.borrow().paragraphId <= 0 {
            return;
        }
        let mut before: Rc<RefCell<DocumentPosition>> = self.edit.borrow().caret();
        if before.borrow()
            .paragraphId == pos.borrow()
            .paragraphId && before.borrow()
            .textOffset == pos.borrow()
            .textOffset {
            return;
        }
        let mut _tmp_7073 = pos.borrow().paragraphId.clone();
        let mut _tmp_7074 = pos.borrow().textOffset.clone();
        self.edit.clone().borrow_mut().extendTo(_tmp_7073, _tmp_7074);
        self.renderPage(self.currentPage.clone());
    }
    pub fn typeText(&mut self, text: &str) {
        if !self.edit.borrow().isEnabled() {
            return;
        }
        self.edit.clone().borrow_mut().insertText(text.to_string());
        self.renderPage(self.currentPage.clone());
    }
    pub fn keyBackspace(&mut self) {
        self.edit.clone().borrow_mut().backspace();
        self.renderPage(self.currentPage.clone());
    }
    pub fn keyDelete(&mut self) {
        self.edit.clone().borrow_mut().deleteForward();
        self.renderPage(self.currentPage.clone());
    }
    pub fn keyEnter(&mut self) {
        self.edit.clone().borrow_mut().splitAtCaret();
        self.renderPage(self.currentPage.clone());
    }
    /// --- caret movement -------------------------------------------------------
    /// Layout lives in the view, so every movement re-lays out first (cheap:
    /// renderPage already does it) and then follows the caret onto its page, so
    /// Up / Down / PageUp / PageDown walk across page boundaries.
    pub fn pageOfCaret(&self) -> i64 {
        let mut cur: Vec<i64> = self.edit.borrow().caretLineAt(&self.view.borrow().layout.clone());
        let cp: i64 = cur[0];
        if cp < 0 {
            return self.currentPage;
        }
        cp
    }
    pub fn followCaret(&mut self) {
        let target: i64 = self.pageOfCaret();
        if target != self.currentPage {
            self.currentPage = target;
        }
        self.renderPage(self.currentPage.clone());
    }
    /// --- reading rather than editing -----------------------------------------
    /// Every movement below moves a CARET, and `edit` does nothing at all when
    /// editing is off. That is the whole of "PageDown does not go to the next
    /// page": in a document you are only reading there is no caret to move, and
    /// nobody had said what the key should do instead.
    ///
    /// What it should do is what it does in every reader — turn the page. The
    /// keys are the same keys and the hosts already send them, so none of them
    /// has to learn anything; the key simply now means something on both sides
    /// of the edit switch.
    ///
    /// The viewer draws one page at a time, so paging IS the scroll here. A
    /// continuous ribbon of pages is a different layout and a bigger change
    /// than this; what a reader needs first is for the key to do something.
    pub fn goToPage(&mut self, p: i64) -> bool {
        let mut want: i64 = p;
        let n: i64 = self.pageCount();
        if want < 0 {
            want = 0;
        }
        if n > 0 {
            if want > n - 1 {
                want = n - 1;
            }
        }
        if want == self.currentPage {
            return false;
        }
        // A page arrived at by a button or a key starts at its top. `scrollBy`
        // sets its own position afterwards, which is what lets a gesture carry
        // travel across the boundary instead of landing at the top of the next
        // page whatever the reader did.
        self.scrollY = 0;
        self.renderPage(want);
        true
    }
    pub fn turnPage(&mut self, delta: i64) -> bool {
        self.goToPage(self.currentPage + delta)
    }
    /// How far the page can be scrolled before its bottom edge is on screen.
    /// Zero for a page shorter than the window, which then does not scroll at
    /// all and turns on the first gesture instead.
    pub fn maxScrollY(&self) -> i64 {
        if self.viewportH <= 0 {
            return 0;
        }
        let h: i64 = self.view.borrow().sceneHeight();
        if h <= self.viewportH {
            return 0;
        }
        h - self.viewportH
    }
    /// A wheel notch or a swipe, in scene pixels, positive downwards.
    ///
    /// The page scrolls, and when it runs out the next one begins — which is
    /// how every reader of paged documents behaves and what this did not do.
    /// Travel past the bottom of a page is not thrown away: it carries into the
    /// top of the next one, so a long flick crosses a page boundary without
    /// stopping dead at it and without skipping the paper in between.
    ///
    /// A page shorter than the window has nowhere to scroll, so the first
    /// gesture turns it. That is the old behaviour, and it is still right for
    /// the case it was written for.
    pub fn scrollBy(&mut self, dy: i64) -> bool {
        if dy == 0 {
            return false;
        }
        let mut moved: bool = false;
        let mut want: i64 = self.scrollY + dy;
        let mut guard: i64 = 0;
        while guard < 64 {
            guard += 1;
            let max_y: i64 = self.maxScrollY();
            if want > max_y {
                let over: i64 = want - max_y;
                if self.turnPage(1) {
                    moved = true;
                    // The travel left over is spent on the NEW page, and only
                    // as far as the new page can take it. Without that clamp a
                    // page with nowhere to scroll — one shorter than the
                    // window — hands the whole gesture straight to the page
                    // after it, and one flick runs through the entire
                    // document.
                    let room: i64 = self.maxScrollY();
                    let mut rest: i64 = over - 1;
                    if rest > room {
                        rest = room;
                    }
                    want = rest;
                    self.scrollY = 0;
                } else {
                    want = max_y;
                    guard = 64;
                }
            } else {
                if want < 0 {
                    if self.turnPage(-1) {
                        moved = true;
                        let new_max: i64 = self.maxScrollY();
                        let mut back: i64 = (new_max + want) + 1;
                        if back < 0 {
                            back = 0;
                        }
                        want = back;
                        self.scrollY = new_max;
                    } else {
                        want = 0;
                        guard = 64;
                    }
                } else {
                    guard = 64;
                }
            }
        }
        let max_end: i64 = self.maxScrollY();
        if want > max_end {
            want = max_end;
        }
        if want < 0 {
            want = 0;
        }
        if want != self.scrollY {
            moved = true;
        }
        self.scrollY = want;
        moved
    }
    pub fn keyLeft(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            self.turnPage(-1);
            return;
        }
        self.edit.clone().borrow_mut().moveLeft(extend);
        self.followCaret();
    }
    /// Ctrl+Left / Ctrl+Right. The word rule is `OfficeCaretMotion`'s, shared
    /// with the slide editor, so a document and a deck jump the same gaps.
    pub fn keyWordLeft(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            return;
        }
        self.edit.clone().borrow_mut().moveWordLeft(extend);
        self.followCaret();
    }
    pub fn keyWordRight(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            return;
        }
        self.edit.clone().borrow_mut().moveWordRight(extend);
        self.followCaret();
    }
    pub fn keyRight(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            self.turnPage(1);
            return;
        }
        self.edit.clone().borrow_mut().moveRight(extend);
        self.followCaret();
    }
    pub fn keyUp(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            self.turnPage(-1);
            return;
        }
        let mut _tmp_7108 = self.view.borrow().layout.clone();
        self.edit.clone().borrow_mut().moveUp(&_tmp_7108, extend);
        self.followCaret();
    }
    pub fn keyDown(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            self.turnPage(1);
            return;
        }
        let mut _tmp_7112 = self.view.borrow().layout.clone();
        self.edit.clone().borrow_mut().moveDown(&_tmp_7112, extend);
        self.followCaret();
    }
    pub fn keyPageUp(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            self.turnPage(-1);
            return;
        }
        let mut _tmp_7116 = self.view.borrow().layout.clone();
        self.edit.clone().borrow_mut().pageUp(&_tmp_7116, extend);
        self.followCaret();
    }
    pub fn keyPageDown(&mut self, extend: bool) {
        if !self.edit.borrow().isEnabled() {
            self.turnPage(1);
            return;
        }
        let mut _tmp_7120 = self.view.borrow().layout.clone();
        self.edit.clone().borrow_mut().pageDown(&_tmp_7120, extend);
        self.followCaret();
    }
    pub fn keyHome(&mut self, extend: bool, ctrl: bool) {
        if !self.edit.borrow().isEnabled() {
            self.goToPage(0);
            return;
        }
        if ctrl {
            self.edit.clone().borrow_mut().moveDocEdge(false, extend);
        } else {
            let mut _tmp_7126 = self.view.borrow().layout.clone();
            self.edit.clone().borrow_mut().moveLineEdge(&_tmp_7126, false, extend);
        }
        self.followCaret();
    }
    pub fn keyEnd(&mut self, extend: bool, ctrl: bool) {
        if !self.edit.borrow().isEnabled() {
            let __arg_0 = self.pageCount() - 1;
            self.goToPage(__arg_0);
            return;
        }
        if ctrl {
            self.edit.clone().borrow_mut().moveDocEdge(true, extend);
        } else {
            let mut _tmp_7133 = self.view.borrow().layout.clone();
            self.edit.clone().borrow_mut().moveLineEdge(&_tmp_7133, true, extend);
        }
        self.followCaret();
    }
    /// Tab / Shift+Tab: next / previous table cell, selecting its contents the
    /// way Word does. Outside a table it steps into an adjacent one if there is
    /// one; it does not insert a tab character (no tab-stop rendering yet).
    pub fn keyTab(&mut self, backward: bool) -> bool {
        if !self.edit.borrow().isEnabled() {
            return false;
        }
        let mut moved: bool = false;
        if backward {
            moved = self.edit.borrow_mut().tabCell(false);
        } else {
            moved = self.edit.borrow_mut().tabCell(true);
        }
        // Tab at the start of a list item moves it a level, as in Word.
        if !moved {
            if DocumentRange::isCollapsed(&self.edit.borrow().sel) && self.edit.borrow()
                .sel.borrow()
                .focus.borrow()
                .textOffset == 0 {
                let mut p: Rc<RefCell<Paragraph>> = self.edit.borrow().caretParagraph();
                if p.borrow().listNumId > 0 {
                    moved = self.edit.borrow().formatParagraphs(&if backward { "outdent".to_string() } else { "indent".to_string() }, "", &self.parser.borrow().resolver);
                    self.renderPage(self.currentPage.clone());
                }
            }
        }
        if moved {
            self.followCaret();
        }
        moved
    }
    pub fn caretInTable(&self) -> bool {
        return self.edit.borrow().inTable();
    }
    pub fn keySelectAll(&mut self) {
        self.edit.clone().borrow_mut().selectAll();
        self.renderPage(self.currentPage.clone());
    }
    /// --- clipboard out --------------------------------------------------------
    pub fn copySelection(&self) -> String {
        return self.edit.borrow().selectedText();
    }
    pub fn cutSelection(&mut self) -> String {
        let text: String = self.edit.borrow().selectedText();
        if !text.is_empty() {
            self.edit.clone().borrow_mut().deleteSelection();
            self.followCaret();
        }
        text
    }
    pub fn setPage(&mut self, page: i64) -> i64 {
        self.renderPage(page);
        self.currentPage
    }
    pub fn keyUndo(&mut self) {
        self.edit.clone().borrow_mut().undo();
        self.renderPage(self.currentPage.clone());
    }
    pub fn keyRedo(&mut self) {
        self.edit.clone().borrow_mut().redo();
        self.renderPage(self.currentPage.clone());
    }
    /// Clipboard paste from the host. `html` is the text/html flavour (what a
    /// spreadsheet copy puts next to the TSV) and lands as a real table; `text`
    /// is the text/plain fallback. Returns true when something was inserted.
    pub fn pasteClipboard(&mut self, html: &str, text: &str) -> bool {
        if !self.edit.borrow().isEnabled() {
            return false;
        }
        if !self.edit.borrow().isActive() {
            return false;
        }
        // A chart first: the HTML a chart copy puts on the clipboard is an
        // <img> carrying the chart's whole Vela document as an attribute. Read
        // as a table it would be nothing; read as a picture it would be dead
        // pixels. Read as what it is, it stays a chart.
        if !html.is_empty() {
            let mut cdoc: Rc<RefCell<ChartDoc>> = ChartDocCodec::fromHtml(html);
            if cdoc.borrow().ok {
                let json: String = ChartDocCodec::jsonFromHtml(html);
                let mut _tmp_7155 = cdoc.borrow().chart.borrow().w.clone();
                let made: i64 = self.edit.borrow_mut().pasteChart(&json, _tmp_7155, cdoc.borrow().chart.borrow().h.clone());
                if made > 0 {
                    self.view.borrow_mut().selectedChartId = made;
                    self.renderPage(self.currentPage.clone());
                    return true;
                }
            }
        }
        let mut tbl: Rc<RefCell<ClipboardTable>> = Rc::new(RefCell::new(ClipboardTable::new()));
        let mut got: bool = false;
        if !html.is_empty() {
            got = tbl.borrow_mut().parseHtml(html);
        }
        if !got {
            if ClipboardTable::looksTabular(text) {
                got = tbl.borrow_mut().parseTsv(text);
            }
        }
        if got {
            let done: bool = self.edit.borrow_mut().pasteTable(&tbl);
            self.renderPage(self.currentPage.clone());
            return done;
        }
        let typed: bool = self.edit.borrow_mut().pasteText(text);
        self.renderPage(self.currentPage.clone());
        typed
    }
    /// --- charts in the document -----------------------------------------------
    /// Everything a host needs to make a pasted chart editable: find one under
    /// the pointer, select it, and change it. Each edit regenerates the chart's
    /// specification from its own numbers, so what is drawn afterwards is a real
    /// chart of that kind rather than the old picture with a new label.
    pub fn chartAtPoint(&self, x: i64, y: i64) -> i64 {
        let mut pg: Rc<RefCell<LaidPage>> = (self.view.borrow().layout).borrow().pageAt(self.view.borrow().pageIndex.clone());
        let mut i: i64 = 0;
        while i < (pg.borrow().lines.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = pg.borrow().lines[i as usize].clone();
            if line.borrow().kind == LaidLineKind::chart() {
                if x >= line.borrow().x {
                    if x <= line.borrow().x + line.borrow().width {
                        if y >= line.borrow().y {
                            if y <= line.borrow().y + line.borrow().height {
                                return line.borrow().chartId;
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        0
    }
    pub fn selectChart(&mut self, id: i64) {
        self.view.borrow_mut().selectedChartId = id;
        self.renderPage(self.currentPage.clone());
    }
    pub fn selectedChart(&self) -> i64 {
        self.view.borrow().selectedChartId
    }
    /// Click-to-select, so a chart behaves like the pictures beside it. Returns
    /// true when the click landed on one, so the host knows the caret should not
    /// move there.
    pub fn clickChart(&mut self, x: i64, y: i64) -> bool {
        let id: i64 = self.chartAtPoint(x, y);
        if id == 0 {
            if self.view.borrow().selectedChartId != 0 {
                self.view.borrow_mut().selectedChartId = 0;
                self.renderPage(self.currentPage.clone());
            }
            return false;
        }
        self.selectChart(id);
        true
    }
    pub fn editChart(&mut self, what: &str, arg: &str) -> bool {
        let id: i64 = self.view.borrow().selectedChartId;
        if id == 0 {
            return false;
        }
        let done: bool = self.edit.borrow().editChart(id, what, arg.to_string());
        if done {
            // The render cache is keyed by chart id and box, and the chart it
            // describes has changed, so the old picture has to go.
            GridChartView::forget(&self.view.borrow().charts.clone(), id);
            self.renderPage(self.currentPage.clone());
        }
        done
    }
    pub fn deleteChart(&mut self) -> bool {
        let id: i64 = self.view.borrow().selectedChartId;
        if id == 0 {
            return false;
        }
        let done: bool = self.edit.borrow().deleteChart(id);
        if done {
            GridChartView::forget(&self.view.borrow().charts.clone(), id);
            self.view.borrow_mut().selectedChartId = 0;
            self.renderPage(self.currentPage.clone());
        }
        done
    }
    /// --- drawings in the document --------------------------------------------
    /// A drawing behaves like a chart to the pointer: click to select, and a
    /// frame says so. What can be done to one is smaller — it is geometry made
    /// elsewhere, so it can be made bigger or smaller, and drawn again from new
    /// source by whoever drew it — and `DocDrawing` carries what that takes.
    pub fn drawingAtPoint(&self, x: i64, y: i64) -> i64 {
        let mut pg: Rc<RefCell<LaidPage>> = (self.view.borrow().layout).borrow().pageAt(self.view.borrow().pageIndex.clone());
        let mut i: i64 = 0;
        while i < (pg.borrow().lines.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = pg.borrow().lines[i as usize].clone();
            if line.borrow().kind == LaidLineKind::drawing() {
                if x >= line.borrow().x {
                    if x <= line.borrow().x + line.borrow().width {
                        if y >= line.borrow().y {
                            if y <= line.borrow().y + line.borrow().height {
                                return line.borrow().drawingId;
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        0
    }
    pub fn selectDrawing(&mut self, id: i64) {
        self.view.borrow_mut().selectedDrawingId = id;
        self.renderPage(self.currentPage.clone());
    }
    pub fn selectedDrawing(&self) -> i64 {
        self.view.borrow().selectedDrawingId
    }
    pub fn clickDrawing(&mut self, x: i64, y: i64) -> bool {
        let id: i64 = self.drawingAtPoint(x, y);
        if id == 0 {
            if self.view.borrow().selectedDrawingId != 0 {
                self.view.borrow_mut().selectedDrawingId = 0;
                self.renderPage(self.currentPage.clone());
            }
            return false;
        }
        self.selectDrawing(id);
        true
    }
    /// The laid-out line of a drawing on the page the viewer is on, or a line
    /// with no drawing in it when the drawing is on another page.
    pub fn drawingLine(&self, id: i64) -> Rc<RefCell<LaidLine>> {
        let mut pg: Rc<RefCell<LaidPage>> = (self.view.borrow().layout).borrow().pageAt(self.view.borrow().pageIndex.clone());
        let mut i: i64 = 0;
        while i < (pg.borrow().lines.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = pg.borrow().lines[i as usize].clone();
            if line.borrow().kind == LaidLineKind::drawing() {
                if line.borrow().drawingId == id {
                    return line;
                }
            }
            i += 1;
        }
        Rc::new(RefCell::new(LaidLine::new()))
    }
    pub fn drawingHandleAt(&self, x: i64, y: i64) -> i64 {
        if self.view.borrow().selectedDrawingId == 0 {
            return 0;
        }
        let mut line: Rc<RefCell<LaidLine>> = self.drawingLine(self.view.borrow().selectedDrawingId.clone());
        if line.borrow().drawingId == 0 {
            return 0;
        }
        let mut k: i64 = 1;
        while k <= 4 {
            let hx: i64 = DocxView::handleX(&line, k);
            let hy: i64 = DocxView::handleY(&line, k);
            if DocxViewer::away(x, hx) <= 6 {
                if DocxViewer::away(y, hy) <= 6 {
                    return k;
                }
            }
            k += 1;
        }
        0
    }
    /// A press on a handle: remember where the drag starts from, so every
    /// move sets the size from the press rather than from the move before it
    /// and a jittery hand does not accumulate.
    pub fn beginDrawingDrag(&mut self, x: i64, y: i64) -> bool {
        let k: i64 = self.drawingHandleAt(x, y);
        if k == 0 {
            return false;
        }
        let mut line: Rc<RefCell<LaidLine>> = self.drawingLine(self.view.borrow().selectedDrawingId.clone());
        self.dragDrawingId = self.view.borrow().selectedDrawingId;
        self.dragCorner = k;
        self.dragStartW = line.borrow().width;
        self.dragStartH = line.borrow().height;
        self.dragStartX = x;
        self.dragStartY = y;
        true
    }
    pub fn draggingDrawing(&self) -> bool {
        self.dragDrawingId != 0
    }
    /// The pointer moved with a handle held. Whichever axis the hand moved
    /// further along decides the new width — a drag straight down on the
    /// bottom-right corner grows the drawing too — and the height follows the
    /// drawing's own proportion. The left-hand corners grow it leftwards,
    /// which on a drawing that starts at the margin means the same thing.
    pub fn dragDrawingTo(&mut self, x: i64, y: i64) -> bool {
        if self.dragDrawingId == 0 {
            return false;
        }
        if self.dragStartW <= 0 {
            return false;
        }
        let mut dx: i64 = x - self.dragStartX;
        let mut dy: i64 = y - self.dragStartY;
        if self.dragCorner == 1 || self.dragCorner == 3 {
            dx = 0 - dx;
        }
        if self.dragCorner == 1 || self.dragCorner == 2 {
            dy = 0 - dy;
        }
        let from_x: i64 = self.dragStartW + dx;
        let mut from_y: i64 = self.dragStartW;
        if self.dragStartH > 0 {
            from_y = ((((self.dragStartH + dy) as f64) * (self.dragStartW as f64)) / (self.dragStartH as f64)).floor() as i64;
        }
        let mut w: i64 = from_x;
        let mut _tmp_7180 = self.dragStartW.clone();
        if DocxViewer::away(from_y, _tmp_7180) > DocxViewer::away(from_x, self.dragStartW.clone()) {
            w = from_y;
        }
        if w < 24 {
            w = 24;
        }
        let mut dr: Rc<RefCell<DocDrawing>> = self.doc.borrow().drawingById(self.dragDrawingId.clone());
        if dr.borrow().id != self.dragDrawingId {
            return false;
        }
        let mut h: i64 = (((w as f64) * (self.dragStartH as f64)) / (self.dragStartW as f64)).floor() as i64;
        if h < 12 {
            h = 12;
        }
        if dr.borrow().widthPx == w && dr.borrow().heightPx == h {
            return false;
        }
        dr.borrow_mut().widthPx = w;
        dr.borrow_mut().heightPx = h;
        self.doc.clone().borrow_mut().touch();
        self.view.borrow().layout.clone().borrow_mut().invalidate();
        self.renderPage(self.currentPage.clone());
        // The layout will not let a drawing past the column; what it drew is
        // the size the drawing has, so a drag out over the margin does not
        // leave a width nobody can see behind for the next press to fight.
        let mut laid: Rc<RefCell<LaidLine>> = self.drawingLine(self.dragDrawingId.clone());
        if laid.borrow().drawingId == self.dragDrawingId {
            if laid.borrow().width < dr.borrow().widthPx {
                dr.borrow_mut().widthPx = laid.borrow().width;
                dr.borrow_mut().heightPx = laid.borrow().height;
            }
        }
        true
    }
    pub fn endDrawingDrag(&mut self) -> bool {
        if self.dragDrawingId == 0 {
            return false;
        }
        self.dragDrawingId = 0;
        self.dragCorner = 0;
        true
    }
    /// The selected drawing, scaled: 1.1 is a tenth bigger, 0.9 a tenth
    /// smaller. The natural size is untouched — the painter scales the marks
    /// between the two — so growing and shrinking are each other's inverse
    /// and nothing is lost on the way.
    pub fn resizeDrawing(&mut self, factor: f64) -> bool {
        let id: i64 = self.view.borrow().selectedDrawingId;
        if id == 0 {
            return false;
        }
        if factor <= 0.0 {
            return false;
        }
        let mut dr: Rc<RefCell<DocDrawing>> = self.doc.borrow().drawingById(id);
        if dr.borrow().id != id {
            return false;
        }
        let w: i64 = ((dr.borrow().widthPx as f64) * factor + 0.5).floor() as i64;
        let h: i64 = ((dr.borrow().heightPx as f64) * factor + 0.5).floor() as i64;
        if w < 24 {
            return false;
        }
        if h < 12 {
            return false;
        }
        dr.borrow_mut().widthPx = w;
        dr.borrow_mut().heightPx = h;
        self.doc.clone().borrow_mut().touch();
        self.view.borrow().layout.clone().borrow_mut().invalidate();
        self.renderPage(self.currentPage.clone());
        true
    }
    /// The selected drawing's provenance, for a host that wants to show its
    /// source, and the drawing itself for one that can draw it again.
    pub fn selectedDrawingModel(&self) -> Rc<RefCell<DocDrawing>> {
        return self.doc.borrow().drawingById(self.view.borrow().selectedDrawingId.clone());
    }
    /// A drawing drawn again: whoever holds the reader for its notation hands
    /// over the new tree and its natural size, and the box keeps its width so a
    /// redraw does not undo a resize.
    fn replaceDrawing(&mut self, id: i64, root: Rc<RefCell<EVGElement>>, natural_w: f64, natural_h: f64, source: String) -> bool {
        let mut dr: Rc<RefCell<DocDrawing>> = self.doc.borrow().drawingById(id);
        if dr.borrow().id != id {
            return false;
        }
        if natural_w <= 0.0 || natural_h <= 0.0 {
            return false;
        }
        dr.borrow_mut().root = Some(root.clone());
        dr.borrow_mut().naturalW = natural_w;
        dr.borrow_mut().naturalH = natural_h;
        dr.borrow_mut().source = source.clone();
        dr.borrow_mut().note = String::new();
        let w: f64 = dr.borrow().widthPx as f64;
        dr.borrow_mut().heightPx = ((w * natural_h) / natural_w + 0.5).floor() as i64;
        if dr.borrow().heightPx < 12 {
            dr.borrow_mut().heightPx = 12;
        }
        self.doc.clone().borrow_mut().touch();
        self.view.borrow().layout.clone().borrow_mut().invalidate();
        self.renderPage(self.currentPage.clone());
        true
    }
    /// The chart's own document, for a host that wants to show or export it.
    pub fn chartJson(&self, id: i64) -> String {
        let mut c: Rc<RefCell<DocChart>> = self.doc.borrow().chartById(id);
        return c.borrow().json.clone();
    }
    pub fn toggleBold(&mut self) {
        self.edit.clone().borrow_mut().toggleBold();
        self.renderPage(self.currentPage.clone());
    }
    pub fn setFont(&mut self, font: &str) {
        self.edit.clone().borrow_mut().setFont(font.to_string());
        self.renderPage(self.currentPage.clone());
    }
    pub fn caretInfoJson(&self) -> String {
        if !self.edit.borrow().isActive() {
            return "{\"active\":false}".to_string();
        }
        let mut c: Rc<RefCell<DocumentPosition>> = self.edit.borrow().caret();
        let mut sel: Rc<RefCell<DocumentRange>> = self.edit.borrow().selection();
        let a: i64 = DocumentRange::startOffset(&sel);
        let b: i64 = DocumentRange::endOffset(&sel);
        let mut bold_flag: String = "false";
        if self.edit.borrow().typingBold {
            bold_flag = "true";
        }
        return format!("{}{},\"offset\":{},\"selStart\":{},\"selEnd\":{},\"boldTyping\":{}{}", "{\"active\":true,\"paragraphId\":".to_string(), c.borrow().paragraphId, c.borrow().textOffset, a, b, bold_flag, "}".to_string());
    }
}
