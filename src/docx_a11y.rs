// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rich_document::{DocBlock, DocBlockKind, DocChart, DocDrawing, DocImage, DocSection, DocTableCell, DocTableRow, DocumentTable, Paragraph, RichDocument};
use crate::rgr::DxNative;
use crate::rgr::{EVGA11yNode, EVGA11yRole, EVGA11yTree};

/// ==============================================================================
/// DocxA11y — what the page MEANS, beside what it looks like
/// ==============================================================================
///
/// A screen reader cannot read a WebGL canvas. It cannot read a software canvas
/// either. Whatever this viewer draws, the document is simply not there for
/// anyone using one — and that is not a degraded experience, it is a blank
/// window.
///
/// The spreadsheet in this gallery already publishes an `EVGA11yTree` beside its
/// frame, and `EVGA11yTree` lives in `lib/evg` precisely so it is not the
/// spreadsheet's. This is the document's half of it. The deck's is next.
///
/// Where the tree comes from, and why.
///
///   The BLOCKS, not the laid-out lines. A paragraph broken across four visual
///   lines is one thing to read, and it is one thing on the page it is one thing
///   on the next page too. Building this from `LaidLine` would make the reading
///   order depend on the page width, and a reader's position would move when the
///   window was resized.
///
///   Stable ids from identity. `doc/para:12`, `doc/table:3/row:2/cell:1` — every
///   one of those numbers is a model id that survives an edit, not a position in
///   an array. `EVGA11yNode` says this is the most common way a first
///   implementation fails, and it is right: an id that changes each frame makes
///   the reader lose its cursor and reset focus, and everything still looks
///   correct on screen.
///
///   Headings from the style, because that is what they are. Word does not store
///   "this is a heading"; it stores `Heading2`, and every navigate-by-heading
///   command in every reader is built on that. A document whose author used the
///   heading styles becomes navigable here for free; one that faked headings with
///   bold 18pt text does not, which is the honest answer.
///
///   Alt text is the author's or it is absent. `wp:docPr/@descr` is the only
///   thing anyone can say about a picture. Where a document has none, this says
///   "Image" rather than inventing a description — a picture with no alt text is
///   a documented failure of the document, and covering it up makes it harder to
///   find, not easier to live with.
///
/// Headers and footers are published, and deliberately outside the body: they
/// repeat on every page, and a reader that met them in the reading flow would
/// read the page furniture between every two paragraphs.
/// ==============================================================================
#[derive(Clone)]
pub struct DocxA11y {
}

