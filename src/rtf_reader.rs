// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::odt_reader::OdtReader;
use crate::rgr::DxNative;
use crate::rich_document::DocBlock;
use crate::rich_document::DocBlockKind;
use crate::rich_document::DocImage;
use crate::rich_document::DocSection;
use crate::rich_document::DocTableBorder;
use crate::rich_document::DocTableCell;
use crate::rich_document::DocTableRow;
use crate::rich_document::DocumentTable;
use crate::rich_document::Paragraph;
use crate::rich_document::RichDocument;
use crate::rich_document::TextSpan;
use crate::word_ml_parser::WordMlParser;
use crate::word_numbering::WordNumbering;
use crate::word_style_resolver::WordStyleResolver;
use crate::rgr::{EVGCodepoint};

/// ==============================================================================
/// RtfReader - Rich Text Format (.rtf) → RichDocument
/// ==============================================================================
///
/// RTF is one stream of control words and groups, so this is a tokenizer and a
/// stack of group states rather than a tree walk. Character AND paragraph
/// formatting live in the group state (a `}` undoes both); a table row's
/// definition does not (`\trowd` starts one, and it lasts until the next).
///
/// WHAT IS READ. Text in the ANSI code page (1252) with `\'hh` and `\uN`,
/// character formatting (bold, italic, underline, strike, super/subscript,
/// size, font, colour, highlight), paragraph alignment / indents / spacing /
/// line spacing / page breaks, heading styles by name, lists (`\ls`/`\ilvl`
/// against the list table), tables (`\trowd` ... `\cell` ... `\row`, with
/// widths, merged cells, cell shading and borders), PNG and JPEG pictures,
/// hyperlink fields, the page size and margins, the first header and footer.
///
/// WHAT IS NOT. Code pages other than 1252 for `\'hh` (`\uN`, which every
/// current writer emits beside it, is read), WMF/EMF pictures, shapes and
/// text boxes, footnote bodies, nested tables (their text is kept, flat),
/// sections after the first, and annotations.
///
/// It lands in `RichDocument`, like the .odt reader, and a save writes a
/// fresh .docx.
/// ==============================================================================
/// Where the text of a group goes.
#[derive(Clone)]
pub struct RtfDest {
}

impl RtfDest {
    pub fn new() -> Self {
        Self {
        }
    }
    pub fn body() -> i64 {
        0
    }
    pub fn skip() -> i64 {
        1
    }
    pub fn fontTable() -> i64 {
        2
    }
    pub fn colorTable() -> i64 {
        3
    }
    pub fn styleSheet() -> i64 {
        4
    }
    pub fn listText() -> i64 {
        5
    }
    pub fn fieldInst() -> i64 {
        6
    }
    pub fn picture() -> i64 {
        7
    }
    pub fn listTable() -> i64 {
        8
    }
    pub fn overrideTable() -> i64 {
        9
    }
    pub fn title() -> i64 {
        10
    }
    pub fn levelText() -> i64 {
        11
    }
}

/// A list in the list table, level by level as WordNumbering takes them.
#[derive(Clone)]
pub struct RtfListDef {
    pub firstNfc: i64,
    pub specs: Vec<String>,
}

impl RtfListDef {
    pub fn new() -> Self {
        Self {
            firstNfc: -1,
            specs: Vec::new(),
        }
    }
}

/// Everything a `{` saves and a `}` restores.
#[derive(Clone)]
pub struct RtfState {
    pub dest: i64,
    /// Did this group start its destination? Its `}` finishes it.
    pub startsDest: bool,
    /// 0 body, 1 header, 2 footer: which paragraphs the text joins.
    pub target: i64,
    pub ucSkip: i64,
    /// character
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub vertAlign: i64,
    pub halfPoints: i64,
    pub font: i64,
    pub color: i64,
    pub highlight: i64,
    pub hyperlink: String,
    /// paragraph
    pub align: i64,
    pub leftTw: i64,
    pub rightTw: i64,
    pub firstTw: i64,
    pub beforeTw: i64,
    pub afterTw: i64,
    pub lineTw: i64,
    pub lineMult: bool,
    pub inTable: bool,
    pub pageBreakBefore: bool,
    pub style: i64,
    pub ls: i64,
    pub ilvl: i64,
}

impl RtfState {
    pub fn new() -> Self {
        Self {
            dest: 0,
            startsDest: false,
            target: 0,
            ucSkip: 1,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            vertAlign: 0,
            halfPoints: 24,
            font: -1,
            color: 0,
            highlight: 0,
            hyperlink: String::new(),
            align: 0,
            leftTw: 0,
            rightTw: 0,
            firstTw: 0,
            beforeTw: 0,
            afterTw: 0,
            lineTw: 0,
            lineMult: false,
            inTable: false,
            pageBreakBefore: false,
            style: 0,
            ls: 0,
            ilvl: 0,
        }
    }
    pub fn copy(&self) -> Rc<RefCell<RtfState>> {
        let mut s: Rc<RefCell<RtfState>> = Rc::new(RefCell::new(RtfState::new()));
        s.borrow_mut().dest = self.dest;
        s.borrow_mut().target = self.target;
        s.borrow_mut().ucSkip = self.ucSkip;
        s.borrow_mut().bold = self.bold;
        s.borrow_mut().italic = self.italic;
        s.borrow_mut().underline = self.underline;
        s.borrow_mut().strike = self.strike;
        s.borrow_mut().vertAlign = self.vertAlign;
        s.borrow_mut().halfPoints = self.halfPoints;
        s.borrow_mut().font = self.font;
        s.borrow_mut().color = self.color;
        s.borrow_mut().highlight = self.highlight;
        s.borrow_mut().hyperlink = self.hyperlink.clone();
        s.borrow_mut().align = self.align;
        s.borrow_mut().leftTw = self.leftTw;
        s.borrow_mut().rightTw = self.rightTw;
        s.borrow_mut().firstTw = self.firstTw;
        s.borrow_mut().beforeTw = self.beforeTw;
        s.borrow_mut().afterTw = self.afterTw;
        s.borrow_mut().lineTw = self.lineTw;
        s.borrow_mut().lineMult = self.lineMult;
        s.borrow_mut().inTable = self.inTable;
        s.borrow_mut().pageBreakBefore = self.pageBreakBefore;
        s.borrow_mut().style = self.style;
        s.borrow_mut().ls = self.ls;
        s.borrow_mut().ilvl = self.ilvl;
        s
    }
    pub fn resetChar(&mut self) {
        self.bold = false;
        self.italic = false;
        self.underline = false;
        self.strike = false;
        self.vertAlign = 0;
        self.halfPoints = 24;
        self.color = 0;
        self.highlight = 0;
    }
    pub fn resetPara(&mut self) {
        self.align = 0;
        self.leftTw = 0;
        self.rightTw = 0;
        self.firstTw = 0;
        self.beforeTw = 0;
        self.afterTw = 0;
        self.lineTw = 0;
        self.lineMult = false;
        self.inTable = false;
        self.pageBreakBefore = false;
        self.style = 0;
        self.ls = 0;
        self.ilvl = 0;
    }
}

/// One cell of the row being defined by `\cellx`.
#[derive(Clone)]
pub struct RtfCellDef {
    pub rightTw: i64,
    pub merged: bool,
    pub fill: i64,
    pub borderTop: Rc<RefCell<DocTableBorder>>,
    pub borderBottom: Rc<RefCell<DocTableBorder>>,
    pub borderLeft: Rc<RefCell<DocTableBorder>>,
    pub borderRight: Rc<RefCell<DocTableBorder>>,
}

impl RtfCellDef {
    pub fn new() -> Self {
        let mut me = Self {
            rightTw: 0,
            merged: false,
            fill: 0,
            borderTop: Rc::new(RefCell::new(DocTableBorder::new())),
            borderBottom: Rc::new(RefCell::new(DocTableBorder::new())),
            borderLeft: Rc::new(RefCell::new(DocTableBorder::new())),
            borderRight: Rc::new(RefCell::new(DocTableBorder::new())),
        };
        me.borderTop.borrow_mut().style = "none".to_string();
        me.borderBottom.borrow_mut().style = "none".to_string();
        me.borderLeft.borrow_mut().style = "none".to_string();
        me.borderRight.borrow_mut().style = "none".to_string();
        me
    }
}

/// Where a finished row's cells end, for the grid the table gets when it ends.
#[derive(Clone)]
pub struct RtfRowInfo {
    pub left: i64,
    pub edges: Vec<i64>,
}

