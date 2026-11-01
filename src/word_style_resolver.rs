// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::rich_document::{Paragraph, TextSpan};
use crate::rgr::DxNative;
use crate::word_xml::WordXml;
use crate::rgr::{OfficeFont, OfficeTextStyle, StyleFlag};

/// ==============================================================================
/// WordStyleResolver - document defaults + style inheritance + direct formatting.
/// ==============================================================================
///
/// Chain:
///   docDefaults → basedOn inheritance → paragraph style → character style
///   → direct pPr → direct rPr
/// Loop guards prevent basedOn cycles. EVG only sees Resolved* styles.
/// ==============================================================================
#[derive(Clone)]
pub struct ResolvedRunStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub fontSizePt: f64,
    pub fontFamily: String,
    pub colorHex: String,
    pub strike: bool,
    pub vertAlign: i64,
    pub highlight: String,
}

impl ResolvedRunStyle {
    pub fn new() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            fontSizePt: 11.0,
            fontFamily: "Open Sans".to_string(),
            colorHex: "000000".to_string(),
            strike: false,
            vertAlign: 0,
            highlight: String::new(),
        }
    }
    pub fn copy(&self) -> Rc<RefCell<ResolvedRunStyle>> {
        let mut o: Rc<RefCell<ResolvedRunStyle>> = Rc::new(RefCell::new(ResolvedRunStyle::new()));
        o.borrow_mut().bold = self.bold;
        o.borrow_mut().italic = self.italic;
        o.borrow_mut().underline = self.underline;
        o.borrow_mut().fontSizePt = self.fontSizePt;
        o.borrow_mut().fontFamily = self.fontFamily.clone();
        o.borrow_mut().colorHex = self.colorHex.clone();
        o.borrow_mut().strike = self.strike;
        o.borrow_mut().vertAlign = self.vertAlign;
        o.borrow_mut().highlight = self.highlight.clone();
        o
    }
    fn applyToSpan(&self, span: &Rc<RefCell<TextSpan>>) {
        span.borrow_mut().bold = self.bold;
        span.borrow_mut().italic = self.italic;
        span.borrow_mut().underline = self.underline;
        span.borrow_mut().fontSizePt = self.fontSizePt;
        span.borrow_mut().fontFamily = self.fontFamily.clone();
        span.borrow_mut().colorHex = self.colorHex.clone();
        span.borrow_mut().strike = self.strike;
        span.borrow_mut().vertAlign = self.vertAlign;
        span.borrow_mut().highlight = self.highlight.clone();
    }
    pub fn mergeFrom(&mut self, other: &Rc<RefCell<ResolvedRunStyle>>) {
        // later wins for set fields — caller passes incremental layers
        self.bold = other.borrow().bold;
        self.italic = other.borrow().italic;
        self.underline = other.borrow().underline;
        self.fontSizePt = other.borrow().fontSizePt;
        self.fontFamily = other.borrow().fontFamily.clone();
        self.colorHex = other.borrow().colorHex.clone();
        self.strike = other.borrow().strike;
        self.vertAlign = other.borrow().vertAlign;
        self.highlight = other.borrow().highlight.clone();
    }
}

#[derive(Clone)]
pub struct ResolvedParagraphStyle {
    pub align: i64,
    /// Was the alignment actually stated, or is `align` just the default? An
    /// unaligned right-to-left paragraph belongs against the RIGHT margin, and
    /// that can only be decided if "nobody said" is distinguishable from
    /// "somebody said left".
    pub alignSet: bool,
    /// -1 nobody said, 0 left to right, 1 right to left.
    pub bidi: i64,
    pub spaceBeforePt: f64,
    pub spaceAfterPt: f64,
    pub lineSpacing: f64,
    pub indentLeftPt: f64,
    pub indentRightPt: f64,
    pub indentFirstLinePt: f64,
    pub run: Rc<RefCell<ResolvedRunStyle>>,
}

impl ResolvedParagraphStyle {
    pub fn new() -> Self {
        Self {
            align: 0,
            alignSet: false,
            bidi: -1,
            spaceBeforePt: 0.0,
            spaceAfterPt: 8.0,
            lineSpacing: 1.15,
            indentLeftPt: 0.0,
            indentRightPt: 0.0,
            indentFirstLinePt: 0.0,
            run: Rc::new(RefCell::new(ResolvedRunStyle::new())),
        }
    }
}

#[derive(Clone)]
pub struct StyleDef {
    pub styleId: String,
    pub basedOn: String,
    pub typ: String,
    pub align: i64,
    pub bidi: i64,
    pub spaceBeforePt: f64,
    pub spaceAfterPt: f64,
    pub lineSpacing: f64,
    pub indentLeftPt: f64,
    pub indentRightPt: f64,
    pub indentFirstLinePt: f64,
    pub run: Rc<RefCell<ResolvedRunStyle>>,
    pub hasRun: bool,
    pub hasPPr: bool,
    pub hasBold: bool,
    pub hasItalic: bool,
    pub hasUnderline: bool,
    pub hasSize: bool,
    pub hasFont: bool,
    pub hasColor: bool,
}

