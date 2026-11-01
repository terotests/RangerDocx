// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::clipboard_table::ClipCell;
use crate::clipboard_table::ClipRow;
use crate::clipboard_table::ClipboardTable;
use crate::docx_layout::DocxLayout;
use crate::docx_layout::LaidLine;
use crate::docx_layout::LaidLineKind;
use crate::docx_layout::LaidPage;
use crate::docx_para_format::DocxParaFormat;
use crate::docx_text_metrics::DocxTextMetrics;
use crate::rgr::DocChart;
use crate::rgr::DocSection;
use crate::rgr::DocTableCell;
use crate::rgr::DocTableRow;
use crate::rgr::DocumentTable;
use crate::rgr::DxNative;
use crate::rgr::Paragraph;
use crate::rgr::RichDocument;
use crate::rgr::TextSpan;
use crate::rich_document_edit::RichDocumentEdit;
use crate::word_numbering::WordNumbering;
use crate::word_style_resolver::ResolvedParagraphStyle;
use crate::word_style_resolver::ResolvedRunStyle;
use crate::word_style_resolver::WordStyleResolver;
use crate::rgr::{DocEditHistory, ChartDoc, ChartDocCodec, ChartKind, ChartStyle, DocEditKind, DocEditOp, DocumentPosition, DocumentRange, OfficeCaretColumn, OfficeCaretMotion, OfficeLineNav, UITextRenderer};

/// ==============================================================================
/// DocxEditController - TextEditCore-style editing over RichDocument + DocxLayout.
/// ==============================================================================
///
/// Shared types: DocumentPosition / DocumentRange / DocEditOp from TextEditCore.
/// Adapter: RichDocumentEdit. Hit-test + caret geometry come from DocxLayout —
/// the same layout used to paint text.
/// ==============================================================================
pub struct DocxEditController {
    pub adapter: Rc<RefCell<RichDocumentEdit>>,
    sel: Rc<RefCell<DocumentRange>>,
    /// The ops, what counts as one action, what an op's transaction is and how
    /// much falls off the bottom at the limit — all of it
    /// `gallery/office/editor`, shared with the spreadsheet's history. The
    /// document's own rules were missing all three: a paste of three lines was
    /// five separate undos that never got back to where they started. The two
    /// stacks and the trim loop that used to live here are gone with them; a
    /// type parameter means the shared file can hold `DocEditOp` without
    /// knowing anything about it.
    history: Rc<RefCell<DocEditHistory>>,
    pub active: bool,
    pub enabled: bool,
    pub typingBold: bool,
    pub typingItalic: bool,
    pub typingUnderline: bool,
    pub typingStrike: bool,
    pub typingVert: i64,
    pub typingHighlight: String,
    pub typingFont: String,
    pub typingSize: f64,
    pub typingColor: String,
    pub caretBlink: bool,
    /// The x an Up or Down aims at. Shared rule, shared with the markdown
    /// editor: it used to be recomputed from the caret on every vertical move,
    /// so Down through a two-character line and back Up landed at column two
    /// rather than where the reader started.
    column: Rc<RefCell<OfficeCaretColumn>>,
    pub suppressUndo: bool,
    /// Find and replace. The strings live here rather than in a dialog, so a
    /// search survives closing it and the whole thing can be driven without
    /// one, the way EVGSheets' GridApp keeps its own.
    pub findQuery: String,
    pub replaceWith: String,
    pub findMatchCase: bool,
    pub findWholeWord: bool,
    pub findStatus: String,
    /// The format painter: the look picked up, and whether the next selection
    /// gets it. `painterSticky` keeps it on after a paint (a double click).
    pub painterArmed: bool,
    pub painterSticky: bool,
    painterSpan: Rc<RefCell<TextSpan>>,
    painterPara: Rc<RefCell<Paragraph>>,
    /// The document's lists, for the list buttons to put paragraphs in.
    pub numbering: Rc<RefCell<WordNumbering>>,
}

