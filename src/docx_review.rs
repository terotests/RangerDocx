// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rich_document::{DocComment, InlineAnchor, Paragraph, RichDocument, TextSpan};
use crate::rich_document_edit::RichDocumentEdit;
use crate::rgr::DxNative;

/// ==============================================================================
/// DocxReview - tracked changes and comments on the document model.
/// ==============================================================================
///
/// A tracked change is a span marked inserted or deleted (`TextSpan.rev`); the
/// deleted text stays in the paragraph until the change is accepted (then it
/// goes) or rejected (then it is ordinary text again). A comment is an entry in
/// `RichDocument.comments` and three inline anchors in the text: where its
/// range starts, where it ends, and the reference run after the end.
///
/// Undo is a snapshot of the paragraphs touched (text, spans, anchor offsets)
/// and of the comment list, before and after.
/// ==============================================================================
pub struct DocxParaContent {
    pub para: Rc<RefCell<Paragraph>>,
    pub text: String,
    pub spans: Vec<Rc<RefCell<TextSpan>>>,
    pub anchors: Vec<Rc<RefCell<InlineAnchor>>>,
    pub offsets: Vec<i64>,
}

impl DocxParaContent {
    pub fn new() -> Self {
        Self {
            para: Rc::new(RefCell::new(Paragraph::new())),
            text: String::new(),
            spans: Vec::new(),
            anchors: Vec::new(),
            offsets: Vec::new(),
        }
    }
}

pub struct DocxReviewSnap {
    pub paras: Vec<Rc<RefCell<DocxParaContent>>>,
    pub comments: Vec<Rc<RefCell<DocComment>>>,
    pub commentsChanged: bool,
    pub caretPid: i64,
    pub caretOff: i64,
}

impl DocxReviewSnap {
    pub fn new() -> Self {
        Self {
            paras: Vec::new(),
            comments: Vec::new(),
            commentsChanged: false,
            caretPid: 0,
            caretOff: 0,
        }
    }
}

pub struct DocxReview {
}

