// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rich_document::{DocBlock, DocTableCell, DocTableRow, DocumentTable, Paragraph, RichDocument, TextSpan};
use crate::rgr::DxNative;

/// ==============================================================================
/// DocxTableEdit - the Table Layout commands on the document model.
/// ==============================================================================
///
/// Rows and columns are inserted and deleted, cells merged and split, a table
/// split in two, sorted, or turned into text and back. Columns are counted on
/// the table's GRID (`colWidthsTwips`), not by cell index: a merged cell covers
/// several grid columns, and a column command acts on whatever covers it.
///
/// Undo is a snapshot of the shape: the document's block list and, for each
/// table touched, its rows, their cells and the cells' paragraphs. Restoring
/// puts the same objects back, so a caret or a text edit recorded against a
/// cell paragraph still finds it.
/// ==============================================================================
pub struct DocxTableState {
    pub table: Rc<RefCell<DocumentTable>>,
    pub widths: Vec<i64>,
    pub rows: Vec<Rc<RefCell<DocTableRow>>>,
    pub heights: Vec<i64>,
    pub headers: Vec<bool>,
    pub cellStart: Vec<i64>,
    pub cells: Vec<Rc<RefCell<DocTableCell>>>,
    pub spans: Vec<i64>,
    pub valigns: Vec<i64>,
    pub vmerges: Vec<i64>,
    pub fills: Vec<String>,
    pub paraStart: Vec<i64>,
    pub paras: Vec<Rc<RefCell<Paragraph>>>,
    pub aligns: Vec<i64>,
}

impl DocxTableState {
    pub fn new() -> Self {
        Self {
            table: Rc::new(RefCell::new(DocumentTable::new())),
            widths: Vec::new(),
            rows: Vec::new(),
            heights: Vec::new(),
            headers: Vec::new(),
            cellStart: Vec::new(),
            cells: Vec::new(),
            spans: Vec::new(),
            valigns: Vec::new(),
            vmerges: Vec::new(),
            fills: Vec::new(),
            paraStart: Vec::new(),
            paras: Vec::new(),
            aligns: Vec::new(),
        }
    }
}

pub struct DocxTableSnap {
    pub blocks: Vec<Rc<RefCell<DocBlock>>>,
    pub states: Vec<Rc<RefCell<DocxTableState>>>,
    pub caretPid: i64,
}

impl DocxTableSnap {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            states: Vec::new(),
            caretPid: 0,
        }
    }
}

pub struct DocxTableEdit {
}