impl DocxEditController {
    pub fn new() -> Self {
        Self {
            adapter: Rc::new(RefCell::new(RichDocumentEdit::new())),
            sel: Rc::new(RefCell::new(DocumentRange::new())),
            history: Rc::new(RefCell::new(DocEditHistory::new())),
            active: false,
            enabled: false,
            typingBold: false,
            typingItalic: false,
            typingUnderline: false,
            typingStrike: false,
            typingVert: 0,
            typingHighlight: String::new(),
            typingFont: "Open Sans".to_string(),
            typingSize: 11.0,
            typingColor: "000000".to_string(),
            caretBlink: true,
            column: Rc::new(RefCell::new(OfficeCaretColumn::new())),
            suppressUndo: false,
            findQuery: String::new(),
            replaceWith: String::new(),
            findMatchCase: false,
            findWholeWord: false,
            findStatus: String::new(),
            painterArmed: false,
            painterSticky: false,
            painterSpan: Rc::new(RefCell::new(TextSpan::new())),
            painterPara: Rc::new(RefCell::new(Paragraph::new())),
            numbering: Rc::new(RefCell::new(WordNumbering::new())),
        }
    }
    pub fn intOr(s: &str, fallback: i64) -> i64 {
        if s.is_empty() {
            return fallback;
        }
        let v: Option<i64> = s.parse::<i64>().ok();
        if v.is_some() {
            return v.unwrap();
        }
        fallback
    }
    /// The next kind this chart's own numbers can actually be drawn as. Walking
    /// past the ones that do not suit is what makes "next chart type" a usable
    /// control rather than a way to reach an error message.
    fn nextSuitable(doc: &Rc<RefCell<ChartDoc>>, from: i64, step: i64) -> i64 {
        let mut dir: i64 = 1;
        if step < 0 {
            dir = -1;
        }
        let mut k: i64 = from;
        let mut guard: i64 = 0;
        while guard < ChartKind::highest() {
            k += dir;
            if k > ChartKind::highest() {
                k = ChartKind::lowest();
            }
            if k < ChartKind::lowest() {
                k = ChartKind::highest();
            }
            if ChartDoc::suits(&doc, k) {
                return k;
            }
            guard += 1;
        }
        from
    }
    /// --- find and replace --------------------------------------------------------
    /// Body paragraphs only: a table cell's text is not searched yet.
    pub fn isWordChar(c: i64) -> bool {
        if (c >= 48 && c <= 57) {
            return true;
        }
        if (c >= 65 && c <= 90) {
            return true;
        }
        if (c >= 97 && c <= 122) {
            return true;
        }
        c == 95 || c >= 128
    }
    fn bindDocument(&mut self, doc: Rc<RefCell<RichDocument>>) {
        self.adapter.clone().borrow_mut().bind(doc.clone());
        self.active = false;
        self.history.borrow_mut().reset();
    }
    pub fn setEnabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.active = false;
        }
    }
    pub fn isEnabled(&self) -> bool {
        self.enabled
    }
    pub fn isActive(&self) -> bool {
        self.enabled && self.active
    }
    pub fn setCaret(&mut self, paragraph_id: i64, mut offset: i64) {
        let __len: i64 = self.adapter.borrow().textLength(paragraph_id);
        if offset < 0 {
            offset = 0;
        }
        if offset > __len {
            offset = __len;
        }
        let mut p: Rc<RefCell<DocumentPosition>> = DocumentPosition::create(paragraph_id, offset);
        self.sel.borrow_mut().anchor = DocumentPosition::copy(&p);
        self.sel.borrow_mut().focus = DocumentPosition::copy(&p);
        self.active = true;
    }
    pub fn setRange(&mut self, paragraph_id: i64, mut a: i64, mut b: i64) {
        let __len: i64 = self.adapter.borrow().textLength(paragraph_id);
        if a < 0 {
            a = 0;
        }
        if b < 0 {
            b = 0;
        }
        if a > __len {
            a = __len;
        }
        if b > __len {
            b = __len;
        }
        self.sel.borrow_mut().anchor = DocumentPosition::create(paragraph_id, a);
        self.sel.borrow_mut().focus = DocumentPosition::create(paragraph_id, b);
        self.active = true;
    }
    /// Move the moving end of the selection and leave the anchor where it is.
    /// A drag and a shift-click are the same gesture, and both may cross a
    /// paragraph break — `orderedEnds` puts the two ends into document order, so
    /// nothing below here cares which way round they are.
    pub fn extendTo(&mut self, paragraph_id: i64, mut offset: i64) {
        if !self.active {
            self.setCaret(paragraph_id, offset);
            return;
        }
        let __len: i64 = self.adapter.borrow().textLength(paragraph_id);
        if __len < 0 {
            return;
        }
        if offset < 0 {
            offset = 0;
        }
        if offset > __len {
            offset = __len;
        }
        self.sel.borrow_mut().focus = DocumentPosition::create(paragraph_id, offset);
    }
    fn caret(&self) -> Rc<RefCell<DocumentPosition>> {
        return DocumentPosition::copy(&self.sel.borrow().focus);
    }
    fn selection(&self) -> Rc<RefCell<DocumentRange>> {
        self.sel.clone()
    }
    fn pushUndo(&self, op: Rc<RefCell<DocEditOp>>) {
        // Undo/redo replay through the same edit primitives; their ops must not
        // land back on the stack.
        if self.suppressUndo {
            return;
        }
        // Stamped with the action it belongs to, trimmed only when no action
        // is open, and the redo stack dropped because a new edit makes every
        // undone op unreachable — one call, and none of those three rules
        // written here.
        self.history.borrow_mut().record(op.clone());
    }
    /// One action, however many primitives it takes. Nested safely, so a paste
    /// that is an insert and a split and another insert is still one undo.
    pub fn beginAction(&self) {
        self.history.borrow_mut().begin();
    }
    pub fn endAction(&self) {
        self.history.borrow_mut().end();
    }
    /// --- multi-paragraph selection -------------------------------------------
    /// DocumentRange is anchor→focus in the order the user dragged. These give it
    /// in document order, so a backwards or multi-paragraph selection behaves the
    /// same as a forward one. Results land in the out arrays as
    /// [paragraphId, offset].
    pub fn orderedEnds(&self, mut start_out: &mut Vec<i64>, mut end_out: &mut Vec<i64>) {
        start_out.clear();
        end_out.clear();
        let a_pid: i64 = self.sel.borrow().anchor.borrow().paragraphId;
        let b_pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let a_off: i64 = self.sel.borrow().anchor.borrow().textOffset;
        let b_off: i64 = self.sel.borrow().focus.borrow().textOffset;
        let mut swap: bool = false;
        if a_pid == b_pid {
            if a_off > b_off {
                swap = true;
            }
        } else {
            let ao: i64 = self.adapter.borrow().bodyOrderOf(a_pid);
            let bo: i64 = self.adapter.borrow().bodyOrderOf(b_pid);
            if ao > bo {
                swap = true;
            }
        }
        if swap {
            start_out.push(b_pid);
            start_out.push(b_off);
            end_out.push(a_pid);
            end_out.push(a_off);
        } else {
            start_out.push(a_pid);
            start_out.push(a_off);
            end_out.push(b_pid);
            end_out.push(b_off);
        }
    }
    /// orderedEnds as one list, for callers that cannot pass `&mut` places:
    /// [startParagraphId, startOffset, endParagraphId, endOffset].
    pub fn orderedEndList(&self) -> Vec<i64> {
        let mut a: Vec<i64> = Vec::new();
        let mut b: Vec<i64> = Vec::new();
        self.orderedEnds(&mut a, &mut b);
        vec![a[0], a[1], b[0], b[1]]
    }
    pub fn spansParagraphs(&self) -> bool {
        if DocumentRange::isCollapsed(&self.sel) {
            return false;
        }
        return !DocumentRange::sameParagraph(&self.sel);
    }
    /// Selected text, "\n" between paragraphs — what Ctrl+C puts on the clipboard.
    pub fn selectedText(&self) -> String {
        if !self.isActive() {
            return String::new();
        }
        if DocumentRange::isCollapsed(&self.sel) {
            return String::new();
        }
        let mut a: Vec<i64> = Vec::new();
        let mut b: Vec<i64> = Vec::new();
        self.orderedEnds(&mut a, &mut b);
        let s_pid: i64 = a[0];
        let s_off: i64 = a[1];
        let e_pid: i64 = b[0];
        let e_off: i64 = b[1];
        if s_pid == e_pid {
            let t: String = self.adapter.borrow().getText(s_pid);
            let mut x0: i64 = s_off;
            let mut x1: i64 = e_off;
            if x1 > (DxNative::str_len(&t)) {
                x1 = DxNative::str_len(&t);
            }
            if x0 < 0 {
                x0 = 0;
            }
            if x0 >= x1 {
                return String::new();
            }
            return DxNative::substr(&t, x0, x1);
        }
        let s_order: i64 = self.adapter.borrow().bodyOrderOf(s_pid);
        let e_order: i64 = self.adapter.borrow().bodyOrderOf(e_pid);
        if s_order < 0 || e_order < 0 {
            return String::new();
        }
        let mut out: String = String::new();
        let mut o: i64 = s_order;
        while o <= e_order {
            let pid: i64 = self.adapter.borrow().bodyParagraphIdAt(o);
            let txt: String = self.adapter.borrow().getText(pid);
            let mut piece: String = txt.clone();
            if o == s_order {
                let mut s0: i64 = s_off;
                if s0 > (DxNative::str_len(&txt)) {
                    s0 = DxNative::str_len(&txt);
                }
                piece = DxNative::substr(&txt, s0, (DxNative::str_len(&txt)));
            }
            if o == e_order {
                let base: String = piece.clone();
                let mut e0: i64 = e_off;
                if o == s_order {
                    e0 = e_off - s_off;
                }
                if e0 < 0 {
                    e0 = 0;
                }
                if e0 > (DxNative::str_len(&base)) {
                    e0 = DxNative::str_len(&base);
                }
                piece = DxNative::substr(&base, 0, e0);
            }
            if o > s_order {
                out = format!("{}\n", out);
            }
            out = format!("{}{}", out, piece);
            o += 1;
        }
        out
    }
    /// Delete across paragraphs: trim the first, empty the ones between, trim the
    /// last, then merge them all back into the first.
    pub fn deleteSelectionMulti(&mut self) {
        let mut a: Vec<i64> = Vec::new();
        let mut b: Vec<i64> = Vec::new();
        self.orderedEnds(&mut a, &mut b);
        let s_pid: i64 = a[0];
        let s_off: i64 = a[1];
        let e_pid: i64 = b[0];
        let e_off: i64 = b[1];
        let s_order: i64 = self.adapter.borrow().bodyOrderOf(s_pid);
        let e_order: i64 = self.adapter.borrow().bodyOrderOf(e_pid);
        if s_order < 0 || e_order < 0 {
            return;
        }
        if e_order <= s_order {
            return;
        }
        let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
        op.borrow_mut().kind = DocEditKind::deleteSpan();
        op.borrow_mut().paragraphId = s_pid;
        op.borrow_mut().start = s_off;
        op.borrow_mut().oldText = self.selectedText();
        // tail of the first paragraph
        let _tmp_3053 = self.adapter.borrow().textLength(s_pid);
        self.adapter.clone().borrow().replaceRange(s_pid, s_off, _tmp_3053, "");
        // whole paragraphs in between
        let mut o: i64 = s_order + 1;
        while o < e_order {
            let mid_pid: i64 = self.adapter.borrow().bodyParagraphIdAt(o);
            let _tmp_3058 = self.adapter.borrow().textLength(mid_pid);
            self.adapter.clone().borrow().replaceRange(mid_pid, 0, _tmp_3058, "");
            o += 1;
        }
        // head of the last paragraph
        let mut e_tail: i64 = e_off;
        if e_tail > self.adapter.borrow().textLength(e_pid) {
            e_tail = self.adapter.borrow().textLength(e_pid);
        }
        self.adapter.clone().borrow().replaceRange(e_pid, 0, e_tail, "");
        // collapse the now-empty run back into the first paragraph
        let merges: i64 = e_order - s_order;
        let mut m: i64 = 0;
        while m < merges {
            let next_pid: i64 = self.adapter.borrow().bodyParagraphIdAt(s_order + 1);
            if next_pid <= 0 {
                m = merges;
            } else {
                self.adapter.clone().borrow_mut().mergePrevious(next_pid);
                m += 1;
            }
        }
        self.pushUndo(op.clone());
        self.setCaret(s_pid, s_off);
    }
    pub fn deleteSelection(&mut self) {
        if DocumentRange::isCollapsed(&self.sel) {
            return;
        }
        if !DocumentRange::sameParagraph(&self.sel) {
            self.deleteSelectionMulti();
            return;
        }
        let pid: i64 = self.sel.borrow().anchor.borrow().paragraphId;
        let a: i64 = DocumentRange::startOffset(&self.sel);
        let b: i64 = DocumentRange::endOffset(&self.sel);
        let old: String = DxNative::substr(&self.adapter.borrow().getText(pid), a, b);
        let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
        op.borrow_mut().kind = 0;
        op.borrow_mut().paragraphId = pid;
        op.borrow_mut().start = a;
        op.borrow_mut().end = a;
        op.borrow_mut().oldText = old.clone();
        op.borrow_mut().newText = String::new();
        self.adapter.clone().borrow().replaceRange(pid, a, b, "");
        self.pushUndo(op.clone());
        self.setCaret(pid, a);
    }
    pub fn insertText(&mut self, text: String) {
        if !self.isActive() {
            return;
        }
        if text.is_empty() {
            return;
        }
        if !DocumentRange::sameParagraph(&self.sel) {
            return;
        }
        if !DocumentRange::isCollapsed(&self.sel) {
            self.deleteSelection();
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
        op.borrow_mut().kind = 0;
        op.borrow_mut().paragraphId = pid;
        op.borrow_mut().start = off;
        op.borrow_mut().end = off + (DxNative::str_len(&text));
        op.borrow_mut().oldText = String::new();
        op.borrow_mut().newText = text.clone();
        self.adapter.clone().borrow().replaceRange(pid, off, off, &text);
        let up_to: i64 = off + (DxNative::str_len(&text));
        if self.typingBold {
            self.adapter.clone().borrow().applyRun(pid, off, up_to, "bold", true, 0.0, String::new());
        }
        if self.typingItalic {
            self.adapter.clone().borrow().applyRun(pid, off, up_to, "italic", true, 0.0, String::new());
        }
        if self.typingUnderline {
            self.adapter.clone().borrow().applyRun(pid, off, up_to, "underline", true, 0.0, String::new());
        }
        if self.typingStrike {
            self.adapter.clone().borrow().applyRun(pid, off, up_to, "strike", true, 0.0, String::new());
        }
        if self.typingVert == 1 {
            self.adapter.clone().borrow().applyRun(pid, off, up_to, "super", true, 0.0, String::new());
        }
        if self.typingVert == 2 {
            self.adapter.clone().borrow().applyRun(pid, off, up_to, "sub", true, 0.0, String::new());
        }
        if !self.typingHighlight.is_empty() {
            let mut _tmp_3091 = self.typingHighlight.clone();
            self.adapter.clone().borrow().applyRun(pid, off, up_to, "highlight", true, 0.0, _tmp_3091);
        }
        self.pushUndo(op.clone());
        self.setCaret(pid, off + (DxNative::str_len(&text)));
    }
    pub fn backspace(&mut self) {
        if !self.isActive() {
            return;
        }
        if !DocumentRange::sameParagraph(&self.sel) {
            return;
        }
        if !DocumentRange::isCollapsed(&self.sel) {
            self.deleteSelection();
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        if off > 0 {
            let old: String = DxNative::substr(&self.adapter.borrow().getText(pid), off - 1, off);
            let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
            op.borrow_mut().kind = 0;
            op.borrow_mut().paragraphId = pid;
            op.borrow_mut().start = off - 1;
            op.borrow_mut().end = off - 1;
            op.borrow_mut().oldText = old.clone();
            op.borrow_mut().newText = String::new();
            self.adapter.clone().borrow().replaceRange(pid, off - 1, off, "");
            self.pushUndo(op.clone());
            self.setCaret(pid, off - 1);
            return;
        }
        let mut dest: Rc<RefCell<DocumentPosition>> = self.adapter.borrow_mut().mergePrevious(pid);
        if dest.borrow().paragraphId > 0 {
            let mut op2: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
            op2.borrow_mut().kind = DocEditKind::mergeParagraph();
            op2.borrow_mut().paragraphId = pid;
            op2.borrow_mut().secondId = dest.borrow().paragraphId;
            op2.borrow_mut().start = dest.borrow().textOffset;
            self.pushUndo(op2.clone());
            let mut _tmp_3102 = dest.borrow().paragraphId.clone();
            self.setCaret(_tmp_3102, dest.borrow().textOffset.clone());
        }
    }
    pub fn deleteForward(&mut self) {
        if !self.isActive() {
            return;
        }
        if !DocumentRange::isCollapsed(&self.sel) {
            self.deleteSelection();
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        let __len: i64 = self.adapter.borrow().textLength(pid);
        if off < __len {
            let old: String = DxNative::substr(&self.adapter.borrow().getText(pid), off, off + 1);
            let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
            op.borrow_mut().kind = 0;
            op.borrow_mut().paragraphId = pid;
            op.borrow_mut().start = off;
            op.borrow_mut().end = off;
            op.borrow_mut().oldText = old.clone();
            op.borrow_mut().newText = String::new();
            self.adapter.clone().borrow().replaceRange(pid, off, off + 1, "");
            self.pushUndo(op.clone());
            self.setCaret(pid, off);
        }
    }
    pub fn splitAtCaret(&mut self) {
        if !self.isActive() {
            return;
        }
        if !DocumentRange::isCollapsed(&self.sel) {
            self.deleteSelection();
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        // Word: Enter on an empty list item ends the list instead of adding
        // another empty bullet, so there is a way out of a list from the keyboard.
        let mut cur: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
        if cur.borrow().listNumId > 0 {
            if cur.borrow().text.is_empty() {
                let mut op0: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
                op0.borrow_mut().kind = DocEditKind::listEnd();
                op0.borrow_mut().paragraphId = pid;
                op0.borrow_mut().start = cur.borrow().listNumId;
                op0.borrow_mut().end = cur.borrow().listLevel;
                op0.borrow_mut().newText = cur.borrow().listFmt.clone();
                op0.borrow_mut().oldText = cur.borrow().listText.clone();
                op0.borrow_mut().secondId = cur.borrow().listKind;
                self.pushUndo(op0.clone());
                cur.borrow_mut().listNumId = 0;
                cur.borrow_mut().listKind = 0;
                cur.borrow_mut().listLevel = 0;
                cur.borrow_mut().listFmt = String::new();
                cur.borrow_mut().listText = String::new();
                RichDocument::touch(&self.adapter.borrow().doc.clone());
                return;
            }
        }
        let new_id: i64 = self.adapter.borrow().splitParagraph(pid, off);
        let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
        op.borrow_mut().kind = DocEditKind::splitParagraph();
        op.borrow_mut().paragraphId = pid;
        op.borrow_mut().start = off;
        op.borrow_mut().secondId = new_id;
        self.pushUndo(op.clone());
        self.setCaret(new_id, 0);
    }
    /// --- clipboard paste ------------------------------------------------------
    /// Word's behaviour: the paragraph is split at the caret and the table lands
    /// between the halves, with the caret on the paragraph that follows it.
    pub fn pasteTable(&mut self, tbl: &Rc<RefCell<ClipboardTable>>) -> bool {
        if !self.isActive() {
            return false;
        }
        if tbl.borrow().rowCount() == 0 {
            return false;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        self.splitAtCaret();
        let after_id: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let mut doc: Rc<RefCell<RichDocument>> = self.adapter.borrow()
            .doc.clone()
            .clone();
        let mut t: Rc<RefCell<DocumentTable>> = self.adapter.borrow().insertTableBefore(after_id);
        let mut cols: i64 = tbl.borrow().columnCount();
        if cols < 1 {
            cols = 1;
        }
        // Spread the table across the text column, as Word does for a paste.
        let width_px: i64 = DocSection::contentWidthPx(&doc.borrow().section);
        let total_twips: i64 = (((width_px as f64) * 1440.0) / 96.0).floor() as i64;
        let mut per: i64 = (total_twips as f64 / cols as f64).floor() as i64;
        if per < 240 {
            per = 240;
        }
        t.borrow_mut().colWidthsTwips.clear();
        let mut ci: i64 = 0;
        while ci < cols {
            t.borrow_mut().colWidthsTwips.push(per);
            ci += 1;
        }
        let mut ri: i64 = 0;
        while ri < tbl.borrow().rowCount() {
            let mut src: Rc<RefCell<ClipRow>> = tbl.borrow().rowAt(ri);
            let mut row: Rc<RefCell<DocTableRow>> = Rc::new(RefCell::new(DocTableRow::new()));
            let mut cj: i64 = 0;
            while cj < (src.borrow().cells.len() as i64) {
                let mut sc: Rc<RefCell<ClipCell>> = src.borrow().cells[cj as usize].clone();
                let mut cell: Rc<RefCell<DocTableCell>> = Rc::new(RefCell::new(DocTableCell::new()));
                if sc.borrow().gridSpan > 1 {
                    cell.borrow_mut().gridSpan = sc.borrow().gridSpan;
                }
                if (DxNative::str_len(&sc.borrow().fillHex)) == 6 {
                    cell.borrow_mut().fillHex = sc.borrow().fillHex.clone();
                }
                let mut p: Rc<RefCell<Paragraph>> = RichDocument::addParagraphOnly(&doc);
                p.borrow_mut().styleId = "Normal".to_string();
                p.borrow_mut().spaceBeforePt = 0.0;
                p.borrow_mut().spaceAfterPt = 0.0;
                p.borrow_mut().lineSpacing = 1.0;
                if sc.borrow().align == "center" {
                    p.borrow_mut().align = 1;
                }
                if sc.borrow().align == "right" {
                    p.borrow_mut().align = 2;
                }
                let mut span: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
                span.borrow_mut().bold = sc.borrow().bold;
                span.borrow_mut().fontFamily = self.typingFont.clone();
                Paragraph::appendText(&p, &sc.borrow().text.clone(), &span);
                cell.borrow_mut().paragraphs.push(p.clone());
                row.borrow_mut().cells.push(cell.clone());
                cj += 1;
            }
            t.borrow_mut().rows.push(row.clone());
            ri += 1;
        }
        let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
        op.borrow_mut().kind = DocEditKind::pasteTable();
        op.borrow_mut().paragraphId = pid;
        op.borrow_mut().secondId = after_id;
        op.borrow_mut().start = t.borrow().id;
        self.pushUndo(op.clone());
        self.setCaret(after_id, 0);
        true
    }
    /// A chart pasted from the grid. Same shape as a table paste — the paragraph
    /// splits and the chart lands between the halves — but what is inserted is
    /// the chart's own JSON document, not a rendering of it, so it stays
    /// editable here.
    pub fn pasteChart(&mut self, json: &str, w: i64, h: i64) -> i64 {
        if !self.isActive() {
            return 0;
        }
        if json.is_empty() {
            return 0;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        self.splitAtCaret();
        let after_id: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let mut c: Rc<RefCell<DocChart>> = self.adapter.borrow().insertChartBefore(after_id, json);
        if w > 0 {
            c.borrow_mut().widthPx = w;
        }
        if h > 0 {
            c.borrow_mut().heightPx = h;
        }
        let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
        op.borrow_mut().kind = DocEditKind::pasteChart();
        op.borrow_mut().paragraphId = pid;
        op.borrow_mut().secondId = after_id;
        op.borrow_mut().start = c.borrow().id;
        self.pushUndo(op.clone());
        self.setCaret(after_id, 0);
        return c.borrow().id;
    }
    /// --- editing a chart in the document --------------------------------------
    /// The chart is a Vela document, so an edit is: decode, change one thing,
    /// regenerate the specification through the same generator the spreadsheet
    /// uses, encode again. Nothing here writes a specification by hand.
    pub fn editChart(&self, chart_id: i64, what: &str, arg: String) -> bool {
        let mut c: Rc<RefCell<DocChart>> = RichDocument::chartById(&self.adapter.borrow().doc, chart_id);
        if c.borrow().id != chart_id {
            return false;
        }
        let mut doc: Rc<RefCell<ChartDoc>> = ChartDocCodec::decode(&c.borrow().json.clone());
        if !doc.borrow().ok {
            return false;
        }
        let mut changed: bool = false;
        if what == "kind" {
            let want: i64 = DocxEditController::intOr(&arg, doc.borrow().chart.borrow().kind.clone());
            if want != doc.borrow().chart.borrow().kind {
                if ChartDoc::suits(&doc, want) {
                    doc.borrow_mut().chart.borrow_mut().kind = want;
                    changed = true;
                }
            }
        }
        if what == "kind.next" {
            let step: i64 = DocxEditController::intOr(&arg, 1);
            let next: i64 = DocxEditController::nextSuitable(&doc, doc.borrow().chart.borrow().kind.clone(), step);
            if next != doc.borrow().chart.borrow().kind {
                doc.borrow_mut().chart.borrow_mut().kind = next;
                changed = true;
            }
        }
        if what == "style" {
            let st: i64 = DocxEditController::intOr(&arg, doc.borrow().chart.borrow().style.clone());
            if st >= ChartStyle::lowest() {
                if st <= ChartStyle::highest() {
                    doc.borrow_mut().chart.borrow_mut().style = st;
                    changed = true;
                }
            }
        }
        if what == "style.next" {
            let sstep: i64 = DocxEditController::intOr(&arg, 1);
            let mut ns: i64 = doc.borrow().chart.borrow().style + sstep;
            while ns > ChartStyle::highest() {
                ns -= ChartStyle::highest();
            }
            while ns < ChartStyle::lowest() {
                ns += ChartStyle::highest();
            }
            doc.borrow_mut().chart.borrow_mut().style = ns;
            changed = true;
        }
        if what == "title" {
            doc.borrow_mut().chart.borrow_mut().title = arg.clone();
            changed = true;
        }
        if what == "legend" {
            doc.borrow_mut().chart.borrow_mut().showLegend = !doc.borrow().chart.borrow().showLegend;
            changed = true;
        }
        if what == "width" {
            let nw: i64 = DocxEditController::intOr(&arg, c.borrow().widthPx.clone());
            if nw >= 120 {
                c.borrow_mut().widthPx = nw;
                doc.borrow_mut().chart.borrow_mut().w = nw;
                changed = true;
            }
        }
        if what == "height" {
            let nh: i64 = DocxEditController::intOr(&arg, c.borrow().heightPx.clone());
            if nh >= 90 {
                c.borrow_mut().heightPx = nh;
                doc.borrow_mut().chart.borrow_mut().h = nh;
                changed = true;
            }
        }
        if !changed {
            return false;
        }
        let mut _tmp_3137 = c.borrow().widthPx.clone();
        ChartDoc::rebuildSpec(&doc, _tmp_3137, c.borrow().heightPx.clone());
        let mut _tmp_3139 = Rc::clone(&doc.borrow().cells);
        let _tmp_3141 = ChartDocCodec::encode(&_tmp_3139, &doc.borrow().chart.clone());
        c.borrow_mut().json = _tmp_3141;
        true
    }
    pub fn deleteChart(&self, chart_id: i64) -> bool {
        return self.adapter.borrow().removeChartBlock(chart_id);
    }
    /// Plain text paste: newlines become paragraph splits.
    /// Pasting is ONE action however many primitives it takes. It used to push
    /// one op per line and one per split — three lines were five undos, and
    /// twenty presses still did not get the document back to where it started.
    pub fn pasteText(&mut self, text: &str) -> bool {
        if !self.isActive() {
            return false;
        }
        if text.is_empty() {
            return false;
        }
        self.beginAction();
        let ok: bool = self.pasteLines(text);
        self.endAction();
        ok
    }
    /// One action, however many lines. `pasteText` already opened a transaction
    /// before calling this, and `OfficeHistory.begin` nests — "so a paste that
    /// is an insert and a split and another insert is still one undo" — so this
    /// pair costs nothing there and is the difference between one undo and five
    /// for anything that reaches the primitive directly. It is public; something
    /// will.
    pub fn pasteLines(&mut self, text: &str) -> bool {
        self.beginAction();
        let ok: bool = self.pasteLinesInner(text);
        self.endAction();
        ok
    }
    pub fn pasteLinesInner(&mut self, text: &str) -> bool {
        let mut lines: Vec<String> = text.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut i: i64 = 0;
        let n: i64 = lines.len() as i64;
        while i < n {
            let mut line: String = lines[i as usize].clone();
            let __len: i64 = DxNative::str_len(&line);
            if __len > 0 {
                if DxNative::char_at(&line, __len - 1) == 13 {
                    line = DxNative::substr(&line, 0, __len - 1);
                }
            }
            if !line.is_empty() {
                self.insertText(line.clone());
            }
            if i < n - 1 {
                self.splitAtCaret();
            }
            i += 1;
        }
        true
    }
    pub fn toggleBold(&mut self) {
        self.toggleRun("bold");
    }
    /// Bold, italic and underline are the same operation with one field
    /// changed. Writing the second and third out longhand is how the third one
    /// ends up with a subtly different answer for an empty selection.
    pub fn toggleRun(&mut self, what: &str) {
        if !self.isActive() {
            return;
        }
        if DocumentRange::isCollapsed(&self.sel) {
            // Nothing selected: the switch applies to what you type NEXT,
            // which is what Word does.
            if what == "bold" {
                self.typingBold = !self.typingBold;
            }
            if what == "italic" {
                self.typingItalic = !self.typingItalic;
            }
            if what == "underline" {
                self.typingUnderline = !self.typingUnderline;
            }
            if what == "strike" {
                self.typingStrike = !self.typingStrike;
            }
            if what == "super" {
                self.typingVert = if (self.typingVert == 1) { 0 } else { 1 };
            }
            if what == "sub" {
                self.typingVert = if (self.typingVert == 2) { 0 } else { 2 };
            }
            return;
        }
        if !DocumentRange::sameParagraph(&self.sel) {
            self.toggleRunMulti(what);
            return;
        }
        let pid: i64 = self.sel.borrow().anchor.borrow().paragraphId;
        let a: i64 = DocumentRange::startOffset(&self.sel);
        let b: i64 = DocumentRange::endOffset(&self.sel);
        let mut turn_on: bool = true;
        if self.adapter.borrow().rangeIsOn(pid, a, b, what) {
            turn_on = false;
        }
        self.adapter.clone().borrow().applyRun(pid, a, b, what, turn_on, 0.0, String::new());
        self.rememberTyping(what, turn_on);
        // Only bold has an undo op of its own; the others ride the document
        // snapshot the same way a paste does.
        if what == "bold" {
            let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
            op.borrow_mut().kind = DocEditKind::bold();
            op.borrow_mut().paragraphId = pid;
            op.borrow_mut().start = a;
            op.borrow_mut().end = b;
            op.borrow_mut().boldSet = true;
            op.borrow_mut().boldVal = turn_on;
            self.pushUndo(op.clone());
        }
    }
    pub fn rememberTyping(&mut self, what: &str, on: bool) {
        if what == "bold" {
            self.typingBold = on;
        }
        if what == "italic" {
            self.typingItalic = on;
        }
        if what == "underline" {
            self.typingUnderline = on;
        }
        if what == "strike" {
            self.typingStrike = on;
        }
        if what == "super" {
            self.typingVert = if on { 1 } else { 0 };
        }
        if what == "sub" {
            self.typingVert = if on { 2 } else { 0 };
        }
    }
    /// What the caret is standing in, for a toolbar that has to show it.
    pub fn caretIsOn(&self, what: &str) -> bool {
        if !self.isActive() {
            return false;
        }
        if DocumentRange::isCollapsed(&self.sel) {
            if what == "bold" {
                return self.typingBold;
            }
            if what == "italic" {
                return self.typingItalic;
            }
            if what == "underline" {
                return self.typingUnderline;
            }
            if what == "strike" {
                return self.typingStrike;
            }
            if what == "super" {
                return self.typingVert == 1;
            }
            if what == "sub" {
                return self.typingVert == 2;
            }
            if what == "highlight" {
                return !self.typingHighlight.is_empty();
            }
            return false;
        }
        if !DocumentRange::sameParagraph(&self.sel) {
            return false;
        }
        let mut _tmp_3160 = self.sel.borrow().anchor.borrow().paragraphId.clone();
        return self.adapter.borrow().rangeIsOn(_tmp_3160, DocumentRange::startOffset(&self.sel), DocumentRange::endOffset(&self.sel), what);
    }
    /// The face and the size the toolbar should show.
    ///
    /// With something selected that is what the selection is; with nothing
    /// selected it is what you would get if you typed — the typing state, which
    /// is the same rule `caretIsOn` follows for bold. Reading the run under the
    /// caret in both cases made "bigger text" with no selection look like it had
    /// done nothing: the size HAD changed, and the number beside the button
    /// still said what the old letters said.
    pub fn caretFont(&self) -> String {
        if !self.isActive() {
            return self.typingFont.clone();
        }
        if DocumentRange::isCollapsed(&self.sel) {
            return self.typingFont.clone();
        }
        let mut _tmp_3165 = self.sel.borrow().focus.borrow().paragraphId.clone();
        let nm: String = self.adapter.borrow().runFontOf(_tmp_3165, self.sel.borrow().focus.borrow().textOffset.clone());
        if nm.is_empty() {
            return self.typingFont.clone();
        }
        nm
    }
    pub fn caretSize(&self) -> f64 {
        if !self.isActive() {
            return self.typingSize;
        }
        if DocumentRange::isCollapsed(&self.sel) {
            return self.typingSize;
        }
        let mut _tmp_3169 = self.sel.borrow().focus.borrow().paragraphId.clone();
        let pt: f64 = self.adapter.borrow().runSizeOf(_tmp_3169, self.sel.borrow().focus.borrow().textOffset.clone());
        if pt <= 0.0 {
            return self.typingSize;
        }
        pt
    }
    /// Putting the caret somewhere adopts the run it landed in, so the toolbar
    /// describes the text you are standing in rather than the last thing you
    /// asked for somewhere else.
    pub fn adoptRunAtCaret(&mut self) {
        if !self.isActive() {
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        let nm: String = self.adapter.borrow().runFontOf(pid, off);
        if !nm.is_empty() {
            self.typingFont = nm.clone();
        }
        let pt: f64 = self.adapter.borrow().runSizeOf(pid, off);
        if pt > 0.0 {
            self.typingSize = pt;
        }
        let __len: i64 = self.adapter.borrow().textLength(pid);
        let mut probe_end: i64 = off + 1;
        if probe_end > __len {
            probe_end = __len;
        }
        if off < probe_end {
            self.typingBold = self.adapter.borrow().rangeIsOn(pid, off, probe_end, "bold");
            self.typingItalic = self.adapter.borrow().rangeIsOn(pid, off, probe_end, "italic");
            self.typingUnderline = self.adapter.borrow().rangeIsOn(pid, off, probe_end, "underline");
            self.typingStrike = self.adapter.borrow().rangeIsOn(pid, off, probe_end, "strike");
            self.typingVert = 0;
            if self.adapter.borrow().rangeIsOn(pid, off, probe_end, "super") {
                self.typingVert = 1;
            }
            if self.adapter.borrow().rangeIsOn(pid, off, probe_end, "sub") {
                self.typingVert = 2;
            }
            self.typingHighlight = String::new();
        }
    }
    /// A point size or a colour over the selection. Neither is a toggle: there
    /// is a value, and it is applied.
    pub fn setRunSize(&mut self, pt: f64) {
        self.typingSize = pt;
        self.applyToSelection("size", false, pt, "");
    }
    pub fn setRunColor(&mut self, hex: String) {
        self.typingColor = hex.clone();
        self.applyToSelection("color", false, 0.0, &hex);
    }
    pub fn applyToSelection(&self, what: &str, on: bool, num: f64, text: &str) {
        if !self.isActive() {
            return;
        }
        if DocumentRange::isCollapsed(&self.sel) {
            return;
        }
        if DocumentRange::sameParagraph(&self.sel) {
            let mut _tmp_3187 = self.sel.borrow().anchor.borrow().paragraphId.clone();
            let __arg_0 = DocumentRange::startOffset(&self.sel);
            let __arg_1 = DocumentRange::endOffset(&self.sel);
            self.adapter.clone().borrow().applyRun(_tmp_3187, __arg_0, __arg_1, what, on, num, text.to_string());
            return;
        }
        let mut a: Vec<i64> = Vec::new();
        let mut b: Vec<i64> = Vec::new();
        self.orderedEnds(&mut a, &mut b);
        let s_order: i64 = self.adapter.borrow().bodyOrderOf(a[0]);
        let e_order: i64 = self.adapter.borrow().bodyOrderOf(b[0]);
        if s_order < 0 || e_order < 0 {
            return;
        }
        let mut o: i64 = s_order;
        while o <= e_order {
            let pid: i64 = self.adapter.borrow().bodyParagraphIdAt(o);
            let __len: i64 = self.adapter.borrow().textLength(pid);
            let mut x0: i64 = 0;
            let mut x1: i64 = __len;
            if o == s_order {
                x0 = a[1];
            }
            if o == e_order {
                x1 = b[1];
            }
            if x1 > __len {
                x1 = __len;
            }
            if x0 < x1 {
                self.adapter.clone().borrow().applyRun(pid, x0, x1, what, on, num, text.to_string());
            }
            o += 1;
        }
    }
    pub fn toggleBoldMulti(&mut self) {
        self.toggleRunMulti("bold");
    }
    /// A toggle over a selection that crosses paragraphs: one pass to decide the
    /// new value (Word turns it ON unless the WHOLE selection already has it),
    /// then apply that to each paragraph's slice.
    pub fn toggleRunMulti(&mut self, what: &str) {
        let mut a: Vec<i64> = Vec::new();
        let mut b: Vec<i64> = Vec::new();
        self.orderedEnds(&mut a, &mut b);
        let s_pid: i64 = a[0];
        let s_off: i64 = a[1];
        let e_pid: i64 = b[0];
        let e_off: i64 = b[1];
        let s_order: i64 = self.adapter.borrow().bodyOrderOf(s_pid);
        let e_order: i64 = self.adapter.borrow().bodyOrderOf(e_pid);
        if s_order < 0 || e_order < 0 {
            return;
        }
        let mut all_on: bool = true;
        let mut o: i64 = s_order;
        while o <= e_order {
            let pid: i64 = self.adapter.borrow().bodyParagraphIdAt(o);
            let __len: i64 = self.adapter.borrow().textLength(pid);
            let mut x0: i64 = 0;
            let mut x1: i64 = __len;
            if o == s_order {
                x0 = s_off;
            }
            if o == e_order {
                x1 = e_off;
            }
            if x1 > __len {
                x1 = __len;
            }
            if x0 < x1 {
                if !self.adapter.borrow().rangeIsOn(pid, x0, x1, what) {
                    all_on = false;
                }
            }
            o += 1;
        }
        let mut turn_on: bool = true;
        if all_on {
            turn_on = false;
        }
        let mut o2: i64 = s_order;
        while o2 <= e_order {
            let pid2: i64 = self.adapter.borrow().bodyParagraphIdAt(o2);
            let len2: i64 = self.adapter.borrow().textLength(pid2);
            let mut y0: i64 = 0;
            let mut y1: i64 = len2;
            if o2 == s_order {
                y0 = s_off;
            }
            if o2 == e_order {
                y1 = e_off;
            }
            if y1 > len2 {
                y1 = len2;
            }
            if y0 < y1 {
                self.adapter.clone().borrow().applyRun(pid2, y0, y1, what, turn_on, 0.0, String::new());
            }
            o2 += 1;
        }
        self.rememberTyping(what, turn_on);
    }
    pub fn setFont(&mut self, font: String) {
        self.typingFont = font.clone();
        if !self.isActive() {
            return;
        }
        if DocumentRange::isCollapsed(&self.sel) {
            return;
        }
        if !DocumentRange::sameParagraph(&self.sel) {
            return;
        }
        let mut _tmp_3209 = self.sel.borrow().anchor.borrow().paragraphId.clone();
        let __arg_0 = DocumentRange::startOffset(&self.sel);
        let __arg_1 = DocumentRange::endOffset(&self.sel);
        self.adapter.clone().borrow().applyFont(_tmp_3209, __arg_0, __arg_1, &font);
    }
    /// --- caret movement -------------------------------------------------------
    /// `extend` grows the selection from the anchor (Shift+key); otherwise the
    /// selection collapses to the new caret.
    pub fn placeCaret(&mut self, pid: i64, mut off: i64, extend: bool) {
        // Any placement that is not part of a vertical run — a click, an arrow
        // sideways, a word jump, an edit — ends the run. `moveVertical` puts
        // the column back immediately afterwards, which is why it is safe to
        // forget it here and nowhere else.
        OfficeCaretColumn::forget(&self.column.clone());
        if pid <= 0 {
            return;
        }
        let __len: i64 = self.adapter.borrow().textLength(pid);
        if off < 0 {
            off = 0;
        }
        if off > __len {
            off = __len;
        }
        if extend {
            self.sel.borrow_mut().focus = DocumentPosition::create(pid, off);
            self.active = true;
        } else {
            self.setCaret(pid, off);
        }
    }
    pub fn moveLeft(&mut self, extend: bool) {
        if !self.isActive() {
            return;
        }
        // Collapsing a selection with a bare Left goes to its start, like Word.
        if !extend && !DocumentRange::isCollapsed(&self.sel) {
            let mut a: Vec<i64> = Vec::new();
            let mut b: Vec<i64> = Vec::new();
            self.orderedEnds(&mut a, &mut b);
            self.setCaret(a[0], a[1]);
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        if off > 0 {
            // ONE CHARACTER, not one UTF-16 unit. This was `off - 1`, which
            // puts the caret between the halves of a surrogate pair; the next
            // insert or delete then splits an emoji into two units that are
            // not characters at all. `PptxTextEdit` had had the right answer
            // the whole time and nothing connected the two — which is the
            // pattern seam `gallery/PLAN_EDITOR_KERNEL.md` §2 says never to
            // rely on, met in the wild.
            self.placeCaret(pid, OfficeCaretMotion::stepBack(&self.adapter.borrow().getText(pid), off), extend);
            return;
        }
        let order: i64 = self.adapter.borrow().bodyOrderOf(pid);
        if order <= 0 {
            return;
        }
        let prev: i64 = self.adapter.borrow().bodyParagraphIdAt(order - 1);
        self.placeCaret(prev, self.adapter.borrow().textLength(prev), extend);
    }
    pub fn moveRight(&mut self, extend: bool) {
        if !self.isActive() {
            return;
        }
        if !extend && !DocumentRange::isCollapsed(&self.sel) {
            let mut a: Vec<i64> = Vec::new();
            let mut b: Vec<i64> = Vec::new();
            self.orderedEnds(&mut a, &mut b);
            self.setCaret(b[0], b[1]);
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        if off < self.adapter.borrow().textLength(pid) {
            self.placeCaret(pid, OfficeCaretMotion::stepForward(&self.adapter.borrow().getText(pid), off), extend);
            return;
        }
        let order: i64 = self.adapter.borrow().bodyOrderOf(pid);
        let count: i64 = self.adapter.borrow().bodyParagraphCount();
        if order < 0 {
            return;
        }
        if order + 1 >= count {
            return;
        }
        let next: i64 = self.adapter.borrow().bodyParagraphIdAt(order + 1);
        self.placeCaret(next, 0, extend);
    }
    /// The visual line holding (paragraphId, offset), searched across every page
    /// so Up/Down and PageUp/PageDown can cross page boundaries. Fills
    /// [pageIndex, lineIndex]; page -1 when the paragraph is not laid out.
    ///
    /// Wrapping can drop trailing characters, so a caret at the very end of a
    /// paragraph may sit past the last laid line. Falling back to that
    /// paragraph's first / last line keeps the caret findable — without it,
    /// Ctrl+End reported "no line" and the view stayed on the wrong page.
    /// --- one word ------------------------------------------------------------
    ///
    /// Ctrl+Left / Ctrl+Right, which this editor did not have. The rule is
    /// `OfficeCaretMotion`'s, shared with the slide editor and the markdown
    /// one, so a change to what counts as a word reaches all three.
    ///
    /// Off the end of a paragraph it does what the arrow keys do — the previous
    /// paragraph's end, the next paragraph's start — because that part is not
    /// shared and cannot be: a slide's neighbour is in the same shape and this
    /// one's may be in another table cell on another page.
    pub fn moveWordLeft(&mut self, extend: bool) {
        if !self.isActive() {
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        if off > 0 {
            let at: i64 = OfficeCaretMotion::wordLeft(&self.adapter.borrow().getText(pid), off);
            if at < off {
                self.placeCaret(pid, at, extend);
                return;
            }
        }
        self.moveLeft(extend);
    }
    pub fn moveWordRight(&mut self, extend: bool) {
        if !self.isActive() {
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let off: i64 = self.sel.borrow().focus.borrow().textOffset;
        let __len: i64 = self.adapter.borrow().textLength(pid);
        if off < __len {
            let at: i64 = OfficeCaretMotion::wordRight(&self.adapter.borrow().getText(pid), off);
            if at > off {
                self.placeCaret(pid, at, extend);
                return;
            }
        }
        self.moveRight(extend);
    }
    /// The word under the caret, selected — what a double-click takes.
    pub fn selectWord(&mut self) {
        if !self.isActive() {
            return;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let mut ends: Vec<i64> = Vec::new();
        let mut _tmp_3228 = self.sel.borrow().focus.borrow().textOffset.clone();
        OfficeCaretMotion::wordAt(
            &self.adapter.borrow().getText(pid),
            _tmp_3228,
            &mut ends
        );
        if (ends.len() as i64) < 2 {
            return;
        }
        self.setRange(pid, ends[0], ends[1]);
    }
    /// caretLine as a result: [pageIndex, lineIndex], -1s when none.
    pub fn caretLineAt(&self, layout: &Rc<RefCell<DocxLayout>>) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        self.caretLine(layout, &mut out);
        out
    }
    /// findLineFor as a result: [pageIndex, lineIndex], -1s when none.
    pub fn lineFor(&self, layout: &Rc<RefCell<DocxLayout>>, pid: i64, off: i64) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        self.findLineFor(layout, pid, off, &mut out);
        out
    }
    pub fn findLineFor(&self, layout: &Rc<RefCell<DocxLayout>>, pid: i64, off: i64, mut out: &mut Vec<i64>) {
        out.clear();
        out.push(-1);
        out.push(-1);
        let mut first_page: i64 = -1;
        let mut first_line: i64 = -1;
        let mut first_start: i64 = 0;
        let mut last_page: i64 = -1;
        let mut last_line: i64 = -1;
        let mut last_end: i64 = 0;
        let mut pi: i64 = 0;
        while pi < layout.borrow().pageCount() {
            let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(pi);
            let mut li: i64 = 0;
            while li < (page.borrow().lines.len() as i64) {
                let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[li as usize].clone();
                if LaidLineKind::isBodyText(line.borrow().kind.clone()) {
                    if line.borrow().paraIndex == pid {
                        if (off >= line.borrow().docStart && off <= line.borrow().docEnd) {
                            out.clear();
                            out.push(pi);
                            out.push(li);
                            return;
                        }
                        if first_page < 0 {
                            first_page = pi;
                            first_line = li;
                            first_start = line.borrow().docStart;
                        }
                        last_page = pi;
                        last_line = li;
                        last_end = line.borrow().docEnd;
                    }
                }
                li += 1;
            }
            pi += 1;
        }
        if last_page < 0 {
            return;
        }
        out.clear();
        if off < first_start {
            out.push(first_page);
            out.push(first_line);
            return;
        }
        out.push(last_page);
        out.push(last_line);
    }
    pub fn caretLine(&self, layout: &Rc<RefCell<DocxLayout>>, mut out: &mut Vec<i64>) {
        out.clear();
        out.push(-1);
        out.push(-1);
        if !self.isActive() {
            return;
        }
        let mut _tmp_3231 = self.sel.borrow().focus.borrow().paragraphId.clone();
        self.findLineFor(layout, _tmp_3231, self.sel.borrow().focus.borrow().textOffset.clone(), &mut out);
    }
    /// Body lines in document order, flattened across pages. Returns the flat
    /// index of the caret's line, or -1.
    /// Lines the caret can occupy, in document order across pages. Picture lines
    /// count when their paragraph holds nothing else, so ↑ / ↓ can step onto a
    /// standalone image and Enter can add a line next to it.
    pub fn caretStopLine(&self, line: &Rc<RefCell<LaidLine>>) -> bool {
        if line.borrow().paraIndex <= 0 {
            return false;
        }
        if LaidLineKind::isBodyText(line.borrow().kind.clone()) {
            return true;
        }
        if line.borrow().kind == LaidLineKind::image() {
            let mut p: Rc<RefCell<Paragraph>> = line.borrow().paragraphRef.clone().unwrap();
            if p.borrow().text.is_empty() {
                return true;
            }
        }
        false
    }
    pub fn flatBodyLines(&self, layout: &Rc<RefCell<DocxLayout>>, mut out_page: &mut Vec<i64>, mut out_line: &mut Vec<i64>) {
        out_page.clear();
        out_line.clear();
        let mut pi: i64 = 0;
        while pi < layout.borrow().pageCount() {
            let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(pi);
            let mut li: i64 = 0;
            while li < (page.borrow().lines.len() as i64) {
                let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[li as usize].clone();
                if self.caretStopLine(&line) {
                    // An image-only paragraph also emits an empty text line;
                    // keep just one stop for it.
                    let mut dup: bool = false;
                    if line.borrow().kind == LaidLineKind::image() {
                        let mut k: i64 = 0;
                        while k < (out_page.len() as i64) {
                            if out_page[k as usize] == pi {
                                let mut prev_page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(pi);
                                let mut prev_line: Rc<RefCell<LaidLine>> = prev_page.borrow().lines[(out_line[k as usize]) as usize].clone();
                                if prev_line.borrow().paraIndex == line.borrow().paraIndex {
                                    dup = true;
                                }
                            }
                            k += 1;
                        }
                    }
                    if !dup {
                        out_page.push(pi);
                        out_line.push(li);
                    }
                }
                li += 1;
            }
            pi += 1;
        }
    }
    pub fn flatIndexOfCaret(&self, layout: &Rc<RefCell<DocxLayout>>, pages: &[i64], lines: &[i64]) -> i64 {
        let mut cur: Vec<i64> = Vec::new();
        self.caretLine(layout, &mut cur);
        let cp: i64 = cur[0];
        let cl: i64 = cur[1];
        if cp < 0 {
            return -1;
        }
        let mut i: i64 = 0;
        while i < (pages.len() as i64) {
            if pages[i as usize] == cp && lines[i as usize] == cl {
                return i;
            }
            i += 1;
        }
        -1
    }
    /// Put the caret on `line` at the horizontal position `wantX` (screen px).
    pub fn caretToLineAtX(&mut self, layout: &Rc<RefCell<DocxLayout>>, line: &Rc<RefCell<LaidLine>>, want_x: i64, extend: bool) {
        let mut p: Rc<RefCell<Paragraph>> = line.borrow().paragraphRef.clone().unwrap();
        if p.borrow().id <= 0 {
            return;
        }
        let mut tr: Rc<RefCell<UITextRenderer>> = layout.borrow().tr.clone().unwrap();
        let font_px: f64 = line.borrow().fontSizePx;
        DocxTextMetrics::useFamily(&tr, &line.borrow().fontFamily.clone());
        let mut local_x: i64 = want_x - line.borrow().x;
        if local_x < 0 {
            local_x = 0;
        }
        let display: String = line.borrow().text.clone();
        if !line.borrow().listPrefix.is_empty() {
            if DxNative::index_of(&display, &line.borrow().listPrefix) == 0 {
                local_x -= DocxTextMetrics::measurePlain(
                    &tr,
                    &line.borrow().listPrefix,
                    font_px,
                    line.borrow().bold
                );
                if local_x < 0 {
                    local_x = 0;
                }
            }
        }
        let mut _tmp_3235 = line.borrow().docStart.clone();
        let off: i64 = DocxTextMetrics::offsetAtX(&tr, &p, font_px, _tmp_3235, line.borrow().docEnd.clone(), local_x);
        self.placeCaret(p.borrow().id.clone(), off, extend);
    }
    /// Vertical movement by `delta` visual lines, keeping the caret's X. Crossing
    /// a page boundary is just another step in the flattened line list.
    ///
    /// The clamp and the step are `OfficeLineNav`'s; the column the caret aims
    /// at is `OfficeCaretColumn`'s. What is left here is the part that is not
    /// shareable: which of this document's laid lines a caret may stop on, and
    /// how to turn an x into an offset in a Word paragraph.
    pub fn moveVertical(&mut self, layout: &Rc<RefCell<DocxLayout>>, delta: i64, extend: bool) {
        if !self.isActive() {
            return;
        }
        let mut pages: Vec<i64> = Vec::new();
        let mut lines: Vec<i64> = Vec::new();
        self.flatBodyLines(layout, &mut pages, &mut lines);
        let total: i64 = pages.len() as i64;
        if total == 0 {
            return;
        }
        let at: i64 = self.flatIndexOfCaret(layout, &pages, &lines);
        if at < 0 {
            return;
        }
        let mut now_x: i64 = self.caretPixelX(layout, pages[at as usize]);
        if now_x < 0 {
            let mut cur_page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(pages[at as usize]);
            let mut cur_line: Rc<RefCell<LaidLine>> = cur_page.borrow().lines[(lines[at as usize]) as usize].clone();
            now_x = cur_line.borrow().x;
        }
        // The column is remembered across a RUN of vertical moves and dropped
        // by anything else (see `placeCaret`), so Down through a short line and
        // back Up returns to where the reader was.
        let want_x: i64 = (OfficeCaretColumn::want(&self.column, now_x as f64)).floor() as i64;
        let target: i64 = OfficeLineNav::verticalTarget(total, at, delta);
        if target == at {
            // Already at the edge: snap to the start / end of the document.
            let mut edge_page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(pages[at as usize]);
            let mut edge_line: Rc<RefCell<LaidLine>> = edge_page.borrow().lines[(lines[at as usize]) as usize].clone();
            let mut p0: Rc<RefCell<Paragraph>> = edge_line.borrow().paragraphRef.clone().unwrap();
            if delta < 0 {
                let mut _tmp_3239 = p0.borrow().id.clone();
                self.placeCaret(_tmp_3239, edge_line.borrow().docStart.clone(), extend);
            } else {
                let mut _tmp_3241 = p0.borrow().id.clone();
                self.placeCaret(_tmp_3241, edge_line.borrow().docEnd.clone(), extend);
            }
            // Hitting the top or the bottom does not end the run: press Down
            // twice at the last line and then Up, and the column is still the
            // one you left.
            self.column.borrow_mut().wanted = want_x as f64;
            self.column.borrow_mut().known = true;
            return;
        }
        let mut t_page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(pages[target as usize]);
        let mut t_line: Rc<RefCell<LaidLine>> = t_page.borrow().lines[(lines[target as usize]) as usize].clone();
        self.caretToLineAtX(layout, &t_line, want_x, extend);
        self.column.borrow_mut().wanted = want_x as f64;
        self.column.borrow_mut().known = true;
    }
    pub fn moveUp(&mut self, layout: &Rc<RefCell<DocxLayout>>, extend: bool) {
        self.moveVertical(layout, -1, extend);
    }
    pub fn moveDown(&mut self, layout: &Rc<RefCell<DocxLayout>>, extend: bool) {
        self.moveVertical(layout, 1, extend);
    }
    /// Lines a PageUp / PageDown step covers: the caret's page, minus one line of
    /// overlap so the line you were reading stays visible.
    pub fn linesPerPage(&self, layout: &Rc<RefCell<DocxLayout>>) -> i64 {
        let mut cur: Vec<i64> = Vec::new();
        self.caretLine(layout, &mut cur);
        let mut cp: i64 = cur[0];
        if cp < 0 {
            cp = 0;
        }
        if cp >= layout.borrow().pageCount() {
            return 1;
        }
        let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(cp);
        let mut count: i64 = 0;
        let mut li: i64 = 0;
        while li < (page.borrow().lines.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[li as usize].clone();
            if LaidLineKind::isBodyText(line.borrow().kind.clone()) {
                if line.borrow().paraIndex > 0 {
                    count += 1;
                }
            }
            li += 1;
        }
        if count > 1 {
            count -= 1;
        }
        if count < 1 {
            count = 1;
        }
        count
    }
    pub fn pageUp(&mut self, layout: &Rc<RefCell<DocxLayout>>, extend: bool) {
        let __arg_0 = 0 - self.linesPerPage(layout);
        self.moveVertical(layout, __arg_0, extend);
    }
    pub fn pageDown(&mut self, layout: &Rc<RefCell<DocxLayout>>, extend: bool) {
        self.moveVertical(layout, self.linesPerPage(layout), extend);
    }
    /// Home / End work on the VISUAL line, so a wrapped paragraph behaves the way
    /// it looks rather than jumping to the paragraph ends.
    pub fn moveLineEdge(&mut self, layout: &Rc<RefCell<DocxLayout>>, to_end: bool, extend: bool) {
        if !self.isActive() {
            return;
        }
        let mut cur: Vec<i64> = Vec::new();
        self.caretLine(layout, &mut cur);
        let cp: i64 = cur[0];
        let cl: i64 = cur[1];
        if cp < 0 {
            let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
            if to_end {
                self.placeCaret(pid, self.adapter.borrow().textLength(pid), extend);
            } else {
                self.placeCaret(pid, 0, extend);
            }
            return;
        }
        let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(cp);
        let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[cl as usize].clone();
        let mut p: Rc<RefCell<Paragraph>> = line.borrow().paragraphRef.clone().unwrap();
        if to_end {
            let mut _tmp_3246 = p.borrow().id.clone();
            self.placeCaret(_tmp_3246, line.borrow().docEnd.clone(), extend);
        } else {
            let mut _tmp_3248 = p.borrow().id.clone();
            self.placeCaret(_tmp_3248, line.borrow().docStart.clone(), extend);
        }
    }
    pub fn moveDocEdge(&mut self, to_end: bool, extend: bool) {
        if !self.isActive() {
            return;
        }
        let count: i64 = self.adapter.borrow().bodyParagraphCount();
        if count == 0 {
            return;
        }
        if to_end {
            let last_id: i64 = self.adapter.borrow().bodyParagraphIdAt(count - 1);
            self.placeCaret(last_id, self.adapter.borrow().textLength(last_id), extend);
        } else {
            self.placeCaret(self.adapter.borrow().bodyParagraphIdAt(0), 0, extend);
        }
    }
    /// --- Tab: move between table cells ---------------------------------------
    /// Word's Tab in a table steps to the next cell and SELECTS its contents, so
    /// typing replaces them; Shift+Tab steps back. Wrapping past the last cell of
    /// a row continues on the next row, and past the last cell of the table it
    /// leaves for the paragraph after it.
    pub fn selectCellContents(&mut self, pid: i64) {
        if pid <= 0 {
            return;
        }
        let __len: i64 = self.adapter.borrow().textLength(pid);
        self.sel.borrow_mut().anchor = DocumentPosition::create(pid, 0);
        self.sel.borrow_mut().focus = DocumentPosition::create(pid, __len);
        self.active = true;
    }
    pub fn inTable(&self) -> bool {
        if !self.isActive() {
            return false;
        }
        let mut at: Vec<i64> = self.adapter.borrow().tableCellOf(self.sel.borrow().focus.borrow().paragraphId.clone());
        at[0] >= 0
    }
    /// Returns true when the Tab was consumed as cell navigation.
    pub fn tabCell(&mut self, forward: bool) -> bool {
        if !self.isActive() {
            return false;
        }
        let mut at: Vec<i64> = self.adapter.borrow().tableCellOf(self.sel.borrow().focus.borrow().paragraphId.clone());
        let blk: i64 = at[0];
        if blk < 0 {
            // Not in a table. Tab just before one steps into its first cell,
            // Shift+Tab just after one into its last — otherwise leave it alone.
            let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
            if forward {
                let next_tbl: i64 = self.adapter.borrow().tableBlockAfterParagraph(pid);
                if next_tbl >= 0 {
                    let first_id: i64 = self.adapter.borrow().cellParagraphId(next_tbl, 0, 0);
                    if first_id > 0 {
                        self.selectCellContents(first_id);
                        return true;
                    }
                }
                return false;
            }
            let prev_tbl: i64 = self.adapter.borrow().tableBlockBeforeParagraph(pid);
            if prev_tbl >= 0 {
                let last_row: i64 = self.adapter.borrow().rowCountOfTable(prev_tbl) - 1;
                let last_cell: i64 = self.adapter.borrow().cellCountInRow(prev_tbl, last_row) - 1;
                let last_id: i64 = self.adapter.borrow().cellParagraphId(prev_tbl, last_row, last_cell);
                if last_id > 0 {
                    self.selectCellContents(last_id);
                    return true;
                }
            }
            return false;
        }
        let row: i64 = at[1];
        let cell: i64 = at[2];
        let rows: i64 = self.adapter.borrow().rowCountOfTable(blk);
        if forward {
            let mut next_cell: i64 = cell + 1;
            let mut next_row: i64 = row;
            if next_cell >= self.adapter.borrow().cellCountInRow(blk, row) {
                next_cell = 0;
                next_row = row + 1;
            }
            if next_row >= rows {
                // Word never tabs out the bottom of a table: it adds a row.
                let added: i64 = self.adapter.borrow().appendTableRow(blk);
                if added <= 0 {
                    return true;
                }
                let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
                op.borrow_mut().kind = DocEditKind::tableRow();
                op.borrow_mut().start = blk;
                self.pushUndo(op.clone());
                let new_id: i64 = self.adapter.borrow().cellParagraphId(blk, added - 1, 0);
                if new_id > 0 {
                    self.setCaret(new_id, 0);
                }
                return true;
            }
            let nid: i64 = self.adapter.borrow().cellParagraphId(blk, next_row, next_cell);
            if nid > 0 {
                self.selectCellContents(nid);
            }
            return true;
        }
        let mut prev_cell: i64 = cell - 1;
        let mut prev_row: i64 = row;
        if prev_cell < 0 {
            prev_row = row - 1;
            if prev_row >= 0 {
                prev_cell = self.adapter.borrow().cellCountInRow(blk, prev_row) - 1;
            }
        }
        if prev_row < 0 {
            let before: i64 = self.adapter.borrow().bodyParagraphBeforeBlock(blk);
            if before > 0 {
                self.setCaret(before, self.adapter.borrow().textLength(before));
                return true;
            }
            return true;
        }
        let pid2: i64 = self.adapter.borrow().cellParagraphId(blk, prev_row, prev_cell);
        if pid2 > 0 {
            self.selectCellContents(pid2);
        }
        true
    }
    pub fn selectAll(&mut self) {
        let count: i64 = self.adapter.borrow().bodyParagraphCount();
        if count == 0 {
            return;
        }
        let first_id: i64 = self.adapter.borrow().bodyParagraphIdAt(0);
        let last_id: i64 = self.adapter.borrow().bodyParagraphIdAt(count - 1);
        self.sel.borrow_mut().anchor = DocumentPosition::create(first_id, 0);
        self.sel.borrow_mut().focus = DocumentPosition::create(last_id, self.adapter.borrow().textLength(last_id));
        self.active = true;
    }
    /// --- paragraph formatting ---------------------------------------------------
    /// The paragraphs the selection touches, in document order; the caret's
    /// paragraph alone when nothing is selected. A paragraph in a table cell is
    /// not in the body order, so a selection inside one is that paragraph.
    pub fn selectedParagraphIds(&self, mut out: &mut Vec<i64>) {
        out.clear();
        let a_pid: i64 = self.sel.borrow().anchor.borrow().paragraphId;
        let b_pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        if a_pid <= 0 {
            return;
        }
        if a_pid == b_pid {
            out.push(a_pid);
            return;
        }
        let mut ao: i64 = self.adapter.borrow().bodyOrderOf(a_pid);
        let mut bo: i64 = self.adapter.borrow().bodyOrderOf(b_pid);
        if ao < 0 || bo < 0 {
            out.push(b_pid);
            return;
        }
        if ao > bo {
            let t: i64 = ao;
            ao = bo;
            bo = t;
        }
        let mut i: i64 = ao;
        while i <= bo {
            out.push(self.adapter.borrow().bodyParagraphIdAt(i));
            i += 1;
        }
    }
    /// Alignment ("align", "0".."3") or a style ("style", its id) on every
    /// selected paragraph, as one undoable action.
    pub fn formatParagraphs(&self, what: &str, arg: &str, resolver: &Rc<RefCell<WordStyleResolver>>) -> bool {
        if !self.isActive() {
            return false;
        }
        let mut ids: Vec<i64> = Vec::new();
        self.selectedParagraphIds(&mut ids);
        if ids.is_empty() {
            return false;
        }
        let mut list_num: i64 = 0;
        if what == "list" {
            list_num = self.listTarget(&ids, arg);
        }
        self.beginAction();
        let mut i: i64 = 0;
        while i < (ids.len() as i64) {
            let pid: i64 = ids[i as usize];
            if self.adapter.borrow().paragraphExists(pid) {
                let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
                let before: String = DocxParaFormat::snapshot(&p);
                if what == "align" {
                    DocxParaFormat::setAlign(&p, DocxParaFormat::int(arg));
                }
                if what == "style" {
                    DocxParaFormat::applyStyle(&p, arg.to_string(), resolver);
                }
                if what == "list" {
                    if list_num < 0 {
                        DocxParaFormat::leaveList(&p, resolver);
                    } else {
                        if p.borrow().listNumId <= 0 {
                            p.borrow_mut().listLevel = 0;
                        }
                        p.borrow_mut().listNumId = list_num;
                        self.numbering.clone().borrow().applyToParagraph(&p);
                    }
                }
                if what == "indent" || what == "outdent" {
                    let mut step: i64 = 1;
                    if what == "outdent" {
                        step = -1;
                    }
                    if p.borrow().listNumId > 0 {
                        let mut lvl: i64 = p.borrow().listLevel + step;
                        if lvl < 0 {
                            lvl = 0;
                        }
                        if lvl > 8 {
                            lvl = 8;
                        }
                        p.borrow_mut().listLevel = lvl;
                        self.numbering.clone().borrow().applyToParagraph(&p);
                    } else {
                        let _tmp_3289 = DocxParaFormat::nextStop(p.borrow().indentLeftPt.clone(), step);
                        p.borrow_mut().indentLeftPt = _tmp_3289;
                    }
                }
                if what == "spacing" {
                    p.borrow_mut().lineSpacing = DocxParaFormat::num(arg);
                }
                if what == "before" {
                    p.borrow_mut().spaceBeforePt = DocxParaFormat::num(arg);
                }
                if what == "after" {
                    p.borrow_mut().spaceAfterPt = DocxParaFormat::num(arg);
                }
                let after: String = DocxParaFormat::snapshot(&p);
                if after != before {
                    let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
                    op.borrow_mut().kind = DocxParaFormat::opKind();
                    op.borrow_mut().paragraphId = pid;
                    op.borrow_mut().oldText = before.clone();
                    op.borrow_mut().newText = after.clone();
                    self.pushUndo(op.clone());
                }
            }
            i += 1;
        }
        self.endAction();
        RichDocument::touch(&self.adapter.borrow().doc.clone());
        true
    }
    /// The list the list buttons put the selection in: -1 takes it out of
    /// lists, because every selected paragraph already is in one like it.
    /// `arg` is a kind ("bullet", "number") or a look ("decimal:%1)").
    /// A list of the same look right above, or one a selected paragraph is
    /// in, is continued; otherwise a new one starts.
    pub fn listTarget(&self, ids: &[i64], arg: &str) -> i64 {
        if arg == "none" {
            return -1;
        }
        let by_kind: bool = DxNative::index_of(&arg, ":") < 0;
        let mut all_on: bool = true;
        let mut i: i64 = 0;
        while i < (ids.len() as i64) {
            let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(ids[i as usize]);
            if !self.listMatches(p.borrow().listNumId.clone(), arg, by_kind) {
                all_on = false;
            }
            i += 1;
        }
        if all_on {
            return -1;
        }
        let o: i64 = self.adapter.borrow().bodyOrderOf(ids[0]);
        if o > 0 {
            let _tmp_3299 = self.adapter.borrow().bodyParagraphIdAt(o - 1);
            let mut above: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(_tmp_3299);
            if self.listMatches(above.borrow().listNumId.clone(), arg, by_kind) {
                return above.borrow().listNumId;
            }
        }
        i = 0;
        while i < (ids.len() as i64) {
            let mut q: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(ids[i as usize]);
            if self.listMatches(q.borrow().listNumId.clone(), arg, by_kind) {
                return q.borrow().listNumId;
            }
            i += 1;
        }
        let mut look: String = arg.to_string();
        if arg == "bullet" {
            look = "bullet:•".to_string();
        }
        if arg == "number" {
            look = "decimal:%1.".to_string();
        }
        return self.numbering.borrow_mut().newList(&look);
    }
    pub fn listMatches(&self, num_id: i64, arg: &str, by_kind: bool) -> bool {
        if num_id <= 0 {
            return false;
        }
        if by_kind {
            return self.numbering.borrow().kindOf(num_id) == arg;
        }
        return self.numbering.borrow().styleOf(num_id) == arg;
    }
    /// What the list buttons show: "bullet", "number" or "".
    pub fn caretListKind(&self) -> String {
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        if pid <= 0 {
            return String::new();
        }
        if !self.adapter.borrow().paragraphExists(pid) {
            return String::new();
        }
        let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
        return self.numbering.borrow().kindOf(p.borrow().listNumId.clone());
    }
    pub fn caretListStyle(&self) -> String {
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        if pid <= 0 {
            return String::new();
        }
        if !self.adapter.borrow().paragraphExists(pid) {
            return String::new();
        }
        let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
        return self.numbering.borrow().styleOf(p.borrow().listNumId.clone());
    }
    fn caretParagraph(&self) -> Rc<RefCell<Paragraph>> {
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        if pid > 0 {
            if self.adapter.borrow().paragraphExists(pid) {
                return self.adapter.borrow().findParagraph(pid);
            }
        }
        Rc::new(RefCell::new(Paragraph::new()))
    }
    /// --- format painter, page break, a new table --------------------------------
    /// Picks up the look at the caret (the run after it, and its paragraph).
    pub fn painterPick(&mut self, sticky: bool) -> bool {
        if !self.isActive() {
            return false;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        if !self.adapter.borrow().paragraphExists(pid) {
            return false;
        }
        let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
        let mut off: i64 = DocumentRange::startOffset(&self.sel);
        if !DocumentRange::sameParagraph(&self.sel) {
            off = 0;
        }
        let mut sp: Rc<RefCell<TextSpan>> = self.adapter.borrow().spanAt(&p, off);
        self.painterSpan = TextSpan::copy(&sp);
        self.painterPara = Rc::new(RefCell::new(Paragraph::new()));
        Paragraph::copyFormatTo(&p, &self.painterPara.clone());
        self.painterArmed = true;
        self.painterSticky = sticky;
        true
    }
    pub fn painterOff(&mut self) {
        self.painterArmed = false;
        self.painterSticky = false;
    }
    pub fn paintRange(&self, pid: i64, a: i64, b: i64) {
        let mut sp: Rc<RefCell<TextSpan>> = self.painterSpan.clone().clone();
        let mut _tmp_3325 = sp.borrow().bold.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "bold", _tmp_3325, 0.0, String::new());
        let mut _tmp_3328 = sp.borrow().italic.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "italic", _tmp_3328, 0.0, String::new());
        let mut _tmp_3331 = sp.borrow().underline.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "underline", _tmp_3331, 0.0, String::new());
        let mut _tmp_3334 = sp.borrow().strike.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "strike", _tmp_3334, 0.0, String::new());
        let __arg_0 = sp.borrow().vertAlign == 1;
        self.adapter.clone().borrow().applyRun(pid, a, b, "super", __arg_0, 0.0, String::new());
        if sp.borrow().vertAlign == 2 {
            self.adapter.clone().borrow().applyRun(pid, a, b, "sub", true, 0.0, String::new());
        }
        let mut _tmp_3341 = sp.borrow().highlight.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "highlight", false, 0.0, _tmp_3341);
        let mut _tmp_3344 = sp.borrow().fontSizePt.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "size", false, _tmp_3344, String::new());
        let mut _tmp_3347 = sp.borrow().fontFamily.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "font", false, 0.0, _tmp_3347);
        let mut _tmp_3350 = sp.borrow().colorHex.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "color", false, 0.0, _tmp_3350);
    }
    /// A whole paragraph painted takes the source paragraph's look as well.
    fn paintParagraph(&self, p: &Rc<RefCell<Paragraph>>) {
        let mut q: Rc<RefCell<Paragraph>> = self.painterPara.clone().clone();
        p.borrow_mut().styleId = q.borrow().styleId.clone();
        p.borrow_mut().align = q.borrow().align;
        p.borrow_mut().spaceBeforePt = q.borrow().spaceBeforePt;
        p.borrow_mut().spaceAfterPt = q.borrow().spaceAfterPt;
        p.borrow_mut().lineSpacing = q.borrow().lineSpacing;
        p.borrow_mut().indentLeftPt = q.borrow().indentLeftPt;
        p.borrow_mut().indentRightPt = q.borrow().indentRightPt;
        p.borrow_mut().indentFirstLinePt = q.borrow().indentFirstLinePt;
        p.borrow_mut().listNumId = q.borrow().listNumId;
        p.borrow_mut().listLevel = q.borrow().listLevel;
        p.borrow_mut().listKind = q.borrow().listKind;
        p.borrow_mut().listFmt = q.borrow().listFmt.clone();
        p.borrow_mut().listText = q.borrow().listText.clone();
    }
    /// Ctrl+Enter: what follows the caret starts a new page.
    pub fn insertPageBreak(&mut self) -> bool {
        if !self.isActive() {
            return false;
        }
        self.beginAction();
        self.splitAtCaret();
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
        let before: String = DocxParaFormat::snapshot(&p);
        p.borrow_mut().pageBreakBefore = true;
        let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
        op.borrow_mut().kind = DocxParaFormat::opKind();
        op.borrow_mut().paragraphId = pid;
        op.borrow_mut().oldText = before.clone();
        op.borrow_mut().newText = DocxParaFormat::snapshot(&p);
        self.pushUndo(op.clone());
        self.endAction();
        RichDocument::touch(&self.adapter.borrow().doc.clone());
        true
    }
    /// An empty table at the caret, spread across the text column.
    pub fn insertTable(&mut self, rows: i64, cols: i64) -> bool {
        if rows < 1 || cols < 1 {
            return false;
        }
        let mut t: Rc<RefCell<ClipboardTable>> = Rc::new(RefCell::new(ClipboardTable::new()));
        let mut r: i64 = 0;
        while r < rows {
            let mut row: Rc<RefCell<ClipRow>> = Rc::new(RefCell::new(ClipRow::new()));
            let mut c: i64 = 0;
            while c < cols {
                row.borrow_mut().cells.push(Rc::new(RefCell::new(ClipCell::new())));
                c += 1;
            }
            t.borrow_mut().rows.push(row.clone());
            r += 1;
        }
        t.borrow_mut().ok = true;
        self.beginAction();
        let done: bool = self.pasteTable(&t);
        self.endAction();
        done
    }
    /// Where the query next occurs in `text` at or after `from`, or -1.
    pub fn matchIn(&self, text: &str, from: i64) -> i64 {
        let mut q: String = self.findQuery.clone();
        let mut t: String = text.to_string();
        if !self.findMatchCase {
            q = q.to_lowercase();
            t = t.to_lowercase();
        }
        let qn: i64 = DxNative::str_len(&q);
        let mut at: i64 = DxNative::index_of_from(&t, &q, from);
        while at >= 0 {
            if !self.findWholeWord {
                return at;
            }
            let mut ok_l: bool = true;
            if at > 0 {
                ok_l = !(DocxEditController::isWordChar(DxNative::char_at(&t, at - 1)));
            }
            let mut ok_r: bool = true;
            if at + qn < (DxNative::str_len(&t)) {
                ok_r = !(DocxEditController::isWordChar(DxNative::char_at(&t, at + qn)));
            }
            if ok_l && ok_r {
                return at;
            }
            at = DxNative::index_of_from(&t, &q, at + 1);
        }
        -1
    }
    /// From just after the selection to the end, then from the top. Returns
    /// true when the selection landed on a match.
    pub fn findNext(&mut self) -> bool {
        if self.findQuery.is_empty() {
            self.findStatus = "nothing to find".to_string();
            return false;
        }
        let n: i64 = self.adapter.borrow().bodyParagraphCount();
        if n == 0 {
            self.findStatus = format!("no match for '{}'", self.findQuery);
            return false;
        }
        let mut start_order: i64 = 0;
        let mut start_off: i64 = 0;
        if self.isActive() {
            let o: i64 = self.adapter.borrow().bodyOrderOf(self.sel.borrow().focus.borrow().paragraphId.clone());
            if o >= 0 {
                start_order = o;
                start_off = self.sel.borrow().focus.borrow().textOffset;
                if !DocumentRange::isCollapsed(&self.sel) {
                    let mut b: Vec<i64> = Vec::new();
                    let mut a: Vec<i64> = Vec::new();
                    self.orderedEnds(&mut a, &mut b);
                    start_order = self.adapter.borrow().bodyOrderOf(b[0]);
                    start_off = b[1];
                }
            }
        }
        let mut step: i64 = 0;
        while step <= n {
            let order: i64 = (start_order + step) % n;
            let pid: i64 = self.adapter.borrow().bodyParagraphIdAt(order);
            let mut from: i64 = 0;
            if step == 0 {
                from = start_off;
            }
            let mut at: i64 = self.matchIn(&self.adapter.borrow().getText(pid),
                from
            );
            if step == n && at >= start_off {
                at = -1;
            }
            if at >= 0 {
                self.setRange(pid, at, at + (DxNative::str_len(&self.findQuery)));
                self.findStatus = "found".to_string();
                return true;
            }
            step += 1;
        }
        self.findStatus = format!("no match for '{}'", self.findQuery);
        false
    }
    /// Is the selection exactly a match?
    pub fn selectionMatches(&self) -> bool {
        if !self.isActive() {
            return false;
        }
        if DocumentRange::isCollapsed(&self.sel) || !DocumentRange::sameParagraph(&self.sel) {
            return false;
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let a: i64 = DocumentRange::startOffset(&self.sel);
        let b: i64 = DocumentRange::endOffset(&self.sel);
        if b - a != (DxNative::str_len(&self.findQuery)) {
            return false;
        }
        return self.matchIn(&self.adapter.borrow().getText(pid),
            a
        ) == a;
    }
    /// One match, replaced in the look of the text it replaces.
    pub fn replaceAt(&mut self, pid: i64, a: i64, b: i64) {
        self.setCaret(pid, a);
        self.adoptRunAtCaret();
        self.setRange(pid, a, b);
        if self.replaceWith.is_empty() {
            self.deleteSelection();
        } else {
            self.insertText(self.replaceWith.clone());
        }
    }
    /// Replace the selection if it is a match, then go to the next: Word's
    /// Replace button.
    pub fn replaceCurrent(&mut self) -> bool {
        if !self.selectionMatches() {
            return self.findNext();
        }
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        let a: i64 = DocumentRange::startOffset(&self.sel);
        let b: i64 = DocumentRange::endOffset(&self.sel);
        self.beginAction();
        self.replaceAt(pid, a, b);
        self.endAction();
        RichDocument::touch(&self.adapter.borrow().doc.clone());
        let _found: bool = self.findNext();
        self.findStatus = "replaced 1".to_string();
        true
    }
    /// Every match in the body, as one undoable action.
    pub fn replaceAll(&mut self) -> i64 {
        if self.findQuery.is_empty() {
            self.findStatus = "nothing to find".to_string();
            return 0;
        }
        let mut count: i64 = 0;
        let mut last_pid: i64 = 0;
        let mut last_off: i64 = 0;
        self.beginAction();
        let n: i64 = self.adapter.borrow().bodyParagraphCount();
        let mut i: i64 = 0;
        while i < n {
            let pid: i64 = self.adapter.borrow().bodyParagraphIdAt(i);
            let mut at: i64 = self.matchIn(&self.adapter.borrow().getText(pid),
                0
            );
            while at >= 0 {
                self.replaceAt(pid, at, at + (DxNative::str_len(&self.findQuery)));
                count += 1;
                last_pid = pid;
                last_off = at + (DxNative::str_len(&self.replaceWith));
                at = self.matchIn(&self.adapter.borrow().getText(pid),
                    last_off
                );
            }
            i += 1;
        }
        self.endAction();
        if count > 0 {
            self.setCaret(last_pid, last_off);
            RichDocument::touch(&self.adapter.borrow().doc.clone());
        }
        self.findStatus = format!("replaced {} of '{}'", count, self.findQuery);
        count
    }
    /// --- run formatting, undoable ---------------------------------------------
    /// The selection as (paragraph, from, to) triples in document order.
    pub fn selectionRanges(&self, mut pids: &mut Vec<i64>, mut froms: &mut Vec<i64>, mut tos: &mut Vec<i64>) {
        pids.clear();
        froms.clear();
        tos.clear();
        if DocumentRange::isCollapsed(&self.sel) {
            return;
        }
        if DocumentRange::sameParagraph(&self.sel) {
            pids.push(self.sel.borrow().anchor.borrow().paragraphId);
            froms.push(DocumentRange::startOffset(&self.sel));
            tos.push(DocumentRange::endOffset(&self.sel));
            return;
        }
        let mut a: Vec<i64> = Vec::new();
        let mut b: Vec<i64> = Vec::new();
        self.orderedEnds(&mut a, &mut b);
        let s_order: i64 = self.adapter.borrow().bodyOrderOf(a[0]);
        let e_order: i64 = self.adapter.borrow().bodyOrderOf(b[0]);
        if s_order < 0 || e_order < 0 {
            return;
        }
        let mut o: i64 = s_order;
        while o <= e_order {
            let pid: i64 = self.adapter.borrow().bodyParagraphIdAt(o);
            let __len: i64 = self.adapter.borrow().textLength(pid);
            let mut x0: i64 = 0;
            let mut x1: i64 = __len;
            if o == s_order {
                x0 = a[1];
            }
            if o == e_order {
                x1 = b[1];
            }
            if x1 > __len {
                x1 = __len;
            }
            if x0 < x1 {
                pids.push(pid);
                froms.push(x0);
                tos.push(x1);
            }
            o += 1;
        }
    }
    /// One character-formatting command over the selection, recorded as one
    /// undoable action: the touched paragraphs' formatting before and after.
    /// With nothing selected a toggle sets what the next keystroke types.
    ///
    ///   bold italic underline strike super sub   toggles
    ///   highlight <name|"">  color <hex>  size <pt>  font <name>
    ///   clear      back to the paragraph style's run
    ///   case       lower → UPPER → Title → lower
    pub fn formatRuns(&mut self, what: &str, arg: String, resolver: &Rc<RefCell<WordStyleResolver>>) -> bool {
        if !self.isActive() {
            return false;
        }
        let mut ids: Vec<i64> = Vec::new();
        self.selectedParagraphIds(&mut ids);
        let mut befores: Vec<String> = Vec::new();
        let mut i: i64 = 0;
        while i < (ids.len() as i64) {
            let mut p0: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(ids[i as usize]);
            befores.push(DocxParaFormat::snapshot(&p0));
            i += 1;
        }
        let mut pids: Vec<i64> = Vec::new();
        let mut froms: Vec<i64> = Vec::new();
        let mut tos: Vec<i64> = Vec::new();
        self.selectionRanges(&mut pids, &mut froms, &mut tos);
        self.beginAction();
        self.suppressUndo = true;
        let toggles: bool = ((((what == "bold" || what == "italic") || what == "underline") || what == "strike") || what == "super") || what == "sub";
        if toggles {
            self.toggleRun(what);
        }
        if what == "highlight" {
            if DocumentRange::isCollapsed(&self.sel) {
                self.typingHighlight = arg.clone();
            }
            self.applyToSelection("highlight", false, 0.0, &arg);
        }
        if what == "color" {
            self.setRunColor(arg.clone());
        }
        if what == "size" {
            self.setRunSize(DocxParaFormat::num(&arg));
        }
        if what == "font" {
            self.setFont(arg.clone());
        }
        if what == "clear" {
            self.typingBold = false;
            self.typingItalic = false;
            self.typingUnderline = false;
            self.typingStrike = false;
            self.typingVert = 0;
            self.typingHighlight = String::new();
            let mut k: i64 = 0;
            while k < (pids.len() as i64) {
                let mut cp: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pids[k as usize]);
                let mut ps: Rc<RefCell<ResolvedParagraphStyle>> = resolver.borrow().resolveParagraphStyle(cp.borrow().styleId.clone());
                let mut _tmp_3398 = ps.borrow().run.clone();
                self.clearRange(pids[k as usize],
                    froms[k as usize],
                    tos[k as usize],
                    &_tmp_3398
                );
                k += 1;
            }
        }
        if what == "paint" {
            let mut m: i64 = 0;
            while m < (pids.len() as i64) {
                let mp: i64 = pids[m as usize];
                self.paintRange(mp, froms[m as usize], tos[m as usize]);
                if froms[m as usize] == 0 && tos[m as usize] == self.adapter.borrow().textLength(mp) {
                    self.paintParagraph(&self.adapter.borrow().findParagraph(mp).clone());
                }
                m += 1;
            }
            if !self.painterSticky {
                self.painterArmed = false;
            }
        }
        self.suppressUndo = false;
        if what == "case" {
            self.cycleCase(&pids, &froms, &tos, &arg);
        }
        let mut j: i64 = 0;
        while j < (ids.len() as i64) {
            let pid: i64 = ids[j as usize];
            let mut p1: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
            let after: String = DocxParaFormat::snapshot(&p1);
            let before: String = befores[j as usize].clone();
            if after != before {
                let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
                op.borrow_mut().kind = DocxParaFormat::opKind();
                op.borrow_mut().paragraphId = pid;
                op.borrow_mut().oldText = before.clone();
                op.borrow_mut().newText = after.clone();
                self.pushUndo(op.clone());
            }
            j += 1;
        }
        self.endAction();
        RichDocument::touch(&self.adapter.borrow().doc.clone());
        true
    }
    pub fn clearRange(&self, pid: i64, a: i64, b: i64, base: &Rc<RefCell<ResolvedRunStyle>>) {
        let mut _tmp_3404 = base.borrow().bold.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "bold", _tmp_3404, 0.0, String::new());
        let mut _tmp_3407 = base.borrow().italic.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "italic", _tmp_3407, 0.0, String::new());
        let mut _tmp_3410 = base.borrow().underline.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "underline", _tmp_3410, 0.0, String::new());
        let mut _tmp_3413 = base.borrow().strike.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "strike", _tmp_3413, 0.0, String::new());
        self.adapter.clone().borrow().applyRun(pid, a, b, "super", false, 0.0, String::new());
        self.adapter.clone().borrow().applyRun(pid, a, b, "highlight", false, 0.0, String::new());
        let mut _tmp_3420 = base.borrow().fontSizePt.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "size", false, _tmp_3420, String::new());
        let mut _tmp_3423 = base.borrow().fontFamily.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "font", false, 0.0, _tmp_3423);
        let mut _tmp_3426 = base.borrow().colorHex.clone();
        self.adapter.clone().borrow().applyRun(pid, a, b, "color", false, 0.0, _tmp_3426);
    }
    /// Change case across the selection. What it becomes is decided from the
    /// whole selection, so one press turns a mixed selection uniformly.
    /// `pick` names the case ("upper", "lower", "title", "sentence",
    /// "toggle"); empty cycles UPPER, Title, lower as Shift+F3 does.
    pub fn cycleCase(&self, pids: &[i64], froms: &[i64], tos: &[i64], pick: &str) {
        let mut all: String = String::new();
        let mut k: i64 = 0;
        while k < (pids.len() as i64) {
            let t: String = self.adapter.borrow().getText(pids[k as usize]);
            all = format!("{}{}", all, DxNative::substr(&t, froms[k as usize], tos[k as usize]));
            k += 1;
        }
        let mut mode: String = "upper".to_string();
        if all == all.to_uppercase() {
            mode = "title".to_string();
        }
        if all == DocxParaFormat::titleCase(&all) && all != all.to_uppercase() {
            mode = "lower".to_string();
        }
        if !pick.is_empty() {
            mode = pick.to_string();
        }
        let mut m: i64 = 0;
        while m < (pids.len() as i64) {
            let pid: i64 = pids[m as usize];
            let a: i64 = froms[m as usize];
            let b: i64 = tos[m as usize];
            let text: String = self.adapter.borrow().getText(pid);
            let old: String = DxNative::substr(&text, a, b);
            let mut nw: String = old.to_lowercase();
            if mode == "upper" {
                nw = old.to_uppercase();
            }
            if mode == "title" {
                nw = DocxParaFormat::titleCase(&old);
            }
            if mode == "sentence" {
                nw = DocxParaFormat::sentenceCase(&old);
            }
            if mode == "toggle" {
                nw = DocxParaFormat::toggleCase(&old);
            }
            // Only when the length is unchanged: the spans are offsets into
            // the text, and a case change must not move them.
            if (DxNative::str_len(&nw)) == (DxNative::str_len(&old)) && nw != old {
                let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
                p.borrow_mut().text = format!("{}{}{}", DxNative::substr(&text, 0, a), nw, DxNative::substr(&text, b, (DxNative::str_len(&text))));
                let mut op: Rc<RefCell<DocEditOp>> = Rc::new(RefCell::new(DocEditOp::new()));
                op.borrow_mut().kind = DocxParaFormat::caseKind();
                op.borrow_mut().paragraphId = pid;
                op.borrow_mut().start = a;
                op.borrow_mut().oldText = old.clone();
                op.borrow_mut().newText = nw.clone();
                self.pushUndo(op.clone());
            }
            m += 1;
        }
    }
    /// The style of the caret's paragraph, for the gallery to show pressed.
    pub fn caretStyle(&self) -> String {
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        if pid <= 0 {
            return String::new();
        }
        if !self.adapter.borrow().paragraphExists(pid) {
            return String::new();
        }
        let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
        return p.borrow().styleId.clone();
    }
    pub fn caretAlign(&self) -> i64 {
        let pid: i64 = self.sel.borrow().focus.borrow().paragraphId;
        if pid <= 0 {
            return -1;
        }
        if !self.adapter.borrow().paragraphExists(pid) {
            return -1;
        }
        let mut p: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(pid);
        return p.borrow().align;
    }
    /// --- undo and redo, which are ONE function with a direction ---------------
    ///
    /// They used to be two switches over `op.kind`, and they drifted: `undo`
    /// knew five kinds and `redo` knew one. Redo of anything else did nothing to
    /// the document — and pushed the op back onto the undo stack anyway, so the
    /// NEXT undo ran it a second time and invented text that had never been
    /// there. Written as one function that cannot happen: a kind is handled in
    /// both directions or in neither.
    fn applyOp(&mut self, op: &Rc<RefCell<DocEditOp>>, forward: bool) -> bool {
        if op.borrow().kind == DocxParaFormat::caseKind() {
            if self.adapter.borrow().paragraphExists(op.borrow().paragraphId.clone()) {
                let mut cp: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(op.borrow().paragraphId.clone());
                let put: String = if forward { op.borrow().newText.clone() } else { op.borrow().oldText.clone() };
                let n: i64 = DxNative::str_len(&put);
                let tx: String = cp.borrow().text.clone();
                if op.borrow().start + n <= (DxNative::str_len(&tx)) {
                    cp.borrow_mut().text = format!("{}{}{}", DxNative::substr(&tx, 0, op.borrow().start), put, DxNative::substr(&tx, op.borrow().start + n, (DxNative::str_len(&tx))));
                }
                RichDocument::touch(&self.adapter.borrow().doc.clone());
                return true;
            }
            return false;
        }
        if op.borrow().kind == DocxParaFormat::opKind() {
            if self.adapter.borrow().paragraphExists(op.borrow().paragraphId.clone()) {
                let mut fp: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(op.borrow().paragraphId.clone());
                if forward {
                    DocxParaFormat::restore(&fp, &op.borrow().newText.clone());
                } else {
                    DocxParaFormat::restore(&fp, &op.borrow().oldText.clone());
                }
                RichDocument::touch(&self.adapter.borrow().doc.clone());
                self.setCaret(op.borrow().paragraphId.clone(), 0);
                return true;
            }
            return false;
        }
        if op.borrow().kind == DocEditKind::text() {
            if forward {
                let mut _tmp_3451 = op.borrow().paragraphId.clone();
                let mut _tmp_3452 = op.borrow().start.clone();
                let mut _tmp_3453 = op.borrow().newText.clone();
                let __arg_0 = op.borrow().start + (DxNative::str_len(&op.borrow().oldText));
                self.adapter.clone().borrow().replaceRange(_tmp_3451, _tmp_3452, __arg_0, &_tmp_3453);
                let mut _tmp_3455 = op.borrow().paragraphId.clone();
                self.setCaret(_tmp_3455, op.borrow().start + (DxNative::str_len(&op.borrow().newText)));
            } else {
                let mut _tmp_3457 = op.borrow().paragraphId.clone();
                let mut _tmp_3458 = op.borrow().start.clone();
                let mut _tmp_3459 = op.borrow().oldText.clone();
                let __arg_0 = op.borrow().start + (DxNative::str_len(&op.borrow().newText));
                self.adapter.clone().borrow().replaceRange(_tmp_3457, _tmp_3458, __arg_0, &_tmp_3459);
                let mut _tmp_3461 = op.borrow().paragraphId.clone();
                self.setCaret(_tmp_3461, op.borrow().start + (DxNative::str_len(&op.borrow().oldText)));
            }
            return true;
        }
        if op.borrow().kind == DocEditKind::listEnd() {
            let mut lp: Rc<RefCell<Paragraph>> = self.adapter.borrow().findParagraph(op.borrow().paragraphId.clone());
            if forward {
                // Ending the list again: clear what undo put back.
                lp.borrow_mut().listNumId = 0;
                lp.borrow_mut().listKind = 0;
                lp.borrow_mut().listLevel = 0;
                lp.borrow_mut().listFmt = String::new();
                lp.borrow_mut().listText = String::new();
            } else {
                // Restore the list membership that "Enter on an empty item"
                // cleared.
                lp.borrow_mut().listNumId = op.borrow().start;
                lp.borrow_mut().listLevel = op.borrow().end;
                lp.borrow_mut().listFmt = op.borrow().newText.clone();
                lp.borrow_mut().listText = op.borrow().oldText.clone();
                lp.borrow_mut().listKind = op.borrow().secondId;
            }
            self.setCaret(op.borrow().paragraphId.clone(), 0);
            return true;
        }
        if op.borrow().kind == DocEditKind::tableRow() {
            let blk_idx: i64 = op.borrow().start;
            if forward {
                let added: i64 = self.adapter.borrow().appendTableRow(blk_idx);
                if added > 0 {
                    let first_id: i64 = self.adapter.borrow().cellParagraphId(blk_idx, added - 1, 0);
                    if first_id > 0 {
                        self.setCaret(first_id, 0);
                    }
                }
                return true;
            }
            // Undo the row Tab appended, and step back into the last cell.
            if self.adapter.borrow().removeLastTableRow(blk_idx) {
                let last_row: i64 = self.adapter.borrow().rowCountOfTable(blk_idx) - 1;
                let last_cell: i64 = self.adapter.borrow().cellCountInRow(blk_idx, last_row) - 1;
                let back_id: i64 = self.adapter.borrow().cellParagraphId(blk_idx, last_row, last_cell);
                if back_id > 0 {
                    self.setCaret(back_id, self.adapter.borrow().textLength(back_id));
                }
            }
            return true;
        }
        if op.borrow().kind == DocEditKind::deleteSpan() {
            self.suppressUndo = true;
            if forward {
                // Delete the span again: select exactly what undo put back and
                // drop it, which is the same primitive the original edit used.
                let mut _tmp_3476 = op.borrow().paragraphId.clone();
                self.setCaret(_tmp_3476, op.borrow().start.clone());
                self.selectForward(&op.borrow().oldText.clone());
                self.deleteSelection();
            } else {
                // Put the text back, newlines re-splitting.
                let mut _tmp_3479 = op.borrow().paragraphId.clone();
                self.setCaret(_tmp_3479, op.borrow().start.clone());
                self.pasteText(&op.borrow().oldText.clone());
                let mut _tmp_3482 = op.borrow().paragraphId.clone();
                self.setCaret(_tmp_3482, op.borrow().start.clone());
            }
            self.suppressUndo = false;
            return true;
        }
        if op.borrow().kind == DocEditKind::splitParagraph() {
            if forward {
                // The SAME id back, not a fresh one: the ops after this in the
                // same action still name the paragraph this split created.
                let mut _tmp_3485 = op.borrow().paragraphId.clone();
                let mut _tmp_3486 = op.borrow().start.clone();
                let again: i64 = self.adapter.borrow().splitParagraphAs(_tmp_3485, _tmp_3486, op.borrow().secondId.clone());
                op.borrow_mut().secondId = again;
                self.setCaret(again, 0);
            } else {
                // Rejoin the two halves. Enter used to record a kind NOTHING
                // could undo — neither direction knew a 1 — so pressing it
                // made an op that stopped every undo below it dead.
                let mut joined: Rc<RefCell<DocumentPosition>> = self.adapter.borrow_mut().mergePrevious(op.borrow().secondId.clone());
                if joined.borrow().paragraphId > 0 {
                    let mut _tmp_3490 = joined.borrow().paragraphId.clone();
                    self.setCaret(_tmp_3490, joined.borrow().textOffset.clone());
                }
            }
            return true;
        }
        if op.borrow().kind == DocEditKind::mergeParagraph() {
            if forward {
                let mut dest2: Rc<RefCell<DocumentPosition>> = self.adapter.borrow_mut().mergePrevious(op.borrow().paragraphId.clone());
                if dest2.borrow().paragraphId > 0 {
                    let mut _tmp_3494 = dest2.borrow().paragraphId.clone();
                    self.setCaret(_tmp_3494, dest2.borrow().textOffset.clone());
                }
            } else {
                // Split them apart again where the join fell, with the id the
                // paragraph had before the merge.
                let mut _tmp_3497 = op.borrow().secondId.clone();
                let mut _tmp_3498 = op.borrow().start.clone();
                let re: i64 = self.adapter.borrow().splitParagraphAs(_tmp_3497, _tmp_3498, op.borrow().paragraphId.clone());
                op.borrow_mut().paragraphId = re;
                self.setCaret(re, 0);
            }
            return true;
        }
        if op.borrow().kind == DocEditKind::bold() {
            // What the op records is what bold was turned TO, so the inverse is
            // turning it the other way. A range that was MIXED before the
            // toggle comes back uniform rather than mixed — the op would have
            // to carry the run boundaries to do better, and it does not.
            let mut want: bool = op.borrow().boldVal;
            if !forward {
                want = !op.borrow().boldVal;
            }
            let mut _tmp_3501 = op.borrow().paragraphId.clone();
            let mut _tmp_3502 = op.borrow().start.clone();
            let mut _tmp_3503 = op.borrow().end.clone();
            self.adapter.clone().borrow().applyRun(_tmp_3501, _tmp_3502, _tmp_3503, "bold", want, 0.0, String::new());
            return true;
        }
        if op.borrow().kind == DocEditKind::pasteTable() {
            if forward {
                // The table's content is gone once undo removed it, so a redo
                // that pretended otherwise would put an empty table back.
                // Refusing is the honest answer, and refusing WITHOUT moving
                // the op is what keeps the two stacks in step.
                return false;
            }
            let mut _tmp_3506 = op.borrow().start.clone();
            self.adapter.clone().borrow().removeTableById(_tmp_3506);
            let mut back: Rc<RefCell<DocumentPosition>> = self.adapter.borrow_mut().mergePrevious(op.borrow().secondId.clone());
            if back.borrow().paragraphId > 0 {
                let mut _tmp_3510 = back.borrow().paragraphId.clone();
                self.setCaret(_tmp_3510, back.borrow().textOffset.clone());
            } else {
                let mut _tmp_3512 = op.borrow().paragraphId.clone();
                self.setCaret(_tmp_3512, self.adapter.borrow().textLength(op.borrow().paragraphId));
            }
            return true;
        }
        if op.borrow().kind == DocEditKind::pasteChart() {
            if forward {
                return false;
            }
            // A chart paste is the same shape as a table paste: the chart goes,
            // and the paragraph it split rejoins. This used to record itself as
            // a multi-paragraph DELETE — 5, one more than the table's 4 — so
            // undo ran the delete's restore path and the chart stayed.
            let mut _tmp_3514 = op.borrow().start.clone();
            self.adapter.clone().borrow().removeChartBlock(_tmp_3514);
            let mut cback: Rc<RefCell<DocumentPosition>> = self.adapter.borrow_mut().mergePrevious(op.borrow().secondId.clone());
            if cback.borrow().paragraphId > 0 {
                let mut _tmp_3518 = cback.borrow().paragraphId.clone();
                self.setCaret(_tmp_3518, cback.borrow().textOffset.clone());
            } else {
                let mut _tmp_3520 = op.borrow().paragraphId.clone();
                self.setCaret(_tmp_3520, self.adapter.borrow().textLength(op.borrow().paragraphId));
            }
            return true;
        }
        false
    }
    /// Select `text` forward from the caret, across paragraph breaks.
    pub fn selectForward(&mut self, text: &str) {
        let steps: i64 = DxNative::str_len(&text);
        let mut i: i64 = 0;
        while i < steps {
            self.moveRight(true);
            i += 1;
        }
    }
    /// One press of undo is one ACTION: the top op, and every op below it that
    /// was stamped with the same transaction.
    pub fn undo(&mut self) {
        if !self.history.borrow().canUndo() {
            return;
        }
        let span: i64 = self.history.borrow().undoSpan();
        let mut moved: i64 = 0;
        while moved < span {
            let mut top: Rc<RefCell<DocEditOp>> = self.history.borrow().peekUndo();
            if !self.applyOp(&top, false) {
                // An op that could not be undone stays where it is. Moving it
                // anyway is how the two stacks come apart.
                return;
            }
            self.history.borrow_mut().moveUndoToRedo();
            moved += 1;
        }
    }
    pub fn redo(&mut self) {
        if !self.history.borrow().canRedo() {
            return;
        }
        let span: i64 = self.history.borrow().redoSpan();
        let mut moved: i64 = 0;
        while moved < span {
            let mut top: Rc<RefCell<DocEditOp>> = self.history.borrow().peekRedo();
            if !self.applyOp(&top, true) {
                return;
            }
            self.history.borrow_mut().moveRedoToUndo();
            moved += 1;
        }
    }
    /// Hit-test using laid lines on the current page (same metrics as paint).
    /// Prefer the line whose content box contains (x,y). Y-only matching used to
    /// pick the last table-cell line on a row (always the rightmost column).
    fn hitTest(&self, layout: &Rc<RefCell<DocxLayout>>, page_index: i64, x: i64, y: i64) -> Rc<RefCell<DocumentPosition>> {
        let mut miss: Rc<RefCell<DocumentPosition>> = DocumentPosition::create(0, 0);
        if page_index < 0 || page_index >= layout.borrow().pageCount() {
            return miss;
        }
        let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(page_index);
        let mut best: Option<Rc<RefCell<LaidLine>>> = None;
        let mut found: bool = false;
        let mut best_box_w: i64 = 1000000;
        let mut i: i64 = 0;
        while i < (page.borrow().lines.len() as i64) {
            let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[i as usize].clone();
            if LaidLineKind::isBodyText(line.borrow().kind.clone()) {
                let mut bw: i64 = line.borrow().boxW;
                if bw < 4 {
                    bw = line.borrow().width;
                }
                if bw < 4 {
                    bw = 4;
                }
                let bx: i64 = line.borrow().boxX;
                if y >= line.borrow().y && y < line.borrow().y + line.borrow().height {
                    if x >= bx && x < bx + bw {
                        // Innermost box wins when boxes nest / overlap.
                        if !found || bw <= best_box_w {
                            best = Some(line.clone());
                            found = true;
                            best_box_w = bw;
                        }
                    }
                }
            }
            i += 1;
        }
        // Clicking the picture itself puts the caret in the paragraph that owns
        // it — before it on the left half, after it on the right — so there is a
        // way to land next to an image and press Enter. Only when no text line
        // already claimed the point.
        if !found {
            let mut ii: i64 = 0;
            while ii < (page.borrow().lines.len() as i64) {
                let mut img_line: Rc<RefCell<LaidLine>> = page.borrow().lines[ii as usize].clone();
                if img_line.borrow().kind == LaidLineKind::image() {
                    if y >= img_line.borrow().y && y < img_line.borrow().y + img_line.borrow().height {
                        if x >= img_line.borrow().x && x < img_line.borrow().x + img_line.borrow().width {
                            let mut ip: Rc<RefCell<Paragraph>> = img_line.borrow().paragraphRef.clone().unwrap();
                            if ip.borrow().id > 0 {
                                let half: i64 = img_line.borrow().x + (img_line.borrow().width / 2);
                                if x < half {
                                    return DocumentPosition::create(ip.borrow().id.clone(), 0);
                                }
                                return DocumentPosition::create(ip.borrow().id.clone(), (DxNative::str_len(&ip.borrow().text)));
                            }
                        }
                    }
                }
                ii += 1;
            }
        }
        if !found {
            // Nearest body line by distance to its content box (2D).
            let mut best_dist: i64 = 1000000;
            let mut j: i64 = 0;
            while j < (page.borrow().lines.len() as i64) {
                let mut line2: Rc<RefCell<LaidLine>> = page.borrow().lines[j as usize].clone();
                if LaidLineKind::isBodyText(line2.borrow().kind.clone()) {
                    let mut bw2: i64 = line2.borrow().boxW;
                    if bw2 < 4 {
                        bw2 = line2.borrow().width;
                    }
                    if bw2 < 4 {
                        bw2 = 4;
                    }
                    let bx2: i64 = line2.borrow().boxX;
                    let by2: i64 = line2.borrow().y;
                    let bh2: i64 = line2.borrow().height;
                    let mut dx: i64 = 0;
                    if x < bx2 {
                        dx = bx2 - x;
                    } else {
                        if x >= bx2 + bw2 {
                            dx = x - ((bx2 + bw2) - 1);
                        }
                    }
                    let mut dy: i64 = 0;
                    if y < by2 {
                        dy = by2 - y;
                    } else {
                        if y >= by2 + bh2 {
                            dy = y - ((by2 + bh2) - 1);
                        }
                    }
                    let d: i64 = dx * dx + dy * dy;
                    if d < best_dist {
                        best_dist = d;
                        best = Some(line2.clone());
                        found = true;
                    }
                }
                j += 1;
            }
        }
        if !found {
            return miss;
        }
        // `found` says `best` was assigned
        let mut best_line: Rc<RefCell<LaidLine>> = best.clone().unwrap();
        let mut p: Rc<RefCell<Paragraph>> = best_line.borrow().paragraphRef.clone().unwrap();
        if p.borrow().id <= 0 {
            return miss;
        }
        let mut tr: Rc<RefCell<UITextRenderer>> = layout.borrow().tr.clone().unwrap();
        let mut font_px: f64 = best_line.borrow().fontSizePx;
        DocxTextMetrics::useFamily(&tr, &best_line.borrow().fontFamily.clone());
        if font_px < 8.0 {
            font_px = 8.0;
        }
        // Map x → offset within this visual line, then to document offset.
        let mut local_x: i64 = x - best_line.borrow().x;
        if local_x < 0 {
            local_x = 0;
        }
        let display: String = best_line.borrow().text.clone();
        let mut prefix_len: i64 = 0;
        if !best_line.borrow().listPrefix.is_empty() {
            if DxNative::index_of(&display, &best_line.borrow().listPrefix) == 0 {
                prefix_len = DxNative::str_len(&best_line.borrow().listPrefix);
            }
        }
        if prefix_len > 0 {
            local_x -= DocxTextMetrics::measurePlain(
                &tr,
                &DxNative::substr(&display, 0, prefix_len),
                font_px,
                best_line.borrow().bold
            );
            if local_x < 0 {
                local_x = 0;
            }
        }
        // Measure through the same per-span faces the painter used, so a click
        // inside a bold run lands on the character under the pointer.
        let mut _tmp_3536 = best_line.borrow().docStart.clone();
        let mut _tmp_3537 = best_line.borrow().docEnd.clone();
        let mut doc_off: i64 = DocxTextMetrics::offsetAtX(
            &tr,
            &p,
            font_px,
            _tmp_3536,
            _tmp_3537,
            local_x
        );
        let plen: i64 = DxNative::str_len(&p.borrow().text);
        if doc_off > plen {
            doc_off = plen;
        }
        if doc_off < 0 {
            doc_off = 0;
        }
        return DocumentPosition::create(p.borrow().id.clone(), doc_off);
    }
    /// --- caret geometry -------------------------------------------------------
    /// All three share findLineFor, so the caret, its height and the selection
    /// agree about which line the caret is on — including a caret past the last
    /// laid character of a paragraph.
    pub fn caretPixelX(&self, layout: &Rc<RefCell<DocxLayout>>, page_index: i64) -> i64 {
        if !self.isActive() {
            return -1;
        }
        let mut at: Vec<i64> = Vec::new();
        self.caretLine(layout, &mut at);
        if at[0] != page_index {
            return -1;
        }
        let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(page_index);
        let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[(at[1]) as usize].clone();
        let mut c: Rc<RefCell<DocumentPosition>> = self.caret();
        let mut tr: Rc<RefCell<UITextRenderer>> = layout.borrow().tr.clone().unwrap();
        let font_px: f64 = line.borrow().fontSizePx;
        DocxTextMetrics::useFamily(&tr, &line.borrow().fontFamily.clone());
        let display: String = line.borrow().text.clone();
        let mut x: i64 = line.borrow().x;
        if !line.borrow().listPrefix.is_empty() {
            if DxNative::index_of(&display, &line.borrow().listPrefix) == 0 {
                x += DocxTextMetrics::measurePlain(
                    &tr,
                    &line.borrow().listPrefix,
                    font_px,
                    line.borrow().bold
                );
            }
        }
        let mut up_to: i64 = c.borrow().textOffset;
        if up_to > line.borrow().docEnd {
            up_to = line.borrow().docEnd;
        }
        if up_to < line.borrow().docStart {
            up_to = line.borrow().docStart;
        }
        let mut para: Rc<RefCell<Paragraph>> = line.borrow().paragraphRef.clone().unwrap();
        x += DocxTextMetrics::measureRange(&tr, &para, font_px, line.borrow().docStart, up_to);
        x
    }
    pub fn caretPixelY(&self, layout: &Rc<RefCell<DocxLayout>>, page_index: i64) -> i64 {
        if !self.isActive() {
            return -1;
        }
        let mut at: Vec<i64> = Vec::new();
        self.caretLine(layout, &mut at);
        if at[0] != page_index {
            return -1;
        }
        let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(page_index);
        let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[(at[1]) as usize].clone();
        return line.borrow().y;
    }
    pub fn caretHeight(&self, layout: &Rc<RefCell<DocxLayout>>, page_index: i64) -> i64 {
        if !self.isActive() {
            return 14;
        }
        let mut at: Vec<i64> = Vec::new();
        self.caretLine(layout, &mut at);
        if at[0] != page_index {
            return 14;
        }
        let mut page: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(page_index);
        let mut line: Rc<RefCell<LaidLine>> = page.borrow().lines[(at[1]) as usize].clone();
        return line.borrow().height;
    }
}
