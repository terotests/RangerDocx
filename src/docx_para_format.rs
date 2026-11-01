// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::DxNative;
use crate::rgr::Paragraph;
use crate::rgr::TextSpan;
use crate::word_style_resolver::ResolvedParagraphStyle;
use crate::word_style_resolver::ResolvedRunStyle;
use crate::word_style_resolver::WordStyleResolver;

/// ==============================================================================
/// DocxParaFormat.rgr — paragraph-level formatting commands, undoable.
/// ==============================================================================
///
/// Alignment and styles change a paragraph's properties and (for a style) the
/// look of every run in it. The undo history records text edits as small
/// diffs; a property change is recorded as a SNAPSHOT of the paragraph's
/// formatting before and after, so any later paragraph command (spacing,
/// indents, lists) gets undo for nothing by going through `apply`.
///
/// The snapshot is the paragraph's properties and its spans, written as text:
/// it rides in a `DocEditOp`'s two strings, which is all an op can carry.
/// ==============================================================================
#[derive(Clone)]
pub struct DocxParaFormat {
}

impl DocxParaFormat {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The undo kind, outside the range the shared text editor uses.
    pub fn opKind() -> i64 {
        40
    }
    /// A change of case: same length, so the spans stay where they are.
    pub fn caseKind() -> i64 {
        41
    }
    /// Every word's first letter upper case, the rest lower.
    pub fn titleCase(s: &str) -> String {
        let lower: String = s.to_lowercase();
        let upper: String = s.to_uppercase();
        if (DxNative::str_len(&lower)) != (DxNative::str_len(&s)) {
            return s.to_string();
        }
        if (DxNative::str_len(&upper)) != (DxNative::str_len(&s)) {
            return s.to_string();
        }
        let mut out: String = String::new();
        let mut start: bool = true;
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&s);
        while i < n {
            let c: i64 = DxNative::char_at(&s, i);
            let space: bool = ((c == 32 || c == 9) || c == 10) || c == 45;
            if space {
                out = format!("{}{}", out, DxNative::substr(&s, i, i + 1));
                start = true;
            } else {
                if start {
                    out = format!("{}{}", out, DxNative::substr(&upper, i, i + 1));
                } else {
                    out = format!("{}{}", out, DxNative::substr(&lower, i, i + 1));
                }
                start = false;
            }
            i += 1;
        }
        out
    }
    /// The first letter of each sentence upper case, the rest lower.
    pub fn sentenceCase(s: &str) -> String {
        let lower: String = s.to_lowercase();
        let upper: String = s.to_uppercase();
        if (DxNative::str_len(&lower)) != (DxNative::str_len(&s)) || (DxNative::str_len(&upper)) != (DxNative::str_len(&s)) {
            return s.to_string();
        }
        let mut out: String = String::new();
        let mut start: bool = true;
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&s);
        while i < n {
            let c: i64 = DxNative::char_at(&s, i);
            let lc: String = DxNative::substr(&lower, i, i + 1);
            let uc: String = DxNative::substr(&upper, i, i + 1);
            if lc != uc {
                if start {
                    out = format!("{}{}", out, uc);
                } else {
                    out = format!("{}{}", out, lc);
                }
                start = false;
            } else {
                out = format!("{}{}", out, DxNative::substr(&s, i, i + 1));
                if (c == 46 || c == 33) || c == 63 {
                    start = true;
                }
            }
            i += 1;
        }
        out
    }
    /// Every letter the other way round.
    pub fn toggleCase(s: &str) -> String {
        let lower: String = s.to_lowercase();
        let upper: String = s.to_uppercase();
        if (DxNative::str_len(&lower)) != (DxNative::str_len(&s)) || (DxNative::str_len(&upper)) != (DxNative::str_len(&s)) {
            return s.to_string();
        }
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&s);
        while i < n {
            let ch: String = DxNative::substr(&s, i, i + 1);
            let uc: String = DxNative::substr(&upper, i, i + 1);
            if ch == uc {
                out = format!("{}{}", out, DxNative::substr(&lower, i, i + 1));
            } else {
                out = format!("{}{}", out, uc);
            }
            i += 1;
        }
        out
    }
    pub fn b(v: bool) -> String {
        if v {
            return "1".to_string();
        }
        "0".to_string()
    }
    pub fn num(s: &str) -> f64 {
        let d: Option<f64> = s.parse::<f64>().ok();
        if d.is_some() {
            return d.unwrap();
        }
        0.0
    }
    pub fn int(s: &str) -> i64 {
        (DocxParaFormat::num(s)).floor() as i64
    }
    /// The paragraph's formatting, as one string. Text is not in it: a format
    /// change never changes the text, and undoing one must not either.
    fn snapshot(p: &Rc<RefCell<Paragraph>>) -> String {
        let mut s: String = format!("{}\t{}\t{}", p.borrow().styleId, p.borrow().align, p.borrow().bidi);
        s = format!("{}\t{}\t{}\t{}", s, p.borrow().listKind, p.borrow().listLevel, p.borrow().listNumId);
        s = format!("{}\t{}\t{}", s, p.borrow().listFmt, p.borrow().listText);
        s = format!("{}\t{}\t{}\t{}", s, p.borrow().spaceBeforePt, p.borrow().spaceAfterPt, p.borrow().lineSpacing);
        s = format!("{}\t{}\t{}\t{}", s, p.borrow().indentLeftPt, p.borrow().indentRightPt, p.borrow().indentFirstLinePt);
        s = format!("{}\t{}", s, DocxParaFormat::b(p.borrow().pageBreakBefore));
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            s = format!("{}\n{}\t{}", s, sp.borrow().start, sp.borrow().end);
            s = format!("{}\t{}\t{}\t{}", s, DocxParaFormat::b(sp.borrow().bold), DocxParaFormat::b(sp.borrow().italic), DocxParaFormat::b(sp.borrow().underline));
            s = format!("{}\t{}\t{}\t{}\t{}", s, sp.borrow().fontSizePt, sp.borrow().fontFamily, sp.borrow().colorHex, sp.borrow().hyperlink);
            s = format!("{}\t{}\t{}\t{}", s, DocxParaFormat::b(sp.borrow().strike), sp.borrow().vertAlign, sp.borrow().highlight);
            i += 1;
        }
        s
    }
    fn restore(p: &Rc<RefCell<Paragraph>>, snap: &str) {
        let mut lines: Vec<String> = snap.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        if lines.is_empty() {
            return;
        }
        let mut f: Vec<String> = lines[0].clone()
            .split("\t")
            .map(|s| s.to_string())
            .collect::<Vec<String>>();
        if (f.len() as i64) < 15 {
            return;
        }
        p.borrow_mut().styleId = f[0].clone();
        p.borrow_mut().align = DocxParaFormat::int(&f[1].clone());
        p.borrow_mut().bidi = DocxParaFormat::int(&f[2].clone());
        p.borrow_mut().listKind = DocxParaFormat::int(&f[3].clone());
        p.borrow_mut().listLevel = DocxParaFormat::int(&f[4].clone());
        p.borrow_mut().listNumId = DocxParaFormat::int(&f[5].clone());
        p.borrow_mut().listFmt = f[6].clone();
        p.borrow_mut().listText = f[7].clone();
        p.borrow_mut().spaceBeforePt = DocxParaFormat::num(&f[8].clone());
        p.borrow_mut().spaceAfterPt = DocxParaFormat::num(&f[9].clone());
        p.borrow_mut().lineSpacing = DocxParaFormat::num(&f[10].clone());
        p.borrow_mut().indentLeftPt = DocxParaFormat::num(&f[11].clone());
        p.borrow_mut().indentRightPt = DocxParaFormat::num(&f[12].clone());
        p.borrow_mut().indentFirstLinePt = DocxParaFormat::num(&f[13].clone());
        p.borrow_mut().pageBreakBefore = f[14].clone() == "1";
        p.borrow_mut().spans.clear();
        let mut i: i64 = 1;
        while i < (lines.len() as i64) {
            let mut g: Vec<String> = lines[i as usize].clone()
                .split("\t")
                .map(|s| s.to_string())
                .collect::<Vec<String>>();
            if (g.len() as i64) >= 9 {
                let mut sp: Rc<RefCell<TextSpan>> = TextSpan::create(DocxParaFormat::int(&g[0].clone()), DocxParaFormat::int(&g[1].clone()));
                sp.borrow_mut().bold = g[2].clone() == "1";
                sp.borrow_mut().italic = g[3].clone() == "1";
                sp.borrow_mut().underline = g[4].clone() == "1";
                sp.borrow_mut().fontSizePt = DocxParaFormat::num(&g[5].clone());
                sp.borrow_mut().fontFamily = g[6].clone();
                sp.borrow_mut().colorHex = g[7].clone();
                sp.borrow_mut().hyperlink = g[8].clone();
                if (g.len() as i64) >= 12 {
                    sp.borrow_mut().strike = g[9].clone() == "1";
                    sp.borrow_mut().vertAlign = DocxParaFormat::int(&g[10].clone());
                    sp.borrow_mut().highlight = g[11].clone();
                }
                p.borrow_mut().spans.push(sp.clone());
            }
            i += 1;
        }
    }
    /// --- the commands -----------------------------------------------------------
    fn setAlign(p: &Rc<RefCell<Paragraph>>, align: i64) {
        p.borrow_mut().align = align;
    }
    /// Give the paragraph a style. Properties the paragraph had FROM its old
    /// style take the new style's values; ones set on the paragraph or run
    /// directly (a bold word, a red word) are kept — which is what Word does,
    /// and what makes "Heading 1" not wipe out the italic in a heading.
    fn applyStyle(p: &Rc<RefCell<Paragraph>>, style_id: String, resolver: &Rc<RefCell<WordStyleResolver>>) {
        let mut was: Rc<RefCell<ResolvedParagraphStyle>> = resolver.borrow().resolveParagraphStyle(p.borrow().styleId.clone());
        let mut now: Rc<RefCell<ResolvedParagraphStyle>> = resolver.borrow().resolveParagraphStyle(style_id.clone());
        p.borrow_mut().styleId = style_id.clone();
        if p.borrow().align == was.borrow().align {
            p.borrow_mut().align = now.borrow().align;
        }
        if p.borrow().spaceBeforePt == was.borrow().spaceBeforePt {
            p.borrow_mut().spaceBeforePt = now.borrow().spaceBeforePt;
        }
        if p.borrow().spaceAfterPt == was.borrow().spaceAfterPt {
            p.borrow_mut().spaceAfterPt = now.borrow().spaceAfterPt;
        }
        if p.borrow().lineSpacing == was.borrow().lineSpacing {
            p.borrow_mut().lineSpacing = now.borrow().lineSpacing;
        }
        if p.borrow().indentLeftPt == was.borrow().indentLeftPt {
            p.borrow_mut().indentLeftPt = now.borrow().indentLeftPt;
        }
        if p.borrow().indentRightPt == was.borrow().indentRightPt {
            p.borrow_mut().indentRightPt = now.borrow().indentRightPt;
        }
        if p.borrow().indentFirstLinePt == was.borrow().indentFirstLinePt {
            p.borrow_mut().indentFirstLinePt = now.borrow().indentFirstLinePt;
        }
        let mut o: Rc<RefCell<ResolvedRunStyle>> = was.borrow().run.clone().clone();
        let mut n: Rc<RefCell<ResolvedRunStyle>> = now.borrow().run.clone().clone();
        let mut i: i64 = 0;
        while i < (p.borrow().spans.len() as i64) {
            let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if sp.borrow().bold == o.borrow().bold {
                sp.borrow_mut().bold = n.borrow().bold;
            }
            if sp.borrow().italic == o.borrow().italic {
                sp.borrow_mut().italic = n.borrow().italic;
            }
            if sp.borrow().underline == o.borrow().underline {
                sp.borrow_mut().underline = n.borrow().underline;
            }
            if sp.borrow().fontSizePt == o.borrow().fontSizePt {
                sp.borrow_mut().fontSizePt = n.borrow().fontSizePt;
            }
            if sp.borrow().fontFamily == o.borrow().fontFamily {
                sp.borrow_mut().fontFamily = n.borrow().fontFamily.clone();
            }
            if sp.borrow().colorHex == o.borrow().colorHex {
                sp.borrow_mut().colorHex = n.borrow().colorHex.clone();
            }
            i += 1;
        }
    }
    /// Out of its list: back to the indent its style gives it.
    fn leaveList(p: &Rc<RefCell<Paragraph>>, resolver: &Rc<RefCell<WordStyleResolver>>) {
        let mut ps: Rc<RefCell<ResolvedParagraphStyle>> = resolver.borrow().resolveParagraphStyle(p.borrow().styleId.clone());
        p.borrow_mut().listNumId = 0;
        p.borrow_mut().listKind = 0;
        p.borrow_mut().listLevel = 0;
        p.borrow_mut().listFmt = String::new();
        p.borrow_mut().listText = String::new();
        p.borrow_mut().indentLeftPt = ps.borrow().indentLeftPt;
        p.borrow_mut().indentFirstLinePt = ps.borrow().indentFirstLinePt;
    }
    /// The next half-inch stop from `left`, in the direction of `step`: what
    /// Increase and Decrease Indent move a paragraph to.
    pub fn nextStop(left: f64, step: i64) -> f64 {
        let mut k: i64 = (left / 36.0 + 0.001).floor() as i64;
        if step > 0 {
            return ((k + 1) as f64) * 36.0;
        }
        let on_stop: bool = left - (k as f64) * 36.0 < 0.01;
        if on_stop {
            k -= 1;
        }
        if k < 0 {
            k = 0;
        }
        (k as f64) * 36.0
    }
    /// Does the document define this style? Only the styles a file has can be
    /// written back into it.
    pub fn hasStyle(resolver: &Rc<RefCell<WordStyleResolver>>, style_id: &str) -> bool {
        return resolver.borrow().styles.contains_key(style_id);
    }
}