impl StyleDef {
    pub fn new() -> Self {
        Self {
            styleId: String::new(),
            basedOn: String::new(),
            typ: "paragraph".to_string(),
            align: -1,
            bidi: -1,
            spaceBeforePt: -1.0,
            spaceAfterPt: -1.0,
            lineSpacing: -1.0,
            indentLeftPt: -1.0,
            indentRightPt: -1.0,
            indentFirstLinePt: -1.0,
            run: Rc::new(RefCell::new(ResolvedRunStyle::new())),
            hasRun: false,
            hasPPr: false,
            hasBold: false,
            hasItalic: false,
            hasUnderline: false,
            hasSize: false,
            hasFont: false,
            hasColor: false,
        }
    }
}

#[derive(Clone)]
pub struct WordStyleResolver {
    pub styles: HashMap<String,Rc<RefCell<StyleDef>>>,
    pub defaults: Rc<RefCell<ResolvedRunStyle>>,
    pub defaultAfterPt: f64,
    pub defaultLineSpacing: f64,
    pub defaultAlign: i64,
    /// The theme's Latin faces (word/theme/theme1.xml): what `w:asciiTheme=
    /// "minorHAnsi"` and its kin name. Word's own documents set body text and
    /// headings this way rather than by name.
    pub majorFont: String,
    pub minorFont: String,
}