impl DocxReview {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The undo kind, after DocxTableEdit's 44.
    pub fn opKind() -> i64 {
        45
    }
    // --- snapshots ---------------------------------------------------------------
    pub fn capture(doc: &Rc<RefCell<RichDocument>>, paras: &Vec<Rc<RefCell<Paragraph>>>, pid: i64, off: i64) -> Rc<RefCell<DocxReviewSnap>> {
        let mut snap: Rc<RefCell<DocxReviewSnap>> = Rc::new(RefCell::new(DocxReviewSnap::new()));
        snap.borrow_mut().caretPid = pid;
        snap.borrow_mut().caretOff = off;
        snap.borrow_mut().commentsChanged = doc.borrow().commentsChanged;
        let mut ci: i64 = 0;
        while ci < (doc.borrow().comments.len() as i64) {
            let c: Rc<RefCell<DocComment>> = doc.borrow().comments[ci as usize].clone();
            snap.borrow_mut().comments.push(c.clone());
            ci += 1;
        }
        let mut i: i64 = 0;
        while i < (paras.len() as i64) {
            let p: Rc<RefCell<Paragraph>> = paras[i as usize].clone();
            let mut pc: Rc<RefCell<DocxParaContent>> = Rc::new(RefCell::new(DocxParaContent::new()));
            pc.borrow_mut().para = p.clone();
            pc.borrow_mut().text = p.borrow().text.clone();
            let mut si: i64 = 0;
            while si < (p.borrow().spans.len() as i64) {
                let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[si as usize].clone();
                pc.borrow_mut().spans.push(sp.borrow().copy());
                si += 1;
            }
            let mut ai: i64 = 0;
            while ai < (p.borrow().anchors.len() as i64) {
                let an: Rc<RefCell<InlineAnchor>> = p.borrow().anchors[ai as usize].clone();
                pc.borrow_mut().anchors.push(an.clone());
                pc.borrow_mut().offsets.push(an.borrow().offset);
                ai += 1;
            }
            snap.borrow_mut().paras.push(pc.clone());
            i += 1;
        }
        snap
    }
    pub fn restore(doc: &Rc<RefCell<RichDocument>>, snap: &Rc<RefCell<DocxReviewSnap>>) {
        doc.borrow_mut().comments.clear();
        let mut ci: i64 = 0;
        while ci < (snap.borrow().comments.len() as i64) {
            let c: Rc<RefCell<DocComment>> = snap.borrow().comments[ci as usize].clone();
            doc.borrow_mut().comments.push(c.clone());
            ci += 1;
        }
        doc.borrow_mut().commentsChanged = snap.borrow().commentsChanged;
        let mut i: i64 = 0;
        while i < (snap.borrow().paras.len() as i64) {
            let pc: Rc<RefCell<DocxParaContent>> = snap.borrow().paras[i as usize].clone();
            let p: Rc<RefCell<Paragraph>> = pc.borrow().para.clone();
            p.borrow_mut().text = pc.borrow().text.clone();
            p.borrow_mut().spans.clear();
            let mut si: i64 = 0;
            while si < (pc.borrow().spans.len() as i64) {
                let sp: Rc<RefCell<TextSpan>> = pc.borrow().spans[si as usize].clone();
                p.borrow_mut().spans.push(sp.borrow().copy());
                si += 1;
            }
            p.borrow_mut().anchors.clear();
            let mut ai: i64 = 0;
            while ai < (pc.borrow().anchors.len() as i64) {
                let an: Rc<RefCell<InlineAnchor>> = pc.borrow().anchors[ai as usize].clone();
                an.borrow_mut().offset = pc.borrow().offsets[ai as usize];
                p.borrow_mut().anchors.push(an.clone());
                ai += 1;
            }
            i += 1;
        }
        RichDocument::touch(doc);
    }
    // --- tracked changes ---------------------------------------------------------
    pub fn hasChanges(p: &Rc<RefCell<Paragraph>>) -> bool {
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if sp.borrow().rev != 0 && sp.borrow().end > sp.borrow().start {
                return true;
            }
            i += 1;
        }
        false
    }
    fn sameChange(a: &Rc<RefCell<TextSpan>>, b: &Rc<RefCell<TextSpan>>) -> bool {
        a.borrow().rev == b.borrow().rev && a.borrow().revAuthor == b.borrow().revAuthor && a.borrow().revId == b.borrow().revId
    }
    /// The change at offset `off`: [start, end] of the run of spans with the
    /// same change around it, or [-1, -1].
    pub fn changeAt(p: &Rc<RefCell<Paragraph>>, off: i64) -> Vec<i64> {
        let n: i64 = p.borrow().spans.len() as i64;
        let mut hit: i64 = -1;
        let mut i: i64 = 0;
        while i < n {
            let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if sp.borrow().rev != 0 && sp.borrow().end > sp.borrow().start {
                if off >= sp.borrow().start && off < sp.borrow().end {
                    hit = i;
                }
                if hit < 0 && off == sp.borrow().end {
                    hit = i;
                }
            }
            i += 1;
        }
        if hit < 0 {
            return vec![-1, -1];
        }
        let base: Rc<RefCell<TextSpan>> = p.borrow().spans[hit as usize].clone();
        let mut lo: i64 = hit;
        while lo > 0 && DocxReview::sameChange(&p.borrow().spans[(lo - 1) as usize].clone(), &base) {
            lo -= 1;
        }
        let mut hi: i64 = hit;
        while hi + 1 < n && DocxReview::sameChange(&p.borrow().spans[(hi + 1) as usize].clone(), &base) {
            hi += 1;
        }
        let a: i64 = p.borrow().spans[lo as usize].borrow().start;
        let b: i64 = p.borrow().spans[hi as usize].borrow().end;
        vec![a, b]
    }
    /// Where each change in the paragraph starts and ends: [s, e, s, e, ...].
    pub fn changes(p: &Rc<RefCell<Paragraph>>) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        let n: i64 = p.borrow().spans.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if sp.borrow().rev != 0 && sp.borrow().end > sp.borrow().start {
                let r: Vec<i64> = DocxReview::changeAt(p, sp.borrow().start);
                out.push(r[0]);
                out.push(r[1]);
                // past the whole change
                let mut j: i64 = i;
                while j < n && p.borrow().spans[j as usize].borrow().end <= r[1] {
                    j += 1;
                }
                i = j;
            } else {
                i += 1;
            }
        }
        out
    }
    /// Split the span covering `off` so a span starts there.
    fn splitAt(p: &Rc<RefCell<Paragraph>>, off: i64) {
        let n: i64 = p.borrow().spans.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if off > sp.borrow().start && off < sp.borrow().end {
                let right: Rc<RefCell<TextSpan>> = sp.borrow().copy();
                right.borrow_mut().start = off;
                sp.borrow_mut().end = off;
                p.borrow_mut().spans.insert((i + 1) as usize, right.clone());
                return;
            }
            i += 1;
        }
    }
    /// Accept (or reject) the changes in [a, b) of paragraph `pid`. Accepting
    /// keeps an insertion and removes a deletion; rejecting does the opposite.
    /// Answers whether anything changed.
    pub fn resolve(ad: &Rc<RefCell<RichDocumentEdit>>, p: &Rc<RefCell<Paragraph>>, a: i64, b: i64, accept: bool) -> bool {
        DocxReview::splitAt(p, a);
        DocxReview::splitAt(p, b);
        let mut starts: Vec<i64> = Vec::new();
        let mut ends: Vec<i64> = Vec::new();
        let mut kinds: Vec<i64> = Vec::new();
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if sp.borrow().rev != 0 && sp.borrow().start >= a && sp.borrow().end <= b && sp.borrow().end > sp.borrow().start {
                starts.push(sp.borrow().start);
                ends.push(sp.borrow().end);
                kinds.push(sp.borrow().rev);
            }
            i += 1;
        }
        if starts.is_empty() {
            return false;
        }
        let pid: i64 = p.borrow().id;
        let mut k: i64 = (starts.len() as i64) - 1;
        while k >= 0 {
            let rv: i64 = kinds[k as usize];
            let drop: bool = (accept && rv == 2) || (!accept && rv == 1);
            if drop {
                ad.borrow().replaceRange(pid, starts[k as usize], ends[k as usize], "");
            } else {
                let mut si: i64 = 0;
                while si < (p.borrow().spans.len() as i64) {
                    let sp: Rc<RefCell<TextSpan>> = p.borrow().spans[si as usize].clone();
                    if sp.borrow().start >= starts[k as usize] && sp.borrow().end <= ends[k as usize] {
                        sp.borrow_mut().rev = 0;
                        sp.borrow_mut().revAuthor = String::new();
                        sp.borrow_mut().revDate = String::new();
                        sp.borrow_mut().revId = 0;
                    }
                    si += 1;
                }
            }
            k -= 1;
        }
        // A paragraph emptied by it keeps one span; that span is no change.
        if p.borrow().text.is_empty() {
            let mut si2: i64 = 0;
            while si2 < (p.borrow().spans.len() as i64) {
                let sp2: Rc<RefCell<TextSpan>> = p.borrow().spans[si2 as usize].clone();
                sp2.borrow_mut().rev = 0;
                sp2.borrow_mut().revAuthor = String::new();
                si2 += 1;
            }
        }
        RichDocument::touch(&ad.borrow().doc.clone());
        true
    }
    // --- comments ----------------------------------------------------------------
    /// The comment id an anchor belongs to ("" when it is no comment's).
    pub fn commentIdOf(xml: &str) -> String {
        let mut at: i64 = DxNative::index_of(xml, "commentRangeStart");
        if at < 0 {
            at = DxNative::index_of(xml, "commentRangeEnd");
        }
        if at < 0 {
            at = DxNative::index_of(xml, "commentReference");
        }
        if at < 0 {
            return String::new();
        }
        let q: i64 = DxNative::index_of_from(xml, "w:id=\"", at);
        if q < 0 {
            return String::new();
        }
        let e: i64 = DxNative::index_of_from(xml, "\"", q + 6);
        if e < 0 {
            return String::new();
        }
        DxNative::substr(xml, q + 6, e)
    }
    pub fn isCommentStart(xml: &str) -> bool {
        DxNative::index_of(xml, "<w:commentRangeStart") == 0
    }
    pub fn isCommentEnd(xml: &str) -> bool {
        DxNative::index_of(xml, "<w:commentRangeEnd") == 0
    }
    /// Remove every anchor of comment `id`. Answers whether any went.
    pub fn removeCommentAnchors(p: &Rc<RefCell<Paragraph>>, id: &str) -> bool {
        let mut keep: Vec<Rc<RefCell<InlineAnchor>>> = Vec::new();
        let mut gone: bool = false;
        let mut i: i64 = 0;
        while i < (p.borrow().anchors.len() as i64) {
            let an: Rc<RefCell<InlineAnchor>> = p.borrow().anchors[i as usize].clone();
            if DocxReview::commentIdOf(&an.borrow().xml) == id {
                gone = true;
            } else {
                keep.push(an.clone());
            }
            i += 1;
        }
        if gone {
            p.borrow_mut().anchors = keep;
        }
        gone
    }
    /// An anchor at `off`, after the ones already there (or before them when
    /// `first`), keeping the list in offset order.
    pub fn insertAnchor(p: &Rc<RefCell<Paragraph>>, off: i64, xml: &str, first: bool) {
        let mut a: Rc<RefCell<InlineAnchor>> = Rc::new(RefCell::new(InlineAnchor::new()));
        a.borrow_mut().offset = off;
        a.borrow_mut().xml = xml.to_string();
        let n: i64 = p.borrow().anchors.len() as i64;
        let mut at: i64 = n;
        let mut i: i64 = 0;
        while i < n {
            let o: i64 = p.borrow().anchors[i as usize].borrow().offset;
            let past: bool = if first { o >= off } else { o > off };
            if past && at == n {
                at = i;
            }
            i += 1;
        }
        p.borrow_mut().anchors.insert(at as usize, a.clone());
    }
    pub fn nextCommentId(doc: &Rc<RefCell<RichDocument>>) -> String {
        let mut best: i64 = -1;
        let mut i: i64 = 0;
        while i < (doc.borrow().comments.len() as i64) {
            let c: Rc<RefCell<DocComment>> = doc.borrow().comments[i as usize].clone();
            let v: Option<i64> = c.borrow().id.parse::<i64>().ok();
            if v.is_some() && v.unwrap() > best {
                best = v.unwrap();
            }
            i += 1;
        }
        (best + 1).to_string()
    }
    pub fn commentById(doc: &Rc<RefCell<RichDocument>>, id: &str) -> i64 {
        let mut i: i64 = 0;
        while i < (doc.borrow().comments.len() as i64) {
            let c: Rc<RefCell<DocComment>> = doc.borrow().comments[i as usize].clone();
            if c.borrow().id == id {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn initialsOf(name: &str) -> String {
        let mut out: String = String::new();
        let mut start: bool = true;
        let n: i64 = DxNative::str_len(name);
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = DxNative::char_at(name, i);
            if c == 32 {
                start = true;
            } else {
                if start {
                    out = format!("{}{}", out, DxNative::substr(name, i, i + 1));
                }
                start = false;
            }
            i += 1;
        }
        out.to_uppercase()
    }
}