impl DocxTableEdit {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The undo kind; DocxParaFormat has 40 and 41, the editor 42 and 43.
    pub fn opKind() -> i64 {
        44
    }
    // --- snapshots ---------------------------------------------------------------
    pub fn capture(doc: &Rc<RefCell<RichDocument>>, tables: &Vec<Rc<RefCell<DocumentTable>>>, caret_pid: i64) -> Rc<RefCell<DocxTableSnap>> {
        let mut snap: Rc<RefCell<DocxTableSnap>> = Rc::new(RefCell::new(DocxTableSnap::new()));
        snap.borrow_mut().caretPid = caret_pid;
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(doc) {
            snap.borrow_mut().blocks.push(RichDocument::blockAt(doc, bi));
            bi += 1;
        }
        let mut ti: i64 = 0;
        while ti < (tables.len() as i64) {
            let t: Rc<RefCell<DocumentTable>> = tables[ti as usize].clone();
            let mut st: Rc<RefCell<DocxTableState>> = Rc::new(RefCell::new(DocxTableState::new()));
            st.borrow_mut().table = t.clone();
            st.borrow_mut().widths = t.borrow().colWidthsTwips.clone();
            let mut ri: i64 = 0;
            while ri < (t.borrow().rows.len() as i64) {
                let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
                st.borrow_mut().rows.push(row.clone());
                st.borrow_mut().heights.push(row.borrow().heightTwips);
                st.borrow_mut().headers.push(row.borrow().header);
                let at: i64 = st.borrow().cells.len() as i64;
                st.borrow_mut().cellStart.push(at);
                let mut ci: i64 = 0;
                while ci < (row.borrow().cells.len() as i64) {
                    let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                    st.borrow_mut().cells.push(cell.clone());
                    st.borrow_mut().spans.push(cell.borrow().gridSpan);
                    st.borrow_mut().valigns.push(cell.borrow().vAlign);
                    st.borrow_mut().vmerges.push(cell.borrow().vMerge);
                    st.borrow_mut().fills.push(cell.borrow().fillHex.clone());
                    let pat: i64 = st.borrow().paras.len() as i64;
                    st.borrow_mut().paraStart.push(pat);
                    let mut pi: i64 = 0;
                    while pi < (cell.borrow().paragraphs.len() as i64) {
                        let p: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
                        st.borrow_mut().paras.push(p.clone());
                        st.borrow_mut().aligns.push(p.borrow().align);
                        pi += 1;
                    }
                    ci += 1;
                }
                ri += 1;
            }
            let cend: i64 = st.borrow().cells.len() as i64;
            st.borrow_mut().cellStart.push(cend);
            let pend: i64 = st.borrow().paras.len() as i64;
            st.borrow_mut().paraStart.push(pend);
            snap.borrow_mut().states.push(st.clone());
            ti += 1;
        }
        snap
    }
    pub fn restore(doc: &Rc<RefCell<RichDocument>>, snap: &Rc<RefCell<DocxTableSnap>>) {
        doc.borrow_mut().blocks.clear();
        let mut bi: i64 = 0;
        while bi < (snap.borrow().blocks.len() as i64) {
            let b: Rc<RefCell<DocBlock>> = snap.borrow().blocks[bi as usize].clone();
            doc.borrow_mut().blocks.push(b.clone());
            bi += 1;
        }
        let mut si: i64 = 0;
        while si < (snap.borrow().states.len() as i64) {
            let st: Rc<RefCell<DocxTableState>> = snap.borrow().states[si as usize].clone();
            let t: Rc<RefCell<DocumentTable>> = st.borrow().table.clone();
            t.borrow_mut().colWidthsTwips = st.borrow().widths.clone();
            t.borrow_mut().rows.clear();
            let mut ri: i64 = 0;
            while ri < (st.borrow().rows.len() as i64) {
                let row: Rc<RefCell<DocTableRow>> = st.borrow().rows[ri as usize].clone();
                row.borrow_mut().heightTwips = st.borrow().heights[ri as usize];
                row.borrow_mut().header = st.borrow().headers[ri as usize];
                row.borrow_mut().cells.clear();
                let mut ci: i64 = st.borrow().cellStart[ri as usize];
                let cend: i64 = st.borrow().cellStart[(ri + 1) as usize];
                while ci < cend {
                    let cell: Rc<RefCell<DocTableCell>> = st.borrow().cells[ci as usize].clone();
                    cell.borrow_mut().gridSpan = st.borrow().spans[ci as usize];
                    cell.borrow_mut().vAlign = st.borrow().valigns[ci as usize];
                    cell.borrow_mut().vMerge = st.borrow().vmerges[ci as usize];
                    cell.borrow_mut().fillHex = st.borrow().fills[ci as usize].clone();
                    cell.borrow_mut().paragraphs.clear();
                    let mut pi: i64 = st.borrow().paraStart[ci as usize];
                    let pend: i64 = st.borrow().paraStart[(ci + 1) as usize];
                    while pi < pend {
                        let p: Rc<RefCell<Paragraph>> = st.borrow().paras[pi as usize].clone();
                        p.borrow_mut().align = st.borrow().aligns[pi as usize];
                        cell.borrow_mut().paragraphs.push(p.clone());
                        pi += 1;
                    }
                    row.borrow_mut().cells.push(cell.clone());
                    ci += 1;
                }
                t.borrow_mut().rows.push(row.clone());
                ri += 1;
            }
            si += 1;
        }
        RichDocument::touch(doc);
    }
    // --- the grid ----------------------------------------------------------------
    pub fn spanOf(cell: &Rc<RefCell<DocTableCell>>) -> i64 {
        let s: i64 = cell.borrow().gridSpan;
        if s < 1 {
            return 1;
        }
        s
    }
    /// The first grid column a cell covers.
    pub fn gridStart(row: &Rc<RefCell<DocTableRow>>, ci: i64) -> i64 {
        let mut g: i64 = 0;
        let mut k: i64 = 0;
        while k < ci && k < (row.borrow().cells.len() as i64) {
            g += DocxTableEdit::spanOf(&row.borrow().cells[k as usize].clone());
            k += 1;
        }
        g
    }
    /// The cell covering grid column g, or -1.
    pub fn cellAtGrid(row: &Rc<RefCell<DocTableRow>>, g: i64) -> i64 {
        let mut at: i64 = 0;
        let mut k: i64 = 0;
        while k < (row.borrow().cells.len() as i64) {
            let s: i64 = DocxTableEdit::spanOf(&row.borrow().cells[k as usize].clone());
            if g >= at && g < at + s {
                return k;
            }
            at += s;
            k += 1;
        }
        -1
    }
    pub fn totalWidth(t: &Rc<RefCell<DocumentTable>>) -> i64 {
        let mut s: i64 = 0;
        let mut i: i64 = 0;
        while i < (t.borrow().colWidthsTwips.len() as i64) {
            s += t.borrow().colWidthsTwips[i as usize];
            i += 1;
        }
        s
    }
    /// Scale the grid so it adds up to `total` again.
    pub fn rescale(t: &Rc<RefCell<DocumentTable>>, total: i64) {
        let now: i64 = DocxTableEdit::totalWidth(t);
        if now <= 0 || total <= 0 {
            return;
        }
        let n: i64 = t.borrow().colWidthsTwips.len() as i64;
        let mut used: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let w: i64 = t.borrow().colWidthsTwips[i as usize];
            let mut nw: i64 = (((w as f64) * (total as f64)) / (now as f64)).floor() as i64;
            if i == n - 1 {
                nw = total - used;
            }
            t.borrow_mut().colWidthsTwips[i as usize] = nw;
            used += nw;
            i += 1;
        }
    }
    /// A paragraph for a new cell, looking like `like` (its alignment and the
    /// font of its first run), empty.
    pub fn emptyParaLike(doc: &Rc<RefCell<RichDocument>>, like: &Rc<RefCell<DocTableCell>>) -> Rc<RefCell<Paragraph>> {
        let mut p: Rc<RefCell<Paragraph>> = RichDocument::addParagraphOnly(doc);
        p.borrow_mut().styleId = "Normal".to_string();
        p.borrow_mut().spaceBeforePt = 0.0;
        p.borrow_mut().spaceAfterPt = 0.0;
        p.borrow_mut().lineSpacing = 1.0;
        let mut sp: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
        if !like.borrow().paragraphs.is_empty() {
            let src: Rc<RefCell<Paragraph>> = like.borrow().paragraphs[0].clone();
            p.borrow_mut().styleId = src.borrow().styleId.clone();
            p.borrow_mut().spaceBeforePt = src.borrow().spaceBeforePt;
            p.borrow_mut().spaceAfterPt = src.borrow().spaceAfterPt;
            p.borrow_mut().lineSpacing = src.borrow().lineSpacing;
            p.borrow_mut().align = src.borrow().align;
            if !src.borrow().spans.is_empty() {
                let s0: Rc<RefCell<TextSpan>> = src.borrow().spans[0].clone();
                sp = s0.borrow().copy();
                sp.borrow_mut().start = 0;
                sp.borrow_mut().end = 0;
            }
        }
        p.borrow_mut().spans.push(sp.clone());
        p
    }
    pub fn cellLike(doc: &Rc<RefCell<RichDocument>>, like: &Rc<RefCell<DocTableCell>>) -> Rc<RefCell<DocTableCell>> {
        let mut cell: Rc<RefCell<DocTableCell>> = Rc::new(RefCell::new(DocTableCell::new()));
        cell.borrow_mut().vAlign = like.borrow().vAlign;
        cell.borrow_mut().fillHex = like.borrow().fillHex.clone();
        cell.borrow_mut().padTopPx = like.borrow().padTopPx;
        cell.borrow_mut().padRightPx = like.borrow().padRightPx;
        cell.borrow_mut().padBottomPx = like.borrow().padBottomPx;
        cell.borrow_mut().padLeftPx = like.borrow().padLeftPx;
        let p: Rc<RefCell<Paragraph>> = DocxTableEdit::emptyParaLike(doc, like);
        cell.borrow_mut().paragraphs.push(p.clone());
        cell
    }
    // --- rows ----------------------------------------------------------------------
    /// A row shaped like row `like`, put at index `at`.
    pub fn insertRow(doc: &Rc<RefCell<RichDocument>>, t: &Rc<RefCell<DocumentTable>>, like: i64, at: i64) -> Rc<RefCell<DocTableRow>> {
        let src: Rc<RefCell<DocTableRow>> = t.borrow().rows[like as usize].clone();
        let mut row: Rc<RefCell<DocTableRow>> = Rc::new(RefCell::new(DocTableRow::new()));
        row.borrow_mut().heightTwips = src.borrow().heightTwips;
        let mut ci: i64 = 0;
        while ci < (src.borrow().cells.len() as i64) {
            let sc: Rc<RefCell<DocTableCell>> = src.borrow().cells[ci as usize].clone();
            let c: Rc<RefCell<DocTableCell>> = DocxTableEdit::cellLike(doc, &sc);
            c.borrow_mut().gridSpan = sc.borrow().gridSpan;
            row.borrow_mut().cells.push(c.clone());
            ci += 1;
        }
        t.borrow_mut().rows.insert(at as usize, row.clone());
        RichDocument::touch(doc);
        row
    }
    /// Rows r0..r1 out. False when that would leave no row: the caller deletes
    /// the table instead.
    pub fn deleteRows(t: &Rc<RefCell<DocumentTable>>, r0: i64, r1: i64) -> bool {
        let n: i64 = t.borrow().rows.len() as i64;
        if r0 <= 0 && r1 >= n - 1 {
            return false;
        }
        let mut k: i64 = r1;
        while k >= r0 {
            if k >= 0 && k < (t.borrow().rows.len() as i64) {
                t.borrow_mut().rows.remove(k as usize);
            }
            k -= 1;
        }
        true
    }
    // --- columns -------------------------------------------------------------------
    /// A grid column next to column g, as wide as it, the table keeping its
    /// width. A merged cell that covers g past the insertion point just widens.
    pub fn insertColumn(doc: &Rc<RefCell<RichDocument>>, t: &Rc<RefCell<DocumentTable>>, g: i64, right: bool) {
        let total: i64 = DocxTableEdit::totalWidth(t);
        let mut ri: i64 = 0;
        while ri < (t.borrow().rows.len() as i64) {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
            let ci: i64 = DocxTableEdit::cellAtGrid(&row, g);
            if ci < 0 {
                let n: i64 = row.borrow().cells.len() as i64;
                if n > 0 {
                    let last: Rc<RefCell<DocTableCell>> = row.borrow().cells[(n - 1) as usize].clone();
                    let c: Rc<RefCell<DocTableCell>> = DocxTableEdit::cellLike(doc, &last);
                    row.borrow_mut().cells.push(c.clone());
                }
            } else {
                let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                let gs: i64 = DocxTableEdit::gridStart(&row, ci);
                let ge: i64 = gs + DocxTableEdit::spanOf(&cell) - 1;
                let edge: bool = if right { g == ge } else { g == gs };
                if edge {
                    let c: Rc<RefCell<DocTableCell>> = DocxTableEdit::cellLike(doc, &cell);
                    let at: i64 = if right { ci + 1 } else { ci };
                    row.borrow_mut().cells.insert(at as usize, c.clone());
                } else {
                    let s: i64 = DocxTableEdit::spanOf(&cell);
                    cell.borrow_mut().gridSpan = s + 1;
                    cell.borrow_mut().fresh = true;
                }
            }
            ri += 1;
        }
        let mut w: i64 = 1440;
        if g >= 0 && g < (t.borrow().colWidthsTwips.len() as i64) {
            w = t.borrow().colWidthsTwips[g as usize];
        }
        let mut at: i64 = if right { g + 1 } else { g };
        if at > (t.borrow().colWidthsTwips.len() as i64) {
            at = t.borrow().colWidthsTwips.len() as i64;
        }
        if at < 0 {
            at = 0;
        }
        t.borrow_mut().colWidthsTwips.insert(at as usize, w);
        DocxTableEdit::rescale(t, total);
        RichDocument::touch(doc);
    }
    /// Grid columns g0..g1 out; a cell that covers some of them narrows, one
    /// that covers only them goes. False when no column would be left.
    pub fn deleteColumns(t: &Rc<RefCell<DocumentTable>>, g0: i64, g1: i64) -> bool {
        let n: i64 = t.borrow().colWidthsTwips.len() as i64;
        if g0 <= 0 && g1 >= n - 1 {
            return false;
        }
        let total: i64 = DocxTableEdit::totalWidth(t);
        let mut ri: i64 = 0;
        while ri < (t.borrow().rows.len() as i64) {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
            let mut keep: Vec<Rc<RefCell<DocTableCell>>> = Vec::new();
            let mut gs: i64 = 0;
            let mut ci: i64 = 0;
            while ci < (row.borrow().cells.len() as i64) {
                let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                let s: i64 = DocxTableEdit::spanOf(&cell);
                let ge: i64 = gs + s - 1;
                let lo: i64 = if gs > g0 { gs } else { g0 };
                let hi: i64 = if ge < g1 { ge } else { g1 };
                let overlap: i64 = if hi >= lo { hi - lo + 1 } else { 0 };
                if overlap < s {
                    if overlap > 0 {
                        cell.borrow_mut().gridSpan = s - overlap;
                        cell.borrow_mut().fresh = true;
                    }
                    keep.push(cell.clone());
                }
                gs += s;
                ci += 1;
            }
            row.borrow_mut().cells = keep;
            ri += 1;
        }
        let mut k: i64 = g1;
        while k >= g0 {
            if k >= 0 && k < (t.borrow().colWidthsTwips.len() as i64) {
                t.borrow_mut().colWidthsTwips.remove(k as usize);
            }
            k -= 1;
        }
        // Rows left with no cell go too.
        let mut rk: i64 = (t.borrow().rows.len() as i64) - 1;
        while rk >= 0 {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[rk as usize].clone();
            if row.borrow().cells.is_empty() {
                t.borrow_mut().rows.remove(rk as usize);
            }
            rk -= 1;
        }
        DocxTableEdit::rescale(t, total);
        true
    }
    /// Delete cells, shifting the rest of each row left.
    pub fn deleteCellsLeft(t: &Rc<RefCell<DocumentTable>>, r0: i64, r1: i64, g0: i64, g1: i64) -> bool {
        let mut ri: i64 = r0;
        while ri <= r1 && ri < (t.borrow().rows.len() as i64) {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
            let mut keep: Vec<Rc<RefCell<DocTableCell>>> = Vec::new();
            let mut gs: i64 = 0;
            let mut ci: i64 = 0;
            while ci < (row.borrow().cells.len() as i64) {
                let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                let s: i64 = DocxTableEdit::spanOf(&cell);
                let inside: bool = gs >= g0 && gs <= g1;
                if !inside {
                    keep.push(cell.clone());
                }
                gs += s;
                ci += 1;
            }
            row.borrow_mut().cells = keep;
            ri += 1;
        }
        let mut rk: i64 = (t.borrow().rows.len() as i64) - 1;
        while rk >= 0 {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[rk as usize].clone();
            if row.borrow().cells.is_empty() {
                t.borrow_mut().rows.remove(rk as usize);
            }
            rk -= 1;
        }
        !t.borrow().rows.is_empty()
    }
    // --- merge and split -----------------------------------------------------------
    /// The cell of row r whose grid columns start at g, if there is one.
    pub fn cellStartingAt(t: &Rc<RefCell<DocumentTable>>, r: i64, g: i64) -> Option<Rc<RefCell<DocTableCell>>> {
        let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[r as usize].clone();
        let mut gs: i64 = 0;
        let mut ci: i64 = 0;
        while ci < (row.borrow().cells.len() as i64) {
            let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
            if gs == g {
                return Some(cell);
            }
            gs += DocxTableEdit::spanOf(&cell);
            ci += 1;
        }
        None
    }
    pub fn paraIsEmpty(p: &Rc<RefCell<Paragraph>>) -> bool {
        p.borrow().text.is_empty() && p.borrow().images.is_empty()
    }
    /// In each row r0..r1, the cells covering g0..g1 become one: its span is
    /// theirs added up and its paragraphs theirs, empty ones dropped; over
    /// several rows those then merge vertically. Answers how many cells changed.
    pub fn mergeCells(doc: &Rc<RefCell<RichDocument>>, t: &Rc<RefCell<DocumentTable>>, r0: i64, r1: i64, g0: i64, g1: i64) -> i64 {
        let mut merged: i64 = 0;
        let mut ri: i64 = r0;
        while ri <= r1 && ri < (t.borrow().rows.len() as i64) {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
            let mut keep: Vec<Rc<RefCell<DocTableCell>>> = Vec::new();
            let mut first_at: i64 = -1;
            let mut span_sum: i64 = 0;
            let mut paras: Vec<Rc<RefCell<Paragraph>>> = Vec::new();
            let mut fallback: Vec<Rc<RefCell<Paragraph>>> = Vec::new();
            let mut joined: i64 = 0;
            let mut gs: i64 = 0;
            let mut ci: i64 = 0;
            while ci < (row.borrow().cells.len() as i64) {
                let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                let s: i64 = DocxTableEdit::spanOf(&cell);
                let ge: i64 = gs + s - 1;
                let touches: bool = ge >= g0 && gs <= g1;
                if touches {
                    joined += 1;
                    span_sum += s;
                    let mut pi: i64 = 0;
                    while pi < (cell.borrow().paragraphs.len() as i64) {
                        let p: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
                        if fallback.is_empty() {
                            fallback.push(p.clone());
                        }
                        if !DocxTableEdit::paraIsEmpty(&p) {
                            paras.push(p.clone());
                        }
                        pi += 1;
                    }
                    if first_at < 0 {
                        first_at = keep.len() as i64;
                        keep.push(cell.clone());
                    }
                } else {
                    keep.push(cell.clone());
                }
                gs += s;
                ci += 1;
            }
            if first_at >= 0 && joined > 1 {
                let first: Rc<RefCell<DocTableCell>> = keep[first_at as usize].clone();
                first.borrow_mut().gridSpan = span_sum;
                first.borrow_mut().fresh = true;
                if paras.is_empty() {
                    paras = fallback.clone();
                }
                first.borrow_mut().paragraphs = paras;
                row.borrow_mut().cells = keep;
                merged += 1;
            }
            ri += 1;
        }
        // Over more than one row the per-row cells then merge downwards
        // (w:vMerge): the top one keeps every row's text, the ones below stay
        // as empty continuations.
        if r1 > r0 {
            let mut top: Option<Rc<RefCell<DocTableCell>>> = None;
            let mut rr: i64 = r0;
            while rr <= r1 && rr < (t.borrow().rows.len() as i64) {
                let c: Option<Rc<RefCell<DocTableCell>>> = DocxTableEdit::cellStartingAt(t, rr, g0);
                if c.is_some() {
                    let cell: Rc<RefCell<DocTableCell>> = c.unwrap();
                    if top.is_none() {
                        cell.borrow_mut().vMerge = 1;
                        cell.borrow_mut().fresh = true;
                        top = Some(cell.clone());
                    } else {
                        let tc: Rc<RefCell<DocTableCell>> = top.clone().unwrap();
                        let mut pi: i64 = 0;
                        while pi < (cell.borrow().paragraphs.len() as i64) {
                            let p: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
                            if !DocxTableEdit::paraIsEmpty(&p) {
                                if tc.borrow().paragraphs.len() == 1 && DocxTableEdit::paraIsEmpty(&tc.borrow().paragraphs[0].clone()) {
                                    tc.borrow_mut().paragraphs.clear();
                                }
                                tc.borrow_mut().paragraphs.push(p.clone());
                            }
                            pi += 1;
                        }
                        let blank: Rc<RefCell<Paragraph>> = DocxTableEdit::emptyParaLike(doc, &cell);
                        let mut only: Vec<Rc<RefCell<Paragraph>>> = Vec::new();
                        only.push(blank);
                        cell.borrow_mut().paragraphs = only;
                        cell.borrow_mut().vMerge = 2;
                        cell.borrow_mut().fresh = true;
                    }
                    merged += 1;
                }
                rr += 1;
            }
        }
        merged
    }
    /// Cell ci of row r into n cells side by side. A cell covering n grid
    /// columns or more shares them out; a narrower one splits its last column,
    /// and the cells above and below that cover it widen to match.
    pub fn splitCell(doc: &Rc<RefCell<RichDocument>>, t: &Rc<RefCell<DocumentTable>>, r: i64, ci: i64, n: i64) -> bool {
        if n < 2 || r < 0 || r >= (t.borrow().rows.len() as i64) {
            return false;
        }
        let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[r as usize].clone();
        if ci < 0 || ci >= (row.borrow().cells.len() as i64) {
            return false;
        }
        let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
        let s: i64 = DocxTableEdit::spanOf(&cell);
        let gs: i64 = DocxTableEdit::gridStart(&row, ci);
        let mut pieces: Vec<i64> = Vec::new();
        if s >= n {
            let base: i64 = s / n;
            let mut k: i64 = 0;
            while k < n {
                pieces.push(if k == n - 1 { s - base * (n - 1) } else { base });
                k += 1;
            }
        } else {
            let extra: i64 = n - s;
            let last_g: i64 = gs + s - 1;
            // The other rows: whatever covers last_g widens by `extra`.
            let mut ri: i64 = 0;
            while ri < (t.borrow().rows.len() as i64) {
                if ri != r {
                    let orow: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
                    let oc: i64 = DocxTableEdit::cellAtGrid(&orow, last_g);
                    if oc >= 0 {
                        let ocell: Rc<RefCell<DocTableCell>> = orow.borrow().cells[oc as usize].clone();
                        let os: i64 = DocxTableEdit::spanOf(&ocell);
                        ocell.borrow_mut().gridSpan = os + extra;
                        ocell.borrow_mut().fresh = true;
                    }
                }
                ri += 1;
            }
            let mut w: i64 = 1440;
            if last_g < (t.borrow().colWidthsTwips.len() as i64) {
                w = t.borrow().colWidthsTwips[last_g as usize];
            }
            let part: i64 = w / (extra + 1);
            t.borrow_mut().colWidthsTwips[last_g as usize] = w - part * extra;
            let mut k2: i64 = 0;
            while k2 < extra {
                t.borrow_mut().colWidthsTwips.insert((last_g + 1) as usize, part);
                k2 += 1;
            }
            let mut k3: i64 = 0;
            while k3 < n {
                pieces.push(1);
                k3 += 1;
            }
        }
        cell.borrow_mut().gridSpan = pieces[0];
        cell.borrow_mut().fresh = true;
        let mut k4: i64 = (pieces.len() as i64) - 1;
        while k4 >= 1 {
            let c: Rc<RefCell<DocTableCell>> = DocxTableEdit::cellLike(doc, &cell);
            c.borrow_mut().gridSpan = pieces[k4 as usize];
            row.borrow_mut().cells.insert((ci + 1) as usize, c.clone());
            k4 -= 1;
        }
        RichDocument::touch(doc);
        true
    }
    /// Rows r.. into a table of their own, which the caller puts in a block.
    pub fn splitOff(doc: &Rc<RefCell<RichDocument>>, t: &Rc<RefCell<DocumentTable>>, r: i64) -> Rc<RefCell<DocumentTable>> {
        let mut nt: Rc<RefCell<DocumentTable>> = RichDocument::addTableOnly(doc);
        nt.borrow_mut().colWidthsTwips = t.borrow().colWidthsTwips.clone();
        nt.borrow_mut().borderColor = t.borrow().borderColor.clone();
        nt.borrow_mut().cellPadPx = t.borrow().cellPadPx;
        nt.borrow_mut().natural = t.borrow().natural;
        nt.borrow_mut().widthTwips = t.borrow().widthTwips;
        nt.borrow_mut().widthPct = t.borrow().widthPct;
        while (t.borrow().rows.len() as i64) > r {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[r as usize].clone();
            t.borrow_mut().rows.remove(r as usize);
            nt.borrow_mut().rows.push(row.clone());
        }
        nt
    }
    // --- sizes ---------------------------------------------------------------------
    pub fn distributeColumns(t: &Rc<RefCell<DocumentTable>>, g0: i64, g1: i64) {
        let mut sum: i64 = 0;
        let mut k: i64 = g0;
        while k <= g1 {
            sum += t.borrow().colWidthsTwips[k as usize];
            k += 1;
        }
        let n: i64 = g1 - g0 + 1;
        let each: i64 = sum / n;
        let mut k2: i64 = g0;
        while k2 <= g1 {
            t.borrow_mut().colWidthsTwips[k2 as usize] = if k2 == g1 { sum - each * (n - 1) } else { each };
            k2 += 1;
        }
    }
    // --- sort ----------------------------------------------------------------------
    pub fn cellText(row: &Rc<RefCell<DocTableRow>>, g: i64) -> String {
        let ci: i64 = DocxTableEdit::cellAtGrid(row, g);
        if ci < 0 {
            return String::new();
        }
        let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
        let mut s: String = String::new();
        let mut pi: i64 = 0;
        while pi < (cell.borrow().paragraphs.len() as i64) {
            let p: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
            if pi > 0 {
                s = format!("{} ", s);
            }
            s = format!("{}{}", s, p.borrow().text);
            pi += 1;
        }
        s
    }
    /// a before b? Numbers by value when both are numbers, else text, case
    /// ignored.
    pub fn before(a: &str, b: &str) -> bool {
        let na: Option<f64> = a.trim().parse::<f64>().ok();
        let nb: Option<f64> = b.trim().parse::<f64>().ok();
        if na.is_some() && nb.is_some() {
            return na.unwrap() < nb.unwrap();
        }
        a.to_lowercase() < b.to_lowercase()
    }
    /// Rows from `first` on, by the text in grid column g. Stable.
    pub fn sortRows(t: &Rc<RefCell<DocumentTable>>, g: i64, descending: bool, first: i64) -> bool {
        let n: i64 = t.borrow().rows.len() as i64;
        let mut changed: bool = false;
        let mut a: i64 = first + 1;
        while a < n {
            let mut b: i64 = a;
            while b > first {
                let ra: Rc<RefCell<DocTableRow>> = t.borrow().rows[(b - 1) as usize].clone();
                let rb: Rc<RefCell<DocTableRow>> = t.borrow().rows[b as usize].clone();
                let x: String = DocxTableEdit::cellText(&ra, g);
                let y: String = DocxTableEdit::cellText(&rb, g);
                let swap: bool = if descending { DocxTableEdit::before(&x, &y) } else { DocxTableEdit::before(&y, &x) };
                if !swap {
                    break;
                }
                t.borrow_mut().rows[(b - 1) as usize] = rb.clone();
                t.borrow_mut().rows[b as usize] = ra.clone();
                changed = true;
                b -= 1;
            }
            a += 1;
        }
        changed
    }
    // --- text and tables -----------------------------------------------------------
    /// Copy p's characters a..b, with their formatting, to the end of `into`.
    pub fn copyRange(p: &Rc<RefCell<Paragraph>>, a: i64, b: i64, into: &Rc<RefCell<Paragraph>>) {
        let mut si: i64 = 0;
        while si < (p.borrow().spans.len() as i64) {
            let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[si as usize].clone();
            let lo: i64 = if sp.borrow().start > a { sp.borrow().start } else { a };
            let hi: i64 = if sp.borrow().end < b { sp.borrow().end } else { b };
            if hi > lo {
                let piece: String = DxNative::substr(&p.borrow().text, lo, hi);
                let c: Rc<RefCell<TextSpan>> = sp.borrow().copy();
                Paragraph::appendText(into, &piece, &c);
            }
            si += 1;
        }
        if into.borrow().spans.is_empty() {
            let mut c0: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
            if !p.borrow().spans.is_empty() {
                let s0: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
                c0 = s0.borrow().copy();
                c0.borrow_mut().start = 0;
                c0.borrow_mut().end = 0;
            }
            into.borrow_mut().spans.push(c0.clone());
        }
    }
    /// One body paragraph per row, the cells separated by tabs.
    pub fn rowsToText(doc: &Rc<RefCell<RichDocument>>, t: &Rc<RefCell<DocumentTable>>) -> Vec<Rc<RefCell<Paragraph>>> {
        let mut out: Vec<Rc<RefCell<Paragraph>>> = Vec::new();
        let mut ri: i64 = 0;
        while ri < (t.borrow().rows.len() as i64) {
            let row: Rc<RefCell<DocTableRow>> = t.borrow().rows[ri as usize].clone();
            let mut p: Rc<RefCell<Paragraph>> = RichDocument::addParagraphOnly(doc);
            p.borrow_mut().styleId = "Normal".to_string();
            let mut first_para: bool = true;
            let mut ci: i64 = 0;
            while ci < (row.borrow().cells.len() as i64) {
                let cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[ci as usize].clone();
                let mut pi: i64 = 0;
                while pi < (cell.borrow().paragraphs.len() as i64) {
                    let cp: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[pi as usize].clone();
                    if first_para {
                        p.borrow_mut().align = cp.borrow().align;
                        first_para = false;
                    }
                    let mut sep: String = String::new();
                    if ci > 0 && pi == 0 {
                        sep = "\t".to_string();
                    }
                    if pi > 0 {
                        sep = " ".to_string();
                    }
                    if !sep.is_empty() {
                        let mut ts: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
                        if !cp.borrow().spans.is_empty() {
                            let s0: Rc<RefCell<TextSpan>> = cp.borrow().spans[0].clone();
                            ts = s0.borrow().copy();
                        }
                        Paragraph::appendText(&p, &sep, &ts);
                    }
                    let n: i64 = DxNative::str_len(&cp.borrow().text);
                    DocxTableEdit::copyRange(&cp, 0, n, &p);
                    pi += 1;
                }
                ci += 1;
            }
            if p.borrow().spans.is_empty() {
                p.borrow_mut().spans.push(TextSpan::create(0, 0));
            }
            out.push(p.clone());
            ri += 1;
        }
        out
    }
    /// Paragraphs into a table: one row each, a cell per tab-separated piece,
    /// as many columns as the longest row needs, spread over `width` twips.
    pub fn textToTable(doc: &Rc<RefCell<RichDocument>>, paras: &Vec<Rc<RefCell<Paragraph>>>, width: i64) -> Rc<RefCell<DocumentTable>> {
        let mut t: Rc<RefCell<DocumentTable>> = RichDocument::addTableOnly(doc);
        let mut cols: i64 = 1;
        let mut pi: i64 = 0;
        while pi < (paras.len() as i64) {
            let p: Rc<RefCell<Paragraph>> = paras[pi as usize].clone();
            let mut n: i64 = 1;
            let mut at: i64 = DxNative::index_of_from(&p.borrow().text, "\t", 0);
            while at >= 0 {
                n += 1;
                at = DxNative::index_of_from(&p.borrow().text, "\t", at + 1);
            }
            if n > cols {
                cols = n;
            }
            pi += 1;
        }
        let each: i64 = width / cols;
        let mut k: i64 = 0;
        while k < cols {
            t.borrow_mut().colWidthsTwips.push(each);
            k += 1;
        }
        let mut pj: i64 = 0;
        while pj < (paras.len() as i64) {
            let p: Rc<RefCell<Paragraph>> = paras[pj as usize].clone();
            let txt: String = p.borrow().text.clone();
            let len: i64 = DxNative::str_len(&txt);
            let mut row: Rc<RefCell<DocTableRow>> = Rc::new(RefCell::new(DocTableRow::new()));
            let mut from: i64 = 0;
            let mut made: i64 = 0;
            while from <= len {
                let mut to: i64 = DxNative::index_of_from(&txt, "\t", from);
                if to < 0 {
                    to = len;
                }
                let mut cell: Rc<RefCell<DocTableCell>> = Rc::new(RefCell::new(DocTableCell::new()));
                let mut cp: Rc<RefCell<Paragraph>> = RichDocument::addParagraphOnly(doc);
                cp.borrow_mut().styleId = p.borrow().styleId.clone();
                cp.borrow_mut().align = p.borrow().align;
                cp.borrow_mut().spaceBeforePt = 0.0;
                cp.borrow_mut().spaceAfterPt = 0.0;
                cp.borrow_mut().lineSpacing = 1.0;
                DocxTableEdit::copyRange(&p, from, to, &cp);
                cell.borrow_mut().paragraphs.push(cp.clone());
                row.borrow_mut().cells.push(cell.clone());
                made += 1;
                from = to + 1;
            }
            // A short row's last cell covers the rest of the grid.
            if made < cols {
                let n: i64 = row.borrow().cells.len() as i64;
                let last: Rc<RefCell<DocTableCell>> = row.borrow().cells[(n - 1) as usize].clone();
                last.borrow_mut().gridSpan = cols - made + 1;
            }
            t.borrow_mut().rows.push(row.clone());
            pj += 1;
        }
        t
    }
}
