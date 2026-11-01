// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::DocumentTable;
use crate::rgr::Paragraph;
use crate::rgr::RichDocument;
use crate::rgr::TextSpan;

/// ==============================================================================
/// DocxFingerprint - what a paragraph or table looked like when it was read.
/// ==============================================================================
///
/// The parser stamps every paragraph and table it reads from document.xml with
/// these strings; DocxWriter computes them again at save time. Equal means
/// nothing the model holds has changed, so the original XML is written back
/// byte for byte, with everything the model never understood still inside it.
///
/// Everything the writer would regenerate from has to be in here. A field
/// missing from the fingerprint is a change the save silently drops.
/// ==============================================================================
#[derive(Clone)]
pub struct DocxFingerprint {
}

impl DocxFingerprint {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn b(v: bool) -> String {
        if v {
            return "1".to_string();
        }
        "0".to_string()
    }
    /// Paragraph-level properties only: equal means the original w:pPr can be
    /// kept even when the text changed.
    fn paraProps(p: &Rc<RefCell<Paragraph>>) -> String {
        let mut s: String = format!("{}|{}|{}", p.borrow().styleId, p.borrow().align, p.borrow().bidi);
        s = format!("{}|{}|{}|{}", s, p.borrow().listKind, p.borrow().listLevel, p.borrow().listNumId);
        s = format!("{}|{}|{}|{}", s, p.borrow().spaceBeforePt, p.borrow().spaceAfterPt, p.borrow().lineSpacing);
        s = format!("{}|{}|{}|{}", s, p.borrow().indentLeftPt, p.borrow().indentRightPt, p.borrow().indentFirstLinePt);
        s = format!("{}|{}", s, DocxFingerprint::b(p.borrow().pageBreakBefore));
        s
    }
    fn span(sp: &Rc<RefCell<TextSpan>>) -> String {
        let mut s: String = format!("{}:{}", sp.borrow().start.to_string(), sp.borrow().end);
        s = format!("{}:{}{}{}", s, DocxFingerprint::b(sp.borrow().bold), DocxFingerprint::b(sp.borrow().italic), DocxFingerprint::b(sp.borrow().underline));
        s = format!("{}:{}:{}:{}:{}:{}", s, sp.borrow().fontSizePt, sp.borrow().fontFamily, sp.borrow().colorHex, sp.borrow().hyperlink, sp.borrow().math);
        s = format!("{}:{}{}:{}", s, DocxFingerprint::b(sp.borrow().strike), sp.borrow().vertAlign, sp.borrow().highlight);
        s
    }
    fn paragraph(p: &Rc<RefCell<Paragraph>>) -> String {
        let mut s: String = format!("{}#{}#", DocxFingerprint::paraProps(p), p.borrow().text);
        let __it_sp = p.borrow().spans.clone();
        for sp in __it_sp.iter().cloned() {
            s = format!("{}{};", s, DocxFingerprint::span(&sp));
        }
        s = format!("{}#{}", s, p.borrow().images.len() as i64);
        s
    }
    fn table(t: &Rc<RefCell<DocumentTable>>) -> String {
        let mut s: String = String::new();
        let __it_w = t.borrow().colWidthsTwips.clone();
        for w in __it_w.iter().copied() {
            s = format!("{}{},", s, w);
        }
        let __it_row = t.borrow().rows.clone();
        for row in __it_row.iter().cloned() {
            s = format!("{}{}{}", s, "{R".to_string(), row.borrow().heightTwips);
            let __it_cell = row.borrow().cells.clone();
            for cell in __it_cell.iter().cloned() {
                s = format!("{}{}{}:{}:{}", s, "{C".to_string(), cell.borrow().gridSpan, cell.borrow().fillHex, cell.borrow().vAlign);
                let __it_p = cell.borrow().paragraphs.clone();
                for p in __it_p.iter().cloned() {
                    s = format!("{}{}{}{}", s, "{P".to_string(), DocxFingerprint::paragraph(&p), "}".to_string());
                }
                s = format!("{}{}", s, "}".to_string());
            }
            s = format!("{}{}", s, "}".to_string());
        }
        s
    }
    /// Stamp everything that has a source: the state "as read".
    fn stamp(doc: &Rc<RefCell<RichDocument>>) {
        let __it_p = doc.borrow().paragraphs.clone();
        for p in __it_p.iter().cloned() {
            if p.borrow().source.borrow().known {
                p.borrow_mut().srcFp = DocxFingerprint::paragraph(&p);
                p.borrow_mut().srcPFp = DocxFingerprint::paraProps(&p);
            }
        }
        let __it_t = doc.borrow().tables.clone();
        for t in __it_t.iter().cloned() {
            if t.borrow().source.borrow().known {
                t.borrow_mut().srcFp = DocxFingerprint::table(&t);
            }
        }
    }
}
