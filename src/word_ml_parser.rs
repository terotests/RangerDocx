// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::docx_fingerprint::DocxFingerprint;
use crate::docx_package::DocxPackage;
use crate::docx_png::DocxPng;
use crate::rgr::DxNative;
use crate::rich_document::DocBlock;
use crate::rich_document::DocImage;
use crate::rich_document::DocSection;
use crate::rich_document::DocTableBorder;
use crate::rich_document::DocTableCell;
use crate::rich_document::DocTableRow;
use crate::rich_document::DocumentTable;
use crate::rich_document::InlineAnchor;
use crate::rich_document::Paragraph;
use crate::rich_document::RichDocument;
use crate::rich_document::TextSpan;
use crate::word_numbering::WordNumbering;
use crate::word_style_resolver::ResolvedRunStyle;
use crate::word_style_resolver::WordStyleResolver;
use crate::word_xml::WordXml;
use crate::rgr::{JPEGDecoder};

/// ==============================================================================
/// WordMlParser - WordprocessingML document.xml → RichDocument
/// ==============================================================================
/// Walks body children: w:p / w:tbl / w:sectPr. Tables, page breaks, sections,
/// headers/footers, DrawingML images (JPEG+PNG).
/// ==============================================================================
#[derive(Clone)]
pub struct WordMlParser {
    pub resolver: Rc<RefCell<WordStyleResolver>>,
    pub numbering: Rc<RefCell<WordNumbering>>,
    pub pkg: Rc<RefCell<DocxPackage>>,
    pub currentSection: i64,
}