impl WordStyleResolver {
    pub fn new() -> Self {
        let mut me = Self {
            styles: HashMap::new(),
            defaults: Rc::new(RefCell::new(ResolvedRunStyle::new())),
            defaultAfterPt: 8.0,
            defaultLineSpacing: 1.15,
            defaultAlign: 0,
            majorFont: String::new(),
            minorFont: String::new(),
        };
        me.defaults.borrow_mut().fontSizePt = 11.0;
        me.defaults.borrow_mut().fontFamily = "Open Sans".to_string();
        me.defaults.borrow_mut().colorHex = "000000".to_string();
        me
    }
    /// Which family this reader draws a named one with.
    ///
    /// This used to be three hardcoded cases — Cinzel, Josefin, everything else
    /// is Open Sans — beside a shared alias map the spreadsheet was already
    /// using. So a Times New Roman paragraph was Open Sans here and Droid Serif
    /// in the grid, and `gallery/office/README.md` claimed the opposite: "one
    /// map means a document opened in two of these editors looks the same in
    /// both". It does now.
    ///
    /// The shared map already knew Cinzel and Josefin Sans, so nothing this
    /// reader could already do was lost.
    ///
    /// Cambria and Calibri, Word's default faces, go to Caladea and Carlito:
    /// their metric-compatible open (SIL OFL) twins, glyph for glyph the same
    /// advance widths, so a line breaks where Word breaks it. A host that has
    /// not loaded them falls back to Open Sans (`DocxTextMetrics.loadedFamily`).
    pub fn mapFont(name: &str) -> String {
        let n: String = name.trim().to_string().to_lowercase();
        if n == "cambria" || n == "caladea" {
            return "Caladea".to_string();
        }
        if n == "calibri" || n == "carlito" {
            return "Carlito".to_string();
        }
        OfficeFont::aliasFamily(name)
    }
    pub fn themeLatin(xml: &str, tag: &str) -> String {
        let at: i64 = WordXml::findNext(xml, tag, 0);
        if at < 0 {
            return String::new();
        }
        let el: String = WordXml::sliceElement(xml, tag, at);
        let la: i64 = WordXml::findNext(&el, "a:latin", 0);
        if la < 0 {
            return String::new();
        }
        let _tmp_1309 = WordXml::openTag(&WordXml::sliceElement(&el, "a:latin", la));
        WordXml::attr(&_tmp_1309, "typeface")
    }
    pub fn halfPointsToPt(sz: &str) -> f64 {
        if sz.is_empty() {
            return -1.0;
        }
        let v: Option<i64> = sz.parse::<i64>().ok();
        if v.is_none() {
            return -1.0;
        }
        ((v.unwrap()) as f64) / 2.0
    }
    pub fn alignOf(jc: &str) -> i64 {
        if jc == "center" {
            return 1;
        }
        if jc == "right" {
            return 2;
        }
        if jc == "both" {
            return 3;
        }
        if jc == "left" {
            return 0;
        }
        -1
    }
    /// A Word toggle element: <w:bidi/> means on, <w:bidi w:val="0"/> means
    /// off, and absent means "inherit whatever the style said". Returning -1
    /// for absent is what makes a style overridable — a paragraph that says
    /// nothing must not silently mean "left to right" and cancel the
    /// right-to-left style it is based on.
    pub fn toggleOf(xml: &str, name: &str) -> i64 {
        let at: i64 = WordXml::findNext(xml, name, 0);
        if at < 0 {
            return -1;
        }
        let el: String = WordXml::sliceElement(xml, name, at);
        let v: String = WordXml::attr(&WordXml::openTag(&el), "w:val");
        if v.is_empty() {
            return 1;
        }
        if (v == "0" || v == "false") || v == "off" {
            return 0;
        }
        1
    }
    /// A run's `w:rPr` as a style that knows what it did NOT say.
    ///
    /// `<w:b/>` and `<w:b w:val="1"/>` state bold. `<w:b w:val="0"/>` states NOT
    /// bold. No `<w:b>` at all states nothing, and only then may the paragraph's
    /// style decide. Those are three answers and this used to have two of them:
    ///
    ///     if (hasFlag "w:b") { into.bold = true }
    ///
    /// — which turns a run that explicitly un-bolds itself into a run that said
    /// nothing, so a word taken out of bold inside a bold heading drew bold.
    /// Underline was already written with the distinction in mind, which is why
    /// only bold and italic were wrong.
    fn runStyleOf(xml: &str) -> Rc<RefCell<OfficeTextStyle>> {
        let mut st: Rc<RefCell<OfficeTextStyle>> = Rc::new(RefCell::new(OfficeTextStyle::new()));
        if WordXml::findNext(xml, "w:b", 0) >= 0 {
            st.borrow().bold.borrow_mut().set(WordXml::hasFlag(xml, "w:b"));
        }
        if WordXml::findNext(xml, "w:i", 0) >= 0 {
            st.borrow().italic.borrow_mut().set(WordXml::hasFlag(xml, "w:i"));
        }
        if WordXml::findNext(xml, "w:strike", 0) >= 0 {
            st.borrow().strike.borrow_mut().set(WordXml::hasFlag(xml, "w:strike"));
        }
        let u: i64 = WordXml::findNext(xml, "w:u", 0);
        if u >= 0 {
            let u_el: String = WordXml::sliceElement(xml, "w:u", u);
            let uv: String = WordXml::attr(&WordXml::openTag(&u_el), "w:val");
            // `w:u` says WHICH underline, so "none" is its way of saying off —
            // unlike `w:b`, which says whether.
            st.borrow().underline.borrow_mut().set((uv != "none" && uv != "0") && uv != "false");
        }
        st
    }
    /// Read the theme's major and minor Latin typefaces.
    pub fn parseThemeXml(&mut self, xml: &str) {
        self.majorFont = WordStyleResolver::themeLatin(xml, "a:majorFont");
        self.minorFont = WordStyleResolver::themeLatin(xml, "a:minorFont");
    }
    /// The family a <w:rFonts> names: w:ascii, then w:hAnsi, then the theme
    /// face its w:asciiTheme / w:hAnsiTheme points at. "" when it names none
    /// of them (an rFonts that sets only the East Asian face).
    pub fn rFontsName(&self, ot: &str) -> String {
        let mut ascii: String = WordXml::attr(ot, "w:ascii");
        if ascii.is_empty() {
            ascii = WordXml::attr(ot, "w:hAnsi");
        }
        if !ascii.is_empty() {
            return ascii;
        }
        let mut th: String = WordXml::attr(ot, "w:asciiTheme");
        if th.is_empty() {
            th = WordXml::attr(ot, "w:hAnsiTheme");
        }
        if DxNative::index_of(&th, "major") == 0 {
            return self.majorFont.clone();
        }
        if DxNative::index_of(&th, "minor") == 0 {
            return self.minorFont.clone();
        }
        String::new()
    }
    pub fn parseStylesXml(&mut self, xml: &str) {
        let dd: i64 = WordXml::findNext(xml, "w:docDefaults", 0);
        if dd >= 0 {
            let dd_el: String = WordXml::sliceElement(xml, "w:docDefaults", dd);
            let r_def: i64 = WordXml::findNext(&dd_el, "w:rPrDefault", 0);
            if r_def >= 0 {
                let r_def_el: String = WordXml::sliceElement(&dd_el, "w:rPrDefault", r_def);
                self.applyRPr(&r_def_el, &self.defaults.clone());
            } else {
                self.applyRPr(&dd_el, &self.defaults.clone());
            }
            let p_def: i64 = WordXml::findNext(&dd_el, "w:pPrDefault", 0);
            if p_def >= 0 {
                let p_def_el: String = WordXml::sliceElement(&dd_el, "w:pPrDefault", p_def);
                let sp: i64 = WordXml::findNext(&p_def_el, "w:spacing", 0);
                if sp >= 0 {
                    let sp_el: String = WordXml::sliceElement(&p_def_el, "w:spacing", sp);
                    let ot: String = WordXml::openTag(&sp_el);
                    let av: String = WordXml::attr(&ot, "w:after");
                    if !av.is_empty() {
                        let ai: Option<i64> = av.parse::<i64>().ok();
                        if ai.is_some() {
                            self.defaultAfterPt = ((ai.unwrap()) as f64) / 20.0;
                        }
                    }
                    let line: String = WordXml::attr(&ot, "w:line");
                    let rule: String = WordXml::attr(&ot, "w:lineRule");
                    if !line.is_empty() {
                        let li: Option<i64> = line.parse::<i64>().ok();
                        if li.is_some() {
                            if rule == "auto" || rule.is_empty() {
                                self.defaultLineSpacing = ((li.unwrap()) as f64) / 240.0;
                            }
                        }
                    }
                }
                let jc: i64 = WordXml::findNext(&p_def_el, "w:jc", 0);
                if jc >= 0 {
                    let jc_el: String = WordXml::sliceElement(&p_def_el, "w:jc", jc);
                    let a: i64 = WordStyleResolver::alignOf(&WordXml::attr(&WordXml::openTag(&jc_el), "w:val"));
                    if a >= 0 {
                        self.defaultAlign = a;
                    }
                }
            }
        }
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&xml);
        while i < n {
            let s: i64 = WordXml::findNext(xml, "w:style", i);
            if s < 0 {
                return;
            }
            let el: String = WordXml::sliceElement(xml, "w:style", s);
            if el.is_empty() {
                return;
            }
            let ot2: String = WordXml::openTag(&el);
            let sid: String = WordXml::attr(&ot2, "w:styleId");
            let typ: String = WordXml::attr(&ot2, "w:type");
            if !sid.is_empty() {
                if (typ == "paragraph" || typ == "character") || typ.is_empty() {
                    let mut defn: Rc<RefCell<StyleDef>> = Rc::new(RefCell::new(StyleDef::new()));
                    defn.borrow_mut().styleId = sid.clone();
                    defn.borrow_mut().typ = typ.clone();
                    if typ.is_empty() {
                        defn.borrow_mut().typ = "paragraph".to_string();
                    }
                    defn.borrow_mut().basedOn = WordXml::attr(&ot2, "w:basedOn");
                    let bo: i64 = WordXml::findNext(&el, "w:basedOn", 0);
                    if bo >= 0 {
                        let bo_el: String = WordXml::sliceElement(&el, "w:basedOn", bo);
                        let _tmp_1324 = WordXml::attr(&WordXml::openTag(&bo_el), "w:val");
                        defn.borrow_mut().basedOn = _tmp_1324;
                    }
                    let p_pr: i64 = WordXml::findNext(&el, "w:pPr", 0);
                    if p_pr >= 0 {
                        let p_el: String = WordXml::sliceElement(&el, "w:pPr", p_pr);
                        self.applyPPrToStyleDef(&p_el, &defn);
                        defn.borrow_mut().hasPPr = true;
                    }
                    let r_pr: i64 = WordXml::findNext(&el, "w:rPr", 0);
                    if r_pr >= 0 {
                        let r_el: String = WordXml::sliceElement(&el, "w:rPr", r_pr);
                        defn.borrow_mut().run = self.defaults.borrow().copy();
                        self.applyRPrToStyleDef(&r_el, &defn);
                        defn.borrow_mut().hasRun = true;
                    }
                    self.styles.insert(sid.clone(), defn.clone());
                }
            }
            i = s + 1;
        }
    }
    pub fn applyPPrToStyleDef(&self, p_el: &str, defn: &Rc<RefCell<StyleDef>>) {
        let jc: i64 = WordXml::findNext(p_el, "w:jc", 0);
        if jc >= 0 {
            let jc_el: String = WordXml::sliceElement(p_el, "w:jc", jc);
            let _tmp_1328 = WordStyleResolver::alignOf(&WordXml::attr(&WordXml::openTag(&jc_el), "w:val"));
            defn.borrow_mut().align = _tmp_1328;
        }
        // A style can be the right-to-left one — which is how a whole Arabic
        // document is usually written, rather than by marking each paragraph.
        defn.borrow_mut().bidi = WordStyleResolver::toggleOf(p_el, "w:bidi");
        let sp2: i64 = WordXml::findNext(p_el, "w:spacing", 0);
        if sp2 >= 0 {
            let sp_el2: String = WordXml::sliceElement(p_el, "w:spacing", sp2);
            let sot: String = WordXml::openTag(&sp_el2);
            let bef: String = WordXml::attr(&sot, "w:before");
            let aft: String = WordXml::attr(&sot, "w:after");
            if !bef.is_empty() {
                let bi: Option<i64> = bef.parse::<i64>().ok();
                if bi.is_some() {
                    defn.borrow_mut().spaceBeforePt = ((bi.unwrap()) as f64) / 20.0;
                }
            }
            if !aft.is_empty() {
                let ai2: Option<i64> = aft.parse::<i64>().ok();
                if ai2.is_some() {
                    defn.borrow_mut().spaceAfterPt = ((ai2.unwrap()) as f64) / 20.0;
                }
            }
            let line: String = WordXml::attr(&sot, "w:line");
            let rule: String = WordXml::attr(&sot, "w:lineRule");
            if !line.is_empty() {
                let li: Option<i64> = line.parse::<i64>().ok();
                if li.is_some() {
                    if rule == "auto" || rule.is_empty() {
                        defn.borrow_mut().lineSpacing = ((li.unwrap()) as f64) / 240.0;
                    }
                }
            }
        }
        let ind: i64 = WordXml::findNext(p_el, "w:ind", 0);
        if ind >= 0 {
            let ind_el: String = WordXml::sliceElement(p_el, "w:ind", ind);
            let iot: String = WordXml::openTag(&ind_el);
            let left: String = WordXml::attr(&iot, "w:left");
            let right: String = WordXml::attr(&iot, "w:right");
            let first: String = WordXml::attr(&iot, "w:firstLine");
            let hang: String = WordXml::attr(&iot, "w:hanging");
            if !left.is_empty() {
                let lv: Option<i64> = left.parse::<i64>().ok();
                if lv.is_some() {
                    defn.borrow_mut().indentLeftPt = ((lv.unwrap()) as f64) / 20.0;
                }
            }
            if !right.is_empty() {
                let rv: Option<i64> = right.parse::<i64>().ok();
                if rv.is_some() {
                    defn.borrow_mut().indentRightPt = ((rv.unwrap()) as f64) / 20.0;
                }
            }
            if !first.is_empty() {
                let fv: Option<i64> = first.parse::<i64>().ok();
                if fv.is_some() {
                    defn.borrow_mut().indentFirstLinePt = ((fv.unwrap()) as f64) / 20.0;
                }
            }
            if !hang.is_empty() {
                let hv: Option<i64> = hang.parse::<i64>().ok();
                if hv.is_some() {
                    defn.borrow_mut().indentFirstLinePt = 0.0 - ((hv.unwrap()) as f64) / 20.0;
                }
            }
        }
    }
    pub fn applyRPrToStyleDef(&self, xml: &str, defn: &Rc<RefCell<StyleDef>>) {
        if WordXml::findNext(xml, "w:b", 0) >= 0 {
            defn.borrow_mut().hasBold = true;
            defn.borrow_mut().run.borrow_mut().bold = WordXml::hasFlag(xml, "w:b");
        }
        if WordXml::findNext(xml, "w:i", 0) >= 0 {
            defn.borrow_mut().hasItalic = true;
            defn.borrow_mut().run.borrow_mut().italic = WordXml::hasFlag(xml, "w:i");
        }
        if WordXml::findNext(xml, "w:u", 0) >= 0 {
            defn.borrow_mut().hasUnderline = true;
            let u_el: String = WordXml::sliceElement(xml, "w:u", WordXml::findNext(xml, "w:u", 0));
            let uv: String = WordXml::attr(&WordXml::openTag(&u_el), "w:val");
            if (uv == "none" || uv == "0") || uv == "false" {
                defn.borrow_mut().run.borrow_mut().underline = false;
            } else {
                defn.borrow_mut().run.borrow_mut().underline = true;
            }
        }
        let sz: i64 = WordXml::findNext(xml, "w:sz", 0);
        if sz >= 0 {
            let sz_el: String = WordXml::sliceElement(xml, "w:sz", sz);
            let pt: f64 = WordStyleResolver::halfPointsToPt(&WordXml::attr(&WordXml::openTag(&sz_el), "w:val"));
            if pt > 0.0 {
                defn.borrow_mut().hasSize = true;
                defn.borrow_mut().run.borrow_mut().fontSizePt = pt;
            }
        }
        let rf: i64 = WordXml::findNext(xml, "w:rFonts", 0);
        if rf >= 0 {
            let rf_el: String = WordXml::sliceElement(xml, "w:rFonts", rf);
            let ascii: String = self.rFontsName(&WordXml::openTag(&rf_el));
            if !ascii.is_empty() {
                defn.borrow_mut().hasFont = true;
                defn.borrow_mut().run.borrow_mut().fontFamily = WordStyleResolver::mapFont(&ascii);
            }
        }
        let col: i64 = WordXml::findNext(xml, "w:color", 0);
        if col >= 0 {
            let c_el: String = WordXml::sliceElement(xml, "w:color", col);
            let hex: String = WordXml::attr(&WordXml::openTag(&c_el), "w:val");
            if (DxNative::str_len(&hex)) == 6 {
                defn.borrow_mut().hasColor = true;
                defn.borrow_mut().run.borrow_mut().colorHex = hex.clone();
            }
        }
    }
    pub fn mergeStyleRun(&self, into: &Rc<RefCell<ResolvedRunStyle>>, defn: &Rc<RefCell<StyleDef>>) {
        if defn.borrow().hasBold {
            into.borrow_mut().bold = defn.borrow().run.borrow().bold;
        }
        if defn.borrow().hasItalic {
            into.borrow_mut().italic = defn.borrow().run.borrow().italic;
        }
        if defn.borrow().hasUnderline {
            into.borrow_mut().underline = defn.borrow().run.borrow().underline;
        }
        if defn.borrow().hasSize {
            into.borrow_mut().fontSizePt = defn.borrow().run.borrow().fontSizePt;
        }
        if defn.borrow().hasFont {
            into.borrow_mut().fontFamily = defn.borrow().run.borrow().fontFamily.clone();
        }
        if defn.borrow().hasColor {
            into.borrow_mut().colorHex = defn.borrow().run.borrow().colorHex.clone();
        }
    }
    pub fn applyRPr(&self, xml: &str, into: &Rc<RefCell<ResolvedRunStyle>>) {
        // Bold, italic, strike and underline through the shared style, which
        // carries "was this stated?" alongside the value. Everything below is
        // already stated-or-not by construction — a `w:sz` that is not there
        // leaves the size alone — so it stays as it is.
        let mut stated: Rc<RefCell<OfficeTextStyle>> = WordStyleResolver::runStyleOf(xml);
        if stated.borrow().bold.borrow().isSet {
            into.borrow_mut().bold = stated.borrow().bold.borrow().value;
        }
        if stated.borrow().italic.borrow().isSet {
            into.borrow_mut().italic = stated.borrow().italic.borrow().value;
        }
        if stated.borrow().underline.borrow().isSet {
            into.borrow_mut().underline = stated.borrow().underline.borrow().value;
        }
        if stated.borrow().strike.borrow().isSet {
            into.borrow_mut().strike = stated.borrow().strike.borrow().value;
        }
        let va: i64 = WordXml::findNext(xml, "w:vertAlign", 0);
        if va >= 0 {
            let va_el: String = WordXml::sliceElement(xml, "w:vertAlign", va);
            let vv: String = WordXml::attr(&WordXml::openTag(&va_el), "w:val");
            into.borrow_mut().vertAlign = 0;
            if vv == "superscript" {
                into.borrow_mut().vertAlign = 1;
            }
            if vv == "subscript" {
                into.borrow_mut().vertAlign = 2;
            }
        }
        let hl: i64 = WordXml::findNext(xml, "w:highlight", 0);
        if hl >= 0 {
            let hl_el: String = WordXml::sliceElement(xml, "w:highlight", hl);
            let mut hv: String = WordXml::attr(&WordXml::openTag(&hl_el), "w:val");
            if hv == "none" {
                hv = String::new();
            }
            into.borrow_mut().highlight = hv.clone();
        }
        let sz: i64 = WordXml::findNext(xml, "w:sz", 0);
        if sz >= 0 {
            let sz_el: String = WordXml::sliceElement(xml, "w:sz", sz);
            let pt: f64 = WordStyleResolver::halfPointsToPt(&WordXml::attr(&WordXml::openTag(&sz_el), "w:val"));
            if pt > 0.0 {
                into.borrow_mut().fontSizePt = pt;
            }
        }
        let rf: i64 = WordXml::findNext(xml, "w:rFonts", 0);
        if rf >= 0 {
            let rf_el: String = WordXml::sliceElement(xml, "w:rFonts", rf);
            let ascii: String = self.rFontsName(&WordXml::openTag(&rf_el));
            if !ascii.is_empty() {
                into.borrow_mut().fontFamily = WordStyleResolver::mapFont(&ascii);
            }
        }
        let col: i64 = WordXml::findNext(xml, "w:color", 0);
        if col >= 0 {
            let c_el: String = WordXml::sliceElement(xml, "w:color", col);
            let hex: String = WordXml::attr(&WordXml::openTag(&c_el), "w:val");
            if (DxNative::str_len(&hex)) == 6 {
                into.borrow_mut().colorHex = hex.clone();
            }
        }
        // character style reference
        let r_style: i64 = WordXml::findNext(xml, "w:rStyle", 0);
        if r_style >= 0 {
            let rs_el: String = WordXml::sliceElement(xml, "w:rStyle", r_style);
            let rid: String = WordXml::attr(&WordXml::openTag(&rs_el), "w:val");
            if !rid.is_empty() {
                let mut char_run: Rc<RefCell<ResolvedRunStyle>> = self.resolveCharacterStyle(&rid);
                into.borrow_mut().mergeFrom(&char_run);
            }
        }
    }
    pub fn lookup(&self, style_id: &str) -> Rc<RefCell<StyleDef>> {
        let mut hit: Option<Rc<RefCell<StyleDef>>> = self.styles.get(style_id).cloned();
        if hit.is_some() {
            return hit.clone().unwrap();
        }
        Rc::new(RefCell::new(StyleDef::new()))
    }
    /// Walk basedOn chain (root → leaf), guarding against cycles.
    pub fn basedOnChain(&self, style_id: &str, mut out: &mut Vec<String>) {
        let mut seen: HashMap<String,i64> = HashMap::new();
        let mut cur: String = style_id.to_string();
        let mut guard: i64 = 0;
        while !cur.is_empty() {
            if guard > 32 {
                return;
            }
            guard += 1;
            let hit: Option<i64> = seen.get(&cur).cloned();
            if hit.is_some() {
                return;
            }
            seen.insert(cur.clone(), 1);
            out.push(cur.clone());
            let mut defn: Rc<RefCell<StyleDef>> = self.lookup(&cur);
            cur = defn.borrow().basedOn.clone();
        }
    }
    pub fn resolveCharacterStyle(&self, style_id: &str) -> Rc<RefCell<ResolvedRunStyle>> {
        let mut chain: Vec<String> = Vec::new();
        self.basedOnChain(style_id, &mut chain);
        let mut run: Rc<RefCell<ResolvedRunStyle>> = self.defaults.borrow().copy();
        // apply root → leaf
        let mut rev: Vec<String> = Vec::new();
        let mut i: i64 = (chain.len() as i64) - 1;
        while i >= 0 {
            rev.push(chain[i as usize].clone());
            i -= 1;
        }
        let mut j: i64 = 0;
        while j < (rev.len() as i64) {
            let sid: String = rev[j as usize].clone();
            let mut defn: Rc<RefCell<StyleDef>> = self.lookup(&sid);
            if defn.borrow().hasRun {
                self.mergeStyleRun(&run, &defn);
            }
            j += 1;
        }
        run
    }
    pub fn resolveParagraphStyle(&self, mut style_id: String) -> Rc<RefCell<ResolvedParagraphStyle>> {
        let mut ps: Rc<RefCell<ResolvedParagraphStyle>> = Rc::new(RefCell::new(ResolvedParagraphStyle::new()));
        ps.borrow_mut().align = self.defaultAlign;
        ps.borrow_mut().spaceAfterPt = self.defaultAfterPt;
        ps.borrow_mut().lineSpacing = self.defaultLineSpacing;
        ps.borrow_mut().run = self.defaults.borrow().copy();
        let mut chain: Vec<String> = Vec::new();
        if style_id.is_empty() {
            style_id = "Normal".to_string();
        }
        self.basedOnChain(&style_id, &mut chain);
        // root → leaf
        let mut rev: Vec<String> = Vec::new();
        let mut i: i64 = (chain.len() as i64) - 1;
        while i >= 0 {
            rev.push(chain[i as usize].clone());
            i -= 1;
        }
        let mut j: i64 = 0;
        while j < (rev.len() as i64) {
            let sid: String = rev[j as usize].clone();
            let mut defn: Rc<RefCell<StyleDef>> = self.lookup(&sid);
            if defn.borrow().align >= 0 {
                ps.borrow_mut().align = defn.borrow().align;
                ps.borrow_mut().alignSet = true;
            }
            if defn.borrow().bidi >= 0 {
                ps.borrow_mut().bidi = defn.borrow().bidi;
            }
            if defn.borrow().spaceBeforePt >= 0.0 {
                ps.borrow_mut().spaceBeforePt = defn.borrow().spaceBeforePt;
            }
            if defn.borrow().spaceAfterPt >= 0.0 {
                ps.borrow_mut().spaceAfterPt = defn.borrow().spaceAfterPt;
            }
            if defn.borrow().lineSpacing >= 0.0 {
                ps.borrow_mut().lineSpacing = defn.borrow().lineSpacing;
            }
            if defn.borrow().indentLeftPt >= 0.0 {
                ps.borrow_mut().indentLeftPt = defn.borrow().indentLeftPt;
            }
            if defn.borrow().indentRightPt >= 0.0 {
                ps.borrow_mut().indentRightPt = defn.borrow().indentRightPt;
            }
            // first-line can be negative (hanging)
            if defn.borrow().indentFirstLinePt != -1.0 {
                if defn.borrow().indentFirstLinePt != 0.0 || defn.borrow().hasPPr {
                    // Only override when style actually set indent (use != -1 sentinel)
                }
            }
            if defn.borrow().indentFirstLinePt > -100000.0 {
                if defn.borrow().indentLeftPt >= 0.0 || defn.borrow().indentFirstLinePt != -1.0 {
                    if defn.borrow().indentFirstLinePt != -1.0 {
                        ps.borrow_mut().indentFirstLinePt = defn.borrow().indentFirstLinePt;
                    }
                }
            }
            if defn.borrow().hasRun {
                self.mergeStyleRun(&ps.borrow().run.clone(), &defn);
            }
            j += 1;
        }
        ps
    }
    fn applyDirectPPr(&self, p_pr_xml: &str, ps: &Rc<RefCell<ResolvedParagraphStyle>>, p: &Rc<RefCell<Paragraph>>) {
        if p_pr_xml.is_empty() {
            return;
        }
        // The paragraph's own direction beats the style's, and is read BEFORE
        // the alignment because `w:jc` values "start" and "end" mean opposite
        // margins depending on it.
        let b: i64 = WordStyleResolver::toggleOf(p_pr_xml, "w:bidi");
        if b >= 0 {
            ps.borrow_mut().bidi = b;
        }
        let jc: i64 = WordXml::findNext(p_pr_xml, "w:jc", 0);
        if jc >= 0 {
            let jc_el: String = WordXml::sliceElement(p_pr_xml, "w:jc", jc);
            let val: String = WordXml::attr(&WordXml::openTag(&jc_el), "w:val");
            let mut a: i64 = WordStyleResolver::alignOf(&val);
            // "start" and "end" are the direction-relative names, and they are
            // what Word writes in the strict flavour of the format. Reading
            // them as left and right puts every right-to-left paragraph
            // against the wrong margin.
            if val == "start" {
                a = 0;
                if ps.borrow().bidi == 1 {
                    a = 2;
                }
            }
            if val == "end" {
                a = 2;
                if ps.borrow().bidi == 1 {
                    a = 0;
                }
            }
            if a >= 0 {
                ps.borrow_mut().align = a;
                ps.borrow_mut().alignSet = true;
            }
        }
        let sp: i64 = WordXml::findNext(p_pr_xml, "w:spacing", 0);
        if sp >= 0 {
            let sp_el: String = WordXml::sliceElement(p_pr_xml, "w:spacing", sp);
            let sot: String = WordXml::openTag(&sp_el);
            let bef: String = WordXml::attr(&sot, "w:before");
            let aft: String = WordXml::attr(&sot, "w:after");
            if !bef.is_empty() {
                let bi: Option<i64> = bef.parse::<i64>().ok();
                if bi.is_some() {
                    ps.borrow_mut().spaceBeforePt = ((bi.unwrap()) as f64) / 20.0;
                }
            }
            if !aft.is_empty() {
                let ai: Option<i64> = aft.parse::<i64>().ok();
                if ai.is_some() {
                    ps.borrow_mut().spaceAfterPt = ((ai.unwrap()) as f64) / 20.0;
                }
            }
            let line: String = WordXml::attr(&sot, "w:line");
            let rule: String = WordXml::attr(&sot, "w:lineRule");
            if !line.is_empty() {
                let li: Option<i64> = line.parse::<i64>().ok();
                if li.is_some() {
                    if rule == "auto" || rule.is_empty() {
                        ps.borrow_mut().lineSpacing = ((li.unwrap()) as f64) / 240.0;
                    }
                }
            }
        }
        let ind: i64 = WordXml::findNext(p_pr_xml, "w:ind", 0);
        if ind >= 0 {
            let ind_el: String = WordXml::sliceElement(p_pr_xml, "w:ind", ind);
            let iot: String = WordXml::openTag(&ind_el);
            let left: String = WordXml::attr(&iot, "w:left");
            let right: String = WordXml::attr(&iot, "w:right");
            let first: String = WordXml::attr(&iot, "w:firstLine");
            let hang: String = WordXml::attr(&iot, "w:hanging");
            if !left.is_empty() {
                let lv: Option<i64> = left.parse::<i64>().ok();
                if lv.is_some() {
                    ps.borrow_mut().indentLeftPt = ((lv.unwrap()) as f64) / 20.0;
                }
            }
            if !right.is_empty() {
                let rv: Option<i64> = right.parse::<i64>().ok();
                if rv.is_some() {
                    ps.borrow_mut().indentRightPt = ((rv.unwrap()) as f64) / 20.0;
                }
            }
            if !first.is_empty() {
                let fv: Option<i64> = first.parse::<i64>().ok();
                if fv.is_some() {
                    ps.borrow_mut().indentFirstLinePt = ((fv.unwrap()) as f64) / 20.0;
                }
            }
            if !hang.is_empty() {
                let hv: Option<i64> = hang.parse::<i64>().ok();
                if hv.is_some() {
                    ps.borrow_mut().indentFirstLinePt = 0.0 - ((hv.unwrap()) as f64) / 20.0;
                }
            }
        }
        let num: i64 = WordXml::findNext(p_pr_xml, "w:numPr", 0);
        if num >= 0 {
            let num_el: String = WordXml::sliceElement(p_pr_xml, "w:numPr", num);
            let nid: i64 = WordXml::findNext(&num_el, "w:numId", 0);
            if nid >= 0 {
                let n_el: String = WordXml::sliceElement(&num_el, "w:numId", nid);
                let nv: String = WordXml::attr(&WordXml::openTag(&n_el), "w:val");
                let ni: Option<i64> = nv.parse::<i64>().ok();
                if ni.is_some() {
                    p.borrow_mut().listNumId = ni.unwrap();
                    p.borrow_mut().listKind = 2;
                }
            }
            let il: i64 = WordXml::findNext(&num_el, "w:ilvl", 0);
            if il >= 0 {
                let il_el: String = WordXml::sliceElement(&num_el, "w:ilvl", il);
                let iv: String = WordXml::attr(&WordXml::openTag(&il_el), "w:val");
                let ii: Option<i64> = iv.parse::<i64>().ok();
                if ii.is_some() {
                    p.borrow_mut().listLevel = ii.unwrap();
                }
            }
        }
        // page break before
        if WordXml::hasFlag(p_pr_xml, "w:pageBreakBefore") {
            p.borrow_mut().pageBreakBefore = true;
        }
    }
    fn resolveParagraph(&self, p: &Rc<RefCell<Paragraph>>, p_pr_xml: &str) {
        let mut sid: String = p.borrow().styleId.clone();
        if sid.is_empty() {
            sid = "Normal".to_string();
            p.borrow_mut().styleId = sid.clone();
        }
        let mut ps: Rc<RefCell<ResolvedParagraphStyle>> = self.resolveParagraphStyle(sid.clone());
        self.applyDirectPPr(p_pr_xml, &ps, p);
        // An unaligned right-to-left paragraph belongs against the RIGHT
        // margin. Word does not write a `w:jc` for it — the direction is the
        // alignment — so a reader that only looks at `w:jc` puts every Arabic
        // paragraph in the document hard against the left margin, which is the
        // first thing anyone notices and the last thing they forgive.
        if ps.borrow().bidi == 1 {
            if !ps.borrow().alignSet {
                ps.borrow_mut().align = 2;
            }
        }
        p.borrow_mut().bidi = ps.borrow().bidi;
        p.borrow_mut().align = ps.borrow().align;
        p.borrow_mut().spaceBeforePt = ps.borrow().spaceBeforePt;
        p.borrow_mut().spaceAfterPt = ps.borrow().spaceAfterPt;
        p.borrow_mut().lineSpacing = ps.borrow().lineSpacing;
        p.borrow_mut().indentLeftPt = ps.borrow().indentLeftPt;
        p.borrow_mut().indentRightPt = ps.borrow().indentRightPt;
        p.borrow_mut().indentFirstLinePt = ps.borrow().indentFirstLinePt;
        // Stash base run on tip span for parser
        let mut tip: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
        ps.borrow().run.clone().borrow().applyToSpan(&tip);
        p.borrow_mut().spans.clear();
        p.borrow_mut().spans.push(tip.clone());
    }
    fn baseRunFromParagraph(&self, p: &Rc<RefCell<Paragraph>>) -> Rc<RefCell<ResolvedRunStyle>> {
        let mut r: Rc<RefCell<ResolvedRunStyle>> = self.defaults.borrow().copy();
        if !p.borrow().spans.is_empty() {
            let mut tip: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
            r.borrow_mut().bold = tip.borrow().bold;
            r.borrow_mut().italic = tip.borrow().italic;
            r.borrow_mut().underline = tip.borrow().underline;
            r.borrow_mut().fontSizePt = tip.borrow().fontSizePt;
            r.borrow_mut().fontFamily = tip.borrow().fontFamily.clone();
            r.borrow_mut().colorHex = tip.borrow().colorHex.clone();
            r.borrow_mut().strike = tip.borrow().strike;
            r.borrow_mut().vertAlign = tip.borrow().vertAlign;
            r.borrow_mut().highlight = tip.borrow().highlight.clone();
        }
        r
    }
}