impl RtfRowInfo {
    pub fn new() -> Self {
        Self {
            left: 0,
            edges: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct RtfReader {
    pub lastError: String,
    pub numbering: Rc<RefCell<WordNumbering>>,
    pub doc: Rc<RefCell<RichDocument>>,
    pub src: String,
    pub pos: i64,
    pub __len: i64,
    pub st: Rc<RefCell<RtfState>>,
    pub stack: Vec<Rc<RefCell<RtfState>>>,
    pub defaultFont: i64,
    pub fonts: HashMap<i64,String>,
    pub colors: Vec<String>,
    pub styleNames: HashMap<i64,String>,
    /// text a destination collects until its group or entry ends
    pub collected: String,
    pub entryIndex: i64,
    pub red: i64,
    pub green: i64,
    pub blue: i64,
    pub colorSet: bool,
    /// lists: \ls → \listid → first level's \levelnfc
    pub listNfc: HashMap<i64,i64>,
    pub lsToList: HashMap<i64,i64>,
    pub lsNumId: HashMap<i64,i64>,
    pub listLevel: i64,
    pub firstNfc: i64,
    pub levelNfc: i64,
    pub levelText: String,
    pub curListDef: Rc<RefCell<RtfListDef>>,
    pub listDefs: HashMap<i64,Rc<RefCell<RtfListDef>>>,
    pub pendingListId: i64,
    pub lastListText: String,
    /// the paragraph being written, per target (body, header, footer)
    pub cur: Vec<Rc<RefCell<Paragraph>>>,
    pub curOpen: Vec<bool>,
    /// tables
    pub rowDefs: Vec<Rc<RefCell<RtfCellDef>>>,
    pub pendingCell: Rc<RefCell<RtfCellDef>>,
    pub borderSide: i64,
    pub table: Option<Rc<RefCell<DocumentTable>>>,
    pub rowCells: Vec<Rc<RefCell<DocTableCell>>>,
    pub cellParas: Vec<Rc<RefCell<Paragraph>>>,
    pub rowHeightTw: i64,
    pub rowLeftTw: i64,
    pub tableRows: Vec<Rc<RefCell<RtfRowInfo>>>,
    /// pictures
    pub picHex: String,
    pub picKind: String,
    pub picW: i64,
    pub picH: i64,
    pub picGoalW: i64,
    pub picGoalH: i64,
    pub picScaleX: i64,
    pub picScaleY: i64,
    pub fieldInst: String,
    pub skipChars: i64,
    pub sawHeader: bool,
    pub sawFooter: bool,
    pub curSection: i64,
    pub blocksAtSection: i64,
    pub pageDefaults: Rc<RefCell<DocSection>>,
}

impl RtfReader {
    pub fn new() -> Self {
        Self {
            lastError: String::new(),
            numbering: Rc::new(RefCell::new(WordNumbering::new())),
            doc: Rc::new(RefCell::new(RichDocument::new())),
            src: String::new(),
            pos: 0,
            __len: 0,
            st: Rc::new(RefCell::new(RtfState::new())),
            stack: Vec::new(),
            defaultFont: 0,
            fonts: HashMap::new(),
            colors: Vec::new(),
            styleNames: HashMap::new(),
            collected: String::new(),
            entryIndex: 0,
            red: 0,
            green: 0,
            blue: 0,
            colorSet: false,
            listNfc: HashMap::new(),
            lsToList: HashMap::new(),
            lsNumId: HashMap::new(),
            listLevel: -1,
            firstNfc: -1,
            levelNfc: -1,
            levelText: String::new(),
            curListDef: Rc::new(RefCell::new(RtfListDef::new())),
            listDefs: HashMap::new(),
            pendingListId: 0,
            lastListText: String::new(),
            cur: Vec::new(),
            curOpen: Vec::new(),
            rowDefs: Vec::new(),
            pendingCell: Rc::new(RefCell::new(RtfCellDef::new())),
            borderSide: 0,
            table: None,
            rowCells: Vec::new(),
            cellParas: Vec::new(),
            rowHeightTw: 0,
            rowLeftTw: 0,
            tableRows: Vec::new(),
            picHex: String::new(),
            picKind: String::new(),
            picW: 0,
            picH: 0,
            picGoalW: 0,
            picGoalH: 0,
            picScaleX: 100,
            picScaleY: 100,
            fieldInst: String::new(),
            skipChars: 0,
            sawHeader: false,
            sawFooter: false,
            curSection: 0,
            blocksAtSection: 0,
            pageDefaults: Rc::new(RefCell::new(DocSection::new())),
        }
    }
    pub fn isRtf(bytes: &[u8]) -> bool {
        if (bytes.len() as i64) < 5 {
            return false;
        }
        let want: String = "{\\rtf";
        let mut i: i64 = 0;
        while i < 5 {
            if (bytes[i as usize] as i64) != DxNative::char_at(&want, i) {
                return false;
            }
            i += 1;
        }
        true
    }
    /// ------------------------------------------------------------------
    /// The tokenizer
    /// ------------------------------------------------------------------
    pub fn isLetter(c: i64) -> bool {
        (c >= 97 && c <= 122) || (c >= 65 && c <= 90)
    }
    pub fn isDigit(c: i64) -> bool {
        (c >= 48 && c <= 57)
    }
    pub fn hexVal(c: i64) -> i64 {
        if (c >= 48 && c <= 57) {
            return c - 48;
        }
        if (c >= 97 && c <= 102) {
            return c - 87;
        }
        if (c >= 65 && c <= 70) {
            return c - 55;
        }
        -1
    }
    pub fn cp1252(b: i64) -> i64 {
        if b < 128 || b > 159 {
            return b;
        }
        let mut _map: Vec<i64> = vec![8364, 129, 8218, 402, 8222, 8230, 8224, 8225, 710, 8240, 352, 8249, 338, 141, 381, 143, 144, 8216, 8217, 8220, 8221, 8226, 8211, 8212, 732, 8482, 353, 8250, 339, 157, 382, 376];
        _map[(b - 128) as usize]
    }
    /// Destinations whose whole text is collected and used at their end.
    pub fn collects(d: i64) -> bool {
        ((d == RtfDest::listText() || d == RtfDest::fieldInst()) || d == RtfDest::title()) || d == RtfDest::levelText()
    }
    pub fn copyPage(from: &Rc<RefCell<DocSection>>, to: &Rc<RefCell<DocSection>>) {
        to.borrow_mut().pageWTwips = from.borrow().pageWTwips;
        to.borrow_mut().pageHTwips = from.borrow().pageHTwips;
        to.borrow_mut().marginTopTwips = from.borrow().marginTopTwips;
        to.borrow_mut().marginBottomTwips = from.borrow().marginBottomTwips;
        to.borrow_mut().marginLeftTwips = from.borrow().marginLeftTwips;
        to.borrow_mut().marginRightTwips = from.borrow().marginRightTwips;
        to.borrow_mut().landscape = from.borrow().landscape;
    }
    pub fn hex2(v: i64) -> String {
        let digits: String = "0123456789ABCDEF";
        let hi: i64 = v / 16;
        let lo: i64 = v - hi * 16;
        format!("{}{}", DxNative::substr(&digits, hi, hi + 1), DxNative::substr(&digits, lo, lo + 1))
    }
    pub fn trim(s: &str) -> String {
        let mut a: i64 = 0;
        let mut b: i64 = DxNative::str_len(&s);
        while a < b && DxNative::char_at(&s, a) <= 32 {
            a += 1;
        }
        while b > a && DxNative::char_at(&s, b - 1) <= 32 {
            b -= 1;
        }
        DxNative::substr(&s, a, b)
    }
    /// HYPERLINK "https://…" → https://…
    pub fn linkOf(inst: &str) -> String {
        let q: i64 = DxNative::index_of(&inst, "\"");
        if q >= 0 {
            let e: i64 = DxNative::index_of_from(&inst, "\"", q + 1);
            if e > q {
                return DxNative::substr(&inst, q + 1, e);
            }
        }
        RtfReader::trim(&DxNative::substr(&inst, 9, (DxNative::str_len(&inst))))
    }
    /// `\levelnfc` and `\leveltext` as WordNumbering's "format:text". The
    /// level text is Word's own encoding: a length, then the text, where a
    /// character below 9 stands for that level's number.
    pub fn levelSpec(nfc: i64, lt: &str, level: i64) -> String {
        let mut fmt: String = "decimal";
        if nfc == 1 {
            fmt = "upperRoman";
        }
        if nfc == 2 {
            fmt = "lowerRoman";
        }
        if nfc == 3 {
            fmt = "upperLetter";
        }
        if nfc == 4 {
            fmt = "lowerLetter";
        }
        let n: i64 = DxNative::str_len(&lt);
        let mut body: String = String::new();
        if n > 1 {
            let mut i: i64 = 1;
            let mut from: i64 = 1;
            while i < n {
                let c: i64 = DxNative::char_at(&lt, i);
                if c < 9 || c == 59 {
                    if i > from {
                        body = format!("{}{}", body, DxNative::substr(&lt, from, i));
                    }
                    if c < 9 {
                        body = format!("{}%{}", body, c + 1);
                    }
                    from = i + 1;
                    if c == 59 {
                        i = n;
                    }
                }
                i += 1;
            }
            if from < n {
                body = format!("{}{}", body, DxNative::substr(&lt, from, n));
            }
        }
        if nfc == 23 {
            let mut ch: String = body.clone();
            if ch.is_empty() {
                ch = WordNumbering::bulletAt("•", level);
            }
            let c0: i64 = DxNative::char_at(&ch, 0);
            // Symbol and Wingdings bullets, and private-use ones, are not
            // characters any font here has.
            if (c0 >= 57344 && c0 <= 63743) || (c0 == 238 || c0 == 239) {
                ch = WordNumbering::bulletAt("•", level);
            }
            if c0 == 183 {
                ch = "•".to_string();
            }
            return format!("bullet:{}", ch);
        }
        if nfc == 255 || body.is_empty() {
            return format!("{}:%{}.", fmt, level + 1);
        }
        format!("{}:{}", fmt, body)
    }
    pub fn nfcOfMarker(m: &str) -> i64 {
        if m.is_empty() {
            return 23;
        }
        let c: i64 = DxNative::char_at(&m, 0);
        if RtfReader::isDigit(c) {
            return 0;
        }
        if (c >= 97 && c <= 122) {
            return 4;
        }
        if (c >= 65 && c <= 90) {
            return 3;
        }
        23
    }
    pub fn insertSorted(mut list: &mut Vec<i64>, v: i64) {
        let n: i64 = list.len() as i64;
        let mut i: i64 = 0;
        while i < n {
            let x: i64 = list[i as usize];
            if x == v {
                return;
            }
            if x > v {
                list.insert(i as usize, v);
                return;
            }
            i += 1;
        }
        list.push(v);
    }
    pub fn readBytes(&mut self, data: &[u8], name: String) -> Option<Rc<RefCell<RichDocument>>> {
        self.lastError = String::new();
        let mut missing: Option<Rc<RefCell<RichDocument>>> = None;
        if !RtfReader::isRtf(data) {
            self.lastError = format!("{} is not RTF", name);
            return missing;
        }
        let mut d: Rc<RefCell<RichDocument>> = self.readString(String::from_utf8_lossy(&data).to_string());
        if d.borrow().title.is_empty() {
            d.borrow_mut().title = name.clone();
        }
        let mut out: Option<Rc<RefCell<RichDocument>>> = Some(d.clone());
        out
    }
    pub fn readString(&mut self, text: String) -> Rc<RefCell<RichDocument>> {
        self.doc = Rc::new(RefCell::new(RichDocument::new()));
        self.numbering = Rc::new(RefCell::new(WordNumbering::new()));
        self.src = text.clone();
        self.pos = 0;
        self.__len = DxNative::str_len(&text);
        self.st = Rc::new(RefCell::new(RtfState::new()));
        self.curSection = 0;
        self.blocksAtSection = 0;
        self.pageDefaults = Rc::new(RefCell::new(DocSection::new()));
        self.stack.clear();
        self.cur.clear();
        self.curOpen.clear();
        let mut t: i64 = 0;
        while t < 3 {
            self.cur.push(Rc::new(RefCell::new(Paragraph::new())));
            self.curOpen.push(false);
            t += 1;
        }
        self.run();
        self.endParagraph(false);
        self.closeTable();
        if self.doc.borrow().blocks.is_empty() {
            let mut p: Rc<RefCell<Paragraph>> = self.doc.borrow_mut().addParagraph();
            p.borrow_mut().spans.push(self.spanNow());
        }
        let __it_p2 = self.doc.borrow().paragraphs.clone();
        for p2 in __it_p2.iter().cloned() {
            if p2.borrow().listNumId > 0 {
                self.numbering.clone().borrow().applyToParagraph(&p2);
            }
        }
        self.doc.clone()
    }
    pub fn run(&mut self) {
        while self.pos < self.__len {
            let c: i64 = DxNative::char_at(&self.src, self.pos);
            if c == 123 {
                self.stack.push(self.st.clone());
                self.st = self.st.borrow().copy();
                self.pos += 1;
            } else {
                if c == 125 {
                    self.endGroup();
                    self.pos += 1;
                } else {
                    if c == 92 {
                        self.controlWord();
                    } else {
                        if c == 10 || c == 13 {
                            self.pos += 1;
                        } else {
                            self.textRun();
                        }
                    }
                }
            }
        }
    }
    /// Plain text up to the next special character, copied as a run.
    pub fn textRun(&mut self) {
        let start: i64 = self.pos;
        while self.pos < self.__len {
            let c: i64 = DxNative::char_at(&self.src, self.pos);
            if ((c == 123 || c == 125) || c == 92) || (c == 10 || c == 13) {
                break;
            }
            if c >= 128 {
                break;
            }
            self.pos += 1;
        }
        if self.pos > start {
            let mut s: String = DxNative::substr(&self.src, start, self.pos);
            if self.skipChars > 0 {
                let n: i64 = DxNative::str_len(&s);
                if self.skipChars >= n {
                    self.skipChars -= n;
                    return;
                }
                s = DxNative::substr(&s, self.skipChars, n);
                self.skipChars = 0;
            }
            self.text(&s);
            return;
        }
        // A byte above 127 written raw: read as code page 1252.
        let b: i64 = DxNative::char_at(&self.src, self.pos);
        self.pos += 1;
        self.codeUnit(b);
    }
    pub fn codeUnit(&mut self, b: i64) {
        if self.skipChars > 0 {
            self.skipChars -= 1;
            return;
        }
        self.text(&EVGCodepoint::toStr(RtfReader::cp1252(b)).clone());
    }
    pub fn controlWord(&mut self) {
        self.pos += 1;
        if self.pos >= self.__len {
            return;
        }
        let c: i64 = DxNative::char_at(&self.src, self.pos);
        if !RtfReader::isLetter(c) {
            self.pos += 1;
            self.controlSymbol(c);
            return;
        }
        let start: i64 = self.pos;
        while self.pos < self.__len && RtfReader::isLetter(DxNative::char_at(&self.src, self.pos)) {
            self.pos += 1;
        }
        let word: String = DxNative::substr(&self.src, start, self.pos);
        let mut has_param: bool = false;
        let mut param: i64 = 0;
        let mut neg: bool = false;
        if self.pos < self.__len && DxNative::char_at(&self.src, self.pos) == 45 {
            if self.pos + 1 < self.__len && RtfReader::isDigit(DxNative::char_at(&self.src, self.pos + 1)) {
                neg = true;
                self.pos += 1;
            }
        }
        while self.pos < self.__len && RtfReader::isDigit(DxNative::char_at(&self.src, self.pos)) {
            has_param = true;
            param = param * 10 + (DxNative::char_at(&self.src, self.pos) - 48);
            self.pos += 1;
        }
        if neg {
            param = 0 - param;
        }
        if self.pos < self.__len && DxNative::char_at(&self.src, self.pos) == 32 {
            self.pos += 1;
        }
        self.word(&word, has_param, param);
    }
    pub fn controlSymbol(&mut self, c: i64) {
        // \'hh
        if c == 39 {
            if self.pos + 1 < self.__len {
                let hi: i64 = RtfReader::hexVal(DxNative::char_at(&self.src, self.pos));
                let lo: i64 = RtfReader::hexVal(DxNative::char_at(&self.src, self.pos + 1));
                self.pos += 2;
                if hi >= 0 && lo >= 0 {
                    self.codeUnit(hi * 16 + lo);
                }
            }
            return;
        }
        if (c == 92 || c == 123) || c == 125 {
            self.text(&char::from_u32(c as u32).unwrap_or('\0').to_string());
            return;
        }
        if c == 126 {
            self.text(&EVGCodepoint::toStr(160).clone());
            return;
        }
        if c == 95 {
            self.text("-");
            return;
        }
        if c == 42 {
            // \* : a destination this reader may not know. The word after it
            // decides; an unknown one is skipped whole.
            self.starDestination();
            return;
        }
        if c == 10 || c == 13 {
            self.word("par", false, 0);
            return;
        }
    }
    /// After `\*`: the few starred destinations whose content is wanted, and
    /// everything else skipped.
    pub fn starDestination(&mut self) {
        let save: i64 = self.pos;
        if self.pos < self.__len && DxNative::char_at(&self.src, self.pos) == 92 {
            let mut p: i64 = self.pos + 1;
            let s: i64 = p;
            while p < self.__len && RtfReader::isLetter(DxNative::char_at(&self.src, p)) {
                p += 1;
            }
            let w: String = DxNative::substr(&self.src, s, p);
            if w == "shppict" {
                // the picture Word writes for a shape: kept, as a plain group
                self.pos = p;
                return;
            }
            if (w == "fldinst" || w == "listtable") || w == "listoverridetable" {
                return;
            }
        }
        self.pos = save;
        self.beginDest(RtfDest::skip());
    }
    pub fn beginDest(&mut self, d: i64) {
        self.st.borrow_mut().dest = d;
        self.st.borrow_mut().startsDest = true;
        if RtfReader::collects(d) {
            self.collected = String::new();
        }
    }
    pub fn endGroup(&mut self) {
        if self.st.borrow().startsDest {
            self.finishDest(&self.st.clone());
        }
        let n: i64 = self.stack.len() as i64;
        if n == 0 {
            return;
        }
        let mut outer: Rc<RefCell<RtfState>> = self.stack[(n - 1) as usize].clone();
        self.stack.pop();
        // A header's paragraph that never got its \par still ends with it.
        if self.st.borrow()
            .target != outer.borrow()
            .target && self.st.borrow()
            .target > 0 {
            self.endParagraphOf(self.st.borrow().target.clone(), false);
        }
        self.st = outer.clone();
    }
    pub fn finishDest(&mut self, s: &Rc<RefCell<RtfState>>) {
        let d: i64 = s.borrow().dest;
        if d == RtfDest::fontTable() {
            self.endFontEntry();
        }
        if d == RtfDest::styleSheet() {
            self.endStyleEntry();
        }
        if d == RtfDest::listText() {
            self.lastListText = self.collected.clone();
        }
        if d == RtfDest::fieldInst() {
            self.fieldInst = self.collected.clone();
        }
        if d == RtfDest::picture() {
            self.endPicture();
        }
        if d == RtfDest::title() {
            self.doc.borrow_mut().title = self.collected.clone();
        }
        if d == RtfDest::levelText() {
            self.levelText = self.collected.clone();
        }
        if RtfReader::collects(d) {
            self.collected = String::new();
        }
    }
    /// ------------------------------------------------------------------
    /// Control words
    /// ------------------------------------------------------------------
    pub fn word(&mut self, w: &str, has: bool, p: i64) {
        let d: i64 = self.st.borrow().dest;
        if d == RtfDest::skip() {
            return;
        }
        if self.destinationWord(w) {
            return;
        }
        if d == RtfDest::fontTable() {
            if w == "f" {
                self.entryIndex = p;
            }
            return;
        }
        if d == RtfDest::colorTable() {
            if w == "red" {
                self.red = p;
                self.colorSet = true;
            }
            if w == "green" {
                self.green = p;
                self.colorSet = true;
            }
            if w == "blue" {
                self.blue = p;
                self.colorSet = true;
            }
            return;
        }
        if d == RtfDest::styleSheet() {
            if w == "s" {
                self.entryIndex = p;
            }
            if (w == "cs" || w == "ds") || w == "ts" {
                self.entryIndex = -1;
            }
            return;
        }
        if d == RtfDest::listTable() {
            if w == "list" {
                self.listLevel = -1;
                self.firstNfc = -1;
                self.curListDef = Rc::new(RefCell::new(RtfListDef::new()));
                self.levelNfc = -1;
                self.levelText = String::new();
            }
            if w == "listlevel" {
                self.flushLevel();
                self.listLevel += 1;
            }
            if w == "levelnfc" || w == "levelnfcn" {
                self.levelNfc = p;
                if self.listLevel == 0 {
                    self.firstNfc = p;
                }
            }
            if w == "leveltext" {
                self.beginDest(RtfDest::levelText());
            }
            if w == "listid" {
                self.flushLevel();
                self.listNfc.insert(p, self.firstNfc);
                self.curListDef.borrow_mut().firstNfc = self.firstNfc;
                self.listDefs.insert(p, self.curListDef.clone());
                self.curListDef = Rc::new(RefCell::new(RtfListDef::new()));
                self.listLevel = -1;
            }
            return;
        }
        if d == RtfDest::levelText() {
            self.special(w, p);
            return;
        }
        if d == RtfDest::overrideTable() {
            if w == "listid" {
                self.pendingListId = p;
            }
            if w == "ls" {
                self.lsToList.insert(p, self.pendingListId);
            }
            return;
        }
        if d == RtfDest::picture() {
            self.pictureWord(w, p);
            return;
        }
        if d == RtfDest::listText() || d == RtfDest::fieldInst() {
            self.special(w, p);
            return;
        }
        if d == RtfDest::title() {
            self.special(w, p);
            return;
        }
        self.bodyWord(w, has, p);
    }
    /// Words that open a destination. True when `w` was one.
    pub fn destinationWord(&mut self, w: &str) -> bool {
        if w == "fonttbl" {
            self.beginDest(RtfDest::fontTable());
            return true;
        }
        if w == "colortbl" {
            self.beginDest(RtfDest::colorTable());
            self.red = 0;
            self.green = 0;
            self.blue = 0;
            self.colorSet = false;
            return true;
        }
        if w == "stylesheet" {
            self.beginDest(RtfDest::styleSheet());
            return true;
        }
        if w == "listtable" {
            self.beginDest(RtfDest::listTable());
            return true;
        }
        if w == "listoverridetable" {
            self.beginDest(RtfDest::overrideTable());
            return true;
        }
        if w == "listtext" || w == "pntext" {
            self.beginDest(RtfDest::listText());
            return true;
        }
        if w == "fldinst" {
            self.beginDest(RtfDest::fieldInst());
            return true;
        }
        if w == "fldrslt" {
            let inst: String = RtfReader::trim(&self.fieldInst.clone());
            if DxNative::index_of(&inst, "HYPERLINK") == 0 {
                self.st.borrow_mut().hyperlink = RtfReader::linkOf(&inst);
            }
            return true;
        }
        if w == "pict" {
            self.beginDest(RtfDest::picture());
            self.picHex = String::new();
            self.picKind = String::new();
            self.picW = 0;
            self.picH = 0;
            self.picGoalW = 0;
            self.picGoalH = 0;
            self.picScaleX = 100;
            self.picScaleY = 100;
            return true;
        }
        if w == "title" {
            self.beginDest(RtfDest::title());
            return true;
        }
        if w == "header" || w == "headerr" {
            if self.sawHeader {
                self.beginDest(RtfDest::skip());
            } else {
                self.sawHeader = true;
                let mut hsec: Rc<RefCell<DocSection>> = self.doc.borrow().sectionAt(self.curSection.clone());
                hsec.borrow_mut().headerParas.clear();
                self.st.borrow_mut().target = 1;
                self.st.clone().borrow_mut().resetPara();
            }
            return true;
        }
        if w == "footer" || w == "footerr" {
            if self.sawFooter {
                self.beginDest(RtfDest::skip());
            } else {
                self.sawFooter = true;
                let mut fsec: Rc<RefCell<DocSection>> = self.doc.borrow().sectionAt(self.curSection.clone());
                fsec.borrow_mut().footerParas.clear();
                self.st.borrow_mut().target = 2;
                self.st.clone().borrow_mut().resetPara();
            }
            return true;
        }
        let mut skipped: Vec<String> = vec!["info".to_string(), "headerl".to_string(), "headerf".to_string(), "footerl".to_string(), "footerf".to_string(), "footnote".to_string(), "annotation".to_string(), "nonshppict".to_string(), "object".to_string(), "xe".to_string(), "tc".to_string(), "bkmkstart".to_string(), "bkmkend".to_string(), "author".to_string(), "operator".to_string(), "subject".to_string(), "keywords".to_string(), "comment".to_string(), "doccomm".to_string(), "pn".to_string(), "shp".to_string(), "shpinst".to_string(), "fldtype".to_string(), "datafield".to_string(), "template".to_string(), "revtbl".to_string(), "rsidtbl".to_string(), "generator".to_string(), "filetbl".to_string(), "pgdsctbl".to_string(), "colorschememapping".to_string(), "themedata".to_string(), "latentstyles".to_string(), "xmlnstbl".to_string(), "mmathPr".to_string(), "falt".to_string(), "panose".to_string(), "listpicture".to_string()];
        for k in skipped.iter().cloned() {
            if k == w {
                if w == "info" {
                    // the title is in here; the rest is not
                    self.st.borrow_mut().dest = RtfDest::skip();
                    self.st.borrow_mut().startsDest = true;
                    self.infoTitle();
                }
                self.beginDest(RtfDest::skip());
                return true;
            }
        }
        false
    }
    /// `{\info ... {\title X} ...}`: find the title without walking the rest.
    pub fn infoTitle(&self) {
        let at: i64 = DxNative::index_of_from(&self.src, "{\\title", self.pos);
        if at < 0 {
            return;
        }
        let end_info: i64 = self.groupEnd(self.pos.clone());
        if end_info > 0 && at > end_info {
            return;
        }
        let s: i64 = at + 7;
        let e: i64 = DxNative::index_of_from(&self.src, "}", s);
        if e > s {
            self.doc.borrow_mut().title = RtfReader::trim(&DxNative::substr(&self.src, s, e));
        }
    }
    /// The index of the `}` closing the group `from` is inside.
    pub fn groupEnd(&self, from: i64) -> i64 {
        let mut depth: i64 = 1;
        let mut i: i64 = from;
        while i < self.__len {
            let c: i64 = DxNative::char_at(&self.src, i);
            if c == 92 {
                i += 2;
            } else {
                if c == 123 {
                    depth += 1;
                }
                if c == 125 {
                    depth -= 1;
                    if depth <= 0 {
                        return i;
                    }
                }
                i += 1;
            }
        }
        -1
    }
    /// Character words inside a destination whose text is collected (a
    /// list marker, a field instruction, a title).
    pub fn special(&mut self, w: &str, p: i64) {
        if w == "u" {
            self.unicodeChar(p);
            return;
        }
        if w == "uc" {
            self.st.borrow_mut().ucSkip = p;
            return;
        }
        if w == "tab" || w == "line" {
            self.text(" ");
            return;
        }
        self.namedChar(w);
    }
    pub fn unicodeChar(&mut self, p: i64) {
        let mut cp: i64 = p;
        if cp < 0 {
            cp += 65536;
        }
        self.text(&EVGCodepoint::toStr(cp).clone());
        self.skipChars = self.st.borrow().ucSkip;
    }
    /// \bullet, \emdash and the quotes: words that stand for a character.
    pub fn namedChar(&mut self, w: &str) -> bool {
        let mut cp: i64 = -1;
        if w == "bullet" {
            cp = 8226;
        }
        if w == "emdash" {
            cp = 8212;
        }
        if w == "endash" {
            cp = 8211;
        }
        if w == "lquote" {
            cp = 8216;
        }
        if w == "rquote" {
            cp = 8217;
        }
        if w == "ldblquote" {
            cp = 8220;
        }
        if w == "rdblquote" {
            cp = 8221;
        }
        if w == "emspace" {
            cp = 8195;
        }
        if w == "enspace" {
            cp = 8194;
        }
        if w == "qmspace" {
            cp = 8197;
        }
        if cp < 0 {
            return false;
        }
        self.text(&EVGCodepoint::toStr(cp).clone());
        true
    }
    pub fn bodyWord(&mut self, w: &str, has: bool, p: i64) {
        if w == "par" {
            self.endParagraph(true);
            return;
        }
        if w == "cell" {
            self.endCell();
            return;
        }
        if w == "row" {
            self.endRow();
            return;
        }
        if w == "page" {
            self.endParagraph(false);
            if self.st.borrow().target == 0 {
                self.closeTable();
                self.doc.clone().borrow_mut().addPageBreak();
            }
            return;
        }
        if w == "tab" || w == "line" {
            self.text(" ");
            return;
        }
        if w == "u" {
            self.unicodeChar(p);
            return;
        }
        if w == "uc" {
            self.st.borrow_mut().ucSkip = p;
            return;
        }
        if self.namedChar(w) {
            return;
        }
        // character formatting
        let on: bool = !has || p != 0;
        if w == "plain" {
            self.st.clone().borrow_mut().resetChar();
            self.st.borrow_mut().font = self.defaultFont;
            return;
        }
        if w == "b" {
            self.st.borrow_mut().bold = on;
            return;
        }
        if w == "i" {
            self.st.borrow_mut().italic = on;
            return;
        }
        if (w == "ul" || w == "uld") || (w == "uldb" || w == "ulw") {
            self.st.borrow_mut().underline = on;
            return;
        }
        if w == "ulnone" {
            self.st.borrow_mut().underline = false;
            return;
        }
        if w == "strike" || w == "striked" {
            self.st.borrow_mut().strike = on;
            return;
        }
        if w == "super" {
            self.st.borrow_mut().vertAlign = 1;
            return;
        }
        if w == "sub" {
            self.st.borrow_mut().vertAlign = 2;
            return;
        }
        if w == "nosupersub" {
            self.st.borrow_mut().vertAlign = 0;
            return;
        }
        if w == "fs" {
            if p > 0 {
                self.st.borrow_mut().halfPoints = p;
            }
            return;
        }
        if w == "f" {
            self.st.borrow_mut().font = p;
            return;
        }
        if w == "cf" {
            self.st.borrow_mut().color = p;
            return;
        }
        if w == "highlight" || w == "cb" {
            self.st.borrow_mut().highlight = p;
            return;
        }
        // paragraph formatting
        if w == "pard" {
            self.st.clone().borrow_mut().resetPara();
            return;
        }
        if w == "ql" {
            self.st.borrow_mut().align = 0;
            return;
        }
        if w == "qc" {
            self.st.borrow_mut().align = 1;
            return;
        }
        if w == "qr" {
            self.st.borrow_mut().align = 2;
            return;
        }
        if w == "qj" {
            self.st.borrow_mut().align = 3;
            return;
        }
        if w == "li" || w == "lin" {
            self.st.borrow_mut().leftTw = p;
            return;
        }
        if w == "ri" || w == "rin" {
            self.st.borrow_mut().rightTw = p;
            return;
        }
        if w == "fi" {
            self.st.borrow_mut().firstTw = p;
            return;
        }
        if w == "sb" {
            self.st.borrow_mut().beforeTw = p;
            return;
        }
        if w == "sa" {
            self.st.borrow_mut().afterTw = p;
            return;
        }
        if w == "sl" {
            self.st.borrow_mut().lineTw = p;
            return;
        }
        if w == "slmult" {
            self.st.borrow_mut().lineMult = p == 1;
            return;
        }
        if w == "intbl" {
            self.st.borrow_mut().inTable = true;
            return;
        }
        if w == "pagebb" {
            self.st.borrow_mut().pageBreakBefore = true;
            return;
        }
        if w == "s" {
            self.st.borrow_mut().style = p;
            return;
        }
        if w == "ls" {
            self.st.borrow_mut().ls = p;
            return;
        }
        if w == "ilvl" {
            self.st.borrow_mut().ilvl = p;
            return;
        }
        if w == "deff" {
            self.defaultFont = p;
            self.st.borrow_mut().font = p;
            return;
        }
        self.tableWord(w, p);
        self.pageWord(w, p);
    }
    pub fn tableWord(&mut self, w: &str, p: i64) {
        if w == "trowd" {
            self.rowDefs.clear();
            self.pendingCell = Rc::new(RefCell::new(RtfCellDef::new()));
            self.borderSide = 0;
            self.rowHeightTw = 0;
            self.rowLeftTw = 0;
            return;
        }
        if w == "trleft" {
            self.rowLeftTw = p;
            return;
        }
        if w == "trrh" {
            if p < 0 {
                self.rowHeightTw = 0 - p;
            } else {
                self.rowHeightTw = p;
            }
            return;
        }
        if w == "cellx" {
            self.pendingCell.borrow_mut().rightTw = p;
            self.rowDefs.push(self.pendingCell.clone());
            self.pendingCell = Rc::new(RefCell::new(RtfCellDef::new()));
            self.borderSide = 0;
            return;
        }
        if w == "clmrg" {
            self.pendingCell.borrow_mut().merged = true;
            return;
        }
        if w == "clcbpat" {
            self.pendingCell.borrow_mut().fill = p;
            return;
        }
        if w == "clbrdrt" {
            self.borderSide = 1;
            self.pendingCell.borrow_mut()
                .borderTop.borrow_mut()
                .style = "single".to_string();
            return;
        }
        if w == "clbrdrb" {
            self.borderSide = 2;
            self.pendingCell.borrow_mut()
                .borderBottom.borrow_mut()
                .style = "single".to_string();
            return;
        }
        if w == "clbrdrl" {
            self.borderSide = 3;
            self.pendingCell.borrow_mut()
                .borderLeft.borrow_mut()
                .style = "single".to_string();
            return;
        }
        if w == "clbrdrr" {
            self.borderSide = 4;
            self.pendingCell.borrow_mut()
                .borderRight.borrow_mut()
                .style = "single".to_string();
            return;
        }
        if self.borderSide > 0 {
            let mut b: Rc<RefCell<DocTableBorder>> = self.sideBorder();
            if w == "brdrnone" || w == "brdrnil" {
                b.borrow_mut().style = "none".to_string();
            }
            if w == "brdrdb" {
                b.borrow_mut().style = "double".to_string();
            }
            if w == "brdrdot" {
                b.borrow_mut().style = "dotted".to_string();
            }
            if w == "brdrdash" {
                b.borrow_mut().style = "dashed".to_string();
            }
            if w == "brdrw" {
                // twips → eighths of a point
                let mut sz: i64 = (p * 2 + 2) / 5;
                if sz < 2 {
                    sz = 2;
                }
                b.borrow_mut().sz = sz;
            }
            if w == "brdrcf" {
                let hex: String = self.colorHex(p);
                if (DxNative::str_len(&hex)) == 6 {
                    b.borrow_mut().colorHex = hex.clone();
                }
            }
        }
    }
    pub fn sideBorder(&self) -> Rc<RefCell<DocTableBorder>> {
        if self.borderSide == 1 {
            return self.pendingCell.borrow().borderTop.clone();
        }
        if self.borderSide == 2 {
            return self.pendingCell.borrow().borderBottom.clone();
        }
        if self.borderSide == 3 {
            return self.pendingCell.borrow().borderLeft.clone();
        }
        self.pendingCell.borrow().borderRight.clone()
    }
    /// Page words. The document's (\paperw) are every section's default; a
    /// section's own (\pgwsxn) are for the section being read.
    pub fn pageWord(&mut self, w: &str, p: i64) {
        let mut sec: Rc<RefCell<DocSection>> = self.doc.borrow().sectionAt(self.curSection.clone());
        let mut is_doc: bool = false;
        if w == "paperw" || w == "pgwsxn" {
            sec.borrow_mut().pageWTwips = p;
            is_doc = w == "paperw";
        }
        if w == "paperh" || w == "pghsxn" {
            sec.borrow_mut().pageHTwips = p;
            is_doc = w == "paperh";
        }
        if w == "margl" || w == "marglsxn" {
            sec.borrow_mut().marginLeftTwips = p;
            is_doc = w == "margl";
        }
        if w == "margr" || w == "margrsxn" {
            sec.borrow_mut().marginRightTwips = p;
            is_doc = w == "margr";
        }
        if w == "margt" || w == "margtsxn" {
            sec.borrow_mut().marginTopTwips = p;
            is_doc = w == "margt";
        }
        if w == "margb" || w == "margbsxn" {
            sec.borrow_mut().marginBottomTwips = p;
            is_doc = w == "margb";
        }
        if w == "landscape" || w == "lndscpsxn" {
            sec.borrow_mut().landscape = true;
            is_doc = w == "landscape";
        }
        if is_doc {
            RtfReader::copyPage(&sec, &self.pageDefaults.clone());
        }
        if w == "sect" {
            self.endParagraph(false);
            self.closeTable();
            self.newSection();
        }
    }
    /// A \sect. Nothing written since the last one is not a section, and a
    /// page break just before it is the same page break.
    pub fn newSection(&mut self) {
        let n: i64 = self.doc.borrow().blocks.len() as i64;
        if n == self.blocksAtSection {
            return;
        }
        if n > 0 {
            let mut last: Rc<RefCell<DocBlock>> = self.doc.borrow()
                .blocks[(n - 1) as usize].clone();
            if last.borrow().kind == DocBlockKind::pageBreak() {
                self.doc.borrow_mut().blocks.pop();
            }
        }
        let mut prev_sec: Rc<RefCell<DocSection>> = self.doc.borrow().sectionAt(self.curSection.clone());
        self.curSection = self.doc.borrow_mut().addSectionBreak();
        let mut sec: Rc<RefCell<DocSection>> = self.doc.borrow().sectionAt(self.curSection.clone());
        RtfReader::copyPage(&self.pageDefaults.clone(), &sec);
        // a section with no header of its own shows the one before it
        let __it_hp = prev_sec.borrow().headerParas.clone();
        for hp in __it_hp.iter().cloned() {
            sec.borrow_mut().headerParas.push(hp.clone());
        }
        let __it_fp = prev_sec.borrow().footerParas.clone();
        for fp in __it_fp.iter().cloned() {
            sec.borrow_mut().footerParas.push(fp.clone());
        }
        self.sawHeader = false;
        self.sawFooter = false;
        self.blocksAtSection = self.doc.borrow().blocks.len() as i64;
    }
    pub fn pictureWord(&mut self, w: &str, p: i64) {
        if w == "pngblip" {
            self.picKind = "png".to_string();
        }
        if w == "jpegblip" {
            self.picKind = "jpeg".to_string();
        }
        if w == "picw" {
            self.picW = p;
        }
        if w == "pich" {
            self.picH = p;
        }
        if w == "picwgoal" {
            self.picGoalW = p;
        }
        if w == "pichgoal" {
            self.picGoalH = p;
        }
        if w == "picscalex" {
            self.picScaleX = p;
        }
        if w == "picscaley" {
            self.picScaleY = p;
        }
        if w == "bin" {
            // raw binary of `p` bytes: rare, and not hex, so it is stepped over
            self.pos += p;
        }
    }
    /// ------------------------------------------------------------------
    /// Text
    /// ------------------------------------------------------------------
    pub fn text(&mut self, s: &str) {
        let d: i64 = self.st.borrow().dest;
        if d == RtfDest::body() {
            self.bodyText(s);
            return;
        }
        if d == RtfDest::picture() {
            self.picHex = format!("{}{}", self.picHex, s);
            return;
        }
        if ((d == RtfDest::fontTable() || d == RtfDest::styleSheet()) || (d == RtfDest::listText() || d == RtfDest::fieldInst())) || (d == RtfDest::title() || d == RtfDest::levelText()) {
            self.collected = format!("{}{}", self.collected, s);
            if d == RtfDest::fontTable() || d == RtfDest::styleSheet() {
                if DxNative::index_of(&self.collected, ";") >= 0 {
                    if d == RtfDest::fontTable() {
                        self.endFontEntry();
                    } else {
                        self.endStyleEntry();
                    }
                }
            }
            return;
        }
        if d == RtfDest::colorTable() {
            // each `;` ends an entry; the first, empty one is "auto"
            let n: i64 = DxNative::str_len(&s);
            let mut i: i64 = 0;
            while i < n {
                if DxNative::char_at(&s, i) == 59 {
                    if self.colorSet {
                        let mut _tmp_4294 = self.red.clone();
                        let mut _tmp_4295 = self.green.clone();
                        let mut _tmp_4297 = format!("{}{}{}", RtfReader::hex2(_tmp_4294), RtfReader::hex2(_tmp_4295), RtfReader::hex2(self.blue.clone()));
                        self.colors.push(_tmp_4297);
                    } else {
                        self.colors.push(String::new().to_string());
                    }
                    self.red = 0;
                    self.green = 0;
                    self.blue = 0;
                    self.colorSet = false;
                }
                i += 1;
            }
        }
    }
    pub fn colorHex(&self, idx: i64) -> String {
        if idx < 0 || idx >= (self.colors.len() as i64) {
            return String::new();
        }
        self.colors[idx as usize].clone()
    }
    pub fn endFontEntry(&mut self) {
        let mut name: String = self.collected.clone();
        let semi: i64 = DxNative::index_of(&name, ";");
        if semi >= 0 {
            name = DxNative::substr(&name, 0, semi);
        }
        name = RtfReader::trim(&name);
        if !name.is_empty() {
            self.fonts.insert(self.entryIndex, name.clone());
        }
        self.collected = String::new();
    }
    pub fn endStyleEntry(&mut self) {
        let mut name: String = self.collected.clone();
        let semi: i64 = DxNative::index_of(&name, ";");
        if semi >= 0 {
            name = DxNative::substr(&name, 0, semi);
        }
        name = RtfReader::trim(&name);
        if !name.is_empty() && self.entryIndex >= 0 {
            self.styleNames.insert(self.entryIndex, name.clone());
        }
        self.entryIndex = 0;
        self.collected = String::new();
    }
    pub fn spanNow(&self) -> Rc<RefCell<TextSpan>> {
        let mut s: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
        s.borrow_mut().bold = self.st.borrow().bold;
        s.borrow_mut().italic = self.st.borrow().italic;
        s.borrow_mut().underline = self.st.borrow().underline;
        s.borrow_mut().strike = self.st.borrow().strike;
        s.borrow_mut().vertAlign = self.st.borrow().vertAlign;
        s.borrow_mut().fontSizePt = (self.st.borrow().halfPoints as f64) / 2.0;
        let fam: Option<String> = self.fonts.get(&self.st.borrow().font)
            .cloned();
        if fam.is_some() {
            s.borrow_mut().fontFamily = WordStyleResolver::mapFont(&fam.clone().unwrap());
        } else {
            s.borrow_mut().fontFamily = WordStyleResolver::mapFont("Times New Roman");
        }
        let col: String = self.colorHex(self.st.borrow().color.clone());
        if (DxNative::str_len(&col)) == 6 {
            s.borrow_mut().colorHex = col.clone();
        }
        if self.st.borrow().highlight > 0 {
            let mut _tmp_4308 = self.colorHex(self.st.borrow().highlight.clone());
            s.borrow_mut().highlight = OdtReader::highlightOf(&_tmp_4308);
        }
        s.borrow_mut().hyperlink = self.st.borrow().hyperlink.clone();
        s
    }
    pub fn bodyText(&mut self, s: &str) {
        let t: i64 = self.st.borrow().target;
        if !(self.curOpen[t as usize]) {
            self.openParagraph(t);
        }
        let mut p: Rc<RefCell<Paragraph>> = self.cur[t as usize].clone();
        p.borrow_mut().appendText(s, &self.spanNow());
    }
    pub fn openParagraph(&mut self, t: i64) {
        let mut p: Rc<RefCell<Paragraph>> = Rc::new(RefCell::new(Paragraph::new()));
        p.borrow_mut().spans.clear();
        self.cur[t as usize] = p.clone();
        self.curOpen[t as usize] = true;
    }
    /// ------------------------------------------------------------------
    /// Paragraphs
    /// ------------------------------------------------------------------
    pub fn endParagraph(&mut self, explicit: bool) {
        self.endParagraphOf(self.st.borrow().target.clone(), explicit);
    }
    /// `explicit`: a \par, which ends a paragraph even when it is empty.
    pub fn endParagraphOf(&mut self, t: i64, explicit: bool) {
        let open: bool = self.curOpen[t as usize];
        if !open && !explicit {
            return;
        }
        if !open {
            self.openParagraph(t);
        }
        let mut p: Rc<RefCell<Paragraph>> = self.cur[t as usize].clone();
        self.curOpen[t as usize] = false;
        self.formatParagraph(&p);
        if p.borrow().spans.is_empty() {
            p.borrow_mut().spans.push(self.spanNow());
        }
        let mut sec: Rc<RefCell<DocSection>> = self.doc.borrow().sectionAt(self.curSection.clone());
        if t == 1 {
            self.adopt(p.clone());
            sec.borrow_mut().headerParas.push(p.clone());
            return;
        }
        if t == 2 {
            self.adopt(p.clone());
            sec.borrow_mut().footerParas.push(p.clone());
            return;
        }
        if self.st.borrow().inTable {
            self.adopt(p.clone());
            self.cellParas.push(p.clone());
            return;
        }
        self.closeTable();
        if p.borrow().pageBreakBefore && !self.doc.borrow().blocks.is_empty() {
            self.doc.clone().borrow_mut().addPageBreak();
        }
        p.borrow_mut().id = self.doc.borrow().nextParaId;
        self.doc.borrow_mut().nextParaId = self.doc.borrow().nextParaId + 1;
        self.doc.borrow_mut().paragraphs.push(p.clone());
        self.doc.borrow_mut().blocks.push(DocBlock::para(p.clone()));
    }
    /// A paragraph that is not a block of its own still gets an id.
    pub fn adopt(&self, p: Rc<RefCell<Paragraph>>) {
        p.borrow_mut().id = self.doc.borrow().nextParaId;
        self.doc.borrow_mut().nextParaId = self.doc.borrow().nextParaId + 1;
        self.doc.borrow_mut().paragraphs.push(p.clone());
    }
    pub fn formatParagraph(&mut self, p: &Rc<RefCell<Paragraph>>) {
        p.borrow_mut().align = self.st.borrow().align;
        p.borrow_mut().indentLeftPt = (self.st.borrow().leftTw as f64) / 20.0;
        p.borrow_mut().indentRightPt = (self.st.borrow().rightTw as f64) / 20.0;
        p.borrow_mut().indentFirstLinePt = (self.st.borrow().firstTw as f64) / 20.0;
        p.borrow_mut().spaceBeforePt = (self.st.borrow().beforeTw as f64) / 20.0;
        p.borrow_mut().spaceAfterPt = (self.st.borrow().afterTw as f64) / 20.0;
        p.borrow_mut().lineSpacing = 1.0;
        if self.st.borrow().lineMult && self.st.borrow().lineTw > 0 {
            p.borrow_mut().lineSpacing = (self.st.borrow().lineTw as f64) / 240.0;
        }
        p.borrow_mut().pageBreakBefore = self.st.borrow().pageBreakBefore;
        let _tmp_4332 = self.styleIdOf(self.st.borrow().style.clone());
        p.borrow_mut().styleId = _tmp_4332;
        if self.st.borrow().ls > 0 {
            let id: i64 = self.numIdFor(self.st.borrow().ls.clone());
            if id > 0 {
                p.borrow_mut().listNumId = id;
                p.borrow_mut().listLevel = self.st.borrow().ilvl;
            }
        }
        self.lastListText = String::new();
    }
    pub fn styleIdOf(&self, idx: i64) -> String {
        let name: Option<String> = self.styleNames.get(&idx).cloned();
        if name.is_none() {
            return "Normal".to_string();
        }
        let n: String = name.clone().unwrap().to_lowercase();
        if DxNative::index_of(&n, "heading ") == 0 {
            let lv: Option<i64> = DxNative::substr(&n, 8, (DxNative::str_len(&n))).parse::<i64>().ok();
            if lv.is_some() {
                return format!("Heading{}", lv.unwrap());
            }
        }
        if n == "title" {
            return "Title".to_string();
        }
        if n == "subtitle" {
            return "Subtitle".to_string();
        }
        "Normal".to_string()
    }
    /// The level just read into the list being defined.
    pub fn flushLevel(&mut self) {
        if self.listLevel >= 0 {
            let mut _tmp_4335 = self.levelNfc.clone();
            let mut _tmp_4336 = self.levelText.clone();
            let mut _tmp_4338 = RtfReader::levelSpec(_tmp_4335, &_tmp_4336, self.listLevel.clone());
            self.curListDef.borrow_mut().specs.push(_tmp_4338);
        }
        self.levelNfc = -1;
        self.levelText = String::new();
    }
    /// The Word list for an RTF list override, made the first time a paragraph
    /// uses it: its first level's format from the list table, or failing that
    /// from the marker text the writer put in front of the paragraph.
    pub fn numIdFor(&mut self, ls: i64) -> i64 {
        let have: Option<i64> = self.lsNumId.get(&ls).cloned();
        if have.is_some() {
            return have.unwrap();
        }
        let mut nfc: i64 = -1;
        let lid: Option<i64> = self.lsToList.get(&ls).cloned();
        if lid.is_some() {
            let mut ld: Option<Rc<RefCell<RtfListDef>>> = self.listDefs.get(&lid.unwrap())
                .cloned();
            if ld.is_some() {
                let mut defn: Rc<RefCell<RtfListDef>> = ld.clone().unwrap();
                if defn.borrow().firstNfc != 255 && !defn.borrow().specs.is_empty() {
                    let made: i64 = self.numbering.borrow_mut().newListLevels(&defn.borrow().specs.clone());
                    self.lsNumId.insert(ls, made);
                    return made;
                }
            }
            let n: Option<i64> = self.listNfc.get(&lid.unwrap()).cloned();
            if n.is_some() {
                nfc = n.unwrap();
            }
        }
        let marker: String = RtfReader::trim(&self.lastListText.clone());
        if nfc < 0 {
            nfc = RtfReader::nfcOfMarker(&marker);
        }
        let mut id: i64 = 0;
        if nfc == 255 {
            id = 0;
        } else {
            if nfc == 23 {
                id = self.numbering.borrow_mut().newList("bullet:•");
            } else {
                let mut fmt: String = "decimal";
                if nfc == 1 {
                    fmt = "upperRoman";
                }
                if nfc == 2 {
                    fmt = "lowerRoman";
                }
                if nfc == 3 {
                    fmt = "upperLetter";
                }
                if nfc == 4 {
                    fmt = "lowerLetter";
                }
                let mut suffix: String = ".".to_string();
                let ml: i64 = DxNative::str_len(&marker);
                if ml > 0 {
                    let last: i64 = DxNative::char_at(&marker, ml - 1);
                    if last == 41 || last == 46 {
                        suffix = char::from_u32(last as u32).unwrap_or('\0').to_string();
                    }
                }
                id = self.numbering.borrow_mut().newList(&format!("{}:%1{}", fmt, suffix));
            }
        }
        self.lsNumId.insert(ls, id);
        id
    }
    /// ------------------------------------------------------------------
    /// Tables
    /// ------------------------------------------------------------------
    pub fn endCell(&mut self) {
        if self.curOpen[0] {
            self.endParagraph(false);
        }
        let mut cell: Rc<RefCell<DocTableCell>> = Rc::new(RefCell::new(DocTableCell::new()));
        let __it_p = self.cellParas.clone();
        for p in __it_p.iter().cloned() {
            cell.borrow_mut().paragraphs.push(p.clone());
        }
        self.cellParas.clear();
        if cell.borrow().paragraphs.is_empty() {
            let mut e: Rc<RefCell<Paragraph>> = Rc::new(RefCell::new(Paragraph::new()));
            e.borrow_mut().spans.clear();
            e.borrow_mut().spans.push(self.spanNow());
            self.adopt(e.clone());
            cell.borrow_mut().paragraphs.push(e.clone());
        }
        self.rowCells.push(cell.clone());
    }
    /// A row's cells go into the table as they come; the columns cannot be
    /// known until the table ends, because every row states its own cell
    /// edges (a cell merged across two columns is one wide `\cellx`).
    pub fn endRow(&mut self) {
        if self.curOpen[0] || !self.cellParas.is_empty() {
            self.endCell();
        }
        if self.table.is_none() {
            let mut t: Rc<RefCell<DocumentTable>> = self.doc.borrow_mut().addTable();
            self.table = Some(t.clone());
            self.tableRows.clear();
        }
        let mut tbl: Rc<RefCell<DocumentTable>> = self.table.clone().unwrap();
        let mut row: Rc<RefCell<DocTableRow>> = Rc::new(RefCell::new(DocTableRow::new()));
        row.borrow_mut().heightTwips = self.rowHeightTw;
        let mut info: Rc<RefCell<RtfRowInfo>> = Rc::new(RefCell::new(RtfRowInfo::new()));
        info.borrow_mut().left = self.rowLeftTw;
        let __n_ci = self.rowCells.len() as i64;
        for ci in 0..__n_ci {
            let mut cell = self.rowCells[ci as usize].clone();
            let mut merged: bool = false;
            let mut right: i64 = self.rowLeftTw + 1440 * (ci + 1);
            if ci < (self.rowDefs.len() as i64) {
                let mut cd2: Rc<RefCell<RtfCellDef>> = self.rowDefs[ci as usize].clone();
                merged = cd2.borrow().merged;
                right = cd2.borrow().rightTw;
                cell.borrow_mut().borderTop = cd2.borrow().borderTop.clone();
                cell.borrow_mut().borderBottom = cd2.borrow().borderBottom.clone();
                cell.borrow_mut().borderLeft = cd2.borrow().borderLeft.clone();
                cell.borrow_mut().borderRight = cd2.borrow().borderRight.clone();
                if cd2.borrow().fill > 0 {
                    let fill: String = self.colorHex(cd2.borrow().fill.clone());
                    if (DxNative::str_len(&fill)) == 6 {
                        cell.borrow_mut().fillHex = fill.clone();
                    }
                }
            }
            if merged && !row.borrow().cells.is_empty() {
                info.borrow_mut().edges.pop();
            } else {
                row.borrow_mut().cells.push(cell.clone());
            }
            info.borrow_mut().edges.push(right);
        }
        self.rowCells.clear();
        tbl.borrow_mut().rows.push(row.clone());
        self.tableRows.push(info.clone());
    }
    pub fn closeTable(&mut self) {
        if !self.cellParas.is_empty() {
            // text after the last \cell with no \row: it is still the table's
            if self.table.is_some() {
                self.endRow();
            }
        }
        if self.table.is_some() {
            self.finishGrid(&self.table.clone().unwrap().clone());
        }
        let mut none: Option<Rc<RefCell<DocumentTable>>> = None;
        self.table = none.clone();
    }
    /// Every cell edge any row has is a column boundary; each cell spans the
    /// columns between its own edges.
    pub fn finishGrid(&self, tbl: &Rc<RefCell<DocumentTable>>) {
        let mut edges: Vec<i64> = Vec::new();
        let mut left: i64 = 0;
        let mut first: bool = true;
        let __it_info = self.tableRows.clone();
        for info in __it_info.iter().cloned() {
            if first || info.borrow().left < left {
                left = info.borrow().left;
                first = false;
            }
            let __n_ei = info.borrow().edges.len() as i64;
            for ei in 0..__n_ei {
                let mut e = info.borrow().edges[ei as usize];
                RtfReader::insertSorted(&mut edges, e);
            }
        }
        tbl.borrow_mut().colWidthsTwips.clear();
        let mut prev: i64 = left;
        for e2 in edges.iter().copied() {
            let mut w: i64 = e2 - prev;
            if w < 100 {
                w = 100;
            }
            tbl.borrow_mut().colWidthsTwips.push(w);
            prev = e2;
        }
        let ncols: i64 = edges.len() as i64;
        let __n_rj = tbl.borrow().rows.len() as i64;
        for rj in 0..__n_rj {
            let mut row = tbl.borrow().rows[rj as usize].clone();
            if rj < (self.tableRows.len() as i64) {
                let mut info2: Rc<RefCell<RtfRowInfo>> = self.tableRows[rj as usize].clone();
                let mut cell_left: i64 = info2.borrow().left;
                let mut used: i64 = 0;
                let __n_ck = row.borrow().cells.len() as i64;
                for ck in 0..__n_ck {
                    let mut cell = row.borrow().cells[ck as usize].clone();
                    let mut right: i64 = cell_left;
                    if ck < (info2.borrow().edges.len() as i64) {
                        right = info2.borrow().edges[ck as usize];
                    }
                    let mut span: i64 = 0;
                    for e3 in edges.iter().copied() {
                        if e3 > cell_left && e3 <= right {
                            span += 1;
                        }
                    }
                    if span < 1 {
                        span = 1;
                    }
                    cell.borrow_mut().gridSpan = span;
                    used += span;
                    cell_left = right;
                }
                // a row that stops short of the grid still fills it
                if used < ncols && !row.borrow().cells.is_empty() {
                    let mut last_cell: Rc<RefCell<DocTableCell>> = row.borrow().cells[((row.borrow().cells.len() as i64) - 1) as usize].clone();
                    last_cell.borrow_mut().gridSpan = last_cell.borrow().gridSpan + (ncols - used);
                }
            }
        }
    }
    /// ------------------------------------------------------------------
    /// Pictures
    /// ------------------------------------------------------------------
    pub fn endPicture(&mut self) {
        if self.picKind != "png" && self.picKind != "jpeg" {
            return;
        }
        let n: i64 = DxNative::str_len(&self.picHex);
        let mut bytes: Vec<u8> = vec![0u8; (n / 2) as usize];
        let mut count: i64 = 0;
        let mut hi: i64 = -1;
        let mut i: i64 = 0;
        while i < n {
            let v: i64 = RtfReader::hexVal(DxNative::char_at(&self.picHex, i));
            if v >= 0 {
                if hi < 0 {
                    hi = v;
                } else {
                    bytes[count as usize] = (hi * 16 + v) as u8;
                    count += 1;
                    hi = -1;
                }
            }
            i += 1;
        }
        if count < 8 {
            return;
        }
        let mut data: Vec<u8> = bytes[0..(count) as usize].to_vec();
        let mut img: Rc<RefCell<DocImage>> = Rc::new(RefCell::new(DocImage::new()));
        img.borrow_mut().relId = format!("rtf-image-{}.{}", self.doc.borrow().nextParaId, self.picKind);
        img.borrow_mut().partPath = img.borrow().relId.clone();
        let mut gw: i64 = self.picGoalW;
        let mut gh: i64 = self.picGoalH;
        if gw <= 0 {
            gw = self.picW * 1440 / 96;
        }
        if gh <= 0 {
            gh = self.picH * 1440 / 96;
        }
        gw = gw * self.picScaleX / 100;
        gh = gh * self.picScaleY / 100;
        img.borrow_mut().widthEmu = gw * 635;
        img.borrow_mut().heightEmu = gh * 635;
        img.borrow_mut().syncPx();
        WordMlParser::decodeImageBytes(&img, &data);
        if img.borrow().widthEmu <= 0 {
            img.borrow_mut().widthEmu = img.borrow().widthPx * 9525;
            img.borrow_mut().heightEmu = img.borrow().heightPx * 9525;
        }
        // the picture belongs to the paragraph it stands in
        let t: i64 = self.st.borrow().target;
        if !(self.curOpen[t as usize]) {
            self.openParagraph(t);
        }
        let mut p: Rc<RefCell<Paragraph>> = self.cur[t as usize].clone();
        p.borrow_mut().images.push(img.clone());
    }
}
