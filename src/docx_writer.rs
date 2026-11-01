// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx_fingerprint::DocxFingerprint;
use crate::rich_document::DocBlock;
use crate::rich_document::DocBlockKind;
use crate::rich_document::DocSection;
use crate::rich_document::DocumentTable;
use crate::rich_document::InlineAnchor;
use crate::rich_document::Paragraph;
use crate::rich_document::RichDocument;
use crate::rich_document::TextSpan;
use crate::text::DxText;
use crate::word_numbering::WordNumbering;
use crate::word_style_resolver::ResolvedParagraphStyle;
use crate::word_style_resolver::ResolvedRunStyle;
use crate::word_style_resolver::WordStyleResolver;
use crate::word_xml::WordXml;
use crate::rgr::{Utf8, ZipWriter};

/// ==============================================================================
/// DocxWriter - RichDocument → .docx, keeping everything it did not change.
/// ==============================================================================
///
/// A document that was opened from a file is written back by SPLICING its own
/// word/document.xml, not by regenerating it:
///
///   * a paragraph or table whose fingerprint (DocxFingerprint) still matches
///     the one stamped at parse time is copied byte for byte from the source;
///   * one that was edited is regenerated from the model, keeping its original
///     w:p open tag, its w:pPr when the paragraph properties did not change,
///     and its inline anchors (tabs, pictures, field delimiters, note refs);
///   * one that was added is generated;
///   * one that was deleted is skipped, and everything between blocks that the
///     model never saw (bookmarks at body level, sdt wrappers, the final
///     w:sectPr) is copied through.
///
/// Every other part of the package is copied unchanged. A document built in
/// memory gets a minimal package of its own.
///
/// What an EDITED paragraph loses (an unedited one loses nothing): w:hyperlink
/// and w:ins/w:del wrappers around its runs, bookmarks inside it, and run
/// properties the model does not hold (character styles, language, highlight).
/// Runs are written with the differences between each span and the paragraph
/// style's run properties.
/// ==============================================================================
#[derive(Clone)]
pub struct DocxWriter {
    /// The styles the document was read with; the parser's resolver. Run and
    /// paragraph properties are written as differences from what the style
    /// already says.
    pub resolver: Rc<RefCell<WordStyleResolver>>,
    /// The lists; one the editor made is added to numbering.xml.
    pub numbering: Rc<RefCell<WordNumbering>>,
    pub warnings: Vec<String>,
    /// Splice state.
    pub src: String,
    pub out: String,
    pub cursor: i64,
    pub rangeAt: i64,
    pub live: HashMap<String,bool>,
    pub starts: Vec<i64>,
    pub ends: Vec<i64>,
    /// Counters a test can read after a save.
    pub copiedBlocks: i64,
    pub rewrittenBlocks: i64,
    pub newBlocks: i64,
    pub droppedBlocks: i64,
}