impl WordMlParser {
    pub fn new() -> Self {
        Self {
            resolver: Rc::new(RefCell::new(WordStyleResolver::new())),
            numbering: Rc::new(RefCell::new(WordNumbering::new())),
            pkg: Rc::new(RefCell::new(DocxPackage::new())),
            currentSection: 0,
        }
    }
    /// Run children with no text of their own (a tab, a picture, a field
    /// delimiter, a note reference), each kept as a one-child run at the text
    /// offset it stood at. Only an edited paragraph ever reads these.
    pub fn anchorTag(name: &str) -> bool {
        let mut keep: Vec<String> = vec!["w:tab".to_string(), "w:br".to_string(), "w:cr".to_string(), "w:sym".to_string(), "w:noBreakHyphen".to_string(), "w:softHyphen".to_string(), "w:ptab".to_string(), "w:drawing".to_string(), "w:pict".to_string(), "w:object".to_string(), "w:fldChar".to_string(), "w:instrText".to_string(), "w:footnoteReference".to_string(), "w:endnoteReference".to_string(), "w:commentReference".to_string(), "w:separator".to_string(), "w:continuationSeparator".to_string(), "mc:AlternateContent".to_string()];
        for k in keep.iter().cloned() {
            if k == name {
                return true;
            }
        }
        false
    }
    /// A picture's bytes, decoded: what every reader does once it has found the
    /// file (the .odt and .rtf readers too).
    pub fn decodeImageBytes(img: &Rc<RefCell<DocImage>>, bytes: Vec<u8>) {
        if (bytes.len() as i64) < 8 {
            return;
        }
        // Kept as well as decoded: the package is closed once parsing is done,
        // and a browser draws the file rather than the pixels.
        img.borrow_mut().bytes = bytes.clone();
        let b0: i64 = bytes[0] as i64;
        let b1: i64 = bytes[1] as i64;
        if b0 == 255 && b1 == 216 {
            img.borrow_mut().format = "jpeg".to_string();
            let mut jdec: Rc<RefCell<JPEGDecoder>> = Rc::new(RefCell::new(JPEGDecoder::new()));
            img.borrow_mut().pixels = JPEGDecoder::decodeBytes(&jdec, bytes.clone());
            if img.borrow().pixels.borrow().width > 0 && img.borrow().pixels.borrow().height > 0 {
                img.borrow_mut().ok = true;
                if img.borrow().widthEmu == 0 {
                    img.borrow_mut().widthPx = img.borrow().pixels.borrow().width;
                    img.borrow_mut().heightPx = img.borrow().pixels.borrow().height;
                }
            }
            return;
        }
        if b0 == 137 && b1 == 80 {
            img.borrow_mut().format = "png".to_string();
            img.borrow_mut().pixels = DocxPng::decode(&bytes);
            if img.borrow().pixels.borrow().width > 0 && img.borrow().pixels.borrow().height > 0 {
                img.borrow_mut().ok = true;
                if img.borrow().widthEmu == 0 {
                    img.borrow_mut().widthPx = img.borrow().pixels.borrow().width;
                    img.borrow_mut().heightPx = img.borrow().pixels.borrow().height;
                }
            }
            return;
        }
    }
    pub fn parsePackage(&mut self, docx_pkg: Rc<RefCell<DocxPackage>>) -> Rc<RefCell<RichDocument>> {
        self.pkg = docx_pkg.clone();
        // the theme first: styles name its fonts (w:asciiTheme)
        let theme: String = self.pkg.borrow().themeXml();
        if !theme.is_empty() {
            self.resolver.clone().borrow_mut().parseThemeXml(&theme);
        }
        let styles: String = self.pkg.borrow().stylesXml();
        if !styles.is_empty() {
            self.resolver.clone().borrow_mut().parseStylesXml(&styles);
        }
        self.numbering = Rc::new(RefCell::new(WordNumbering::new()));
        let num_xml: String = self.pkg.borrow().numberingXml();
        if !num_xml.is_empty() {
            self.numbering.clone().borrow_mut().parse(&num_xml);
        }
        let mut doc: Rc<RefCell<RichDocument>> = self.parseDocumentXml(self.pkg.borrow().documentXml());
        // Keep the package whole: a save copies every part it does not rewrite.
        doc.borrow_mut().source.borrow_mut().known = true;
        doc.borrow_mut().source.borrow_mut().mainPart = self.pkg.borrow().mainPart.clone();
        doc.borrow_mut().source.borrow_mut().numberingPart = self.pkg.borrow().numberingPart();
        let mut part_names: Vec<String> = self.pkg.borrow().listParts();
        for pn in part_names.iter().cloned() {
            doc.borrow_mut().source.borrow_mut().parts.push(pn.clone());
            doc.borrow_mut()
                .source.borrow_mut()
                .bytes.insert(pn.clone(), self.pkg.borrow().readPartBytes(&pn));
        }
        let mut i: i64 = 0;
        while i < doc.borrow().paragraphCount() {
            let mut p: Rc<RefCell<Paragraph>> = doc.borrow().paragraphAt(i);
            if p.borrow().listNumId > 0 {
                self.numbering.clone().borrow().applyToParagraph(&p);
            }
            i += 1;
        }
        // Load header/footer parts for each section
        let mut si: i64 = 0;
        while si < doc.borrow().sectionCount() {
            let mut sec: Rc<RefCell<DocSection>> = doc.borrow().sectionAt(si);
            if !sec.borrow().headerRelId.is_empty() {
                self.loadHeaderFooter(&doc, &sec, &sec.borrow().headerRelId.clone(), true);
            }
            if !sec.borrow().footerRelId.is_empty() {
                self.loadHeaderFooter(&doc, &sec, &sec.borrow().footerRelId.clone(), false);
            }
            si += 1;
        }
        DocxFingerprint::stamp(&doc);
        doc
    }
    pub fn loadHeaderFooter(&self, doc: &Rc<RefCell<RichDocument>>, sec: &Rc<RefCell<DocSection>>, rel_id: &str, is_header: bool) {
        let part: String = self.pkg.borrow().resolveRel(rel_id);
        if part.is_empty() {
            return;
        }
        let xml: String = self.pkg.borrow().readPart(&part);
        if xml.is_empty() {
            return;
        }
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&xml);
        while i < n {
            let p_start: i64 = WordXml::findNext(&xml, "w:p", i);
            if p_start < 0 {
                return;
            }
            let p_el: String = WordXml::sliceElement(&xml, "w:p", p_start);
            let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraphOnly();
            self.fillParagraph(&p, &p_el);
            if is_header {
                sec.borrow_mut().headerParas.push(p.clone());
            } else {
                sec.borrow_mut().footerParas.push(p.clone());
            }
            let p_end: i64 = WordXml::elementEnd(&xml, "w:p", p_start);
            if p_end < 0 {
                return;
            }
            i = p_end;
        }
    }
    pub fn parseDocumentXml(&mut self, xml: String) -> Rc<RefCell<RichDocument>> {
        let mut doc: Rc<RefCell<RichDocument>> = Rc::new(RefCell::new(RichDocument::new()));
        doc.borrow_mut().source.borrow_mut().mainXml = xml.clone();
        self.currentSection = 0;
        let body_at: i64 = WordXml::findNext(&xml, "w:body", 0);
        let mut body: String = xml.clone();
        // Where `body` starts inside `xml`: every source offset is base + a
        // position in body. -1 when it cannot be pinned down, and then nothing
        // gets a source (a save regenerates instead of splicing).
        let mut base: i64 = 0;
        if body_at >= 0 {
            let body_el: String = WordXml::sliceElement(&xml, "w:body", body_at);
            body = WordXml::innerOf(&body_el, "w:body");
            base = -1;
            let gt: i64 = DxNative::index_of(&body_el, ">");
            if gt >= 0 {
                let cand: i64 = (body_at + gt) + 1;
                if cand + (DxNative::str_len(&body)) <= (DxNative::str_len(&xml)) {
                    if DxNative::substr(&xml, cand, cand + (DxNative::str_len(&body))) == body {
                        base = cand;
                    }
                }
            }
        }
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&body);
        // Where the next `w:p`, `w:tbl` and `w:sectPr` begin — searched once,
        // and again only when the walk has gone past the one we found.
        //
        // The body is walked strictly forward, which makes that sound: a hit
        // still ahead of `i` is still the NEXT hit (it was the first at or
        // after an earlier position, and everything at or after `i` is also at
        // or after that one), and a miss is a miss for every position after it.
        //
        // Searching all three from `i` on every turn is what made opening a
        // long document quadratic. A document with no tables in it — which is
        // most documents — scanned its entire length looking for `w:tbl` once
        // per paragraph and found nothing, 20,000 times: 58 seconds to open a
        // file the spreadsheet's loader reads in under one.
        let mut p_at: i64 = WordXml::findNext(&body, "w:p", 0);
        let mut t_at: i64 = WordXml::findNext(&body, "w:tbl", 0);
        let mut s_at: i64 = WordXml::findNext(&body, "w:sectPr", 0);
        while i < n {
            if p_at >= 0 {
                if p_at < i {
                    p_at = WordXml::findNext(&body, "w:p", i);
                }
            }
            if t_at >= 0 {
                if t_at < i {
                    t_at = WordXml::findNext(&body, "w:tbl", i);
                }
            }
            if s_at >= 0 {
                if s_at < i {
                    s_at = WordXml::findNext(&body, "w:sectPr", i);
                }
            }
            let mut next: i64 = -1;
            let mut kind: i64 = -1;
            if p_at >= 0 {
                next = p_at;
                kind = 0;
            }
            if t_at >= 0 && (next < 0 || t_at < next) {
                next = t_at;
                kind = 1;
            }
            if s_at >= 0 && (next < 0 || s_at < next) {
                next = s_at;
                kind = 2;
            }
            if next < 0 {
                return doc;
            }
            if kind == 0 {
                // Ensure this w:p is not nested inside a table we already passed —
                // body walk uses findNext from i; after tables i advances past tbl.
                let p_el: String = WordXml::sliceElement(&body, "w:p", next);
                // Detect page break in runs before creating paragraph block
                let br_at: i64 = WordXml::findNext(&p_el, "w:br", 0);
                let mut is_page_br: bool = false;
                if br_at >= 0 {
                    let br_el: String = WordXml::sliceElement(&p_el, "w:br", br_at);
                    let bt: String = WordXml::attr(&WordXml::openTag(&br_el), "w:type");
                    if bt == "page" {
                        is_page_br = true;
                    }
                }
                let p_end: i64 = WordXml::elementEnd(&body, "w:p", next);
                if is_page_br {
                    doc.borrow_mut().addPageBreak();
                    self.markLastSynthetic(&doc);
                    // Still parse text on the break paragraph if any
                    let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraph();
                    self.fillParagraph(&p, &p_el);
                    self.setSource(&doc, &p.borrow().source.clone(), base, next, p_end, true);
                    self.maybeSectionFromPara(&doc, &p_el);
                } else {
                    let mut p2: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraph();
                    self.fillParagraph(&p2, &p_el);
                    self.setSource(&doc, &p2.borrow().source.clone(), base, next, p_end, true);
                    if p2.borrow().pageBreakBefore {
                        // Insert page break before this paragraph's block.
                        let bc: i64 = doc.borrow().blocks.len() as i64;
                        if bc >= 1 {
                            let mut last: Rc<RefCell<DocBlock>> = doc.borrow().blocks[(bc - 1) as usize].clone();
                            doc.borrow_mut().blocks.pop();
                            let mut pb: Rc<RefCell<DocBlock>> = DocBlock::pageBreak();
                            pb.borrow_mut().synthetic = true;
                            doc.borrow_mut().blocks.push(pb.clone());
                            doc.borrow_mut().blocks.push(last.clone());
                        }
                    }
                    self.maybeSectionFromPara(&doc, &p_el);
                }
                if p_end < 0 {
                    return doc;
                }
                i = p_end;
            }
            if kind == 1 {
                let t_el: String = WordXml::sliceElement(&body, "w:tbl", next);
                let mut t_base: i64 = -1;
                if base >= 0 {
                    t_base = base + next;
                }
                let mut tbl: Rc<RefCell<DocumentTable>> = self.parseTable(&doc,
                    &t_el,
                    t_base
                );
                let t_end: i64 = WordXml::elementEnd(&body, "w:tbl", next);
                self.setSource(&doc, &tbl.borrow().source.clone(), base, next, t_end, true);
                if t_end < 0 {
                    return doc;
                }
                i = t_end;
            }
            if kind == 2 {
                let s_el: String = WordXml::sliceElement(&body, "w:sectPr", next);
                // Final sectPr applies to current section; mid-body sectPr is
                // typically inside the last paragraph of a section — but some
                // docs place sectPr as body child between content.
                // If we already have content in current section and this isn't
                // the last element, treat as section break.
                let s_end: i64 = WordXml::elementEnd(&body, "w:sectPr", next);
                let mut remaining: bool = false;
                if s_end >= 0 {
                    let mut j: i64 = s_end;
                    while j < n {
                        let ch: i64 = DxNative::char_at(&body, j);
                        if ((ch != 32 && ch != 9) && ch != 10) && ch != 13 {
                            remaining = true;
                            j = n;
                        } else {
                            j += 1;
                        }
                    }
                }
                if remaining {
                    let new_idx: i64 = doc.borrow_mut().addSectionBreak();
                    self.markLastSynthetic(&doc);
                    self.currentSection = new_idx;
                    self.applySectPr(&doc.borrow().sectionAt(new_idx).clone(), &s_el);
                } else {
                    self.applySectPr(&doc.borrow().sectionAt(self.currentSection).clone(), &s_el);
                    let _tmp_3821 = doc.borrow().sectionAt(self.currentSection.clone());
                    doc.borrow_mut().section = _tmp_3821;
                }
                if s_end < 0 {
                    return doc;
                }
                i = s_end;
            }
        }
        doc
    }
    /// `ref` = body[from, to) shifted by `base`; body-level elements also go on
    /// the document's range list. Nothing is recorded when base is unknown.
    fn setSource(&self, doc: &Rc<RefCell<RichDocument>>, r#ref: &Rc<RefCell<SourceRef>>, base: i64, from: i64, to: i64, body_level: bool) {
        if (base < 0 || from < 0) || to <= from {
            return;
        }
        SourceRef::set(&r#ref, "main".to_string(), base + from, base + to);
        if body_level {
            doc.borrow().source.clone().borrow_mut().addRange(base + from, base + to);
        }
    }
    pub fn markLastSynthetic(&self, doc: &Rc<RefCell<RichDocument>>) {
        let n: i64 = doc.borrow().blocks.len() as i64;
        if n > 0 {
            let mut b: Rc<RefCell<DocBlock>> = doc.borrow().blocks[(n - 1) as usize].clone();
            b.borrow_mut().synthetic = true;
        }
    }
    pub fn maybeSectionFromPara(&mut self, doc: &Rc<RefCell<RichDocument>>, p_el: &str) {
        let p_pr: i64 = WordXml::findNext(p_el, "w:pPr", 0);
        if p_pr < 0 {
            return;
        }
        let p_pr_el: String = WordXml::sliceElement(p_el, "w:pPr", p_pr);
        let s_at: i64 = WordXml::findNext(&p_pr_el, "w:sectPr", 0);
        if s_at < 0 {
            return;
        }
        let s_el: String = WordXml::sliceElement(&p_pr_el, "w:sectPr", s_at);
        // Mid-document section properties: finalize current section, open next.
        self.applySectPr(&doc.borrow().sectionAt(self.currentSection).clone(), &s_el);
        let new_idx: i64 = doc.borrow_mut().addSectionBreak();
        self.markLastSynthetic(doc);
        self.currentSection = new_idx;
        doc.borrow_mut().section = doc.borrow().sectionAt(new_idx);
    }
    pub fn applySectPr(&self, sec: &Rc<RefCell<DocSection>>, s_el: &str) {
        let psz: i64 = WordXml::findNext(s_el, "w:pgSz", 0);
        if psz >= 0 {
            let psz_el: String = WordXml::sliceElement(s_el, "w:pgSz", psz);
            let ot: String = WordXml::openTag(&psz_el);
            let _tmp_3825 = WordXml::twipsAttr(&ot, "w:w", sec.borrow().pageWTwips.clone());
            sec.borrow_mut().pageWTwips = _tmp_3825;
            let _tmp_3827 = WordXml::twipsAttr(&ot, "w:h", sec.borrow().pageHTwips.clone());
            sec.borrow_mut().pageHTwips = _tmp_3827;
            let orient: String = WordXml::attr(&ot, "w:orient");
            if orient == "landscape" {
                sec.borrow_mut().landscape = true;
            }
            if sec.borrow().pageWTwips > sec.borrow().pageHTwips {
                sec.borrow_mut().landscape = true;
            }
        }
        let pm: i64 = WordXml::findNext(s_el, "w:pgMar", 0);
        if pm >= 0 {
            let pm_el: String = WordXml::sliceElement(s_el, "w:pgMar", pm);
            let ot2: String = WordXml::openTag(&pm_el);
            let _tmp_3829 = WordXml::twipsAttr(&ot2, "w:top", sec.borrow().marginTopTwips.clone());
            sec.borrow_mut().marginTopTwips = _tmp_3829;
            let _tmp_3831 = WordXml::twipsAttr(&ot2, "w:right", sec.borrow().marginRightTwips.clone());
            sec.borrow_mut().marginRightTwips = _tmp_3831;
            let _tmp_3833 = WordXml::twipsAttr(&ot2, "w:bottom", sec.borrow().marginBottomTwips.clone());
            sec.borrow_mut().marginBottomTwips = _tmp_3833;
            let _tmp_3835 = WordXml::twipsAttr(&ot2, "w:left", sec.borrow().marginLeftTwips.clone());
            sec.borrow_mut().marginLeftTwips = _tmp_3835;
        }
        let hr: i64 = WordXml::findNext(s_el, "w:headerReference", 0);
        if hr >= 0 {
            let hr_el: String = WordXml::sliceElement(s_el, "w:headerReference", hr);
            let _tmp_3837 = WordXml::attr(&WordXml::openTag(&hr_el), "r:id");
            sec.borrow_mut().headerRelId = _tmp_3837;
        }
        let fr: i64 = WordXml::findNext(s_el, "w:footerReference", 0);
        if fr >= 0 {
            let fr_el: String = WordXml::sliceElement(s_el, "w:footerReference", fr);
            let _tmp_3839 = WordXml::attr(&WordXml::openTag(&fr_el), "r:id");
            sec.borrow_mut().footerRelId = _tmp_3839;
        }
    }
    pub fn parseTable(&self, doc: &Rc<RefCell<RichDocument>>, t_el: &str, base: i64) -> Rc<RefCell<DocumentTable>> {
        let mut t: Rc<RefCell<DocumentTable>> = doc.borrow_mut().addTable();
        let grid: i64 = WordXml::findNext(t_el, "w:tblGrid", 0);
        if grid >= 0 {
            let g_el: String = WordXml::sliceElement(t_el, "w:tblGrid", grid);
            let mut gi: i64 = 0;
            let gn: i64 = DxNative::str_len(&g_el);
            while gi < gn {
                let gc: i64 = WordXml::findNext(&g_el, "w:gridCol", gi);
                if gc < 0 {
                    gi = gn;
                } else {
                    let gc_el: String = WordXml::sliceElement(&g_el, "w:gridCol", gc);
                    let w: i64 = WordXml::twipsAttr(&WordXml::openTag(&gc_el), "w:w", 1440);
                    t.borrow_mut().colWidthsTwips.push(w);
                    let ge: i64 = WordXml::elementEnd(&g_el, "w:gridCol", gc);
                    if ge < 0 {
                        gi = gn;
                    } else {
                        gi = ge;
                    }
                }
            }
        }
        let mut ri: i64 = 0;
        let tn: i64 = DxNative::str_len(&t_el);
        while ri < tn {
            let tr: i64 = WordXml::findNext(t_el, "w:tr", ri);
            if tr < 0 {
                return t;
            }
            let tr_el: String = WordXml::sliceElement(t_el, "w:tr", tr);
            let mut row: Rc<RefCell<DocTableRow>> = Rc::new(RefCell::new(DocTableRow::new()));
            let mut row_base: i64 = -1;
            if base >= 0 {
                row_base = base + tr;
                row.borrow().source.borrow_mut().set("main".to_string(), row_base, row_base + (DxNative::str_len(&tr_el)));
            }
            let mut ci: i64 = 0;
            let trn: i64 = DxNative::str_len(&tr_el);
            while ci < trn {
                let tc: i64 = WordXml::findNext(&tr_el, "w:tc", ci);
                if tc < 0 {
                    ci = trn;
                } else {
                    let tc_el: String = WordXml::sliceElement(&tr_el, "w:tc", tc);
                    let mut cell_base: i64 = -1;
                    if row_base >= 0 {
                        cell_base = row_base + tc;
                    }
                    let mut cell: Rc<RefCell<DocTableCell>> = self.parseCell(doc,
                        &tc_el,
                        cell_base
                    );
                    row.borrow_mut().cells.push(cell.clone());
                    let te: i64 = WordXml::elementEnd(&tr_el, "w:tc", tc);
                    if te < 0 {
                        ci = trn;
                    } else {
                        ci = te;
                    }
                }
            }
            t.borrow_mut().rows.push(row.clone());
            let tre: i64 = WordXml::elementEnd(t_el, "w:tr", tr);
            if tre < 0 {
                return t;
            }
            ri = tre;
        }
        if t.borrow().colWidthsTwips.is_empty() {
            // Infer columns from first row grid spans
            if !t.borrow().rows.is_empty() {
                let mut r0: Rc<RefCell<DocTableRow>> = t.borrow().rows[0].clone();
                let mut c: i64 = 0;
                while c < (r0.borrow().cells.len() as i64) {
                    let mut cell0: Rc<RefCell<DocTableCell>> = r0.borrow().cells[c as usize].clone();
                    let mut span: i64 = cell0.borrow().gridSpan;
                    if span < 1 {
                        span = 1;
                    }
                    let mut s: i64 = 0;
                    while s < span {
                        t.borrow_mut().colWidthsTwips.push(1440);
                        s += 1;
                    }
                    c += 1;
                }
            }
        }
        t
    }
    pub fn parseCell(&self, doc: &Rc<RefCell<RichDocument>>, tc_el: &str, base: i64) -> Rc<RefCell<DocTableCell>> {
        let mut cell: Rc<RefCell<DocTableCell>> = Rc::new(RefCell::new(DocTableCell::new()));
        if base >= 0 {
            cell.borrow().source.borrow_mut().set("main".to_string(), base, base + (DxNative::str_len(&tc_el)));
        }
        let tc_pr: i64 = WordXml::findNext(tc_el, "w:tcPr", 0);
        if tc_pr >= 0 {
            let pr: String = WordXml::sliceElement(tc_el, "w:tcPr", tc_pr);
            let gs: i64 = WordXml::findNext(&pr, "w:gridSpan", 0);
            if gs >= 0 {
                let gs_el: String = WordXml::sliceElement(&pr, "w:gridSpan", gs);
                let gv: Option<i64> = WordXml::attr(&WordXml::openTag(&gs_el), "w:val").parse::<i64>().ok();
                if gv.is_some() {
                    cell.borrow_mut().gridSpan = gv.unwrap();
                }
            }
            let shd: i64 = WordXml::findNext(&pr, "w:shd", 0);
            if shd >= 0 {
                let shd_el: String = WordXml::sliceElement(&pr, "w:shd", shd);
                let fill: String = WordXml::attr(&WordXml::openTag(&shd_el), "w:fill");
                if (DxNative::str_len(&fill)) == 6 {
                    cell.borrow_mut().fillHex = fill.clone();
                }
            }
            self.parseBorders(&pr, &cell);
        }
        let mut pi: i64 = 0;
        let n: i64 = DxNative::str_len(&tc_el);
        while pi < n {
            let p_start: i64 = WordXml::findNext(tc_el, "w:p", pi);
            if p_start < 0 {
                return cell;
            }
            // Skip paragraphs inside nested tables (viewer: flat cells only)
            let p_el: String = WordXml::sliceElement(tc_el, "w:p", p_start);
            let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraphOnly();
            self.fillParagraph(&p, &p_el);
            cell.borrow_mut().paragraphs.push(p.clone());
            let p_end: i64 = WordXml::elementEnd(tc_el, "w:p", p_start);
            if base >= 0 {
                self.setSource(doc, &p.borrow().source.clone(), base, p_start, p_end, false);
            }
            if p_end < 0 {
                return cell;
            }
            pi = p_end;
        }
        cell
    }
    pub fn parseBorders(&self, tc_pr: &str, cell: &Rc<RefCell<DocTableCell>>) {
        let tb: i64 = WordXml::findNext(tc_pr, "w:tcBorders", 0);
        if tb < 0 {
            return;
        }
        let el: String = WordXml::sliceElement(tc_pr, "w:tcBorders", tb);
        self.readBorder(&el, "w:top", &cell.borrow().borderTop.clone());
        self.readBorder(&el, "w:left", &cell.borrow().borderLeft.clone());
        self.readBorder(&el, "w:bottom", &cell.borrow().borderBottom.clone());
        self.readBorder(&el, "w:right", &cell.borrow().borderRight.clone());
    }
    pub fn readBorder(&self, xml: &str, tag: &str, into: &Rc<RefCell<DocTableBorder>>) {
        let at: i64 = WordXml::findNext(xml, tag, 0);
        if at < 0 {
            return;
        }
        let el: String = WordXml::sliceElement(xml, tag, at);
        let ot: String = WordXml::openTag(&el);
        into.borrow_mut().style = WordXml::attr(&ot, "w:val");
        let sz: String = WordXml::attr(&ot, "w:sz");
        if !sz.is_empty() {
            let zi: Option<i64> = sz.parse::<i64>().ok();
            if zi.is_some() {
                into.borrow_mut().sz = zi.unwrap();
            }
        }
        let col: String = WordXml::attr(&ot, "w:color");
        if (DxNative::str_len(&col)) == 6 {
            into.borrow_mut().colorHex = col.clone();
        }
    }
    pub fn fillParagraph(&self, p: &Rc<RefCell<Paragraph>>, p_el: &str) {
        let mut p_pr_xml: String = String::new();
        let p_pr: i64 = WordXml::findNext(p_el, "w:pPr", 0);
        if p_pr >= 0 {
            p_pr_xml = WordXml::sliceElement(p_el, "w:pPr", p_pr);
            let ps: i64 = WordXml::findNext(&p_pr_xml, "w:pStyle", 0);
            if ps >= 0 {
                let ps_el: String = WordXml::sliceElement(&p_pr_xml, "w:pStyle", ps);
                let _tmp_3852 = WordXml::attr(&WordXml::openTag(&ps_el), "w:val");
                p.borrow_mut().styleId = _tmp_3852;
            }
        }
        self.resolver.clone().borrow().resolveParagraph(p, &p_pr_xml);
        let mut base: Rc<RefCell<ResolvedRunStyle>> = self.resolver.borrow().baseRunFromParagraph(p);
        p.borrow_mut().spans.clear();
        p.borrow_mut().anchors.clear();
        p.borrow_mut().text = String::new();
        let mut ri: i64 = 0;
        let pn: i64 = DxNative::str_len(&p_el);
        while ri < pn {
            let r_start: i64 = WordXml::findNext(p_el, "w:r", ri);
            if r_start < 0 {
                return;
            }
            let r_el: String = WordXml::sliceElement(p_el, "w:r", r_start);
            if r_el.is_empty() {
                return;
            }
            let mut dr: i64 = WordXml::findNext(&r_el, "w:drawing", 0);
            if dr < 0 {
                dr = WordXml::findNext(&r_el, "w:pict", 0);
            }
            if dr >= 0 {
                let mut img: Rc<RefCell<DocImage>> = self.parseDrawing(&r_el);
                if !img.borrow().relId.is_empty() {
                    p.borrow_mut().images.push(img.clone());
                }
            }
            let mut run_style: Rc<RefCell<ResolvedRunStyle>> = base.borrow().copy();
            let r_pr: i64 = WordXml::findNext(&r_el, "w:rPr", 0);
            let mut r_pr_keep: String = String::new();
            if r_pr >= 0 {
                let r_pr_el: String = WordXml::sliceElement(&r_el, "w:rPr", r_pr);
                self.resolver.clone().borrow().applyRPr(&r_pr_el, &run_style);
                r_pr_keep = r_pr_el.clone();
            }
            self.collectAnchors(p, &r_el, &r_pr_keep, (DxNative::str_len(&p.borrow().text)));
            let mut chunk: String = String::new();
            let mut ti: i64 = 0;
            let rn: i64 = DxNative::str_len(&r_el);
            while ti < rn {
                let t_start: i64 = WordXml::findNext(&r_el, "w:t", ti);
                if t_start < 0 {
                    ti = rn;
                } else {
                    let t_el: String = WordXml::sliceElement(&r_el, "w:t", t_start);
                    let inner: String = WordXml::innerOf(&t_el, "w:t");
                    chunk = format!("{}{}", chunk, WordXml::decodeEntities(&inner));
                    let t_end: i64 = WordXml::elementEnd(&r_el, "w:t", t_start);
                    if t_end < 0 {
                        ti = rn;
                    } else {
                        ti = t_end;
                    }
                }
            }
            if !chunk.is_empty() {
                let mut span: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
                run_style.borrow().applyToSpan(&span);
                p.borrow_mut().appendText(&chunk, &span);
            }
            let r_end: i64 = WordXml::elementEnd(p_el, "w:r", r_start);
            if r_end < 0 {
                return;
            }
            ri = r_end;
        }
    }
    pub fn collectAnchors(&self, p: &Rc<RefCell<Paragraph>>, r_el: &str, r_pr_xml: &str, offset: i64) {
        let gt0: i64 = DxNative::index_of(&r_el, ">");
        if gt0 < 1 {
            return;
        }
        if DxNative::char_at(&r_el, gt0 - 1) == 47 {
            return;
        }
        let mut at: i64 = gt0 + 1;
        let n: i64 = DxNative::str_len(&r_el);
        let mut off: i64 = offset;
        while at < n {
            let lt: i64 = DxNative::index_of_from(&r_el, "<", at);
            if lt < 0 || lt + 1 >= n {
                return;
            }
            // The run's own closing tag: done.
            if DxNative::char_at(&r_el, lt + 1) == 47 {
                return;
            }
            let mut e: i64 = lt + 1;
            let mut stop: bool = false;
            while e < n && !stop {
                let ch: i64 = DxNative::char_at(&r_el, e);
                if ((((ch == 32 || ch == 47) || ch == 62) || ch == 9) || ch == 10) || ch == 13 {
                    stop = true;
                } else {
                    e += 1;
                }
            }
            let name: String = DxNative::substr(&r_el, lt + 1, e);
            let end_at: i64 = WordXml::elementEnd(r_el, &name, lt);
            if end_at <= lt {
                return;
            }
            if name == "w:t" {
                let tx: String = WordXml::decodeEntities(&WordXml::innerOf(&DxNative::substr(&r_el, lt, end_at), "w:t"));
                off += (DxNative::str_len(&tx));
            } else {
                if WordMlParser::anchorTag(&name) {
                    let mut a: Rc<RefCell<InlineAnchor>> = Rc::new(RefCell::new(InlineAnchor::new()));
                    a.borrow_mut().offset = off;
                    a.borrow_mut().xml = format!("<w:r>{}{}</w:r>", r_pr_xml, DxNative::substr(&r_el, lt, end_at));
                    p.borrow_mut().anchors.push(a.clone());
                }
            }
            at = end_at;
        }
    }
    pub fn parseParagraph(&self, doc: &Rc<RefCell<RichDocument>>, p_el: &str) {
        let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraph();
        self.fillParagraph(&p, p_el);
    }
    pub fn parseDrawing(&self, r_el: &str) -> Rc<RefCell<DocImage>> {
        let mut empty: Rc<RefCell<DocImage>> = Rc::new(RefCell::new(DocImage::new()));
        let mut blip_at: i64 = DxNative::index_of(&r_el, "<a:blip");
        if blip_at < 0 {
            blip_at = DxNative::index_of(&r_el, "<v:imagedata");
        }
        if blip_at < 0 {
            return empty;
        }
        let n: i64 = DxNative::str_len(&r_el);
        let mut gt: i64 = blip_at;
        while gt < n {
            if DxNative::char_at(&r_el, gt) == 62 {
                let tag: String = DxNative::substr(&r_el, blip_at, gt + 1);
                let mut rid: String = WordXml::attr(&tag, "r:embed");
                if rid.is_empty() {
                    rid = WordXml::attr(&tag, "r:id");
                }
                if rid.is_empty() {
                    return empty;
                }
                let mut img: Rc<RefCell<DocImage>> = Rc::new(RefCell::new(DocImage::new()));
                img.borrow_mut().relId = rid.clone();
                img.borrow_mut().partPath = self.pkg.borrow().resolveRel(&rid);
                // `wp:docPr` carries the author's alt text. It is the only
                // thing a screen reader can say about the picture, and it was
                // being read past.
                let mut doc_pr_at: i64 = DxNative::index_of(&r_el, "<wp:docPr");
                if doc_pr_at < 0 {
                    doc_pr_at = DxNative::index_of(&r_el, "<pic:cNvPr");
                }
                if doc_pr_at >= 0 {
                    let mut dg: i64 = doc_pr_at;
                    while dg < n {
                        if DxNative::char_at(&r_el, dg) == 62 {
                            let d_tag: String = DxNative::substr(&r_el, doc_pr_at, dg + 1);
                            img.borrow_mut().altText = WordXml::attr(&d_tag, "descr");
                            img.borrow_mut().title = WordXml::attr(&d_tag, "title");
                            if img.borrow().title.is_empty() {
                                img.borrow_mut().title = WordXml::attr(&d_tag, "name");
                            }
                            dg = n;
                        } else {
                            dg += 1;
                        }
                    }
                }
                let mut ext_at: i64 = DxNative::index_of(&r_el, "<wp:extent");
                if ext_at < 0 {
                    ext_at = DxNative::index_of(&r_el, "<a:ext");
                }
                if ext_at >= 0 {
                    let mut eg: i64 = ext_at;
                    while eg < n {
                        if DxNative::char_at(&r_el, eg) == 62 {
                            let e_tag: String = DxNative::substr(&r_el, ext_at, eg + 1);
                            let cx: String = WordXml::attr(&e_tag, "cx");
                            let cy: String = WordXml::attr(&e_tag, "cy");
                            let cxi: Option<i64> = cx.parse::<i64>().ok();
                            let cyi: Option<i64> = cy.parse::<i64>().ok();
                            if cxi.is_some() {
                                img.borrow_mut().widthEmu = cxi.unwrap();
                            }
                            if cyi.is_some() {
                                img.borrow_mut().heightEmu = cyi.unwrap();
                            }
                            eg = n;
                        } else {
                            eg += 1;
                        }
                    }
                }
                img.borrow_mut().syncPx();
                self.decodeImage(&img);
                return img;
            }
            gt += 1;
        }
        empty
    }
    pub fn decodeImage(&self, img: &Rc<RefCell<DocImage>>) {
        if img.borrow().partPath.is_empty() {
            return;
        }
        let _tmp_3864 = self.pkg.borrow().readPartBytes(&img.borrow().partPath);
        WordMlParser::decodeImageBytes(img, _tmp_3864);
    }
}
