// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::rich_document::{Paragraph};
use crate::rgr::DxNative;
use crate::word_xml::WordXml;
use crate::rgr::{EVGCodepoint};

/// ==============================================================================
/// WordNumbering - numbering.xml → list markers (canvas-editor / Word listId).
/// ==============================================================================
///
/// Word: paragraph.numPr.numId → w:num → abstractNumId → w:lvl[ilvl]
/// Editor model keeps only (listNumId, level); marker text is resolved here.
/// ==============================================================================
#[derive(Clone)]
pub struct NumLevel {
    pub ilvl: i64,
    pub start: i64,
    pub numFmt: String,
    pub lvlText: String,
    pub leftTwips: i64,
    pub hangingTwips: i64,
}

impl NumLevel {
    pub fn new() -> Self {
        Self {
            ilvl: 0,
            start: 1,
            numFmt: "decimal".to_string(),
            lvlText: "%1.".to_string(),
            leftTwips: 720,
            hangingTwips: 360,
        }
    }
}

#[derive(Clone)]
pub struct AbstractNum {
    pub id: i64,
    pub levels: Vec<Rc<RefCell<NumLevel>>>,
}

impl AbstractNum {
    pub fn new() -> Self {
        Self {
            id: 0,
            levels: Vec::new(),
        }
    }
    pub fn levelAt(&self, ilvl: i64) -> Rc<RefCell<NumLevel>> {
        let mut i: i64 = 0;
        while i < (self.levels.len() as i64) {
            let mut lv: Rc<RefCell<NumLevel>> = self.levels[i as usize].clone();
            if lv.borrow().ilvl == ilvl {
                return lv;
            }
            i += 1;
        }
        if !self.levels.is_empty() {
            return self.levels[0].clone();
        }
        Rc::new(RefCell::new(NumLevel::new()))
    }
}

/// The list counters of one layout pass: numId * 100 + level -> the last
/// number given. A struct, so that bumpCounter can change it in place (a
/// `&mut HashMap` parameter is a box on the other targets, and .rgr callers
/// pass the map itself).
pub struct ListCounters {
    pub values: HashMap<i64, i64>,
}