impl DocxA11y {
    pub fn new() -> Self {
        Self {
        }
    }
    /// --- headings -------------------------------------------------------------
    /// The outline level of a paragraph style, 1..9, or 0 for body text.
    ///
    /// Word writes the style id without spaces — `Heading1`, `heading 1` in some
    /// producers' files — so the comparison is made on a lowercased id with the
    /// spaces taken out. `Title` is level 1: it is the document's own heading and
    /// a reader should be able to jump to it.
    pub fn headingLevel(style_id: &str) -> i64 {
        let id: String = DocxA11y::squash(style_id);
        if id == "title" {
            return 1;
        }
        if id == "subtitle" {
            return 2;
        }
        if DxNative::index_of(&id, "heading") != 0 {
            return 0;
        }
        let rest: String = DxNative::substr(&id, 7, (DxNative::str_len(&id)));
        if rest.is_empty() {
            return 0;
        }
        let lvl: Option<i64> = rest.parse::<i64>().ok();
        if lvl.is_none() {
            return 0;
        }
        let v: i64 = lvl.unwrap();
        if v < 1 {
            return 0;
        }
        if v > 9 {
            return 0;
        }
        v
    }
    /// Lowercased with the spaces removed, so `Heading 1` and `heading1` are the
    /// same style to this file. They are the same style to Word too.
    pub fn squash(s: &str) -> String {
        let lower: String = s.to_lowercase();
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < (DxNative::str_len(&lower)) {
            let ch: i64 = DxNative::char_at(&lower, i);
            if ch != 32 {
                out = format!("{}{}", out, DxNative::substr(&lower, i, i + 1));
            }
            i += 1;
        }
        out
    }
    /// --- names ----------------------------------------------------------------
    /// What a reader says for a paragraph with no text of its own. An empty
    /// paragraph IS meaningful — it is the blank line the author left — so it is
    /// published rather than skipped, named as what it is.
    fn paragraphName(p: &Rc<RefCell<Paragraph>>) -> String {
        if !p.borrow().text.is_empty() {
            return p.borrow().text.clone();
        }
        "Blank line".to_string()
    }
    /// --- the tree -------------------------------------------------------------
    /// `caretParagraphId` is the paragraph the caret is in, 0 when the document
    /// is not being edited. `caretOffset` and the selection are offsets into that
    /// paragraph's text.
    fn build(doc: &Rc<RefCell<RichDocument>>, caret_paragraph_id: i64, caret_offset: i64, sel_start: i64, sel_end: i64, read_only: bool) -> Rc<RefCell<EVGA11yTree>> {
        let mut t: Rc<RefCell<EVGA11yTree>> = Rc::new(RefCell::new(EVGA11yTree::new()));
        let mut name: String = "Document".to_string();
        if !doc.borrow().title.is_empty() {
            name = format!("Document, {}", doc.borrow().title);
        }
        let mut _root: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::group(&t, "doc", "", name.clone());
        let mut _body: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::group(
            &t,
            "doc/body",
            "doc",
            "Document body".to_string()
        );
        // Lists nest, and flattening them is not a cosmetic loss. A reader
        // announces "3 of 5" from `posInSet`/`setSize`, so an item that is
        // really the second of two in a sub-list gets announced as the third of
        // five in its parent — a position that does not exist. The fixture
        // `lists_demo.docx` has a three-level outline and caught exactly that.
        //
        // The open lists are held as parallel stacks because Ranger has no
        // tuples: level, node id, how many items that list has, how many have
        // been emitted, and the last item emitted — which is the PARENT of any
        // list that opens below it.
        let mut lvl_stack: Vec<i64> = Vec::new();
        let mut id_stack: Vec<String> = Vec::new();
        let mut size_stack: Vec<i64> = Vec::new();
        let mut seen_stack: Vec<i64> = Vec::new();
        let mut last_stack: Vec<String> = Vec::new();
        let mut bi: i64 = 0;
        while bi < RichDocument::blockCount(&doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&doc, bi);
            if b.borrow().kind == DocBlockKind::para() {
                let mut p: Rc<RefCell<Paragraph>> = b.borrow().paragraph.clone().clone();
                let pid: String = format!("doc/para:{}", p.borrow().id);
                if p.borrow().listKind > 0 {
                    // Close every list deeper than this item.
                    let mut depth: i64 = lvl_stack.len() as i64;
                    while depth > 0 {
                        let top: i64 = lvl_stack[(depth - 1) as usize];
                        if top > p.borrow().listLevel {
                            lvl_stack.pop();
                            id_stack.pop();
                            size_stack.pop();
                            seen_stack.pop();
                            last_stack.pop();
                            depth -= 1;
                        } else {
                            depth = 0;
                        }
                    }
                    // Open one if this item is deeper than anything open.
                    let n2: i64 = lvl_stack.len() as i64;
                    let mut need_open: bool = n2 == 0;
                    if !need_open {
                        let cur: i64 = lvl_stack[(n2 - 1) as usize];
                        need_open = cur < p.borrow().listLevel;
                    }
                    if need_open {
                        let mut parent: String = "doc/body".to_string();
                        if n2 > 0 {
                            // A nested list belongs to the item above it, not
                            // to the body: that is what makes "list of 2" come
                            // after the item it hangs off.
                            parent = last_stack[(n2 - 1) as usize].clone();
                        }
                        let new_id: String = format!("doc/list:{}", p.borrow().id);
                        let size: i64 = DocxA11y::itemsAtLevel(doc, bi, p.borrow().listLevel.clone());
                        let mut lst: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                            &t,
                            new_id.clone(),
                            parent.clone(),
                            EVGA11yRole::list()
                        );
                        lst.borrow_mut().name = "List".to_string();
                        lst.borrow_mut().setSize = size;
                        lvl_stack.push(p.borrow().listLevel);
                        id_stack.push(new_id.clone());
                        size_stack.push(size);
                        seen_stack.push(0);
                        last_stack.push(String::new().to_string());
                    }
                    let at: i64 = (lvl_stack.len() as i64) - 1;
                    let seen: i64 = seen_stack[at as usize] + 1;
                    seen_stack[at as usize] = seen;
                    let mut item: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                        &t,
                        pid.clone(),
                        id_stack[at as usize].clone(),
                        EVGA11yRole::listItem()
                    );
                    item.borrow_mut().name = DocxA11y::paragraphName(&p);
                    item.borrow_mut().posInSet = seen;
                    item.borrow_mut().setSize = size_stack[at as usize];
                    if !p.borrow().listText.is_empty() {
                        item.borrow_mut().description = p.borrow().listText.clone();
                    }
                    last_stack[at as usize] = pid.clone();
                    DocxA11y::addImages(&t, &p, &pid);
                } else {
                    DocxA11y::closeLists(
                        &mut lvl_stack,
                        &mut id_stack,
                        &mut size_stack,
                        &mut seen_stack,
                        &mut last_stack
                    );
                    let lvl: i64 = DocxA11y::headingLevel(&p.borrow().styleId.clone());
                    if lvl > 0 {
                        let mut h: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                            &t,
                            pid.clone(),
                            "doc/body".to_string(),
                            EVGA11yRole::heading()
                        );
                        h.borrow_mut().name = DocxA11y::paragraphName(&p);
                        // The level is what every navigate-by-heading command
                        // reads, and there is no dedicated field for it, so it
                        // rides in the one place a reader will speak: "Heading
                        // level 2".
                        h.borrow_mut().description = format!("Heading level {}", lvl);
                        h.borrow_mut().posInSet = lvl;
                    } else {
                        let mut n: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                            &t,
                            pid.clone(),
                            "doc/body".to_string(),
                            EVGA11yRole::text()
                        );
                        n.borrow_mut().name = DocxA11y::paragraphName(&p);
                    }
                    DocxA11y::addImages(&t, &p, &pid);
                }
            } else {
                // Anything that is not a paragraph ends every open list.
                DocxA11y::closeLists(
                    &mut lvl_stack,
                    &mut id_stack,
                    &mut size_stack,
                    &mut seen_stack,
                    &mut last_stack
                );
                if b.borrow().kind == DocBlockKind::table() {
                    DocxA11y::addTable(&t, &b.borrow().table.clone());
                }
                if b.borrow().kind == DocBlockKind::chart() {
                    DocxA11y::addChart(&t, &b.borrow().chart.clone());
                }
                if b.borrow().kind == DocBlockKind::drawing() {
                    DocxA11y::addDrawing(&t, &b.borrow().drawing.clone());
                }
            }
            bi += 1;
        }
        DocxA11y::addChrome(&t, doc);
        // --- where the caret is
        if caret_paragraph_id > 0 {
            let cid: String = format!("doc/para:{}", caret_paragraph_id);
            let mut cn: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::find(&t, &cid);
            if !cn.borrow().id.is_empty() {
                cn.borrow_mut().focusable = true;
                // A viewer that is not in edit mode still has a caret to
                // report; what it does not have is a place to type. Saying so
                // is the difference between "read this" and "type here".
                //
                // "true" or NOTHING, never "false": `readOnly` on the node is
                // `aria-readonly` in the DOM's own three states, and absent
                // and "false" are different claims — one says the question was
                // never asked, the other that it was asked and answered. An
                // editable document says nothing rather than denying it.
                if read_only {
                    cn.borrow_mut().readOnly = "true".to_string();
                }
                cn.borrow_mut().actSetValue = !read_only;
                cn.borrow_mut().value = cn.borrow().name.clone();
                cn.borrow_mut().caret = caret_offset;
                cn.borrow_mut().selStart = sel_start;
                cn.borrow_mut().selEnd = sel_end;
                EVGA11yTree::setFocus(&t, cid.clone());
            }
        }
        t
    }
    /// How many items a list has at `level`, counting from block `from`.
    ///
    /// Deeper items are stepped over rather than counted — they belong to a
    /// sub-list — and anything shallower, or not a list at all, ends it. A
    /// reader announces "1 of 6" from this and nothing else, so a number that is
    /// merely close is worse than no number: the position is the only sense of
    /// place a list gives.
    fn itemsAtLevel(doc: &Rc<RefCell<RichDocument>>, from: i64, level: i64) -> i64 {
        let mut n: i64 = 0;
        let mut i: i64 = from;
        while i < RichDocument::blockCount(&doc) {
            let mut b: Rc<RefCell<DocBlock>> = RichDocument::blockAt(&doc, i);
            if b.borrow().kind != DocBlockKind::para() {
                return n;
            }
            let mut p: Rc<RefCell<Paragraph>> = b.borrow().paragraph.clone().clone();
            if p.borrow().listKind == 0 {
                return n;
            }
            if p.borrow().listLevel < level {
                return n;
            }
            if p.borrow().listLevel == level {
                n += 1;
            }
            i += 1;
        }
        n
    }
    pub fn closeLists(mut lvl_stack: &mut Vec<i64>, mut id_stack: &mut Vec<String>, mut size_stack: &mut Vec<i64>, mut seen_stack: &mut Vec<i64>, mut last_stack: &mut Vec<String>) {
        while !lvl_stack.is_empty() {
            lvl_stack.pop();
            id_stack.pop();
            size_stack.pop();
            seen_stack.pop();
            last_stack.pop();
        }
    }
    fn addImages(t: &Rc<RefCell<EVGA11yTree>>, p: &Rc<RefCell<Paragraph>>, parent_id: &str) {
        let mut i: i64 = 0;
        while i < (p.borrow().images.len() as i64) {
            let mut img: Rc<RefCell<DocImage>> = p.borrow().images[i as usize].clone();
            let mut n: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                &t,
                format!("{}/img:{}", parent_id, i),
                parent_id.to_string(),
                EVGA11yRole::image()
            );
            n.borrow_mut().name = DocImage::accessibleName(&img);
            if !DocImage::hasAltText(&img) {
                // Said out loud rather than hidden: a reader's user can then
                // tell "the author wrote nothing" from "the viewer lost it".
                n.borrow_mut().description = "No alt text".to_string();
            }
            let mut _tmp_1044 = img.borrow().widthPx.clone();
            EVGA11yTree::place(&t, &n, 0, 0, _tmp_1044, img.borrow().heightPx.clone());
            i += 1;
        }
    }
    /// A table is a grid, and its first row is its header row unless the document
    /// says otherwise. Word marks a repeating header with `tblHeader`, which this
    /// reader does not carry yet — so the first row is announced as data, which
    /// is wrong less often than announcing every table's first row as headers
    /// would be.
    fn addTable(t: &Rc<RefCell<EVGA11yTree>>, tbl: &Rc<RefCell<DocumentTable>>) {
        let tid: String = format!("doc/table:{}", tbl.borrow().id);
        let mut g: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
            &t,
            tid.clone(),
            "doc/body".to_string(),
            EVGA11yRole::grid()
        );
        let rows: i64 = tbl.borrow().rows.len() as i64;
        let cols: i64 = DocxA11y::widestRow(tbl);
        g.borrow_mut().name = format!("Table, {} rows by {} columns", rows, cols.to_string());
        g.borrow_mut().rowCount = rows;
        g.borrow_mut().colCount = cols;
        let mut r: i64 = 0;
        while r < rows {
            let mut row: Rc<RefCell<DocTableRow>> = tbl.borrow().rows[r as usize].clone();
            let rid: String = format!("{}/row:{}", tid, r);
            let mut rn: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                &t,
                rid.clone(),
                tid.clone(),
                EVGA11yRole::row()
            );
            rn.borrow_mut().name = format!("Row {}", r + 1);
            rn.borrow_mut().rowIndex = r;
            rn.borrow_mut().colCount = cols;
            let mut c: i64 = 0;
            while c < (row.borrow().cells.len() as i64) {
                let mut cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[c as usize].clone();
                let mut cn: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                    &t,
                    format!("{}/cell:{}", rid, c),
                    rid.clone(),
                    EVGA11yRole::cell()
                );
                cn.borrow_mut().name = DocxA11y::cellText(&cell);
                cn.borrow_mut().rowIndex = r;
                cn.borrow_mut().colIndex = c;
                if cell.borrow().gridSpan > 1 {
                    cn.borrow_mut().colCount = cell.borrow().gridSpan;
                }
                c += 1;
            }
            r += 1;
        }
    }
    /// The widest row decides the column count. A table whose rows disagree is
    /// ordinary in Word — merged cells do it — and claiming fewer columns than
    /// some row has would make a reader announce positions past the end.
    fn widestRow(tbl: &Rc<RefCell<DocumentTable>>) -> i64 {
        let mut w: i64 = 0;
        let mut r: i64 = 0;
        while r < (tbl.borrow().rows.len() as i64) {
            let mut row: Rc<RefCell<DocTableRow>> = tbl.borrow().rows[r as usize].clone();
            let mut n: i64 = 0;
            let mut c: i64 = 0;
            while c < (row.borrow().cells.len() as i64) {
                let mut cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[c as usize].clone();
                n += cell.borrow().gridSpan;
                c += 1;
            }
            if n > w {
                w = n;
            }
            r += 1;
        }
        w
    }
    /// A cell's paragraphs, joined. A reader says a cell as one utterance, and
    /// the paragraph breaks inside it are layout rather than meaning.
    fn cellText(cell: &Rc<RefCell<DocTableCell>>) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        while i < (cell.borrow().paragraphs.len() as i64) {
            let mut p: Rc<RefCell<Paragraph>> = cell.borrow().paragraphs[i as usize].clone();
            if !p.borrow().text.is_empty() {
                if !out.is_empty() {
                    out = format!("{} ", out);
                }
                out = format!("{}{}", out, p.borrow().text);
            }
            i += 1;
        }
        if out.is_empty() {
            return "Empty".to_string();
        }
        out
    }
    /// A chart is a picture to a reader, and the honest name for it is what the
    /// chart itself says it is. `DocChart.note` carries the reason when it could
    /// not be drawn, which is exactly what a reader should hear instead.
    fn addChart(t: &Rc<RefCell<EVGA11yTree>>, ch: &Rc<RefCell<DocChart>>) {
        let mut n: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
            &t,
            format!("doc/chart:{}", ch.borrow().id),
            "doc/body".to_string(),
            EVGA11yRole::image()
        );
        n.borrow_mut().name = "Chart".to_string();
        if !ch.borrow().note.is_empty() {
            n.borrow_mut().description = ch.borrow().note.clone();
        }
        let mut _tmp_1049 = ch.borrow().widthPx.clone();
        EVGA11yTree::place(&t, &n, 0, 0, _tmp_1049, ch.borrow().heightPx.clone());
    }
    /// A drawing is a picture to a reader too. Its name says what it is —
    /// "Mermaid diagram" — and its alt text, when the maker wrote one, says
    /// what is in it; the note is why nothing could be drawn, when nothing
    /// could.
    fn addDrawing(t: &Rc<RefCell<EVGA11yTree>>, dr: &Rc<RefCell<DocDrawing>>) {
        let mut n: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
            &t,
            format!("doc/drawing:{}", dr.borrow().id),
            "doc/body".to_string(),
            EVGA11yRole::image()
        );
        n.borrow_mut().name = dr.borrow().name.clone();
        if !dr.borrow().altText.is_empty() {
            n.borrow_mut().description = dr.borrow().altText.clone();
        }
        if !dr.borrow().note.is_empty() {
            n.borrow_mut().description = dr.borrow().note.clone();
        }
        let mut _tmp_1054 = dr.borrow().widthPx.clone();
        EVGA11yTree::place(&t, &n, 0, 0, _tmp_1054, dr.borrow().heightPx.clone());
    }
    /// Headers and footers, outside the body. They repeat on every page; met in
    /// the reading flow they would be read between every two paragraphs.
    fn addChrome(t: &Rc<RefCell<EVGA11yTree>>, doc: &Rc<RefCell<RichDocument>>) {
        let mut si: i64 = 0;
        while si < RichDocument::sectionCount(&doc) {
            let mut sec: Rc<RefCell<DocSection>> = RichDocument::sectionAt(&doc, si);
            if !sec.borrow().headerParas.is_empty() {
                let hid: String = format!("doc/header:{}", si);
                let mut _hg: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::group(
                    &t,
                    &hid,
                    "doc",
                    "Page header".to_string()
                );
                DocxA11y::addChromeParas(t, &sec.borrow().headerParas.clone(), &hid);
            }
            if !sec.borrow().footerParas.is_empty() {
                let fid: String = format!("doc/footer:{}", si);
                let mut _fg: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::group(
                    &t,
                    &fid,
                    "doc",
                    "Page footer".to_string()
                );
                DocxA11y::addChromeParas(t, &sec.borrow().footerParas.clone(), &fid);
            }
            si += 1;
        }
    }
    fn addChromeParas(t: &Rc<RefCell<EVGA11yTree>>, paras: &Vec<Rc<RefCell<Paragraph>>>, parent_id: &str) {
        let mut i: i64 = 0;
        while i < (paras.len() as i64) {
            let mut p: Rc<RefCell<Paragraph>> = paras[i as usize].clone();
            if !p.borrow().text.is_empty() {
                let mut n: Rc<RefCell<EVGA11yNode>> = EVGA11yTree::node(
                    &t,
                    format!("{}/para:{}", parent_id, i),
                    parent_id.to_string(),
                    EVGA11yRole::text()
                );
                n.borrow_mut().name = p.borrow().text.clone();
            }
            i += 1;
        }
    }
}