impl DocxWriter {
    pub fn new() -> Self {
        Self {
            resolver: Rc::new(RefCell::new(WordStyleResolver::new())),
            numbering: Rc::new(RefCell::new(WordNumbering::new())),
            warnings: Vec::new(),
            src: String::new(),
            out: String::new(),
            cursor: 0,
            rangeAt: 0,
            live: HashMap::new(),
            starts: Vec::new(),
            ends: Vec::new(),
            copiedBlocks: 0,
            rewrittenBlocks: 0,
            newBlocks: 0,
            droppedBlocks: 0,
        }
    }
    /// --- text --------------------------------------------------------------
    pub fn esc(s: &str) -> String {
        WordXml::escapeAttr(s)
    }
    /// Text as UTF-8 bytes, walked by code point so it means the same on every
    /// target.
    pub fn utf8(s: &str) -> Vec<u8> {
        let mut cs: Vec<i64> = s.chars().map(|c| c as i64).collect::<Vec<i64>>();
        let mut total: i64 = 0;
        for c in cs.iter().copied() {
            if c < 128 {
                total += 1;
            } else {
                if c < 2048 {
                    total += 2;
                } else {
                    if c < 65536 {
                        total += 3;
                    } else {
                        total += 4;
                    }
                }
            }
        }
        let mut b: Vec<u8> = vec![0u8; (total) as usize];
        let mut w: i64 = 0;
        for c2 in cs.iter().copied() {
            if c2 < 128 {
                b[w as usize] = (c2) as u8;
                w += 1;
            } else {
                if c2 < 2048 {
                    b[w as usize] = (192 + (c2 / 64)) as u8;
                    b[(w + 1) as usize] = (128 + c2 % 64) as u8;
                    w += 2;
                } else {
                    if c2 < 65536 {
                        b[w as usize] = (224 + (c2 / 4096)) as u8;
                        b[(w + 1) as usize] = (128 + (c2 / 64) % 64) as u8;
                        b[(w + 2) as usize] = (128 + c2 % 64) as u8;
                        w += 3;
                    } else {
                        b[w as usize] = (240 + (c2 / 262144)) as u8;
                        b[(w + 1) as usize] = (128 + (c2 / 4096) % 64) as u8;
                        b[(w + 2) as usize] = (128 + (c2 / 64) % 64) as u8;
                        b[(w + 3) as usize] = (128 + c2 % 64) as u8;
                        w += 4;
                    }
                }
            }
        }
        b
    }
    pub fn twips(pt: f64) -> i64 {
        (pt * 20.0 + 0.5).floor() as i64
    }
    pub fn halfPoints(pt: f64) -> i64 {
        (pt * 2.0 + 0.5).floor() as i64
    }
    /// The child `tag` of `el` only when it is the first child — what w:pPr,
    /// w:tblPr, w:trPr and w:tcPr always are. A search that went deeper would
    /// find the properties of a paragraph inside a text box.
    pub fn firstChild(el: &str, tag: &str) -> String {
        let gt: i64 = DxText::index_of(&el, ">");
        if gt < 1 {
            return String::new();
        }
        if (el.as_bytes()[(gt - 1) as usize] as i64) == 47 {
            return String::new();
        }
        let mut i: i64 = gt + 1;
        let n: i64 = el.as_bytes().len() as i64;
        while i < n {
            let ch: i64 = (el.as_bytes()[i as usize] as i64);
            if ((ch == 32 || ch == 9) || ch == 10) || ch == 13 {
                i += 1;
            } else {
                if WordXml::findNext(el, tag, i) == i {
                    return WordXml::sliceElement(el, tag, i);
                }
                return String::new();
            }
        }
        String::new()
    }
    /// One run of text; a tab or a newline in the text becomes its element.
    pub fn textRun(text: &str, r_pr: &str) -> String {
        if text.is_empty() {
            return String::new();
        }
        let mut body: String = String::new();
        let n: i64 = text.as_bytes().len() as i64;
        let mut from: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = (text.as_bytes()[i as usize] as i64);
            if c == 9 || c == 10 {
                if i > from {
                    body = format!("{}<w:t xml:space=\"preserve\">{}</w:t>", body, DocxWriter::esc(&text[(from) as usize..(i) as usize].to_string()));
                }
                body = format!("{}{}", body, if (c == 9) { "<w:tab/>".to_string() } else { "<w:br/>".to_string() });
                from = i + 1;
            }
            i += 1;
        }
        if n > from {
            body = format!("{}<w:t xml:space=\"preserve\">{}</w:t>", body, DocxWriter::esc(&text[(from) as usize..(n) as usize].to_string()));
        }
        format!("<w:r>{}{}</w:r>", r_pr, body)
    }
    /// --- paragraphs ----------------------------------------------------------
    pub fn alignName(a: i64) -> String {
        if a == 1 {
            return "center".to_string();
        }
        if a == 2 {
            return "right".to_string();
        }
        if a == 3 {
            return "both".to_string();
        }
        "left".to_string()
    }
    pub fn freshStylesXml() -> String {
        let mut w: String = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">".to_string();
        w = format!("{}<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/><w:sz w:val=\"22\"/><w:szCs w:val=\"22\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>", w);
        w = format!("{}<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style>", w);
        let mut lvl: i64 = 1;
        while lvl <= 3 {
            let id: String = format!("Heading{}", lvl);
            let sz: String = (if (lvl == 1) { 32 } else { (if (lvl == 2) { 26 } else { 24 }) }).to_string();
            w = format!("{}<w:style w:type=\"paragraph\" w:styleId=\"{}\"><w:name w:val=\"heading {}\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"80\"/><w:outlineLvl w:val=\"{}\"/></w:pPr><w:rPr><w:color w:val=\"2F5496\"/><w:sz w:val=\"{}\"/><w:szCs w:val=\"{}\"/></w:rPr></w:style>", w, id, lvl, lvl - 1, sz, sz);
            lvl += 1;
        }
        w = format!("{}<w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:qFormat/><w:rPr><w:sz w:val=\"56\"/><w:szCs w:val=\"56\"/></w:rPr></w:style>", w);
        format!("{}</w:styles>", w)
    }
    /// --- the package ---------------------------------------------------------
    pub fn text(b: &[u8]) -> String {
        Utf8::decode(&String::from_utf8_lossy(&b).to_string())
    }
    pub fn dirOf(part: &str) -> String {
        let mut i: i64 = (part.as_bytes().len() as i64) - 1;
        while i >= 0 {
            if (part.as_bytes()[i as usize] as i64) == 47 {
                return part[(0) as usize..(i + 1) as usize].to_string();
            }
            i -= 1;
        }
        String::new()
    }
    pub fn withNumberingType(ct: &str, part_name: &str) -> String {
        if DxText::index_of(&ct, part_name) >= 0 {
            return ct.to_string();
        }
        let end: i64 = DxText::index_of(&ct, "</Types>");
        if end < 0 {
            return ct.to_string();
        }
        format!("{}<Override PartName=\"{}\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/>{}", ct[(0) as usize..(end) as usize].to_string(), part_name, ct[(end) as usize..(ct.as_bytes().len() as i64) as usize].to_string())
    }
    /// The document's relationships with one to numbering.xml, under an id
    /// nothing else uses.
    pub fn withNumberingRel(mut rels: String) -> String {
        if rels.is_empty() {
            rels = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"></Relationships>".to_string();
        }
        let mut n: i64 = 1;
        while DxText::index_of(&rels, &format!("Id=\"rIdNum{}\"", n)) >= 0 {
            n += 1;
        }
        let end: i64 = DxText::index_of(&rels, "</Relationships>");
        if end < 0 {
            return rels;
        }
        format!("{}<Relationship Id=\"rIdNum{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>{}", rels[(0) as usize..(end) as usize].to_string(), n, rels[(end) as usize..(rels.as_bytes().len() as i64) as usize].to_string())
    }
    /// --- runs --------------------------------------------------------------
    pub fn rPrDiff(&self, sp: &Rc<RefCell<TextSpan>>, base: &Rc<RefCell<ResolvedRunStyle>>) -> String {
        // In the order CT_RPr lists them: Word refuses a file whose run
        // properties are out of order.
        let mut r: String = String::new();
        if sp.borrow().fontFamily != base.borrow().fontFamily {
            let f: String = DocxWriter::esc(&sp.borrow().fontFamily.clone());
            r = format!("{}<w:rFonts w:ascii=\"{}\" w:hAnsi=\"{}\" w:cs=\"{}\"/>", r, f, f, f);
        }
        if sp.borrow().bold != base.borrow().bold {
            r = format!("{}{}", r, if sp.borrow().bold { "<w:b/>".to_string() } else { "<w:b w:val=\"0\"/>".to_string() });
        }
        if sp.borrow().italic != base.borrow().italic {
            r = format!("{}{}", r, if sp.borrow().italic { "<w:i/>".to_string() } else { "<w:i w:val=\"0\"/>".to_string() });
        }
        if sp.borrow().strike != base.borrow().strike {
            r = format!("{}{}", r, if sp.borrow().strike { "<w:strike/>".to_string() } else { "<w:strike w:val=\"0\"/>".to_string() });
        }
        if sp.borrow().colorHex != base.borrow().colorHex {
            r = format!("{}<w:color w:val=\"{}\"/>", r, DocxWriter::esc(&sp.borrow().colorHex));
        }
        if sp.borrow().fontSizePt != base.borrow().fontSizePt {
            let hp: String = DocxWriter::halfPoints(sp.borrow().fontSizePt.clone()).to_string();
            r = format!("{}<w:sz w:val=\"{}\"/><w:szCs w:val=\"{}\"/>", r, hp, hp);
        }
        if sp.borrow().highlight != base.borrow().highlight {
            let mut hv: String = sp.borrow().highlight.clone();
            if hv.is_empty() {
                hv = "none".to_string();
            }
            r = format!("{}<w:highlight w:val=\"{}\"/>", r, DocxWriter::esc(&hv));
        }
        if sp.borrow().underline != base.borrow().underline {
            r = format!("{}{}", r, if sp.borrow().underline { "<w:u w:val=\"single\"/>".to_string() } else { "<w:u w:val=\"none\"/>".to_string() });
        }
        if sp.borrow().vertAlign != base.borrow().vertAlign {
            let mut va: String = "baseline";
            if sp.borrow().vertAlign == 1 {
                va = "superscript";
            }
            if sp.borrow().vertAlign == 2 {
                va = "subscript";
            }
            r = format!("{}<w:vertAlign w:val=\"{}\"/>", r, va);
        }
        if r.is_empty() {
            return String::new();
        }
        format!("<w:rPr>{}</w:rPr>", r)
    }
    pub fn runsXml(&self, p: &Rc<RefCell<Paragraph>>) -> String {
        let mut ps: Rc<RefCell<ResolvedParagraphStyle>> = self.resolver.borrow().resolveParagraphStyle(p.borrow().styleId.clone());
        let mut base: Rc<RefCell<ResolvedRunStyle>> = ps.borrow().run.clone().clone();
        let n: i64 = p.borrow().text.as_bytes().len() as i64;
        let na: i64 = p.borrow().anchors.len() as i64;
        let mut ai: i64 = 0;
        let mut r: String = String::new();
        let __n_si = p.borrow().spans.len() as i64;
        for si in 0..__n_si {
            let mut sp = p.borrow().spans[si as usize].clone();
            let a: i64 = if (sp.borrow().start < 0) { 0 } else { sp.borrow().start };
            let b: i64 = if (sp.borrow().end > n) { n } else { sp.borrow().end };
            let r_pr: String = self.rPrDiff(&sp, &base);
            let mut cur: i64 = a;
            let mut going: bool = true;
            while ai < na && going {
                let mut an: Rc<RefCell<InlineAnchor>> = p.borrow().anchors[ai as usize].clone();
                let at: i64 = if (an.borrow().offset > n) { n } else { an.borrow().offset };
                if at < b {
                    if at > cur {
                        r = format!("{}{}", r, DocxWriter::textRun(&(p.borrow().text)[(cur) as usize..(at) as usize].to_string(), &r_pr));
                        cur = at;
                    }
                    r = format!("{}{}", r, an.borrow().xml);
                    ai += 1;
                } else {
                    going = false;
                }
            }
            if b > cur {
                r = format!("{}{}", r, DocxWriter::textRun(&(p.borrow().text)[(cur) as usize..(b) as usize].to_string(), &r_pr));
            }
        }
        while ai < na {
            let mut an2: Rc<RefCell<InlineAnchor>> = p.borrow().anchors[ai as usize].clone();
            r = format!("{}{}", r, an2.borrow().xml);
            ai += 1;
        }
        r
    }
    pub fn pPrXml(&self, p: &Rc<RefCell<Paragraph>>, orig_p_pr: &str) -> String {
        let mut ps: Rc<RefCell<ResolvedParagraphStyle>> = self.resolver.borrow().resolveParagraphStyle(p.borrow().styleId.clone());
        let mut r: String = String::new();
        if !p.borrow().styleId.is_empty() && p.borrow().styleId != "Normal" {
            r = format!("{}<w:pStyle w:val=\"{}\"/>", r, DocxWriter::esc(&p.borrow().styleId));
        }
        if p.borrow().pageBreakBefore {
            r = format!("{}<w:pageBreakBefore/>", r);
        }
        if p.borrow().listNumId > 0 {
            r = format!("{}<w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{}\"/></w:numPr>", r, p.borrow().listLevel, p.borrow().listNumId);
        }
        if p.borrow().bidi != ps.borrow().bidi {
            if p.borrow().bidi == 1 {
                r = format!("{}<w:bidi/>", r);
            }
            if p.borrow().bidi == 0 {
                r = format!("{}<w:bidi w:val=\"0\"/>", r);
            }
        }
        if (p.borrow().spaceBeforePt != ps.borrow().spaceBeforePt || p.borrow().spaceAfterPt != ps.borrow().spaceAfterPt) || p.borrow()
            .lineSpacing != ps.borrow()
            .lineSpacing {
            r = format!("{}<w:spacing w:before=\"{}\" w:after=\"{}\" w:line=\"{}\" w:lineRule=\"auto\"/>", r, DocxWriter::twips(p.borrow().spaceBeforePt), DocxWriter::twips(p.borrow().spaceAfterPt), (p.borrow().lineSpacing * 240.0 + 0.5).floor() as i64);
        }
        if (p.borrow().indentLeftPt != ps.borrow().indentLeftPt || p.borrow().indentRightPt != ps.borrow().indentRightPt) || p.borrow()
            .indentFirstLinePt != ps.borrow()
            .indentFirstLinePt {
            let mut _tmp_3919 = p.borrow().indentLeftPt.clone();
            let mut ind: String = format!("<w:ind w:left=\"{}\" w:right=\"{}\"", DocxWriter::twips(_tmp_3919), DocxWriter::twips(p.borrow().indentRightPt.clone()));
            if p.borrow().indentFirstLinePt >= 0.0 {
                ind = format!("{} w:firstLine=\"{}\"/>", ind, DocxWriter::twips(p.borrow().indentFirstLinePt));
            } else {
                ind = format!("{} w:hanging=\"{}\"/>", ind, DocxWriter::twips(0.0 - p.borrow().indentFirstLinePt));
            }
            r = format!("{}{}", r, ind);
        }
        if p.borrow().align != ps.borrow().align {
            r = format!("{}<w:jc w:val=\"{}\"/>", r, DocxWriter::alignName(p.borrow().align));
        }
        // A section that ends at this paragraph ends here whatever was edited.
        if !orig_p_pr.is_empty() {
            let s_at: i64 = WordXml::findNext(orig_p_pr, "w:sectPr", 0);
            if s_at >= 0 {
                r = format!("{}{}", r, WordXml::sliceElement(orig_p_pr, "w:sectPr", s_at));
            }
        }
        if r.is_empty() {
            return String::new();
        }
        format!("<w:pPr>{}</w:pPr>", r)
    }
    /// `orig` is the paragraph's source XML, or "" for a new one.
    pub fn paragraphXml(&self, p: &Rc<RefCell<Paragraph>>, orig: &str) -> String {
        let mut open: String = "<w:p>".to_string();
        let mut p_pr: String = String::new();
        if !orig.is_empty() {
            let ot: String = WordXml::openTag(orig);
            if DxText::index_of(&ot, "/>") < 0 {
                open = ot.clone();
            }
            let orig_p_pr: String = DocxWriter::firstChild(orig, "w:pPr");
            if DocxFingerprint::paraProps(p) == p.borrow().srcPFp {
                p_pr = orig_p_pr.clone();
            } else {
                p_pr = self.pPrXml(p, &orig_p_pr);
            }
        } else {
            p_pr = self.pPrXml(p, "");
        }
        format!("{}{}{}</w:p>", open, p_pr, self.runsXml(p))
    }
    /// A paragraph inside a table cell: copied when unchanged.
    pub fn cellParagraphXml(&self, p: &Rc<RefCell<Paragraph>>) -> String {
        if p.borrow().source.borrow().known {
            let orig: String = p.borrow().source.borrow_mut().slice(&self.src);
            if !orig.is_empty() && DocxFingerprint::paragraph(p) == p.borrow().srcFp {
                return orig;
            }
            return self.paragraphXml(p, &orig);
        }
        self.paragraphXml(p, "")
    }
    /// --- tables --------------------------------------------------------------
    pub fn tableXml(&self, t: &Rc<RefCell<DocumentTable>>, orig: &str) -> String {
        let mut tbl_pr: String = String::new();
        if !orig.is_empty() {
            tbl_pr = DocxWriter::firstChild(orig, "w:tblPr");
        }
        if tbl_pr.is_empty() {
            tbl_pr = "<w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/></w:tblBorders><w:tblLook w:val=\"04A0\"/></w:tblPr>".to_string();
        }
        let mut r: String = format!("<w:tbl>{}<w:tblGrid>", tbl_pr);
        let __it_w = t.borrow().colWidthsTwips.clone();
        for w in __it_w.iter().copied() {
            r = format!("{}<w:gridCol w:w=\"{}\"/>", r, w);
        }
        r = format!("{}</w:tblGrid>", r);
        let __n_ri = t.borrow().rows.len() as i64;
        for ri in 0..__n_ri {
            let mut row = t.borrow().rows[ri as usize].clone();
            let mut tr_pr: String = String::new();
            if row.borrow().source.borrow().known {
                tr_pr = DocxWriter::firstChild(&SourceRef::slice(&row.borrow().source, &self.src), "w:trPr");
            }
            if tr_pr.is_empty() {
                if row.borrow().heightTwips > 0 {
                    tr_pr = format!("<w:trPr><w:trHeight w:val=\"{}\"/></w:trPr>", row.borrow().heightTwips);
                }
            }
            r = format!("{}<w:tr>{}", r, tr_pr);
            let mut col: i64 = 0;
            let __it_cell = row.borrow().cells.clone();
            for cell in __it_cell.iter().cloned() {
                let mut tc_pr: String = String::new();
                if cell.borrow().source.borrow().known {
                    tc_pr = DocxWriter::firstChild(&SourceRef::slice(&cell.borrow().source, &self.src), "w:tcPr");
                }
                if tc_pr.is_empty() {
                    let span: i64 = if (cell.borrow().gridSpan < 1) { 1 } else { cell.borrow().gridSpan };
                    let mut wsum: i64 = 0;
                    let mut k: i64 = 0;
                    while k < span {
                        if col + k < (t.borrow().colWidthsTwips.len() as i64) {
                            wsum += t.borrow().colWidthsTwips[(col + k) as usize];
                        }
                        k += 1;
                    }
                    tc_pr = format!("<w:tcPr><w:tcW w:w=\"{}\" w:type=\"dxa\"/>", wsum);
                    if span > 1 {
                        tc_pr = format!("{}<w:gridSpan w:val=\"{}\"/>", tc_pr, span);
                    }
                    if (cell.borrow().fillHex.as_bytes().len() as i64) == 6 {
                        tc_pr = format!("{}<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>", tc_pr, cell.borrow().fillHex);
                    }
                    tc_pr = format!("{}</w:tcPr>", tc_pr);
                }
                col += if (cell.borrow().gridSpan < 1) { 1 } else { cell.borrow().gridSpan };
                r = format!("{}<w:tc>{}", r, tc_pr);
                // A cell must end in a paragraph.
                if cell.borrow().paragraphs.is_empty() {
                    r = format!("{}<w:p/>", r);
                }
                let __it_cp = cell.borrow().paragraphs.clone();
                for cp in __it_cp.iter().cloned() {
                    r = format!("{}{}", r, self.cellParagraphXml(&cp));
                }
                r = format!("{}</w:tc>", r);
            }
            r = format!("{}</w:tr>", r);
        }
        format!("{}</w:tbl>", r)
    }
    /// --- the splice ----------------------------------------------------------
    /// src[a, b), leaving out every body-level element whose block is gone.
    pub fn copyGap(&mut self, a: i64, b: i64) {
        let mut from: i64 = a;
        let n: i64 = self.starts.len() as i64;
        while self.rangeAt < n && self.starts[(self.rangeAt) as usize] < a {
            self.rangeAt += 1;
        }
        while self.rangeAt < n && self.ends[(self.rangeAt) as usize] <= b {
            let s: i64 = self.starts[(self.rangeAt) as usize];
            if !self.live.contains_key(&s.to_string()) {
                self.out = format!("{}{}", self.out, self.src[(from) as usize..(s) as usize].to_string());
                from = self.ends[(self.rangeAt) as usize];
                self.droppedBlocks += 1;
            }
            self.rangeAt += 1;
        }
        if b > from {
            self.out = format!("{}{}", self.out, self.src[(from) as usize..(b) as usize].to_string());
        }
        self.cursor = b;
    }
    fn blockRef(&self, b: &Rc<RefCell<DocBlock>>) -> Rc<RefCell<SourceRef>> {
        if b.borrow().kind == DocBlockKind::para() {
            return b.borrow().paragraph.borrow().source.clone();
        }
        if b.borrow().kind == DocBlockKind::table() {
            return b.borrow().table.borrow().source.clone();
        }
        Rc::new(RefCell::new(SourceRef::new()))
    }
    /// A block with no source of its own.
    pub fn newBlockXml(&mut self, b: &Rc<RefCell<DocBlock>>) -> String {
        if b.borrow().kind == DocBlockKind::para() {
            return self.paragraphXml(&b.borrow().paragraph.clone(), "");
        }
        if b.borrow().kind == DocBlockKind::table() {
            return self.tableXml(&b.borrow().table.clone(), "");
        }
        if b.borrow().kind == DocBlockKind::pageBreak() {
            if b.borrow().synthetic {
                return String::new();
            }
            return "<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>".to_string();
        }
        if b.borrow().kind == DocBlockKind::sectionBreak() {
            if !b.borrow().synthetic {
                self.warnings.push("a section break added in the editor is not written".to_string().to_string());
            }
            return String::new();
        }
        if b.borrow().kind == DocBlockKind::chart() {
            self.warnings.push("charts are not written to .docx yet".to_string().to_string());
            return String::new();
        }
        if b.borrow().kind == DocBlockKind::drawing() {
            self.warnings.push("drawings are not written to .docx yet".to_string().to_string());
            return String::new();
        }
        String::new()
    }
    pub fn documentXml(&mut self, doc: &Rc<RefCell<RichDocument>>) -> String {
        self.warnings.clear();
        self.copiedBlocks = 0;
        self.rewrittenBlocks = 0;
        self.newBlocks = 0;
        self.droppedBlocks = 0;
        self.src = doc.borrow().source.borrow().mainXml.clone();
        let body_at: i64 = WordXml::findNext(&self.src.clone(), "w:body", 0);
        if !doc.borrow().source.borrow().known || body_at < 0 {
            return self.freshDocumentXml(doc);
        }
        let body_gt: i64 = DxText::index_of_from(&self.src, ">", body_at);
        if body_gt < 0 {
            return self.freshDocumentXml(doc);
        }
        self.starts = doc.borrow().source.borrow().rangeStarts.clone();
        self.ends = doc.borrow().source.borrow().rangeEnds.clone();
        let mut fresh_live: HashMap<String,bool> = HashMap::new();
        self.live = fresh_live.clone();
        // Which sourced blocks are still where they were. One that now comes
        // before something it used to follow is written as new, and its old
        // place is skipped like a deleted one.
        let mut in_order: Vec<bool> = Vec::new();
        let mut last: i64 = -1;
        let n: i64 = self.src.as_bytes().len() as i64;
        let __it_b = doc.borrow().blocks.clone();
        for b in __it_b.iter().cloned() {
            let mut r#ref: Rc<RefCell<SourceRef>> = self.blockRef(&b);
            let mut ok: bool = false;
            if (r#ref.borrow().known && r#ref.borrow().start > last) && r#ref.borrow().end <= n {
                ok = true;
                last = r#ref.borrow().start;
                self.live.insert(r#ref.borrow().start.to_string(), true);
            }
            in_order.push(ok);
        }
        self.out = String::new();
        self.rangeAt = 0;
        self.cursor = 0;
        self.copyGap(0, body_gt + 1);
        let __n_bj = doc.borrow().blocks.len() as i64;
        for bj in 0..__n_bj {
            let mut b2 = doc.borrow().blocks[bj as usize].clone();
            if in_order[bj as usize] {
                let mut ref2: Rc<RefCell<SourceRef>> = self.blockRef(&b2);
                let mut _tmp_3933 = self.cursor.clone();
                self.copyGap(_tmp_3933, ref2.borrow().start.clone());
                let orig: String = SourceRef::slice(&ref2, &self.src);
                if b2.borrow().kind == DocBlockKind::para() {
                    if DocxFingerprint::paragraph(&b2.borrow().paragraph.clone()) == b2.borrow().paragraph.borrow().srcFp {
                        self.out = format!("{}{}", self.out, orig);
                        self.copiedBlocks += 1;
                    } else {
                        let _tmp_3938 = format!("{}{}", self.out, self.paragraphXml(&b2.borrow().paragraph.clone(),
                            &orig
                        ));
                        self.out = _tmp_3938;
                        self.rewrittenBlocks += 1;
                    }
                } else {
                    if DocxFingerprint::table(&b2.borrow().table.clone()) == b2.borrow().table.borrow().srcFp {
                        self.out = format!("{}{}", self.out, orig);
                        self.copiedBlocks += 1;
                    } else {
                        let _tmp_3942 = format!("{}{}", self.out, self.tableXml(&b2.borrow().table.clone(),
                            &orig
                        ));
                        self.out = _tmp_3942;
                        self.rewrittenBlocks += 1;
                    }
                }
                self.cursor = ref2.borrow().end;
            } else {
                let x: String = self.newBlockXml(&b2);
                if !x.is_empty() {
                    self.out = format!("{}{}", self.out, x);
                    self.newBlocks += 1;
                }
            }
        }
        self.copyGap(self.cursor.clone(), n);
        self.out.clone()
    }
    /// --- a document with no file behind it -----------------------------------
    pub fn freshDocumentXml(&mut self, doc: &Rc<RefCell<RichDocument>>) -> String {
        let mut body: String = String::new();
        let __it_b = doc.borrow().blocks.clone();
        for b in __it_b.iter().cloned() {
            let x: String = self.newBlockXml(&b);
            if !x.is_empty() {
                self.newBlocks += 1;
            }
            body = format!("{}{}", body, x);
        }
        let mut sec: Rc<RefCell<DocSection>> = doc.borrow().section.clone().clone();
        let mut sect: String = format!("<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"", sec.borrow().pageWTwips, sec.borrow().pageHTwips);
        if sec.borrow().landscape {
            sect = format!("{} w:orient=\"landscape\"", sect);
        }
        sect = format!("{}/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr>", sect, sec.borrow().marginTopTwips, sec.borrow().marginRightTwips, sec.borrow().marginBottomTwips, sec.borrow().marginLeftTwips);
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body>{}{}</w:body></w:document>", body, sect)
    }
    fn buildZip(&mut self, doc: &Rc<RefCell<RichDocument>>) -> Rc<RefCell<ZipWriter>> {
        let mut zip: Rc<RefCell<ZipWriter>> = Rc::new(RefCell::new(ZipWriter::new()));
        ZipWriter::create(&zip);
        let xml: String = self.documentXml(doc);
        if doc.borrow().source.borrow().known {
            let mut wrote_main: bool = false;
            // A list the editor made goes into the numbering part, which is
            // made, with its relationship and content type, when there was none.
            let lists: bool = self.numbering.borrow().hasAdditions();
            let mut num_part: String = doc.borrow().source.borrow().numberingPart.clone();
            if lists {
                if !num_part.is_empty() {
                    if !doc.borrow().source.borrow().bytes.contains_key(&num_part) {
                        num_part = String::new();
                    }
                }
            }
            let main_dir: String = DocxWriter::dirOf(&doc.borrow().source.borrow().mainPart.clone());
            let rels_part: String = format!("{}_rels/{}.rels", main_dir, (doc.borrow().source.borrow().mainPart)[(main_dir.as_bytes().len() as i64) as usize..((doc.borrow().source.borrow().mainPart.as_bytes().len() as i64)) as usize].to_string());
            let new_num_part: bool = lists && num_part.is_empty();
            let mut wrote_rels: bool = false;
            let __n_pi = doc.borrow().source.borrow().parts.len() as i64;
            for pi in 0..__n_pi {
                let mut part = doc.borrow().source.borrow().parts[pi as usize].clone();
                if part == doc.borrow().source.borrow().mainPart {
                    ZipWriter::addFile(&zip, &part, &DocxWriter::utf8(&xml));
                    wrote_main = true;
                } else {
                    let mut bytes: Option<Vec<u8>> = doc.borrow().source.borrow().bytes.get(&part).cloned();
                    if bytes.is_some() {
                        let mut handled: bool = false;
                        if lists && part == num_part {
                            let _tmp_3948 = self.numbering.borrow().withAdditions(&DocxWriter::text(&bytes.clone().unwrap()));
                            ZipWriter::addFile(&zip, &part, &DocxWriter::utf8(&_tmp_3948));
                            handled = true;
                        }
                        if new_num_part && part == "[Content_Types].xml" {
                            let _tmp_3950 = DocxWriter::withNumberingType(&DocxWriter::text(&bytes.clone().unwrap()), &format!("/{}numbering.xml", main_dir));
                            ZipWriter::addFile(&zip, &part, &DocxWriter::utf8(&_tmp_3950));
                            handled = true;
                        }
                        if new_num_part && part == rels_part {
                            let _tmp_3952 = DocxWriter::withNumberingRel(DocxWriter::text(&bytes.clone().unwrap()));
                            ZipWriter::addFile(&zip, &part, &DocxWriter::utf8(&_tmp_3952));
                            handled = true;
                            wrote_rels = true;
                        }
                        if !handled {
                            ZipWriter::addFile(&zip, &part, &bytes.clone().unwrap());
                        }
                    }
                }
            }
            if !wrote_main {
                ZipWriter::addFile(&zip, &doc.borrow().source.borrow().mainPart.clone(), &DocxWriter::utf8(&xml));
            }
            if new_num_part {
                ZipWriter::addFile(
                    &zip,
                    &format!("{}numbering.xml", main_dir),
                    &DocxWriter::utf8(&self.numbering.borrow().withAdditions(""))
                );
                if !wrote_rels {
                    ZipWriter::addFile(&zip, &rels_part, &DocxWriter::utf8(&DocxWriter::withNumberingRel(String::new())));
                }
            }
            return zip;
        }
        let mut ct: String = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/></Types>".to_string();
        let rels: String = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
        let mut doc_rels: String = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>".to_string();
        if self.numbering.borrow().hasAdditions() {
            ct = DocxWriter::withNumberingType(&ct, "/word/numbering.xml");
            doc_rels = DocxWriter::withNumberingRel(doc_rels.clone());
        }
        ZipWriter::addFile(&zip, "[Content_Types].xml", &DocxWriter::utf8(&ct));
        ZipWriter::addFile(&zip, "_rels/.rels", &DocxWriter::utf8(rels));
        ZipWriter::addFile(&zip, "word/document.xml", &DocxWriter::utf8(&xml));
        ZipWriter::addFile(&zip, "word/_rels/document.xml.rels", &DocxWriter::utf8(&doc_rels));
        ZipWriter::addFile(&zip, "word/styles.xml", &DocxWriter::utf8(&DocxWriter::freshStylesXml()));
        if self.numbering.borrow().hasAdditions() {
            ZipWriter::addFile(
                &zip,
                "word/numbering.xml",
                &DocxWriter::utf8(&self.numbering.borrow().withAdditions(""))
            );
        }
        zip
    }
    pub fn toBuffer(&mut self, doc: &Rc<RefCell<RichDocument>>) -> Vec<u8> {
        let mut zip: Rc<RefCell<ZipWriter>> = self.buildZip(doc);
        return ZipWriter::toBuffer(&zip);
    }
    pub fn save(&mut self, doc: &Rc<RefCell<RichDocument>>, dir: &str, file: &str) {
        let mut zip: Rc<RefCell<ZipWriter>> = self.buildZip(doc);
        ZipWriter::save(&zip, dir, file);
    }
}