impl ListCounters {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct WordNumbering {
    pub abstracts: HashMap<i64,Rc<RefCell<AbstractNum>>>,
    pub numToAbstract: HashMap<i64,i64>,
    /// The highest ids the file uses, so a list the editor makes gets new ones.
    pub maxAbstractId: i64,
    pub maxNumId: i64,
    /// Lists the editor made, as numbering.xml fragments for a save to add:
    /// definitions go before the first w:num, instances at the end.
    pub addedAbstracts: Vec<String>,
    pub addedNums: Vec<String>,
}

impl WordNumbering {
    pub fn new() -> Self {
        Self {
            abstracts: HashMap::new(),
            numToAbstract: HashMap::new(),
            maxAbstractId: -1,
            maxNumId: 0,
            addedAbstracts: Vec::new(),
            addedNums: Vec::new(),
        }
    }
    pub fn formatToken(fmt: &str, n: i64) -> String {
        if fmt == "bullet" {
            return "•".to_string();
        }
        if fmt == "decimal" {
            return n.to_string();
        }
        if fmt == "lowerLetter" {
            return WordNumbering::letter(n, false);
        }
        if fmt == "upperLetter" {
            return WordNumbering::letter(n, true);
        }
        if fmt == "lowerRoman" {
            return WordNumbering::roman(n, false);
        }
        if fmt == "upperRoman" {
            return WordNumbering::roman(n, true);
        }
        n.to_string()
    }
    pub fn letter(mut n: i64, upper: bool) -> String {
        if n < 1 {
            n = 1;
        }
        let mut base: i64 = 97;
        if upper {
            base = 65;
        }
        let rem: i64 = (n - 1) - ((n - 1) / 26) * 26;
        EVGCodepoint::toStr(base + rem)
    }
    pub fn roman(n: i64, upper: bool) -> String {
        let mut vals: Vec<i64> = Vec::new();
        vals.push(1000);
        vals.push(900);
        vals.push(500);
        vals.push(400);
        vals.push(100);
        vals.push(90);
        vals.push(50);
        vals.push(40);
        vals.push(10);
        vals.push(9);
        vals.push(5);
        vals.push(4);
        vals.push(1);
        let mut syms: Vec<String> = Vec::new();
        if upper {
            syms.push("M".to_string().to_string());
            syms.push("CM".to_string().to_string());
            syms.push("D".to_string().to_string());
            syms.push("CD".to_string().to_string());
            syms.push("C".to_string().to_string());
            syms.push("XC".to_string().to_string());
            syms.push("L".to_string().to_string());
            syms.push("XL".to_string().to_string());
            syms.push("X".to_string().to_string());
            syms.push("IX".to_string().to_string());
            syms.push("V".to_string().to_string());
            syms.push("IV".to_string().to_string());
            syms.push("I".to_string().to_string());
        } else {
            syms.push("m".to_string().to_string());
            syms.push("cm".to_string().to_string());
            syms.push("d".to_string().to_string());
            syms.push("cd".to_string().to_string());
            syms.push("c".to_string().to_string());
            syms.push("xc".to_string().to_string());
            syms.push("l".to_string().to_string());
            syms.push("xl".to_string().to_string());
            syms.push("x".to_string().to_string());
            syms.push("ix".to_string().to_string());
            syms.push("v".to_string().to_string());
            syms.push("iv".to_string().to_string());
            syms.push("i".to_string().to_string());
        }
        let mut out: String = String::new();
        let mut remain: i64 = n;
        if remain < 1 {
            remain = 1;
        }
        if remain > 3999 {
            remain = 3999;
        }
        let mut i: i64 = 0;
        while i < (vals.len() as i64) {
            let v: i64 = vals[i as usize];
            while remain >= v {
                out = format!("{}{}", out, syms[i as usize].clone());
                remain -= v;
            }
            i += 1;
        }
        out
    }
    pub fn bulletAt(first: &str, ilvl: i64) -> String {
        let k: i64 = ilvl - (ilvl / 3) * 3;
        if k == 0 {
            return first.to_string();
        }
        if k == 1 {
            return "◦".to_string();
        }
        "▪".to_string()
    }
    pub fn parse(&mut self, xml: &str) {
        if xml.is_empty() {
            return;
        }
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&xml);
        while i < n {
            let a: i64 = WordXml::findNext(xml, "w:abstractNum", i);
            if a < 0 {
                i = n;
            } else {
                let el: String = WordXml::sliceElement(xml, "w:abstractNum", a);
                let ot: String = WordXml::openTag(&el);
                let id_str: String = WordXml::attr(&ot, "w:abstractNumId");
                let id: Option<i64> = id_str.parse::<i64>().ok();
                if id.is_some() {
                    let mut abs: Rc<RefCell<AbstractNum>> = Rc::new(RefCell::new(AbstractNum::new()));
                    abs.borrow_mut().id = id.unwrap();
                    let mut li: i64 = 0;
                    let en: i64 = DxNative::str_len(&el);
                    while li < en {
                        let ls: i64 = WordXml::findNext(&el, "w:lvl", li);
                        if ls < 0 {
                            li = en;
                        } else {
                            let lvl_el: String = WordXml::sliceElement(&el, "w:lvl", ls);
                            let mut lv: Rc<RefCell<NumLevel>> = self.parseLevel(&lvl_el);
                            abs.borrow_mut().levels.push(lv.clone());
                            let le: i64 = WordXml::elementEnd(&el, "w:lvl", ls);
                            if le < 0 {
                                li = en;
                            } else {
                                li = le;
                            }
                        }
                    }
                    self.abstracts.insert(abs.borrow().id, abs.clone());
                    if abs.borrow().id > self.maxAbstractId {
                        self.maxAbstractId = abs.borrow().id;
                    }
                }
                let ae: i64 = WordXml::elementEnd(xml, "w:abstractNum", a);
                if ae < 0 {
                    i = n;
                } else {
                    i = ae;
                }
            }
        }
        i = 0;
        while i < n {
            let ns: i64 = WordXml::findNext(xml, "w:num", i);
            if ns < 0 {
                return;
            }
            let n_el: String = WordXml::sliceElement(xml, "w:num", ns);
            // Skip w:numFmt / w:numberingPicBullet — require tag boundary findNext.
            let not: String = WordXml::openTag(&n_el);
            let num_id_str: String = WordXml::attr(&not, "w:numId");
            let abs_ref: i64 = WordXml::findNext(&n_el, "w:abstractNumId", 0);
            if !num_id_str.is_empty() {
                if abs_ref >= 0 {
                    let ar_el: String = WordXml::sliceElement(&n_el, "w:abstractNumId", abs_ref);
                    let av: String = WordXml::attr(&WordXml::openTag(&ar_el), "w:val");
                    let ni: Option<i64> = num_id_str.parse::<i64>().ok();
                    let ai: Option<i64> = av.parse::<i64>().ok();
                    if ni.is_some() && ai.is_some() {
                        self.numToAbstract.insert(ni.unwrap(), ai.unwrap());
                        if ni.unwrap() > self.maxNumId {
                            self.maxNumId = ni.unwrap();
                        }
                    }
                }
            }
            let ne: i64 = WordXml::elementEnd(xml, "w:num", ns);
            if ne < 0 {
                return;
            }
            i = ne;
        }
    }
    pub fn parseLevel(&self, lvl_el: &str) -> Rc<RefCell<NumLevel>> {
        let mut lv: Rc<RefCell<NumLevel>> = Rc::new(RefCell::new(NumLevel::new()));
        let ot: String = WordXml::openTag(lvl_el);
        let il: String = WordXml::attr(&ot, "w:ilvl");
        let ili: Option<i64> = il.parse::<i64>().ok();
        if ili.is_some() {
            lv.borrow_mut().ilvl = ili.unwrap();
        }
        let st: i64 = WordXml::findNext(lvl_el, "w:start", 0);
        if st >= 0 {
            let st_el: String = WordXml::sliceElement(lvl_el, "w:start", st);
            let sv: Option<i64> = WordXml::attr(&WordXml::openTag(&st_el), "w:val").parse::<i64>().ok();
            if sv.is_some() {
                lv.borrow_mut().start = sv.unwrap();
            }
        }
        let nf: i64 = WordXml::findNext(lvl_el, "w:numFmt", 0);
        if nf >= 0 {
            let nf_el: String = WordXml::sliceElement(lvl_el, "w:numFmt", nf);
            let _tmp_1103 = WordXml::attr(&WordXml::openTag(&nf_el), "w:val");
            lv.borrow_mut().numFmt = _tmp_1103;
        }
        let lt: i64 = WordXml::findNext(lvl_el, "w:lvlText", 0);
        if lt >= 0 {
            let lt_el: String = WordXml::sliceElement(lvl_el, "w:lvlText", lt);
            let _tmp_1105 = WordXml::attr(&WordXml::openTag(&lt_el), "w:val");
            lv.borrow_mut().lvlText = _tmp_1105;
        }
        let ind: i64 = WordXml::findNext(lvl_el, "w:ind", 0);
        if ind >= 0 {
            let ind_el: String = WordXml::sliceElement(lvl_el, "w:ind", ind);
            let iot: String = WordXml::openTag(&ind_el);
            let _tmp_1107 = WordXml::twipsAttr(&iot, "w:left", lv.borrow().leftTwips.clone());
            lv.borrow_mut().leftTwips = _tmp_1107;
            let _tmp_1109 = WordXml::twipsAttr(&iot, "w:hanging", lv.borrow().hangingTwips.clone());
            lv.borrow_mut().hangingTwips = _tmp_1109;
        }
        lv
    }
    pub fn abstractForNum(&self, num_id: i64) -> Rc<RefCell<AbstractNum>> {
        let abs_id: Option<i64> = self.numToAbstract.get(&num_id).cloned();
        if abs_id.is_none() {
            return Rc::new(RefCell::new(AbstractNum::new()));
        }
        let mut abs: Option<Rc<RefCell<AbstractNum>>> = self.abstracts.get(&abs_id.unwrap())
            .cloned();
        if abs.is_none() {
            return Rc::new(RefCell::new(AbstractNum::new()));
        }
        abs.clone().unwrap()
    }
    pub fn levelFor(&self, num_id: i64, ilvl: i64) -> Rc<RefCell<NumLevel>> {
        let mut abs: Rc<RefCell<AbstractNum>> = self.abstractForNum(num_id);
        return abs.borrow().levelAt(ilvl);
    }
    fn applyToParagraph(&self, p: &Rc<RefCell<Paragraph>>) {
        if p.borrow().listNumId <= 0 {
            return;
        }
        let mut _tmp_1110 = p.borrow().listNumId.clone();
        let mut lv: Rc<RefCell<NumLevel>> = self.levelFor(_tmp_1110, p.borrow().listLevel.clone());
        p.borrow_mut().indentLeftPt = (lv.borrow().leftTwips as f64) / 20.0;
        if lv.borrow().numFmt == "bullet" {
            p.borrow_mut().listKind = 1;
        } else {
            p.borrow_mut().listKind = 2;
        }
        p.borrow_mut().listFmt = lv.borrow().numFmt.clone();
        p.borrow_mut().listText = lv.borrow().lvlText.clone();
    }
    /// Expand lvlText like "%1.%2." using per-level counters for this numId.
    pub fn formatMarker(&self, num_id: i64, ilvl: i64, counters: &ListCounters) -> String {
        let mut lv: Rc<RefCell<NumLevel>> = self.levelFor(num_id, ilvl);
        let tpl: String = lv.borrow().lvlText.clone();
        if tpl.is_empty() {
            if lv.borrow().numFmt == "bullet" {
                return "• ".to_string();
            }
            let mut _tmp_1115 = lv.borrow().start.clone();
            return format!("{}. ", self.counterValue(counters,
                num_id,
                ilvl,
                _tmp_1115
            ).to_string()).clone();
        }
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&tpl);
        while i < n {
            if DxNative::char_at(&tpl, i) == 37 {
                if i + 1 < n {
                    let d: i64 = DxNative::char_at(&tpl, i + 1);
                    if (d >= 49 && d <= 57) {
                        let level: i64 = d - 49;
                        let mut lv2: Rc<RefCell<NumLevel>> = self.levelFor(num_id, level);
                        let mut _tmp_1116 = lv2.borrow().start.clone();
                        let c: i64 = self.counterValue(counters,
                            num_id,
                            level,
                            _tmp_1116
                        );
                        out = format!("{}{}", out, WordNumbering::formatToken(&lv2.borrow().numFmt, c));
                        i += 2;
                    } else {
                        out = format!("{}%", out);
                        i += 1;
                    }
                } else {
                    out = format!("{}%", out);
                    i += 1;
                }
            } else {
                out = format!("{}{}", out, DxNative::substr(&tpl, i, i + 1));
                i += 1;
            }
        }
        if !out.is_empty() {
            let last: i64 = DxNative::char_at(&out, (DxNative::str_len(&out)) - 1);
            if last != 32 {
                out = format!("{} ", out);
            }
        }
        out
    }
    pub fn counterValue(&self, counters: &ListCounters, num_id: i64, ilvl: i64, start: i64) -> i64 {
        let key: i64 = num_id * 100 + ilvl;
        let hit: Option<i64> = counters.values.get(&key).cloned();
        if hit.is_some() {
            let v: i64 = hit.unwrap();
            if v > 0 {
                return v;
            }
        }
        start
    }
    pub fn bumpCounter(&self, counters: &mut ListCounters, num_id: i64, ilvl: i64, start: i64) {
        let key: i64 = num_id * 100 + ilvl;
        let hit: Option<i64> = counters.values.get(&key).cloned();
        let mut n: i64 = start;
        if hit.is_some() {
            let cur: i64 = hit.unwrap();
            if cur > 0 {
                n = cur + 1;
            }
        }
        counters.values.insert(key, n);
        // Reset deeper levels so the next nested item restarts
        let mut d: i64 = ilvl + 1;
        while d < 9 {
            let dk: i64 = num_id * 100 + d;
            counters.values.insert(dk, 0);
            d += 1;
        }
    }
    /// --- lists the editor makes ------------------------------------------------
    pub fn hasAdditions(&self) -> bool {
        !self.addedNums.is_empty()
    }
    /// "bullet" or "number": what a list's first level is.
    pub fn kindOf(&self, num_id: i64) -> String {
        if num_id <= 0 {
            return String::new();
        }
        let mut lv: Rc<RefCell<NumLevel>> = self.levelFor(num_id, 0);
        if lv.borrow().numFmt == "bullet" {
            return "bullet".to_string();
        }
        if lv.borrow().numFmt == "none" {
            return String::new();
        }
        "number".to_string()
    }
    /// The first level's look, "bullet:•" or "decimal:%1.", for a menu to tick.
    pub fn styleOf(&self, num_id: i64) -> String {
        if num_id <= 0 {
            return String::new();
        }
        let mut lv: Rc<RefCell<NumLevel>> = self.levelFor(num_id, 0);
        return format!("{}:{}", lv.borrow().numFmt, lv.borrow().lvlText);
    }
    /// A new list: its own definition, nine levels deep, and an instance of
    /// it. `style` is "bullet:•" or a number format and its text, such as
    /// "decimal:%1." or "lowerLetter:%1)". Deeper levels follow Word's
    /// defaults: bullets ◦ ▪, numbers a. i. 1. ...
    pub fn newList(&mut self, style: &str) -> i64 {
        let mut specs: Vec<String> = Vec::new();
        specs.push(style.to_string());
        self.newListLevels(&specs)
    }
    /// The same with a look for each level a file stated: `specs[l]` is level
    /// l's "format:text" as `newList` takes it ("decimal:%1.%2."), and a level
    /// past the end of `specs` gets the default that follows the first.
    pub fn newListLevels(&mut self, specs: &[String]) -> i64 {
        let mut first: String = "decimal:%1.".to_string();
        if !specs.is_empty() {
            first = specs[0].clone();
        }
        let colon: i64 = DxNative::index_of(&first, ":");
        let mut fmt: String = "decimal".to_string();
        let mut text: String = "%1.".to_string();
        if colon > 0 {
            fmt = DxNative::substr(&first, 0, colon);
            text = DxNative::substr(&first, colon + 1, (DxNative::str_len(&first)));
        }
        self.maxAbstractId += 1;
        self.maxNumId += 1;
        let mut abs: Rc<RefCell<AbstractNum>> = Rc::new(RefCell::new(AbstractNum::new()));
        abs.borrow_mut().id = self.maxAbstractId;
        let mut x: String = format!("<w:abstractNum w:abstractNumId=\"{}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>", abs.borrow().id);
        let mut l: i64 = 0;
        while l < 9 {
            let mut lv: Rc<RefCell<NumLevel>> = Rc::new(RefCell::new(NumLevel::new()));
            lv.borrow_mut().ilvl = l;
            lv.borrow_mut().leftTwips = 720 * (l + 1);
            lv.borrow_mut().hangingTwips = 360;
            if fmt == "bullet" {
                lv.borrow_mut().numFmt = "bullet".to_string();
                lv.borrow_mut().lvlText = WordNumbering::bulletAt(&text, l);
            } else {
                let k: i64 = l - (l / 3) * 3;
                lv.borrow_mut().numFmt = "decimal".to_string();
                if k == 1 {
                    lv.borrow_mut().numFmt = "lowerLetter".to_string();
                }
                if k == 2 {
                    lv.borrow_mut().numFmt = "lowerRoman".to_string();
                }
                lv.borrow_mut().lvlText = format!("%{}.", l + 1);
            }
            if l < (specs.len() as i64) {
                let spec: String = specs[l as usize].clone();
                let c2: i64 = DxNative::index_of(&spec, ":");
                if c2 > 0 {
                    lv.borrow_mut().numFmt = DxNative::substr(&spec, 0, c2);
                    lv.borrow_mut().lvlText = DxNative::substr(&spec, c2 + 1, (DxNative::str_len(&spec)));
                }
            }
            abs.borrow_mut().levels.push(lv.clone());
            x = format!("{}<w:lvl w:ilvl=\"{}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{}\"/><w:lvlText w:val=\"{}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>", x, l, lv.borrow().numFmt, WordXml::escapeAttr(&lv.borrow().lvlText), lv.borrow().leftTwips);
            l += 1;
        }
        x = format!("{}</w:abstractNum>", x);
        self.abstracts.insert(abs.borrow().id, abs.clone());
        self.numToAbstract.insert(self.maxNumId, abs.borrow().id);
        self.addedAbstracts.push(x);
        self.addedNums.push(format!("<w:num w:numId=\"{}\"><w:abstractNumId w:val=\"{}\"/></w:num>", self.maxNumId, abs.borrow().id));
        self.maxNumId
    }
    /// numbering.xml with the editor's lists in it: `xml` is the file's own,
    /// or "" when it had none.
    pub fn withAdditions(&self, xml: &str) -> String {
        let mut defs: String = String::new();
        let __it_d = self.addedAbstracts.clone();
        for d in __it_d.iter().cloned() {
            defs = format!("{}{}", defs, d);
        }
        let mut nums: String = String::new();
        let __it_u = self.addedNums.clone();
        for u in __it_u.iter().cloned() {
            nums = format!("{}{}", nums, u);
        }
        if xml.is_empty() {
            return format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{}{}</w:numbering>", defs, nums);
        }
        // Definitions before the first instance, which the schema requires.
        let mut at: i64 = WordXml::findNext(xml, "w:num", 0);
        if at < 0 {
            at = DxNative::index_of(&xml, "</w:numbering>");
        }
        if at < 0 {
            return xml.to_string();
        }
        let out: String = format!("{}{}{}", DxNative::substr(&xml, 0, at), defs, DxNative::substr(&xml, at, (DxNative::str_len(&xml))));
        let end: i64 = DxNative::index_of(&out, "</w:numbering>");
        if end < 0 {
            return out;
        }
        format!("{}{}{}", DxNative::substr(&out, 0, end), nums, DxNative::substr(&out, end, (DxNative::str_len(&out))))
    }
}
