// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rich_document::{DocBlock, DocBlockKind, DocChart, DocTableBorder, DocTableCell, DocTableRow, DocumentTable, Paragraph, RichDocument, TextSpan};
use crate::rgr::DxNative;
use crate::rgr::{DocumentPosition};

/// ==============================================================================
/// RichDocumentEdit - TextEditingTarget adapter over RichDocument paragraphs.
/// ==============================================================================
/// Mutates Paragraph.text + TextSpan[] only. No OOXML. StyleId preserved.
/// ==============================================================================
pub struct RichDocumentEdit {
    doc: Rc<RefCell<RichDocument>>,
    lastMergeCaret: Rc<RefCell<DocumentPosition>>,
}

impl RichDocumentEdit {
    pub fn new() -> Self {
        Self {
            doc: Rc::new(RefCell::new(RichDocument::new())),
            lastMergeCaret: Rc::new(RefCell::new(DocumentPosition::new())),
        }
    }
    fn bind(&mut self, d: Rc<RefCell<RichDocument>>) {
        self.doc = d.clone();
    }
    fn findParagraph(&self, paragraph_id: i64) -> Rc<RefCell<Paragraph>> {
        let mut i: i64 = 0;
        while i < RichDocument::paragraphCount(&self.doc) {
            let mut p: Rc<RefCell<Paragraph>> = RichDocument::paragraphAt(&self.doc, i);
            if p.borrow().id == paragraph_id {
                return p;
            }
            i += 1;
        }
        Rc::new(RefCell::new(Paragraph::new()))
    }
    pub fn findParagraphIndex(&self, paragraph_id: i64) -> i64 {
        let mut i: i64 = 0;
        while i < RichDocument::paragraphCount(&self.doc) {
            let mut p: Rc<RefCell<Paragraph>> = RichDocument::paragraphAt(&self.doc, i);
            if p.borrow().id == paragraph_id {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn getText(&self, paragraph_id: i64) -> String {
        let mut p: Rc<RefCell<Paragraph>> = self.findParagraph(paragraph_id);
        return p.borrow().text.clone();
    }
    pub fn textLength(&self, paragraph_id: i64) -> i64 {
        (DxNative::str_len(&self.getText(paragraph_id)))
    }
    pub fn paragraphExists(&self, paragraph_id: i64) -> bool {
        self.findParagraphIndex(paragraph_id) >= 0
    }
    /// Span covering offset (or last span).
    fn spanAt(&self, p: &Rc<RefCell<Paragraph>>, offset: i64) -> Rc<RefCell<TextSpan>> {
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if offset >= s.borrow().start && offset < s.borrow().end {
                return s;
            }
            if offset == s.borrow().end && offset == (DxNative::str_len(&p.borrow().text)) {
                return s;
            }
            i += 1;
        }
        if !p.borrow().spans.is_empty() {
            return p.borrow().spans[((p.borrow().spans.len() as i64) - 1) as usize].clone();
        }
        let mut empty: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
        empty
    }
    fn mergeAdjacentSpans(&self, p: &Rc<RefCell<Paragraph>>) {
        if (p.borrow().spans.len() as i64) < 2 {
            return;
        }
        let mut out: Vec<Rc<RefCell<TextSpan>>> = Vec::new();
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = TextSpan::copy(&p.borrow().spans[i as usize].clone());
            if !out.is_empty() {
                let mut prev: Rc<RefCell<TextSpan>> = out[((out.len() as i64) - 1) as usize].clone();
                if prev.borrow().end == s.borrow().start && TextSpan::sameFormat(&prev, &s) {
                    prev.borrow_mut().end = s.borrow().end;
                } else {
                    out.push(s.clone());
                }
            } else {
                out.push(s.clone());
            }
            i += 1;
        }
        p.borrow_mut().spans.clear();
        let mut j: i64 = 0;
        while j < (out.len() as i64) {
            p.borrow_mut().spans.push(out[j as usize].clone());
            j += 1;
        }
    }
    /// Rebuild spans after text replacement at [start,end) → text.
    /// Overlapping spans are split/clipped — never dropped wholesale (that used
    /// to wipe heading/title run formatting down to default 11pt on Backspace).
    pub fn replaceRange(&self, paragraph_id: i64, mut start: i64, mut end: i64, text: &str) {
        RichDocument::touch(&self.doc.clone());
        let mut p: Rc<RefCell<Paragraph>> = self.findParagraph(paragraph_id);
        if p.borrow().id != paragraph_id {
            return;
        }
        let __len: i64 = DxNative::str_len(&p.borrow().text);
        if start < 0 {
            start = 0;
        }
        if end < start {
            end = start;
        }
        if start > __len {
            start = __len;
        }
        if end > __len {
            end = __len;
        }
        let mut tip: Rc<RefCell<TextSpan>> = self.spanAt(&p, start);
        let mut insert_fmt: Rc<RefCell<TextSpan>> = TextSpan::copy(&tip);
        // Align span edges to the edit range before removing the middle.
        self.splitSpansAt(&p, start);
        self.splitSpansAt(&p, end);
        let before: String = DxNative::substr(&p.borrow().text, 0, start);
        let after: String = DxNative::substr(&p.borrow().text, end, __len);
        let delta: i64 = (DxNative::str_len(&text)) - (end - start);
        let mut new_spans: Vec<Rc<RefCell<TextSpan>>> = Vec::new();
        let mut si: i64 = 0;
        while si < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[si as usize].clone();
            if s.borrow().end <= start {
                if s.borrow().end > s.borrow().start {
                    new_spans.push(TextSpan::copy(&s));
                }
            } else {
                if s.borrow().start >= end {
                    let mut r: Rc<RefCell<TextSpan>> = TextSpan::copy(&s);
                    r.borrow_mut().start = r.borrow().start + delta;
                    r.borrow_mut().end = r.borrow().end + delta;
                    if r.borrow().end > r.borrow().start {
                        new_spans.push(r.clone());
                    }
                }
                // fully inside [start,end) — drop
            }
            si += 1;
        }
        p.borrow_mut().text = format!("{}{}{}", before, text, after);
        if !text.is_empty() {
            let mut mid: Rc<RefCell<TextSpan>> = TextSpan::copy(&insert_fmt);
            mid.borrow_mut().start = start;
            mid.borrow_mut().end = start + (DxNative::str_len(&text));
            let mut placed: bool = false;
            let mut out2: Vec<Rc<RefCell<TextSpan>>> = Vec::new();
            let mut k: i64 = 0;
            while k < (new_spans.len() as i64) {
                let mut ns: Rc<RefCell<TextSpan>> = new_spans[k as usize].clone();
                if !placed && ns.borrow().start >= mid.borrow().end {
                    out2.push(mid.clone());
                    placed = true;
                }
                out2.push(ns.clone());
                k += 1;
            }
            if !placed {
                out2.push(mid.clone());
            }
            new_spans.clear();
            let mut k2: i64 = 0;
            while k2 < (out2.len() as i64) {
                new_spans.push(out2[k2 as usize].clone());
                k2 += 1;
            }
        }
        p.borrow_mut().spans.clear();
        let mut m: i64 = 0;
        while m < (new_spans.len() as i64) {
            p.borrow_mut().spans.push(new_spans[m as usize].clone());
            m += 1;
        }
        if p.borrow().spans.is_empty() {
            // Preserve tip formatting even for empty paragraphs / full deletes.
            let mut one: Rc<RefCell<TextSpan>> = TextSpan::copy(&insert_fmt);
            one.borrow_mut().start = 0;
            one.borrow_mut().end = (DxNative::str_len(&p.borrow().text));
            p.borrow_mut().spans.push(one.clone());
        }
        self.mergeAdjacentSpans(&p);
    }
    pub fn splitParagraph(&self, paragraph_id: i64, offset: i64) -> i64 {
        self.splitParagraphAs(paragraph_id, offset, 0)
    }
    /// `keepId` > 0 means "the new paragraph is THIS paragraph coming back" —
    /// what a redo needs, so the ops after it in the same action still name
    /// something that exists.
    pub fn splitParagraphAs(&self, paragraph_id: i64, mut offset: i64, keep_id: i64) -> i64 {
        RichDocument::touch(&self.doc.clone());
        let idx: i64 = self.findParagraphIndex(paragraph_id);
        if idx < 0 {
            return paragraph_id;
        }
        let mut p: Rc<RefCell<Paragraph>> = RichDocument::paragraphAt(&self.doc, idx);
        let __len: i64 = DxNative::str_len(&p.borrow().text);
        if offset < 0 {
            offset = 0;
        }
        if offset > __len {
            offset = __len;
        }
        let left: String = DxNative::substr(&p.borrow().text, 0, offset);
        let right: String = DxNative::substr(&p.borrow().text, offset, __len);
        // New paragraph after p in blocks + paragraphs list
        let mut np: Rc<RefCell<Paragraph>> = RichDocument::addParagraphWithId(&self.doc, keep_id);
        // ALL of the paragraph's formatting, not the thirteen fields somebody
        // remembered — see Paragraph.copyFormatTo. The list membership comes
        // with it, or Enter inside a list drops out of it and leaves a blank
        // line between the items; the marker itself is not copied, because
        // DocxLayout renumbers from listNumId as it lays the list out. And so
        // does the DIRECTION, which the hand-written list had missed: Enter in
        // an Arabic paragraph gave a left-to-right one, and in Word the
        // direction is the alignment when no `w:jc` was written, so it also
        // jumped to the left margin.
        Paragraph::copyFormatTo(&p, &np);
        np.borrow_mut().text = right.clone();
        // Split spans
        np.borrow_mut().spans.clear();
        let mut tip: Rc<RefCell<TextSpan>> = self.spanAt(&p, offset);
        let mut base: Rc<RefCell<TextSpan>> = TextSpan::copy(&tip);
        if !right.is_empty() {
            base.borrow_mut().start = 0;
            base.borrow_mut().end = DxNative::str_len(&right);
            np.borrow_mut().spans.push(base.clone());
        } else {
            let mut empty: Rc<RefCell<TextSpan>> = TextSpan::copy(&tip);
            empty.borrow_mut().start = 0;
            empty.borrow_mut().end = 0;
            np.borrow_mut().spans.push(empty.clone());
        }
        // Truncate original
        self.replaceRange(paragraph_id, offset, __len, "");
        p.borrow_mut().text = left.clone();
        // Ensure left spans clipped
        let mut keep: Vec<Rc<RefCell<TextSpan>>> = Vec::new();
        let mut si: i64 = 0;
        while si < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[si as usize].clone();
            if s.borrow().start < offset {
                let mut c: Rc<RefCell<TextSpan>> = TextSpan::copy(&s);
                if c.borrow().end > offset {
                    c.borrow_mut().end = offset;
                }
                keep.push(c.clone());
            }
            si += 1;
        }
        p.borrow_mut().spans.clear();
        let mut ki: i64 = 0;
        while ki < (keep.len() as i64) {
            p.borrow_mut().spans.push(keep[ki as usize].clone());
            ki += 1;
        }
        if p.borrow().spans.is_empty() {
            p.borrow_mut().spans.push(TextSpan::create(0, (DxNative::str_len(&p.borrow().text))));
        }
        // Insert block after the block that owns p
        let mut bi: i64 = 0;
        let mut found: bool = false;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                if b.borrow().paragraph.borrow().id == paragraph_id {
                    let mut nb: Rc<RefCell<DocBlock>> = DocBlock::para(np.clone());
                    // insert after bi
                    let mut rest: Vec<Rc<RefCell<DocBlock>>> = Vec::new();
                    let mut rj: i64 = bi + 1;
                    while rj < RichDocument::blockCount(&self.doc) {
                        rest.push(RichDocument::blockAt(&self.doc, rj));
                        rj += 1;
                    }
                    while (self.doc.borrow().blocks.len() as i64) > bi + 1 {
                        self.doc.borrow_mut().blocks.pop();
                    }
                    self.doc.borrow_mut().blocks.push(nb.clone());
                    let mut rk: i64 = 0;
                    while rk < (rest.len() as i64) {
                        self.doc.borrow_mut().blocks.push(rest[rk as usize].clone());
                        rk += 1;
                    }
                    found = true;
                    bi = RichDocument::blockCount(&self.doc);
                }
            }
            bi += 1;
        }
        if !found {
            self.doc.borrow_mut().blocks.push(DocBlock::para(np.clone()));
        }
        return np.borrow().id;
    }
    /// --- body order -----------------------------------------------------------
    /// Selection spanning paragraphs needs to know which one comes first. Only
    /// top-level paragraph blocks count: table cells and headers are not part of
    /// the linear caret flow.
    pub fn bodyOrderOf(&self, paragraph_id: i64) -> i64 {
        let mut order: i64 = 0;
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                if b.borrow().paragraph.borrow().id == paragraph_id {
                    return order;
                }
                order += 1;
            }
            bi += 1;
        }
        -1
    }
    pub fn bodyParagraphCount(&self) -> i64 {
        let mut count: i64 = 0;
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                count += 1;
            }
            bi += 1;
        }
        count
    }
    pub fn bodyParagraphIdAt(&self, order: i64) -> i64 {
        let mut seen: i64 = 0;
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                if seen == order {
                    return b.borrow().paragraph.borrow().id;
                }
                seen += 1;
            }
            bi += 1;
        }
        0
    }
    /// --- table cells ----------------------------------------------------------
    /// Cell paragraphs are created with addParagraphOnly, so they are NOT body
    /// blocks and bodyOrderOf returns -1 for them. Tab navigation needs to locate
    /// a caret inside a table instead. Answers [blockIndex, rowIndex, cellIndex,
    /// paragraphIndexInCell]; blockIndex -1 when the paragraph is not in a table.
    pub fn tableCellOf(&self, paragraph_id: i64) -> Vec<i64> {
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::table() {
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
                            if cp.borrow().id == paragraph_id {
                                return vec![bi, ri, ci, pi];
                            }
                            pi += 1;
                        }
                        ci += 1;
                    }
                    ri += 1;
                }
            }
            bi += 1;
        }
        vec![-1, -1, -1, -1]
    }
    fn tableAtBlock(&self, block_index: i64) -> Rc<RefCell<DocumentTable>> {
        let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, block_index);
        return b.borrow().table.clone();
    }
    /// First paragraph of a cell, or 0 when the coordinates are out of range.
    pub fn cellParagraphId(&self, block_index: i64, row_index: i64, cell_index: i64) -> i64 {
        if block_index < 0 || block_index >= RichDocument::blockCount(&self.doc) {
            return 0;
        }
        let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, block_index);
        if b.borrow().kind != DocBlockKind::table() {
            return 0;
        }
        let mut t: Rc<RefCell<DocumentTable>> = b.borrow().table.clone().clone();
        if row_index < 0 || row_index >= (t.borrow().rows.len() as i64) {
            return 0;
        }
        let mut row: Rc<RefCell<DocTableRow>> = t.borrow().rows[row_index as usize].clone();
        if cell_index < 0 || cell_index >= (row.borrow().cells.len() as i64) {
            return 0;
        }
        let mut cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[cell_index as usize].clone();
        if cell.borrow().paragraphs.is_empty() {
            return 0;
        }
        let mut first: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[0].clone();
        return first.borrow().id;
    }
    pub fn cellCountInRow(&self, block_index: i64, row_index: i64) -> i64 {
        if block_index < 0 || block_index >= RichDocument::blockCount(&self.doc) {
            return 0;
        }
        let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, block_index);
        if b.borrow().kind != DocBlockKind::table() {
            return 0;
        }
        let mut t: Rc<RefCell<DocumentTable>> = b.borrow().table.clone().clone();
        if row_index < 0 || row_index >= (t.borrow().rows.len() as i64) {
            return 0;
        }
        let mut row: Rc<RefCell<DocTableRow>> = t.borrow().rows[row_index as usize].clone();
        return row.borrow().cells.len() as i64;
    }
    pub fn rowCountOfTable(&self, block_index: i64) -> i64 {
        if block_index < 0 || block_index >= RichDocument::blockCount(&self.doc) {
            return 0;
        }
        let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, block_index);
        if b.borrow().kind != DocBlockKind::table() {
            return 0;
        }
        let mut t: Rc<RefCell<DocumentTable>> = b.borrow().table.clone().clone();
        return t.borrow().rows.len() as i64;
    }
    /// First body paragraph appearing after / before a given block index.
    pub fn bodyParagraphAfterBlock(&self, block_index: i64) -> i64 {
        let mut bi: i64 = block_index + 1;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                return b.borrow().paragraph.borrow().id;
            }
            bi += 1;
        }
        0
    }
    pub fn bodyParagraphBeforeBlock(&self, block_index: i64) -> i64 {
        let mut bi: i64 = block_index - 1;
        while bi >= 0 {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                return b.borrow().paragraph.borrow().id;
            }
            bi -= 1;
        }
        0
    }
    /// The table block that immediately follows / precedes a body paragraph, or -1.
    pub fn tableBlockAfterParagraph(&self, paragraph_id: i64) -> i64 {
        let at: i64 = self.blockIndexOfParagraph(paragraph_id);
        if at < 0 {
            return -1;
        }
        let mut bi: i64 = at + 1;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::table() {
                return bi;
            }
            if b.borrow().kind == DocBlockKind::para() {
                return -1;
            }
            bi += 1;
        }
        -1
    }
    pub fn tableBlockBeforeParagraph(&self, paragraph_id: i64) -> i64 {
        let at: i64 = self.blockIndexOfParagraph(paragraph_id);
        if at < 0 {
            return -1;
        }
        let mut bi: i64 = at - 1;
        while bi >= 0 {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::table() {
                return bi;
            }
            if b.borrow().kind == DocBlockKind::para() {
                return -1;
            }
            bi -= 1;
        }
        -1
    }
    /// Word adds a row when you Tab out of the last cell. The new row copies the
    /// last row's cell count and cell chrome so it matches the table.
    pub fn appendTableRow(&self, block_index: i64) -> i64 {
        RichDocument::touch(&self.doc.clone());
        if block_index < 0 || block_index >= RichDocument::blockCount(&self.doc) {
            return -1;
        }
        let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, block_index);
        if b.borrow().kind != DocBlockKind::table() {
            return -1;
        }
        let mut t: Rc<RefCell<DocumentTable>> = b.borrow().table.clone().clone();
        let row_n: i64 = t.borrow().rows.len() as i64;
        if row_n == 0 {
            return -1;
        }
        let mut template: Rc<RefCell<DocTableRow>> = t.borrow().rows[(row_n - 1) as usize].clone();
        let mut row: Rc<RefCell<DocTableRow>> = Rc::new(RefCell::new(DocTableRow::new()));
        row.borrow_mut().heightTwips = template.borrow().heightTwips;
        let mut ci: i64 = 0;
        while ci < (template.borrow().cells.len() as i64) {
            let mut src: Rc<RefCell<DocTableCell>> = template.borrow().cells[ci as usize].clone();
            let mut cell: Rc<RefCell<DocTableCell>> = Rc::new(RefCell::new(DocTableCell::new()));
            cell.borrow_mut().gridSpan = src.borrow().gridSpan;
            cell.borrow_mut().vAlign = src.borrow().vAlign;
            cell.borrow_mut().padTopPx = src.borrow().padTopPx;
            cell.borrow_mut().padRightPx = src.borrow().padRightPx;
            cell.borrow_mut().padBottomPx = src.borrow().padBottomPx;
            cell.borrow_mut().padLeftPx = src.borrow().padLeftPx;
            cell.borrow_mut().borderTop = Rc::new(RefCell::new(DocTableBorder::new()));
            cell.borrow_mut().borderLeft = Rc::new(RefCell::new(DocTableBorder::new()));
            cell.borrow_mut().borderBottom = Rc::new(RefCell::new(DocTableBorder::new()));
            cell.borrow_mut().borderRight = Rc::new(RefCell::new(DocTableBorder::new()));
            let mut p: Rc<RefCell<Paragraph>> = RichDocument::addParagraphOnly(&self.doc);
            p.borrow_mut().styleId = "Normal".to_string();
            p.borrow_mut().spaceBeforePt = 0.0;
            p.borrow_mut().spaceAfterPt = 0.0;
            p.borrow_mut().lineSpacing = 1.0;
            let mut sp: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
            if !src.borrow().paragraphs.is_empty() {
                let mut src_para: Rc<RefCell<Paragraph>> = src.borrow().paragraphs[0].clone();
                p.borrow_mut().align = src_para.borrow().align;
                if !src_para.borrow().spans.is_empty() {
                    let mut src_span: Rc<RefCell<TextSpan>> = src_para.borrow().spans[0].clone();
                    sp.borrow_mut().fontSizePt = src_span.borrow().fontSizePt;
                    sp.borrow_mut().fontFamily = src_span.borrow().fontFamily.clone();
                    sp.borrow_mut().colorHex = src_span.borrow().colorHex.clone();
                }
            }
            p.borrow_mut().spans.push(sp.clone());
            cell.borrow_mut().paragraphs.push(p.clone());
            row.borrow_mut().cells.push(cell.clone());
            ci += 1;
        }
        t.borrow_mut().rows.push(row.clone());
        return t.borrow().rows.len() as i64;
    }
    pub fn removeLastTableRow(&self, block_index: i64) -> bool {
        RichDocument::touch(&self.doc.clone());
        if block_index < 0 || block_index >= RichDocument::blockCount(&self.doc) {
            return false;
        }
        let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, block_index);
        if b.borrow().kind != DocBlockKind::table() {
            return false;
        }
        let mut t: Rc<RefCell<DocumentTable>> = b.borrow().table.clone().clone();
        if (t.borrow().rows.len() as i64) <= 1 {
            return false;
        }
        t.borrow_mut().rows.pop();
        true
    }
    /// Block index of the top-level block holding this paragraph, or -1.
    pub fn blockIndexOfParagraph(&self, paragraph_id: i64) -> i64 {
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                if b.borrow().paragraph.borrow().id == paragraph_id {
                    return bi;
                }
            }
            bi += 1;
        }
        -1
    }
    /// Splice a fresh table block immediately before the given paragraph's block.
    fn insertTableBefore(&self, paragraph_id: i64) -> Rc<RefCell<DocumentTable>> {
        RichDocument::touch(&self.doc.clone());
        let mut t: Rc<RefCell<DocumentTable>> = RichDocument::addTableOnly(&self.doc);
        let at: i64 = self.blockIndexOfParagraph(paragraph_id);
        if at < 0 {
            self.doc.borrow_mut().blocks.push(DocBlock::tbl(t.clone()));
            return t;
        }
        let mut rest: Vec<Rc<RefCell<DocBlock>>> = Vec::new();
        let mut rj: i64 = at;
        while rj < RichDocument::blockCount(&self.doc) {
            rest.push(RichDocument::blockAt(&self.doc, rj));
            rj += 1;
        }
        while (self.doc.borrow().blocks.len() as i64) > at {
            self.doc.borrow_mut().blocks.pop();
        }
        self.doc.borrow_mut().blocks.push(DocBlock::tbl(t.clone()));
        let mut rk: i64 = 0;
        while rk < (rest.len() as i64) {
            self.doc.borrow_mut().blocks.push(rest[rk as usize].clone());
            rk += 1;
        }
        t
    }
    /// A chart, inserted before the paragraph the caret is in. Same shape as a
    /// table paste: a chart is a block, and the paragraph it interrupts has
    /// already been split in two.
    fn insertChartBefore(&self, paragraph_id: i64, json: &str) -> Rc<RefCell<DocChart>> {
        RichDocument::touch(&self.doc.clone());
        let mut c: Rc<RefCell<DocChart>> = RichDocument::addChart(&self.doc, json.to_string());
        // addChart appended a block for it; take that back off and put it
        // where the caret is instead.
        self.doc.borrow_mut().blocks.pop();
        let at: i64 = self.blockIndexOfParagraph(paragraph_id);
        if at < 0 {
            self.doc.borrow_mut().blocks.push(DocBlock::chartBlock(c.clone()));
            return c;
        }
        let mut rest: Vec<Rc<RefCell<DocBlock>>> = Vec::new();
        let mut rj: i64 = at;
        while rj < RichDocument::blockCount(&self.doc) {
            rest.push(RichDocument::blockAt(&self.doc, rj));
            rj += 1;
        }
        while (self.doc.borrow().blocks.len() as i64) > at {
            self.doc.borrow_mut().blocks.pop();
        }
        self.doc.borrow_mut().blocks.push(DocBlock::chartBlock(c.clone()));
        let mut rk: i64 = 0;
        while rk < (rest.len() as i64) {
            self.doc.borrow_mut().blocks.push(rest[rk as usize].clone());
            rk += 1;
        }
        c
    }
    /// Remove the block holding this chart (used to undo a chart paste, and to
    /// delete one).
    pub fn removeChartBlock(&self, chart_id: i64) -> bool {
        RichDocument::touch(&self.doc.clone());
        let mut keep: Vec<Rc<RefCell<DocBlock>>> = Vec::new();
        let mut found: bool = false;
        let mut i: i64 = 0;
        while i < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, i);
            let mut drop: bool = false;
            if b.borrow().kind == DocBlockKind::chart() {
                if b.borrow().chart.borrow().id == chart_id {
                    drop = true;
                    found = true;
                }
            }
            if !drop {
                keep.push(b.clone());
            }
            i += 1;
        }
        if found {
            self.doc.borrow_mut().blocks.clear();
            let mut k: i64 = 0;
            while k < (keep.len() as i64) {
                self.doc.borrow_mut().blocks.push(keep[k as usize].clone());
                k += 1;
            }
        }
        found
    }
    /// Remove the block holding this table (used to undo a table paste).
    pub fn removeTableById(&self, table_id: i64) -> bool {
        RichDocument::touch(&self.doc.clone());
        let mut at: i64 = -1;
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::table() {
                if b.borrow().table.borrow().id == table_id {
                    at = bi;
                    bi = RichDocument::blockCount(&self.doc);
                }
            }
            bi += 1;
        }
        if at < 0 {
            return false;
        }
        let mut rest: Vec<Rc<RefCell<DocBlock>>> = Vec::new();
        let mut rj: i64 = at + 1;
        while rj < RichDocument::blockCount(&self.doc) {
            rest.push(RichDocument::blockAt(&self.doc, rj));
            rj += 1;
        }
        while (self.doc.borrow().blocks.len() as i64) > at {
            self.doc.borrow_mut().blocks.pop();
        }
        let mut rk: i64 = 0;
        while rk < (rest.len() as i64) {
            self.doc.borrow_mut().blocks.push(rest[rk as usize].clone());
            rk += 1;
        }
        true
    }
    /// Merge paragraphId into previous top-level body paragraph.
    fn mergePrevious(&mut self, paragraph_id: i64) -> Rc<RefCell<DocumentPosition>> {
        RichDocument::touch(&self.doc.clone());
        let mut fail: Rc<RefCell<DocumentPosition>> = DocumentPosition::create(0, 0);
        let idx: i64 = self.findParagraphIndex(paragraph_id);
        if idx <= 0 {
            return fail;
        }
        // Find previous paragraph that appears as a body block before this one
        let mut prev_id: i64 = 0;
        let mut bi: i64 = 0;
        let mut saw: bool = false;
        while bi < RichDocument::blockCount(&self.doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                if b.borrow().paragraph.borrow().id == paragraph_id {
                    saw = true;
                    bi = RichDocument::blockCount(&self.doc);
                } else {
                    prev_id = b.borrow().paragraph.borrow().id;
                }
            }
            bi += 1;
        }
        if !saw {
            return fail;
        }
        if prev_id <= 0 {
            return fail;
        }
        let mut prev: Rc<RefCell<Paragraph>> = self.findParagraph(prev_id);
        let mut cur: Rc<RefCell<Paragraph>> = self.findParagraph(paragraph_id);
        let join_at: i64 = DxNative::str_len(&prev.borrow().text);
        let joined: String = cur.borrow().text.clone();
        // Append text with formatting from first span of cur
        let mut fmt: Rc<RefCell<TextSpan>> = self.spanAt(&cur, 0);
        self.replaceRange(prev_id, join_at, join_at, &joined);
        if !joined.is_empty() {
            // apply cur's first span attrs onto inserted range roughly
            let a: i64 = join_at;
            let b_1: i64 = join_at + (DxNative::str_len(&joined));
            self.applyBold(prev_id, a, b_1, fmt.borrow().bold.clone());
        }
        // Remove cur from blocks + paragraphs
        let mut nb: Vec<Rc<RefCell<DocBlock>>> = Vec::new();
        let mut bj: i64 = 0;
        while bj < RichDocument::blockCount(&self.doc) {
            let mut bb: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&self.doc, bj);
            if bb.borrow().kind == DocBlockKind::para() {
                if bb.borrow().paragraph.borrow().id == paragraph_id {
                    // skip
                } else {
                    nb.push(bb.clone());
                }
            } else {
                nb.push(bb.clone());
            }
            bj += 1;
        }
        self.doc.borrow_mut().blocks.clear();
        let mut bk: i64 = 0;
        while bk < (nb.len() as i64) {
            self.doc.borrow_mut().blocks.push(nb[bk as usize].clone());
            bk += 1;
        }
        let mut np: Vec<Rc<RefCell<Paragraph>>> = Vec::new();
        let mut pi: i64 = 0;
        while pi < RichDocument::paragraphCount(&self.doc) {
            let mut pp: Rc<RefCell<Paragraph>> = RichDocument::paragraphAt(&self.doc, pi);
            if pp.borrow().id == paragraph_id {
                // skip
            } else {
                np.push(pp.clone());
            }
            pi += 1;
        }
        self.doc.borrow_mut().paragraphs.clear();
        let mut pk: i64 = 0;
        while pk < (np.len() as i64) {
            self.doc.borrow_mut().paragraphs.push(np[pk as usize].clone());
            pk += 1;
        }
        self.lastMergeCaret = DocumentPosition::create(prev_id, join_at);
        return DocumentPosition::copy(&self.lastMergeCaret);
    }
    /// --- run properties, by name ---------------------------------------------
    /// There was one of these, for bold, written out longhand — split the spans
    /// at both ends, walk them, set the field, merge back. Italic, underline, a
    /// point size and a colour are the same six lines with one field changed, and
    /// writing them out five more times is how a file grows a bug in the fifth
    /// copy only. So the field is a NAME, and there is one applier.
    ///
    ///   "bold" "italic" "underline"   flag, from `on`
    ///   "size"                        points, from `num`
    ///   "font" "color"                text, from `text`
    pub fn applyRun(&self, paragraph_id: i64, start: i64, end: i64, what: &str, on: bool, num: f64, text: String) {
        RichDocument::touch(&self.doc.clone());
        let mut p: Rc<RefCell<Paragraph>> = self.findParagraph(paragraph_id);
        if p.borrow().id != paragraph_id {
            return;
        }
        if start >= end {
            return;
        }
        self.splitSpansAt(&p, start);
        self.splitSpansAt(&p, end);
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if s.borrow().start >= start && s.borrow().end <= end {
                if what == "bold" {
                    s.borrow_mut().bold = on;
                }
                if what == "italic" {
                    s.borrow_mut().italic = on;
                }
                if what == "underline" {
                    s.borrow_mut().underline = on;
                }
                if what == "size" {
                    s.borrow_mut().fontSizePt = num;
                }
                if what == "font" {
                    s.borrow_mut().fontFamily = text.clone();
                }
                if what == "color" {
                    s.borrow_mut().colorHex = text.clone();
                }
                if what == "strike" {
                    s.borrow_mut().strike = on;
                }
                if what == "super" {
                    s.borrow_mut().vertAlign = if on { 1 } else { 0 };
                }
                if what == "sub" {
                    s.borrow_mut().vertAlign = if on { 2 } else { 0 };
                }
                if what == "highlight" {
                    s.borrow_mut().highlight = text.clone();
                }
            }
            i += 1;
        }
        self.mergeAdjacentSpans(&p);
    }
    /// True when every span covering [start,end) already has this flag on.
    /// "Covered" matters: an empty range has nothing to be on, and answering
    /// yes for one would make a toggle turn bold OFF on an empty selection.
    pub fn rangeIsOn(&self, paragraph_id: i64, start: i64, end: i64, what: &str) -> bool {
        let mut p: Rc<RefCell<Paragraph>> = self.findParagraph(paragraph_id);
        if p.borrow().id != paragraph_id {
            return false;
        }
        if start >= end {
            return false;
        }
        let mut covered: bool = false;
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            let mut a: i64 = s.borrow().start;
            let mut b: i64 = s.borrow().end;
            if a < start {
                a = start;
            }
            if b > end {
                b = end;
            }
            if a < b {
                covered = true;
                let mut on: bool = false;
                if what == "bold" {
                    on = s.borrow().bold;
                }
                if what == "italic" {
                    on = s.borrow().italic;
                }
                if what == "underline" {
                    on = s.borrow().underline;
                }
                if what == "strike" {
                    on = s.borrow().strike;
                }
                if what == "super" {
                    on = s.borrow().vertAlign == 1;
                }
                if what == "sub" {
                    on = s.borrow().vertAlign == 2;
                }
                if what == "highlight" {
                    on = !s.borrow().highlight.is_empty();
                }
                if !on {
                    return false;
                }
            }
            i += 1;
        }
        covered
    }
    /// What the first span of a range says, for a toolbar that has to show the
    /// face and the size of what the caret is in.
    pub fn runFontOf(&self, paragraph_id: i64, offset: i64) -> String {
        let mut p: Rc<RefCell<Paragraph>> = self.findParagraph(paragraph_id);
        if p.borrow().id != paragraph_id {
            return String::new();
        }
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if (offset >= s.borrow().start && offset <= s.borrow().end) {
                return s.borrow().fontFamily.clone();
            }
            i += 1;
        }
        String::new()
    }
    pub fn runSizeOf(&self, paragraph_id: i64, offset: i64) -> f64 {
        let mut p: Rc<RefCell<Paragraph>> = self.findParagraph(paragraph_id);
        if p.borrow().id != paragraph_id {
            return 0.0;
        }
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if (offset >= s.borrow().start && offset <= s.borrow().end) {
                return s.borrow().fontSizePt;
            }
            i += 1;
        }
        0.0
    }
    pub fn applyBold(&self, paragraph_id: i64, start: i64, end: i64, value: bool) {
        self.applyRun(paragraph_id,
            start,
            end,
            "bold",
            value,
            0.0,
            String::new()
        );
    }
    pub fn rangeIsBold(&self, paragraph_id: i64, start: i64, end: i64) -> bool {
        self.rangeIsOn(paragraph_id, start, end, "bold")
    }
    pub fn applyFont(&self, paragraph_id: i64, start: i64, end: i64, font: &str) {
        self.applyRun(paragraph_id,
            start,
            end,
            "font",
            false,
            0.0,
            font.to_string()
        );
    }
    fn splitSpansAt(&self, p: &Rc<RefCell<Paragraph>>, offset: i64) {
        let __len: i64 = DxNative::str_len(&p.borrow().text);
        if offset <= 0 || offset >= __len {
            return;
        }
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut s: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if offset > s.borrow().start && offset < s.borrow().end {
                let mut right: Rc<RefCell<TextSpan>> = TextSpan::copy(&s);
                right.borrow_mut().start = offset;
                s.borrow_mut().end = offset;
                p.borrow_mut().spans.insert((i + 1) as usize, right.clone());
                return;
            }
            i += 1;
        }
    }
}
