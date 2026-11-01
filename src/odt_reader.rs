// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::odt_xml::OdtNode;
use crate::odt_xml::OdtXmlParser;
use crate::rgr::DxNative;
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
use crate::rgr::{OdfPackage, OdpUnits};

/// ==============================================================================
/// OdtReader - OpenDocument text (.odt) → RichDocument
/// ==============================================================================
///
/// The container is `gallery/odf/OdfPackage.rgr`, which the .odp reader
/// stands on too, and the XML tree is OdtXml (XmlCore's parser, renamed: see
/// there why). What is
/// here is the part that is about a text document: ODF's styles resolved into
/// the formatting a `TextSpan` and a `Paragraph` hold, and the body walked into
/// blocks.
///
/// It lands in `RichDocument` and nothing else (PLAN_FORMATS.md: DOCX, ODT,
/// EPUB, HTML and Markdown converge on one model). So an .odt opens in the same
/// editor, and its save is a fresh .docx: there is no ODF writer.
///
/// WHAT IS READ. Paragraphs and headings, character formatting through spans
/// and their style chains, paragraph alignment / margins / indents / line
/// height / page breaks, lists (bullets and numbers, nested), tables (column
/// widths, spans, cell fill, borders), pictures in frames, hyperlinks, the
/// page size and margins of the first master page and its header and footer.
///
/// WHAT IS NOT. Text boxes and drawn shapes, notes (the citation stays, the
/// note body does not), tracked changes (the document as it now reads), fields
/// beyond their current text, and the ODF outline numbering of headings.
///
/// STYLES. ODF states formatting in named styles that inherit through
/// `style:parent-style-name`, with one `style:default-style` per family under
/// them all. Automatic styles (the ones in content.xml) are the same thing
/// with a generated name. Everything is resolved here, at read time, into the
/// spans: the model holds the formatting a reader sees, as it does for DOCX.
/// ==============================================================================
/// One ODF style, as far as a text document needs it. Every field has an
/// "unset" value, because a style that says nothing must not override the one
/// it inherits from.
#[derive(Clone)]
pub struct OdtStyle {
    pub name: String,
    pub family: String,
    pub parent: String,
    pub displayName: String,
    pub listStyle: String,
    pub masterPage: String,
    pub outlineLevel: i64,
    /// text
    pub fontName: String,
    pub sizePt: f64,
    pub sizePct: f64,
    pub bold: i64,
    pub italic: i64,
    pub underline: i64,
    pub strike: i64,
    pub vertAlign: i64,
    pub colorHex: String,
    pub highlight: String,
    /// paragraph (-100000 is "not stated"; an indent can be negative)
    pub align: i64,
    pub marginTop: f64,
    pub marginBottom: f64,
    pub marginLeft: f64,
    pub marginRight: f64,
    pub textIndent: f64,
    pub lineSpacing: f64,
    pub breakBefore: i64,
    pub breakAfter: i64,
    /// table cell / row / column
    pub fillHex: String,
    pub borderAll: String,
    pub borderTop: String,
    pub borderBottom: String,
    pub borderLeft: String,
    pub borderRight: String,
    pub padding: f64,
    pub columnWidthPt: f64,
    pub rowHeightPt: f64,
}

impl OdtStyle {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            family: String::new(),
            parent: String::new(),
            displayName: String::new(),
            listStyle: String::new(),
            masterPage: String::new(),
            outlineLevel: 0,
            fontName: String::new(),
            sizePt: -1.0,
            sizePct: -1.0,
            bold: -1,
            italic: -1,
            underline: -1,
            strike: -1,
            vertAlign: -1,
            colorHex: String::new(),
            highlight: String::new(),
            align: -1,
            marginTop: -100000.0,
            marginBottom: -100000.0,
            marginLeft: -100000.0,
            marginRight: -100000.0,
            textIndent: -100000.0,
            lineSpacing: -1.0,
            breakBefore: -1,
            breakAfter: -1,
            fillHex: String::new(),
            borderAll: String::new(),
            borderTop: String::new(),
            borderBottom: String::new(),
            borderLeft: String::new(),
            borderRight: String::new(),
            padding: -1.0,
            columnWidthPt: -1.0,
            rowHeightPt: -1.0,
        }
    }
}

#[derive(Clone)]
pub struct OdtListLevel {
    pub bullet: bool,
    pub bulletChar: String,
    pub numFmt: String,
    pub prefix: String,
    pub suffix: String,
    /// how many levels' numbers the marker shows: 2 is "1.1"
    pub displayLevels: i64,
}

impl OdtListLevel {
    pub fn new() -> Self {
        Self {
            bullet: true,
            bulletChar: "•".to_string(),
            numFmt: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            displayLevels: 1,
        }
    }
}

#[derive(Clone)]
pub struct OdtListStyle {
    pub name: String,
    pub levels: HashMap<i64,Rc<RefCell<OdtListLevel>>>,
}

impl OdtListStyle {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            levels: HashMap::new(),
        }
    }
    pub fn levelAt(&self, level: i64) -> Rc<RefCell<OdtListLevel>> {
        let mut lv: Option<Rc<RefCell<OdtListLevel>>> = self.levels.get(&level).cloned();
        if lv.is_none() {
            return Rc::new(RefCell::new(OdtListLevel::new()));
        }
        lv.clone().unwrap()
    }
}

#[derive(Clone)]
pub struct OdtPageLayout {
    pub widthPt: f64,
    pub heightPt: f64,
    pub marginTop: f64,
    pub marginBottom: f64,
    pub marginLeft: f64,
    pub marginRight: f64,
    pub landscape: bool,
    /// the header's height and its gap to the text, and the footer's
    pub headerExtra: f64,
    pub footerExtra: f64,
}

impl OdtPageLayout {
    pub fn new() -> Self {
        Self {
            widthPt: 612.0,
            heightPt: 792.0,
            marginTop: 72.0,
            marginBottom: 72.0,
            marginLeft: 72.0,
            marginRight: 72.0,
            landscape: false,
            headerExtra: 0.0,
            footerExtra: 0.0,
        }
    }
}

/// Whether the last thing a paragraph got was a space, which is what ODF's
/// white-space collapsing needs to know across text nodes and spans.
#[derive(Clone)]
pub struct OdtSpace {
    pub lastSpace: bool,
}

impl OdtSpace {
    pub fn new() -> Self {
        Self {
            lastSpace: true,
        }
    }
}

pub struct OdtReader {
    pkg: Rc<RefCell<OdfPackage>>,
    pub numbering: Rc<RefCell<WordNumbering>>,
    pub lastError: String,
    pub styles: HashMap<String,Rc<RefCell<OdtStyle>>>,
    pub defaults: HashMap<String,Rc<RefCell<OdtStyle>>>,
    pub listStyles: HashMap<String,Rc<RefCell<OdtListStyle>>>,
    pub fontFaces: HashMap<String,String>,
    pub pageLayouts: HashMap<String,Rc<RefCell<OdtPageLayout>>>,
    pub masterLayout: HashMap<String,String>,
    pub masterHeader: HashMap<String,Rc<RefCell<OdtNode>>>,
    pub masterFooter: HashMap<String,Rc<RefCell<OdtNode>>>,
    pub firstMaster: String,
    pub initialMaster: String,
    /// The list a `text:continue-numbering` goes on with, by list style.
    pub lastNumIdByStyle: HashMap<String,i64>,
    pub blockCount: i64,
}

impl OdtReader {
    pub fn new() -> Self {
        Self {
            pkg: Rc::new(RefCell::new(OdfPackage::new())),
            numbering: Rc::new(RefCell::new(WordNumbering::new())),
            lastError: String::new(),
            styles: HashMap::new(),
            defaults: HashMap::new(),
            listStyles: HashMap::new(),
            fontFaces: HashMap::new(),
            pageLayouts: HashMap::new(),
            masterLayout: HashMap::new(),
            masterHeader: HashMap::new(),
            masterFooter: HashMap::new(),
            firstMaster: String::new(),
            initialMaster: String::new(),
            lastNumIdByStyle: HashMap::new(),
            blockCount: 0,
        }
    }
    /// ------------------------------------------------------------------
    /// Opening
    /// ------------------------------------------------------------------
    pub fn isOdt(bytes: &[u8]) -> bool {
        OdfPackage::sniffKind(bytes) == "text"
    }
    /// ------------------------------------------------------------------
    /// Styles
    /// ------------------------------------------------------------------
    pub fn keyOf(family: &str, name: &str) -> String {
        format!("{}|{}", family, name)
    }
    pub fn unquote(s: &str) -> String {
        let t: String = OdpUnits::trimText(s);
        let n: i64 = DxNative::str_len(&t);
        if n >= 2 {
            let a: i64 = DxNative::char_at(&t, 0);
            if a == 39 || a == 34 {
                return DxNative::substr(&t, 1, n - 1);
            }
        }
        let comma: i64 = DxNative::index_of(&t, ",");
        if comma > 0 {
            return OdtReader::unquote(&DxNative::substr(&t, 0, comma));
        }
        t
    }
    pub fn hexOf(c: &str) -> String {
        if (DxNative::str_len(&c)) == 7 && DxNative::char_at(&c, 0) == 35 {
            return DxNative::substr(&c, 1, 7);
        }
        String::new()
    }
    /// A colour as one of Word's highlight names, when it is one; a highlight
    /// is a name in the model, so a colour that is not one is not invented.
    pub fn highlightOf(hex: &str) -> String {
        let h: String = hex.to_lowercase();
        if h == "ffff00" {
            return "yellow".to_string();
        }
        if h == "00ff00" {
            return "green".to_string();
        }
        if h == "00ffff" {
            return "cyan".to_string();
        }
        if h == "ff00ff" {
            return "magenta".to_string();
        }
        if h == "0000ff" {
            return "blue".to_string();
        }
        if h == "ff0000" {
            return "red".to_string();
        }
        if h == "000080" {
            return "darkBlue".to_string();
        }
        if h == "008080" {
            return "darkCyan".to_string();
        }
        if h == "008000" {
            return "darkGreen".to_string();
        }
        if h == "800080" {
            return "darkMagenta".to_string();
        }
        if h == "800000" {
            return "darkRed".to_string();
        }
        if h == "808000" {
            return "darkYellow".to_string();
        }
        if h == "808080" {
            return "darkGray".to_string();
        }
        if h == "c0c0c0" {
            return "lightGray".to_string();
        }
        String::new()
    }
    pub fn bandExtent(layout: &Rc<RefCell<OdtNode>>, band: &str) -> f64 {
        let mut b: Option<Rc<RefCell<OdtNode>>> = layout.borrow().firstChild(band);
        if b.is_none() {
            return 0.0;
        }
        let mut bn: Rc<RefCell<OdtNode>> = b.clone().unwrap();
        let mut props: Option<Rc<RefCell<OdtNode>>> = bn.borrow().firstChild("style:header-footer-properties");
        if props.is_none() {
            return 0.0;
        }
        let mut pp: Rc<RefCell<OdtNode>> = props.clone().unwrap();
        let mut h: String = pp.borrow().attr("svg:height");
        if h.is_empty() {
            h = pp.borrow().attr("fo:min-height");
        }
        // The height includes the gap to the text: ODF measures a header
        // from the margin to the body.
        OdpUnits::toPt(&h)
    }
    /// ------------------------------------------------------------------
    /// The body
    /// ------------------------------------------------------------------
    pub fn isContainer(n: &str) -> bool {
        let mut names: Vec<String> = vec!["text:section".to_string(), "text:table-of-content".to_string(), "text:index-body".to_string(), "text:index-title".to_string(), "text:illustration-index".to_string(), "text:table-index".to_string(), "text:object-index".to_string(), "text:user-index".to_string(), "text:alphabetical-index".to_string(), "text:bibliography".to_string(), "text:change-region".to_string()];
        for k in names.iter().cloned() {
            if k == n {
                return true;
            }
        }
        false
    }
    /// Elements whose current text is what the reader sees: fields and the
    /// small wrappers ODF puts around text.
    pub fn isField(n: &str) -> bool {
        let mut names: Vec<String> = vec!["text:page-number".to_string(), "text:page-count".to_string(), "text:date".to_string(), "text:time".to_string(), "text:title".to_string(), "text:subject".to_string(), "text:author-name".to_string(), "text:author-initials".to_string(), "text:chapter".to_string(), "text:file-name".to_string(), "text:sequence".to_string(), "text:sequence-ref".to_string(), "text:bookmark-ref".to_string(), "text:reference-ref".to_string(), "text:note-ref".to_string(), "text:user-defined".to_string(), "text:variable-get".to_string(), "text:variable-set".to_string(), "text:user-field-get".to_string(), "text:expression".to_string(), "text:text-input".to_string(), "text:placeholder".to_string(), "text:hidden-text".to_string(), "text:conditional-text".to_string(), "text:meta".to_string(), "text:meta-field".to_string(), "text:ruby-base".to_string(), "text:word-count".to_string(), "text:character-count".to_string(), "text:paragraph-count".to_string(), "text:initial-creator".to_string(), "text:creation-date".to_string(), "text:description".to_string(), "text:keywords".to_string(), "text:sender-firstname".to_string(), "text:sender-lastname".to_string(), "text:database-display".to_string()];
        for k in names.iter().cloned() {
            if k == n {
                return true;
            }
        }
        false
    }
    pub fn wordNumFmt(fmt: &str) -> String {
        if fmt == "a" {
            return "lowerLetter".to_string();
        }
        if fmt == "A" {
            return "upperLetter".to_string();
        }
        if fmt == "i" {
            return "lowerRoman".to_string();
        }
        if fmt == "I" {
            return "upperRoman".to_string();
        }
        "decimal".to_string()
    }
    /// One level as WordNumbering takes it: "bullet:•", "decimal:%1.%2."
    pub fn levelSpec(lv: &Rc<RefCell<OdtListLevel>>, level: i64) -> String {
        if lv.borrow().bullet {
            let mut ch: String = lv.borrow().bulletChar.clone();
            // OpenSymbol and Wingdings bullets are private-use code points that
            // no font here draws.
            let c0: i64 = DxNative::char_at(&ch, 0);
            if (c0 >= 57344 && c0 <= 63743) || (c0 == 238 || c0 == 239) {
                ch = WordNumbering::bulletAt("•", level - 1);
            }
            return format!("bullet:{}", ch);
        }
        if lv.borrow().numFmt.is_empty() {
            return String::new();
        }
        let mut shown: i64 = lv.borrow().displayLevels;
        if shown < 1 {
            shown = 1;
        }
        if shown > level {
            shown = level;
        }
        let mut text: String = lv.borrow().prefix.clone();
        let mut k: i64 = (level - shown) + 1;
        while k <= level {
            text = format!("{}%{}", text, k);
            if k < level {
                text = format!("{}.", text);
            }
            k += 1;
        }
        return format!("{}:{}{}", OdtReader::wordNumFmt(&lv.borrow().numFmt.clone()), text, lv.borrow().suffix);
    }
    /// "0.5pt solid #000000" → style, eighths of a point, colour.
    pub fn readBorder(spec: &str, into: &Rc<RefCell<DocTableBorder>>) {
        if spec.is_empty() {
            return;
        }
        if DxNative::index_of(&spec, "none") >= 0 {
            into.borrow_mut().style = "none".to_string();
            return;
        }
        into.borrow_mut().style = "single".to_string();
        if DxNative::index_of(&spec, "double") >= 0 {
            into.borrow_mut().style = "double".to_string();
        }
        if DxNative::index_of(&spec, "dotted") >= 0 {
            into.borrow_mut().style = "dotted".to_string();
        }
        if DxNative::index_of(&spec, "dashed") >= 0 {
            into.borrow_mut().style = "dashed".to_string();
        }
        let sp: i64 = DxNative::index_of(&spec, " ");
        if sp > 0 {
            let pt: f64 = OdpUnits::toPt(&DxNative::substr(&spec, 0, sp));
            let mut sz: i64 = (pt * 8.0 + 0.5).floor() as i64;
            if sz < 2 {
                sz = 2;
            }
            into.borrow_mut().sz = sz;
        }
        let hash: i64 = DxNative::index_of(&spec, "#");
        if hash >= 0 && hash + 7 <= (DxNative::str_len(&spec)) {
            into.borrow_mut().colorHex = DxNative::substr(&spec, hash + 1, hash + 7);
        }
    }
    pub fn twips(pt: f64) -> i64 {
        (pt * 20.0 + 0.5).floor() as i64
    }
    pub fn readBytes(&mut self, data: &[u8], name: String) -> Option<Rc<RefCell<RichDocument>>> {
        self.lastError = String::new();
        let mut missing: Option<Rc<RefCell<RichDocument>>> = None;
        if !OdfPackage::openBytes(&self.pkg, data, name.clone()) {
            self.lastError = format!("failed to open {}", name);
            return missing;
        }
        if OdfPackage::kind(&self.pkg) != "text" {
            self.lastError = format!("{} is not an OpenDocument text", name);
            OdfPackage::close(&self.pkg.clone());
            return missing;
        }
        let mut doc: Rc<RefCell<RichDocument>> = self.readPackage();
        OdfPackage::close(&self.pkg.clone());
        if doc.borrow().title.is_empty() {
            doc.borrow_mut().title = name.clone();
        }
        let mut out: Option<Rc<RefCell<RichDocument>>> = Some(doc.clone());
        out
    }
    pub fn parsePart(&self, part_name: &str) -> Option<Rc<RefCell<OdtNode>>> {
        let mut missing: Option<Rc<RefCell<OdtNode>>> = None;
        let xml: String = OdfPackage::readXml(&self.pkg, part_name);
        if xml.is_empty() {
            return missing;
        }
        let mut parser: Rc<RefCell<OdtXmlParser>> = Rc::new(RefCell::new(OdtXmlParser::new()));
        return parser.borrow_mut().parse(xml.clone());
    }
    pub fn readPackage(&mut self) -> Rc<RefCell<RichDocument>> {
        let mut doc: Rc<RefCell<RichDocument>> = Rc::new(RefCell::new(RichDocument::new()));
        self.numbering = Rc::new(RefCell::new(WordNumbering::new()));
        let mut styles_root: Option<Rc<RefCell<OdtNode>>> = self.parsePart("styles.xml");
        if styles_root.is_some() {
            self.collect(&styles_root.clone().unwrap());
        }
        let mut content_root: Option<Rc<RefCell<OdtNode>>> = self.parsePart("content.xml");
        if content_root.is_none() {
            self.lastError = "missing content.xml".to_string();
            return doc;
        }
        let mut cr: Rc<RefCell<OdtNode>> = content_root.clone().unwrap();
        // Automatic styles name common ones as their parents, never the other
        // way round, so they are read after and into the same table.
        self.collect(&cr);
        self.readTitle(&doc);
        let mut body: Option<Rc<RefCell<OdtNode>>> = cr.borrow().find("office:text");
        if body.is_some() {
            self.readBlocks(&doc, &body.clone().unwrap());
        }
        if doc.borrow().blocks.is_empty() {
            doc.borrow_mut().addParagraph();
        }
        self.applyPage(&doc);
        let __it_p = doc.borrow().paragraphs.clone();
        for p in __it_p.iter().cloned() {
            if p.borrow().listNumId > 0 {
                self.numbering.clone().borrow().applyToParagraph(&p);
            }
        }
        doc
    }
    pub fn readTitle(&self, doc: &Rc<RefCell<RichDocument>>) {
        let mut meta: Option<Rc<RefCell<OdtNode>>> = self.parsePart("meta.xml");
        if meta.is_none() {
            return;
        }
        let mut t: Option<Rc<RefCell<OdtNode>>> = meta.clone().unwrap().find("dc:title");
        if t.is_some() {
            doc.borrow_mut().title = OdpUnits::trimText(&t.clone().unwrap().textContent());
        }
    }
    pub fn collect(&mut self, node: &Rc<RefCell<OdtNode>>) {
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                let n: String = c.borrow().name.clone();
                if (n == "office:styles" || n == "office:automatic-styles") || n == "office:master-styles" {
                    self.collect(&c);
                }
                if n == "office:font-face-decls" {
                    self.collect(&c);
                }
                if n == "style:font-face" {
                    let mut fam: String = c.borrow().attr("svg:font-family");
                    fam = OdtReader::unquote(&fam);
                    self.fontFaces.insert(c.borrow().attr("style:name"), fam.clone());
                }
                if n == "style:style" {
                    self.addStyle(&c, false);
                }
                if n == "style:default-style" {
                    self.addStyle(&c, true);
                }
                if n == "text:list-style" {
                    self.addListStyle(&c);
                }
                if n == "style:page-layout" {
                    self.addPageLayout(&c);
                }
                if n == "style:master-page" {
                    self.addMasterPage(&c);
                }
            }
        }
    }
    pub fn addStyle(&mut self, node: &Rc<RefCell<OdtNode>>, is_default: bool) {
        let mut st: Rc<RefCell<OdtStyle>> = Rc::new(RefCell::new(OdtStyle::new()));
        st.borrow_mut().name = node.borrow().attr("style:name");
        st.borrow_mut().family = node.borrow().attr("style:family");
        st.borrow_mut().parent = node.borrow().attr("style:parent-style-name");
        st.borrow_mut().displayName = node.borrow().attr("style:display-name");
        st.borrow_mut().listStyle = node.borrow().attr("style:list-style-name");
        st.borrow_mut().masterPage = node.borrow().attr("style:master-page-name");
        let ol: String = node.borrow().attr("style:default-outline-level");
        if !ol.is_empty() {
            st.borrow_mut().outlineLevel = if ((ol.parse::<i64>().ok()).is_some()) { ((ol.parse::<i64>().ok()).unwrap()) } else { 0 };
        }
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                if c.borrow().name == "style:text-properties" {
                    self.readTextProps(&st, &c);
                }
                if c.borrow().name == "style:paragraph-properties" {
                    self.readParaProps(&st, &c);
                }
                if c.borrow().name == "style:table-cell-properties" {
                    self.readCellProps(&st, &c);
                }
                if c.borrow().name == "style:table-column-properties" {
                    let w: String = c.borrow().attr("style:column-width");
                    if !w.is_empty() {
                        st.borrow_mut().columnWidthPt = OdpUnits::toPt(&w);
                    }
                }
                if c.borrow().name == "style:table-row-properties" {
                    let mut h: String = c.borrow().attr("style:row-height");
                    if h.is_empty() {
                        h = c.borrow().attr("style:min-row-height");
                    }
                    if !h.is_empty() {
                        st.borrow_mut().rowHeightPt = OdpUnits::toPt(&h);
                    }
                }
            }
        }
        if is_default {
            self.defaults.insert(st.borrow().family.clone(), st.clone());
        } else {
            let mut _tmp_4109 = st.borrow().family.clone();
            let mut _tmp_4111 = OdtReader::keyOf(&_tmp_4109, &st.borrow().name.clone());
            self.styles.insert(_tmp_4111, st.clone());
        }
    }
    pub fn readTextProps(&mut self, st: &Rc<RefCell<OdtStyle>>, c: &Rc<RefCell<OdtNode>>) {
        let mut fn0: String = c.borrow().attr("style:font-name");
        if fn0.is_empty() {
            fn0 = OdtReader::unquote(&c.borrow().attr("fo:font-family"));
            if !fn0.is_empty() {
                self.fontFaces.insert(fn0.clone(), fn0.clone());
            }
        }
        if !fn0.is_empty() {
            st.borrow_mut().fontName = fn0.clone();
        }
        let size: String = c.borrow().attr("fo:font-size");
        if !size.is_empty() {
            if DxNative::char_at(&size, (DxNative::str_len(&size)) - 1) == 37 {
                st.borrow_mut().sizePct = OdpUnits::percent(&size, -1.0);
            } else {
                st.borrow_mut().sizePt = OdpUnits::toPt(&size);
            }
        }
        let w: String = c.borrow().attr("fo:font-weight");
        if !w.is_empty() {
            st.borrow_mut().bold = 0;
            if w == "bold" || (if ((w.parse::<f64>().ok()).is_some()) { ((w.parse::<f64>().ok()).unwrap()) } else { 400.0 }) >= 600.0 {
                st.borrow_mut().bold = 1;
            }
        }
        let it: String = c.borrow().attr("fo:font-style");
        if !it.is_empty() {
            st.borrow_mut().italic = 0;
            if it == "italic" || it == "oblique" {
                st.borrow_mut().italic = 1;
            }
        }
        let ul: String = c.borrow().attr("style:text-underline-style");
        if !ul.is_empty() {
            st.borrow_mut().underline = 1;
            if ul == "none" {
                st.borrow_mut().underline = 0;
            }
        }
        let lt: String = c.borrow().attr("style:text-line-through-style");
        if !lt.is_empty() {
            st.borrow_mut().strike = 1;
            if lt == "none" {
                st.borrow_mut().strike = 0;
            }
        }
        let pos: String = c.borrow().attr("style:text-position");
        if !pos.is_empty() {
            st.borrow_mut().vertAlign = 0;
            if DxNative::index_of(&pos, "super") == 0 {
                st.borrow_mut().vertAlign = 1;
            }
            if DxNative::index_of(&pos, "sub") == 0 {
                st.borrow_mut().vertAlign = 2;
            }
            if DxNative::char_at(&pos, 0) == 45 {
                st.borrow_mut().vertAlign = 2;
            }
            let c0: i64 = DxNative::char_at(&pos, 0);
            if (c0 >= 49 && c0 <= 57) {
                st.borrow_mut().vertAlign = 1;
            }
        }
        let col: String = OdtReader::hexOf(&c.borrow().attr("fo:color"));
        if !col.is_empty() {
            st.borrow_mut().colorHex = col.clone();
        }
        let bg: String = c.borrow().attr("fo:background-color");
        if !bg.is_empty() {
            if bg == "transparent" {
                st.borrow_mut().highlight = "-".to_string();
            } else {
                let hl: String = OdtReader::highlightOf(&OdtReader::hexOf(&bg));
                if !hl.is_empty() {
                    st.borrow_mut().highlight = hl.clone();
                }
            }
        }
    }
    pub fn readParaProps(&self, st: &Rc<RefCell<OdtStyle>>, c: &Rc<RefCell<OdtNode>>) {
        let ta: String = c.borrow().attr("fo:text-align");
        if !ta.is_empty() {
            st.borrow_mut().align = 0;
            if ta == "center" {
                st.borrow_mut().align = 1;
            }
            if ta == "end" || ta == "right" {
                st.borrow_mut().align = 2;
            }
            if ta == "justify" {
                st.borrow_mut().align = 3;
            }
        }
        let mt: String = c.borrow().attr("fo:margin-top");
        if !mt.is_empty() {
            st.borrow_mut().marginTop = OdpUnits::toPt(&mt);
        }
        let mb: String = c.borrow().attr("fo:margin-bottom");
        if !mb.is_empty() {
            st.borrow_mut().marginBottom = OdpUnits::toPt(&mb);
        }
        let ml: String = c.borrow().attr("fo:margin-left");
        if !ml.is_empty() {
            st.borrow_mut().marginLeft = OdpUnits::toPt(&ml);
        }
        let mr: String = c.borrow().attr("fo:margin-right");
        if !mr.is_empty() {
            st.borrow_mut().marginRight = OdpUnits::toPt(&mr);
        }
        let ti: String = c.borrow().attr("fo:text-indent");
        if !ti.is_empty() {
            st.borrow_mut().textIndent = OdpUnits::toPt(&ti);
        }
        let lh: String = c.borrow().attr("fo:line-height");
        if !lh.is_empty() {
            if DxNative::char_at(&lh, (DxNative::str_len(&lh)) - 1) == 37 {
                st.borrow_mut().lineSpacing = OdpUnits::percent(&lh, -1.0);
            }
            if lh == "normal" {
                st.borrow_mut().lineSpacing = 1.0;
            }
        }
        let bb: String = c.borrow().attr("fo:break-before");
        if !bb.is_empty() {
            st.borrow_mut().breakBefore = 0;
            if bb == "page" {
                st.borrow_mut().breakBefore = 1;
            }
        }
        let ba: String = c.borrow().attr("fo:break-after");
        if !ba.is_empty() {
            st.borrow_mut().breakAfter = 0;
            if ba == "page" {
                st.borrow_mut().breakAfter = 1;
            }
        }
    }
    pub fn readCellProps(&self, st: &Rc<RefCell<OdtStyle>>, c: &Rc<RefCell<OdtNode>>) {
        let bg: String = OdtReader::hexOf(&c.borrow().attr("fo:background-color"));
        if !bg.is_empty() {
            st.borrow_mut().fillHex = bg.clone();
        }
        st.borrow_mut().borderAll = c.borrow().attr("fo:border");
        st.borrow_mut().borderTop = c.borrow().attr("fo:border-top");
        st.borrow_mut().borderBottom = c.borrow().attr("fo:border-bottom");
        st.borrow_mut().borderLeft = c.borrow().attr("fo:border-left");
        st.borrow_mut().borderRight = c.borrow().attr("fo:border-right");
        let pad: String = c.borrow().attr("fo:padding");
        if !pad.is_empty() {
            st.borrow_mut().padding = OdpUnits::toPt(&pad);
        }
    }
    pub fn addListStyle(&mut self, node: &Rc<RefCell<OdtNode>>) {
        let mut ls: Rc<RefCell<OdtListStyle>> = Rc::new(RefCell::new(OdtListStyle::new()));
        ls.borrow_mut().name = node.borrow().attr("style:name");
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                let lv_num: i64 = if ((c.borrow().attr("text:level").parse::<i64>().ok()).is_some()) { ((c.borrow().attr("text:level").parse::<i64>().ok()).unwrap()) } else { 1 };
                let mut lv: Rc<RefCell<OdtListLevel>> = Rc::new(RefCell::new(OdtListLevel::new()));
                if c.borrow().name == "text:list-level-style-bullet" {
                    lv.borrow_mut().bullet = true;
                    let ch: String = c.borrow().attr("text:bullet-char");
                    if !ch.is_empty() {
                        lv.borrow_mut().bulletChar = ch.clone();
                    }
                    ls.borrow_mut().levels.insert(lv_num, lv.clone());
                }
                if c.borrow().name == "text:list-level-style-number" {
                    lv.borrow_mut().bullet = false;
                    lv.borrow_mut().numFmt = c.borrow().attr("style:num-format");
                    lv.borrow_mut().prefix = c.borrow().attr("style:num-prefix");
                    lv.borrow_mut().suffix = c.borrow().attr("style:num-suffix");
                    lv.borrow_mut().displayLevels = if ((c.borrow().attr("text:display-levels").parse::<i64>().ok()).is_some()) { ((c.borrow().attr("text:display-levels").parse::<i64>().ok()).unwrap()) } else { 1 };
                    ls.borrow_mut().levels.insert(lv_num, lv.clone());
                }
                if c.borrow().name == "text:list-level-style-image" {
                    lv.borrow_mut().bullet = true;
                    ls.borrow_mut().levels.insert(lv_num, lv.clone());
                }
            }
        }
        self.listStyles.insert(ls.borrow().name.clone(), ls.clone());
    }
    pub fn addPageLayout(&mut self, node: &Rc<RefCell<OdtNode>>) {
        let mut pl: Rc<RefCell<OdtPageLayout>> = Rc::new(RefCell::new(OdtPageLayout::new()));
        let mut props: Option<Rc<RefCell<OdtNode>>> = node.borrow().firstChild("style:page-layout-properties");
        if props.is_some() {
            let mut p: Rc<RefCell<OdtNode>> = props.clone().unwrap();
            let w: String = p.borrow().attr("fo:page-width");
            if !w.is_empty() {
                pl.borrow_mut().widthPt = OdpUnits::toPt(&w);
            }
            let h: String = p.borrow().attr("fo:page-height");
            if !h.is_empty() {
                pl.borrow_mut().heightPt = OdpUnits::toPt(&h);
            }
            let mt: String = p.borrow().attr("fo:margin-top");
            if !mt.is_empty() {
                pl.borrow_mut().marginTop = OdpUnits::toPt(&mt);
            }
            let mb: String = p.borrow().attr("fo:margin-bottom");
            if !mb.is_empty() {
                pl.borrow_mut().marginBottom = OdpUnits::toPt(&mb);
            }
            let ml: String = p.borrow().attr("fo:margin-left");
            if !ml.is_empty() {
                pl.borrow_mut().marginLeft = OdpUnits::toPt(&ml);
            }
            let mr: String = p.borrow().attr("fo:margin-right");
            if !mr.is_empty() {
                pl.borrow_mut().marginRight = OdpUnits::toPt(&mr);
            }
            pl.borrow_mut().landscape = p.borrow().attr("style:print-orientation") == "landscape";
        }
        pl.borrow_mut().headerExtra = OdtReader::bandExtent(node, "style:header-style");
        pl.borrow_mut().footerExtra = OdtReader::bandExtent(node, "style:footer-style");
        self.pageLayouts.insert(node.borrow().attr("style:name"), pl.clone());
    }
    pub fn addMasterPage(&mut self, node: &Rc<RefCell<OdtNode>>) {
        let name: String = node.borrow().attr("style:name");
        if self.firstMaster.is_empty() {
            self.firstMaster = name.clone();
        }
        self.masterLayout.insert(name.clone(), node.borrow().attr("style:page-layout-name"));
        let mut hd: Option<Rc<RefCell<OdtNode>>> = node.borrow().firstChild("style:header");
        if hd.is_some() {
            self.masterHeader.insert(name.clone(), hd.clone().unwrap());
        }
        let mut ft: Option<Rc<RefCell<OdtNode>>> = node.borrow().firstChild("style:footer");
        if ft.is_some() {
            self.masterFooter.insert(name.clone(), ft.clone().unwrap());
        }
    }
    pub fn findStyle(&self, family: &str, name: &str) -> Option<Rc<RefCell<OdtStyle>>> {
        self.styles.get(&OdtReader::keyOf(family, name)).cloned()
    }
    /// The style and its ancestors, root first, with the family's default
    /// style in front of them all.
    pub fn chainOf(&self, family: &str, name: &str) -> Vec<Rc<RefCell<OdtStyle>>> {
        let mut leaf_first: Vec<Rc<RefCell<OdtStyle>>> = Vec::new();
        let mut cur: String = name.to_string();
        let mut guard: i64 = 0;
        while !cur.is_empty() && guard < 32 {
            let mut st: Option<Rc<RefCell<OdtStyle>>> = self.findStyle(family, &cur);
            if st.is_none() {
                cur = String::new();
            } else {
                let mut s: Rc<RefCell<OdtStyle>> = st.clone().unwrap();
                leaf_first.push(s.clone());
                cur = s.borrow().parent.clone();
            }
            guard += 1;
        }
        let mut out: Vec<Rc<RefCell<OdtStyle>>> = Vec::new();
        let mut d: Option<Rc<RefCell<OdtStyle>>> = self.defaults.get(family).cloned();
        if d.is_some() {
            out.push(d.clone().unwrap());
        }
        let mut i: i64 = (leaf_first.len() as i64) - 1;
        while i >= 0 {
            out.push(leaf_first[i as usize].clone());
            i -= 1;
        }
        out
    }
    pub fn fontFamilyOf(&self, font_name: &str) -> String {
        let fam: Option<String> = self.fontFaces.get(font_name).cloned();
        if fam.is_some() {
            return WordStyleResolver::mapFont(&fam.clone().unwrap());
        }
        WordStyleResolver::mapFont(font_name)
    }
    pub fn applyText(&self, st: &Rc<RefCell<OdtStyle>>, span: &Rc<RefCell<TextSpan>>) {
        if !st.borrow().fontName.is_empty() {
            let _tmp_4144 = self.fontFamilyOf(&st.borrow().fontName.clone());
            span.borrow_mut().fontFamily = _tmp_4144;
        }
        if st.borrow().sizePt > 0.0 {
            span.borrow_mut().fontSizePt = st.borrow().sizePt;
        }
        if st.borrow().sizePct > 0.0 {
            span.borrow_mut().fontSizePt = span.borrow().fontSizePt * st.borrow().sizePct;
        }
        if st.borrow().bold >= 0 {
            span.borrow_mut().bold = st.borrow().bold == 1;
        }
        if st.borrow().italic >= 0 {
            span.borrow_mut().italic = st.borrow().italic == 1;
        }
        if st.borrow().underline >= 0 {
            span.borrow_mut().underline = st.borrow().underline == 1;
        }
        if st.borrow().strike >= 0 {
            span.borrow_mut().strike = st.borrow().strike == 1;
        }
        if st.borrow().vertAlign >= 0 {
            span.borrow_mut().vertAlign = st.borrow().vertAlign;
        }
        if !st.borrow().colorHex.is_empty() {
            span.borrow_mut().colorHex = st.borrow().colorHex.clone();
        }
        if !st.borrow().highlight.is_empty() {
            if st.borrow().highlight == "-" {
                span.borrow_mut().highlight = String::new();
            } else {
                span.borrow_mut().highlight = st.borrow().highlight.clone();
            }
        }
    }
    /// A paragraph's own formatting and the run every span in it starts from.
    /// ODF's defaults are not Word's: no space after a paragraph, single line
    /// height, 12 pt text.
    pub fn resolveParagraph(&self, p: &Rc<RefCell<Paragraph>>, style_name: &str) -> Rc<RefCell<TextSpan>> {
        let mut base: Rc<RefCell<TextSpan>> = TextSpan::create(0, 0);
        base.borrow_mut().fontSizePt = 12.0;
        base.borrow_mut().fontFamily = WordStyleResolver::mapFont("Liberation Serif");
        p.borrow_mut().spaceBeforePt = 0.0;
        p.borrow_mut().spaceAfterPt = 0.0;
        p.borrow_mut().lineSpacing = 1.0;
        p.borrow_mut().align = 0;
        p.borrow_mut().indentLeftPt = 0.0;
        p.borrow_mut().indentRightPt = 0.0;
        p.borrow_mut().indentFirstLinePt = 0.0;
        let mut chain: Vec<Rc<RefCell<OdtStyle>>> = self.chainOf("paragraph",
            style_name
        );
        for st in chain.iter().cloned() {
            self.applyText(&st, &base);
            if st.borrow().align >= 0 {
                p.borrow_mut().align = st.borrow().align;
            }
            if st.borrow().marginTop > -99999.0 {
                p.borrow_mut().spaceBeforePt = st.borrow().marginTop;
            }
            if st.borrow().marginBottom > -99999.0 {
                p.borrow_mut().spaceAfterPt = st.borrow().marginBottom;
            }
            if st.borrow().marginLeft > -99999.0 {
                p.borrow_mut().indentLeftPt = st.borrow().marginLeft;
            }
            if st.borrow().marginRight > -99999.0 {
                p.borrow_mut().indentRightPt = st.borrow().marginRight;
            }
            if st.borrow().textIndent > -99999.0 {
                p.borrow_mut().indentFirstLinePt = st.borrow().textIndent;
            }
            if st.borrow().lineSpacing > 0.0 {
                p.borrow_mut().lineSpacing = st.borrow().lineSpacing;
            }
        }
        base
    }
    pub fn breakOf(&self, style_name: &str, before: bool) -> i64 {
        let mut r: i64 = 0;
        let mut chain: Vec<Rc<RefCell<OdtStyle>>> = self.chainOf("paragraph",
            style_name
        );
        for st in chain.iter() {
            if before {
                if st.borrow().breakBefore >= 0 {
                    r = st.borrow().breakBefore;
                }
            } else {
                if st.borrow().breakAfter >= 0 {
                    r = st.borrow().breakAfter;
                }
            }
        }
        r
    }
    pub fn listStyleOfParagraph(&self, style_name: &str) -> String {
        let mut r: String = String::new();
        let mut chain: Vec<Rc<RefCell<OdtStyle>>> = self.chainOf("paragraph",
            style_name
        );
        for st in chain.iter().cloned() {
            if !st.borrow().listStyle.is_empty() {
                r = st.borrow().listStyle.clone();
            }
        }
        r
    }
    /// "Heading1".."Heading9" or "Title" when the paragraph is one, so the
    /// navigation pane and a .docx save see a heading.
    pub fn styleIdOf(&self, style_name: &str, outline: i64) -> String {
        if outline > 0 {
            return format!("Heading{}", outline);
        }
        let mut chain: Vec<Rc<RefCell<OdtStyle>>> = self.chainOf("paragraph",
            style_name
        );
        let mut i: i64 = (chain.len() as i64) - 1;
        while i >= 0 {
            let mut st: Rc<RefCell<OdtStyle>> = chain[i as usize].clone();
            let mut dn: String = st.borrow().displayName.clone();
            if dn.is_empty() {
                dn = st.borrow().name.clone();
            }
            if dn == "Title" || dn == "Subtitle" {
                return dn;
            }
            if DxNative::index_of(&dn, "Heading_20_") == 0 {
                dn = format!("Heading {}", DxNative::substr(&dn, 11, (DxNative::str_len(&dn))));
            }
            if DxNative::index_of(&dn, "Heading ") == 0 {
                let lv: Option<i64> = DxNative::substr(&dn, 8, (DxNative::str_len(&dn))).parse::<i64>().ok();
                if lv.is_some() {
                    return format!("Heading{}", lv.unwrap());
                }
            }
            if st.borrow().outlineLevel > 0 {
                return format!("Heading{}", st.borrow().outlineLevel);
            }
            i -= 1;
        }
        "Normal".to_string()
    }
    pub fn readBlocks(&mut self, doc: &Rc<RefCell<RichDocument>>, node: &Rc<RefCell<OdtNode>>) {
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                let n: String = c.borrow().name.clone();
                if n == "text:p" || n == "text:h" {
                    self.bodyParagraph(doc, &c, 0, -1);
                }
                if n == "text:list" {
                    self.readList(doc, &c, 0, "", 0, true);
                }
                if n == "table:table" {
                    self.readTable(doc, &c);
                }
                if OdtReader::isContainer(&n) && n != "text:change-region" {
                    self.readBlocks(doc, &c);
                }
            }
        }
    }
    /// A paragraph at the top level, with the page breaks its style asks for
    /// around it. `numId` 0 is not in a list; `level` is the list depth.
    pub fn bodyParagraph(&mut self, doc: &Rc<RefCell<RichDocument>>, node: &Rc<RefCell<OdtNode>>, num_id: i64, level: i64) -> Rc<RefCell<Paragraph>> {
        let style_name: String = node.borrow().attr("text:style-name");
        // A paragraph whose style names a master page starts a page of that
        // kind: the first section, or a new one.
        let mut new_section: bool = false;
        let mut own: Option<Rc<RefCell<OdtStyle>>> = self.findStyle("paragraph",
            &style_name
        );
        if own.is_some() {
            let mut own_st: Rc<RefCell<OdtStyle>> = own.clone().unwrap();
            if !own_st.borrow().masterPage.is_empty() {
                if doc.borrow().blocks.is_empty() {
                    self.initialMaster = own_st.borrow().masterPage.clone();
                } else {
                    let idx: i64 = doc.borrow_mut().addSectionBreak();
                    let mut _tmp_4163 = own_st.borrow().masterPage.clone();
                    self.applyMaster(doc, &doc.borrow().sectionAt(idx).clone(), &_tmp_4163);
                    new_section = true;
                }
            }
        }
        if self.breakOf(&style_name, true) == 1 && !new_section {
            if !doc.borrow().blocks.is_empty() {
                doc.borrow_mut().addPageBreak();
            }
        }
        let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraph();
        self.fillParagraph(doc, &p, node);
        if num_id > 0 {
            p.borrow_mut().listNumId = num_id;
            p.borrow_mut().listLevel = level;
        }
        if self.breakOf(&style_name, false) == 1 {
            doc.borrow_mut().addPageBreak();
        }
        p
    }
    pub fn fillParagraph(&self, doc: &Rc<RefCell<RichDocument>>, p: &Rc<RefCell<Paragraph>>, node: &Rc<RefCell<OdtNode>>) {
        let style_name: String = node.borrow().attr("text:style-name");
        let mut outline: i64 = 0;
        if node.borrow().name == "text:h" {
            outline = if ((node.borrow().attr("text:outline-level").parse::<i64>().ok()).is_some()) { ((node.borrow().attr("text:outline-level").parse::<i64>().ok()).unwrap()) } else { 1 };
        }
        let mut base: Rc<RefCell<TextSpan>> = self.resolveParagraph(p, &style_name);
        p.borrow_mut().styleId = self.styleIdOf(&style_name, outline);
        p.borrow_mut().spans.clear();
        p.borrow_mut().text = String::new();
        let mut state: Rc<RefCell<OdtSpace>> = Rc::new(RefCell::new(OdtSpace::new()));
        self.readInline(doc, p, node, &base, &state);
        if p.borrow().spans.is_empty() {
            // An empty paragraph still has a size: the caret and the line
            // height come from it.
            p.borrow_mut().spans.push(base.borrow().copy());
        }
    }
    /// ODF collapses white space in the markup the way HTML does: a run of
    /// spaces, tabs and line ends is one space, and none at all at the start of
    /// a paragraph.
    pub fn appendCollapsed(&self, p: &Rc<RefCell<Paragraph>>, raw: &str, span: &Rc<RefCell<TextSpan>>, state: &Rc<RefCell<OdtSpace>>) {
        let mut out: String = String::new();
        let n: i64 = DxNative::str_len(&raw);
        let mut i: i64 = 0;
        let mut run_start: i64 = 0;
        let mut last_space: bool = state.borrow().lastSpace;
        while i < n {
            let c: i64 = DxNative::char_at(&raw, i);
            if ((c == 32 || c == 9) || c == 10) || c == 13 {
                if i > run_start {
                    out = format!("{}{}", out, DxNative::substr(&raw, run_start, i));
                }
                if !last_space {
                    out = format!("{} ", out);
                }
                last_space = true;
                run_start = i + 1;
            } else {
                last_space = false;
            }
            i += 1;
        }
        if n > run_start {
            out = format!("{}{}", out, DxNative::substr(&raw, run_start, n));
        }
        state.borrow_mut().lastSpace = last_space;
        if !out.is_empty() {
            p.borrow_mut().appendText(&out, &span.borrow().copy());
        }
    }
    pub fn appendLiteral(&self, p: &Rc<RefCell<Paragraph>>, s: &str, span: &Rc<RefCell<TextSpan>>, state: &Rc<RefCell<OdtSpace>>) {
        state.borrow_mut().lastSpace = true;
        p.borrow_mut().appendText(s, &span.borrow().copy());
    }
    pub fn readInline(&self, doc: &Rc<RefCell<RichDocument>>, p: &Rc<RefCell<Paragraph>>, node: &Rc<RefCell<OdtNode>>, span: &Rc<RefCell<TextSpan>>, state: &Rc<RefCell<OdtSpace>>) {
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if c.borrow().isText {
                self.appendCollapsed(p, &c.borrow().text.clone(), span, state);
            } else {
                let n: String = c.borrow().name.clone();
                if n == "text:span" {
                    let mut inner: Rc<RefCell<TextSpan>> = span.borrow().copy();
                    let mut chain: Vec<Rc<RefCell<OdtStyle>>> = self.chainOf("text",
                        &c.borrow().attr("text:style-name")
                    );
                    for st in chain.iter().cloned() {
                        // Not the family default (it has no name): the
                        // paragraph's run already holds the defaults.
                        if !st.borrow().name.is_empty() {
                            self.applyText(&st, &inner);
                        }
                    }
                    self.readInline(doc, p, &c, &inner, state);
                }
                if n == "text:a" {
                    let mut link: Rc<RefCell<TextSpan>> = span.borrow().copy();
                    link.borrow_mut().hyperlink = c.borrow().attr("xlink:href");
                    let mut lchain: Vec<Rc<RefCell<OdtStyle>>> = self.chainOf("text",
                        &c.borrow().attr("text:style-name")
                    );
                    for st2 in lchain.iter().cloned() {
                        if !st2.borrow().name.is_empty() {
                            self.applyText(&st2, &link);
                        }
                    }
                    self.readInline(doc, p, &c, &link, state);
                }
                if n == "text:s" {
                    let count: i64 = if ((c.borrow().attr("text:c").parse::<i64>().ok()).is_some()) { ((c.borrow().attr("text:c").parse::<i64>().ok()).unwrap()) } else { 1 };
                    let mut spaces: String = String::new();
                    let mut k: i64 = 0;
                    while k < count && k < 256 {
                        spaces = format!("{} ", spaces);
                        k += 1;
                    }
                    self.appendLiteral(p, &spaces, span, state);
                }
                // A tab and a line break have no character of their own in the
                // model (the .docx reader keeps them as anchors), so they are a
                // space: the words either side stay apart.
                if n == "text:tab" || n == "text:line-break" {
                    self.appendLiteral(p, " ", span, state);
                }
                if n == "draw:frame" {
                    self.readFrame(p, &c);
                }
                if n == "draw:a" {
                    self.readInline(doc, p, &c, span, state);
                }
                if n == "text:note" {
                    let mut cit: Option<Rc<RefCell<OdtNode>>> = c.borrow().firstChild("text:note-citation");
                    if cit.is_some() {
                        let mut sup: Rc<RefCell<TextSpan>> = span.borrow().copy();
                        sup.borrow_mut().vertAlign = 1;
                        self.appendLiteral(p,
                            &cit.clone().unwrap().textContent(),
                            &sup,
                            state
                        );
                    }
                }
                if OdtReader::isField(&n) {
                    self.readInline(doc, p, &c, span, state);
                }
            }
        }
    }
    pub fn readFrame(&self, p: &Rc<RefCell<Paragraph>>, frame: &Rc<RefCell<OdtNode>>) {
        let mut img_node: Option<Rc<RefCell<OdtNode>>> = frame.borrow().firstChild("draw:image");
        if img_node.is_none() {
            return;
        }
        let mut im: Rc<RefCell<OdtNode>> = img_node.clone().unwrap();
        let href: String = im.borrow().attr("xlink:href");
        if href.is_empty() || OdfPackage::isExternalHref(&href) {
            return;
        }
        let mut img: Rc<RefCell<DocImage>> = Rc::new(RefCell::new(DocImage::new()));
        img.borrow_mut().relId = href.clone();
        img.borrow_mut().partPath = href.clone();
        let w: f64 = OdpUnits::toPt(&frame.borrow().attr("svg:width"));
        let h: f64 = OdpUnits::toPt(&frame.borrow().attr("svg:height"));
        img.borrow_mut().widthEmu = (w * 12700.0 + 0.5).floor() as i64;
        img.borrow_mut().heightEmu = (h * 12700.0 + 0.5).floor() as i64;
        img.borrow_mut().title = frame.borrow().attr("draw:name");
        let mut t: Option<Rc<RefCell<OdtNode>>> = frame.borrow().firstChild("svg:title");
        if t.is_some() {
            img.borrow_mut().title = t.clone().unwrap().textContent();
        }
        let mut d: Option<Rc<RefCell<OdtNode>>> = frame.borrow().firstChild("svg:desc");
        if d.is_some() {
            img.borrow_mut().altText = d.clone().unwrap().textContent();
        }
        img.borrow_mut().syncPx();
        WordMlParser::decodeImageBytes(&img, &OdfPackage::readBinary(&self.pkg, &href));
        if img.borrow().widthEmu == 0 {
            img.borrow_mut().widthEmu = img.borrow().widthPx * 9525;
            img.borrow_mut().heightEmu = img.borrow().heightPx * 9525;
        }
        p.borrow_mut().images.push(img.clone());
    }
    /// ------------------------------------------------------------------
    /// Lists
    /// ------------------------------------------------------------------
    /// A list and the lists inside it. Each top-level list is its own Word
    /// list (its own counter) unless it says it continues the one before.
    pub fn readList(&mut self, doc: &Rc<RefCell<RichDocument>>, node: &Rc<RefCell<OdtNode>>, level: i64, inherited_style: &str, num_id: i64, top_level: bool) {
        let mut style_name: String = node.borrow().attr("text:style-name");
        if style_name.is_empty() {
            style_name = inherited_style.to_string();
        }
        if style_name.is_empty() {
            style_name = self.firstParagraphListStyle(node);
        }
        let mut id: i64 = num_id;
        if top_level {
            let cont: bool = node.borrow().attr("text:continue-numbering") == "true" || (DxNative::str_len(&node.borrow().attr("text:continue-list"))) > 0;
            let prev: Option<i64> = self.lastNumIdByStyle.get(&style_name).cloned();
            if cont && prev.is_some() {
                id = prev.unwrap();
            } else {
                id = self.newListFor(&style_name);
            }
            if id > 0 {
                self.lastNumIdByStyle.insert(style_name.clone(), id);
            }
        }
        let __it_item = node.borrow().children.clone();
        for item in __it_item.iter().cloned() {
            if !item.borrow().isText {
                if item.borrow().name == "text:list-item" || item.borrow().name == "text:list-header" {
                    let mut marked: bool = item.borrow().name == "text:list-item";
                    let __it_c = item.borrow().children.clone();
                    for c in __it_c.iter().cloned() {
                        if !c.borrow().isText {
                            if c.borrow().name == "text:p" || c.borrow().name == "text:h" {
                                if marked && id > 0 {
                                    self.bodyParagraph(doc, &c, id, level);
                                    marked = false;
                                } else {
                                    let mut p: Rc<RefCell<Paragraph>> = self.bodyParagraph(doc,
                                        &c,
                                        0,
                                        -1
                                    );
                                    p.borrow_mut().indentLeftPt = p.borrow().indentLeftPt + 36.0 * ((level + 1) as f64);
                                }
                            }
                            if c.borrow().name == "text:list" {
                                self.readList(doc, &c, level + 1, &style_name, id, false);
                            }
                            if c.borrow().name == "table:table" {
                                self.readTable(doc, &c);
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn firstParagraphListStyle(&self, node: &Rc<RefCell<OdtNode>>) -> String {
        let mut p: Option<Rc<RefCell<OdtNode>>> = node.borrow().find("text:p");
        if p.is_none() {
            return String::new();
        }
        self.listStyleOfParagraph(&p.clone().unwrap().attr("text:style-name"))
    }
    /// A Word list with the look of the ODF list style, level by level. 0
    /// when its first level has no marker at all.
    pub fn newListFor(&self, style_name: &str) -> i64 {
        let mut ls: Option<Rc<RefCell<OdtListStyle>>> = self.listStyles.get(style_name)
            .cloned();
        if ls.is_none() {
            return self.numbering.borrow_mut().newList("bullet:•");
        }
        let mut style: Rc<RefCell<OdtListStyle>> = ls.clone().unwrap();
        let mut first: Rc<RefCell<OdtListLevel>> = style.borrow().levelAt(1);
        if !first.borrow().bullet && first.borrow().numFmt.is_empty() {
            return 0;
        }
        let mut specs: Vec<String> = Vec::new();
        let mut l: i64 = 1;
        while l <= 9 {
            let mut lv_at: Option<Rc<RefCell<OdtListLevel>>> = style.borrow().levels.get(&l).cloned();
            if lv_at.is_none() {
                l = 10;
            } else {
                let spec: String = OdtReader::levelSpec(&lv_at.clone().unwrap(), l);
                if spec.is_empty() {
                    l = 10;
                } else {
                    specs.push(spec.clone());
                    l += 1;
                }
            }
        }
        return self.numbering.borrow_mut().newListLevels(&specs);
    }
    /// ------------------------------------------------------------------
    /// Tables
    /// ------------------------------------------------------------------
    pub fn readTable(&self, doc: &Rc<RefCell<RichDocument>>, node: &Rc<RefCell<OdtNode>>) {
        let mut t: Rc<RefCell<DocumentTable>> = doc.borrow_mut().addTable();
        let mut widths: Vec<f64> = Vec::new();
        self.readColumns(node, &mut widths);
        let mut rows: Vec<Rc<RefCell<OdtNode>>> = Vec::new();
        self.collectRows(node, &mut rows);
        for r in rows.iter().cloned() {
            let mut rep: i64 = if ((r.borrow().attr("table:number-rows-repeated").parse::<i64>().ok()).is_some()) { ((r.borrow().attr("table:number-rows-repeated").parse::<i64>().ok()).unwrap()) } else { 1 };
            if rep > 50 {
                rep = 1;
            }
            let mut k: i64 = 0;
            while k < rep {
                t.borrow_mut().rows.push(self.readRow(doc, &r));
                k += 1;
            }
        }
        // Widths the file did not state share what is left of the text width.
        let mut ncols: i64 = widths.len() as i64;
        if ncols == 0 {
            if !t.borrow().rows.is_empty() {
                let mut r0: Rc<RefCell<DocTableRow>> = t.borrow().rows[0].clone();
                let __it_c0 = r0.borrow().cells.clone();
                for c0 in __it_c0.iter() {
                    let mut s: i64 = 0;
                    while s < c0.borrow().gridSpan {
                        widths.push(-1.0);
                        s += 1;
                    }
                }
            }
            ncols = widths.len() as i64;
        }
        let mut known: f64 = 0.0;
        let mut unknown: i64 = 0;
        for w in widths.iter().copied() {
            if w > 0.0 {
                known += w;
            } else {
                unknown += 1;
            }
        }
        let text_w: f64 = 451.0;
        let mut each: f64 = 72.0;
        if unknown > 0 {
            each = (text_w - known) / (unknown as f64);
            if each < 36.0 {
                each = 36.0;
            }
        }
        t.borrow_mut().colWidthsTwips.clear();
        for w2 in widths.iter().copied() {
            let mut pt: f64 = w2;
            if pt <= 0.0 {
                pt = each;
            }
            t.borrow_mut().colWidthsTwips.push((pt * 20.0 + 0.5).floor() as i64);
        }
    }
    pub fn readColumns(&self, node: &Rc<RefCell<OdtNode>>, mut widths: &mut Vec<f64>) {
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                if c.borrow().name == "table:table-column" {
                    let mut rep: i64 = if ((c.borrow().attr("table:number-columns-repeated").parse::<i64>().ok()).is_some()) { ((c.borrow().attr("table:number-columns-repeated").parse::<i64>().ok()).unwrap()) } else { 1 };
                    if rep > 64 {
                        rep = 64;
                    }
                    let mut w: f64 = -1.0;
                    let mut st: Option<Rc<RefCell<OdtStyle>>> = self.findStyle("table-column",
                        &c.borrow().attr("table:style-name")
                    );
                    if st.is_some() {
                        let mut col_st: Rc<RefCell<OdtStyle>> = st.clone().unwrap();
                        w = col_st.borrow().columnWidthPt;
                    }
                    let mut k: i64 = 0;
                    while k < rep {
                        widths.push(w);
                        k += 1;
                    }
                }
                if (c.borrow().name == "table:table-columns" || c.borrow().name == "table:table-header-columns") || c.borrow().name == "table:table-column-group" {
                    self.readColumns(&c, &mut widths);
                }
            }
        }
    }
    pub fn collectRows(&self, node: &Rc<RefCell<OdtNode>>, mut rows: &mut Vec<Rc<RefCell<OdtNode>>>) {
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                if c.borrow().name == "table:table-row" {
                    rows.push(c.clone());
                }
                if (c.borrow().name == "table:table-rows" || c.borrow().name == "table:table-header-rows") || c.borrow().name == "table:table-row-group" {
                    self.collectRows(&c, &mut rows);
                }
            }
        }
    }
    pub fn readRow(&self, doc: &Rc<RefCell<RichDocument>>, r: &Rc<RefCell<OdtNode>>) -> Rc<RefCell<DocTableRow>> {
        let mut row: Rc<RefCell<DocTableRow>> = Rc::new(RefCell::new(DocTableRow::new()));
        let mut rst: Option<Rc<RefCell<OdtStyle>>> = self.findStyle("table-row",
            &r.borrow().attr("table:style-name")
        );
        if rst.is_some() {
            let mut row_st: Rc<RefCell<OdtStyle>> = rst.clone().unwrap();
            let h: f64 = row_st.borrow().rowHeightPt;
            if h > 0.0 {
                row.borrow_mut().heightTwips = (h * 20.0 + 0.5).floor() as i64;
            }
        }
        // Covered cells after a cell that spans columns are part of it; a
        // covered cell anywhere else is under a cell spanning rows, and stays
        // as an empty cell so the columns line up.
        let mut span_left: i64 = 0;
        let __it_c = r.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                let mut rep: i64 = if ((c.borrow().attr("table:number-columns-repeated").parse::<i64>().ok()).is_some()) { ((c.borrow().attr("table:number-columns-repeated").parse::<i64>().ok()).unwrap()) } else { 1 };
                if rep > 64 {
                    rep = 1;
                }
                let mut k: i64 = 0;
                while k < rep {
                    if c.borrow().name == "table:table-cell" {
                        let mut cell: Rc<RefCell<DocTableCell>> = self.readCell(doc, &c);
                        row.borrow_mut().cells.push(cell.clone());
                        span_left = cell.borrow().gridSpan - 1;
                    }
                    if c.borrow().name == "table:covered-table-cell" {
                        if span_left > 0 {
                            span_left -= 1;
                        } else {
                            let mut empty: Rc<RefCell<DocTableCell>> = self.readCell(doc, &c);
                            row.borrow_mut().cells.push(empty.clone());
                        }
                    }
                    k += 1;
                }
            }
        }
        row
    }
    pub fn readCell(&self, doc: &Rc<RefCell<RichDocument>>, c: &Rc<RefCell<OdtNode>>) -> Rc<RefCell<DocTableCell>> {
        let mut cell: Rc<RefCell<DocTableCell>> = Rc::new(RefCell::new(DocTableCell::new()));
        cell.borrow_mut().gridSpan = if ((c.borrow().attr("table:number-columns-spanned").parse::<i64>().ok()).is_some()) { ((c.borrow().attr("table:number-columns-spanned").parse::<i64>().ok()).unwrap()) } else { 1 };
        if cell.borrow().gridSpan < 1 {
            cell.borrow_mut().gridSpan = 1;
        }
        let none: String = "none";
        cell.borrow_mut().borderTop.borrow_mut().style = none.to_string();
        cell.borrow_mut().borderBottom.borrow_mut().style = none.to_string();
        cell.borrow_mut().borderLeft.borrow_mut().style = none.to_string();
        cell.borrow_mut().borderRight.borrow_mut().style = none.to_string();
        let mut st: Option<Rc<RefCell<OdtStyle>>> = self.findStyle("table-cell",
            &c.borrow().attr("table:style-name")
        );
        if st.is_some() {
            let mut s: Rc<RefCell<OdtStyle>> = st.clone().unwrap();
            cell.borrow_mut().fillHex = s.borrow().fillHex.clone();
            let mut _tmp_4180 = s.borrow().borderAll.clone();
            OdtReader::readBorder(&_tmp_4180, &cell.borrow().borderTop.clone());
            let mut _tmp_4182 = s.borrow().borderAll.clone();
            OdtReader::readBorder(&_tmp_4182, &cell.borrow().borderBottom.clone());
            let mut _tmp_4184 = s.borrow().borderAll.clone();
            OdtReader::readBorder(&_tmp_4184, &cell.borrow().borderLeft.clone());
            let mut _tmp_4186 = s.borrow().borderAll.clone();
            OdtReader::readBorder(&_tmp_4186, &cell.borrow().borderRight.clone());
            let mut _tmp_4188 = s.borrow().borderTop.clone();
            OdtReader::readBorder(&_tmp_4188, &cell.borrow().borderTop.clone());
            let mut _tmp_4190 = s.borrow().borderBottom.clone();
            OdtReader::readBorder(&_tmp_4190, &cell.borrow().borderBottom.clone());
            let mut _tmp_4192 = s.borrow().borderLeft.clone();
            OdtReader::readBorder(&_tmp_4192, &cell.borrow().borderLeft.clone());
            let mut _tmp_4194 = s.borrow().borderRight.clone();
            OdtReader::readBorder(&_tmp_4194, &cell.borrow().borderRight.clone());
            if s.borrow().padding >= 0.0 {
                let px: i64 = ((s.borrow().padding * 96.0) / 72.0 + 0.5).floor() as i64;
                cell.borrow_mut().padTopPx = px;
                cell.borrow_mut().padBottomPx = px;
                cell.borrow_mut().padLeftPx = px;
                cell.borrow_mut().padRightPx = px;
            }
        }
        self.cellContent(doc, &cell, c, 0);
        if cell.borrow().paragraphs.is_empty() {
            let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraphOnly();
            let mut base: Rc<RefCell<TextSpan>> = self.resolveParagraph(&p, "");
            p.borrow_mut().spans.push(base.clone());
            cell.borrow_mut().paragraphs.push(p.clone());
        }
        cell
    }
    /// What a cell holds, flattened: the model's cells hold paragraphs, so a
    /// list keeps its paragraphs and a table inside a cell keeps its text.
    pub fn cellContent(&self, doc: &Rc<RefCell<RichDocument>>, cell: &Rc<RefCell<DocTableCell>>, node: &Rc<RefCell<OdtNode>>, level: i64) {
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                if c.borrow().name == "text:p" || c.borrow().name == "text:h" {
                    let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraphOnly();
                    self.fillParagraph(doc, &p, &c);
                    if level > 0 {
                        p.borrow_mut().indentLeftPt = p.borrow().indentLeftPt + 18.0 * (level as f64);
                    }
                    cell.borrow_mut().paragraphs.push(p.clone());
                }
                if c.borrow().name == "text:list" || c.borrow().name == "text:list-item" {
                    self.cellContent(doc, cell, &c, level + 1);
                }
                if c.borrow().name == "text:list-header" {
                    self.cellContent(doc, cell, &c, level);
                }
                if c.borrow().name == "table:table" {
                    let mut rows: Vec<Rc<RefCell<OdtNode>>> = Vec::new();
                    self.collectRows(&c, &mut rows);
                    for r in rows.iter().cloned() {
                        let __it_rc = r.borrow().children.clone();
                        for rc in __it_rc.iter().cloned() {
                            if !rc.borrow().isText && rc.borrow().name == "table:table-cell" {
                                self.cellContent(doc, cell, &rc, level);
                            }
                        }
                    }
                }
                if OdtReader::isContainer(&c.borrow().name.clone()) {
                    self.cellContent(doc, cell, &c, level);
                }
            }
        }
    }
    /// ------------------------------------------------------------------
    /// The page
    /// ------------------------------------------------------------------
    /// The first section's page: the master page the first paragraph asked
    /// for, else "Standard", else the first one there is.
    pub fn applyPage(&self, doc: &Rc<RefCell<RichDocument>>) {
        let mut master: String = self.initialMaster.clone();
        if master.is_empty() {
            master = self.firstMaster.clone();
            let std: Option<String> = self.masterLayout.get("Standard").cloned();
            if std.is_some() {
                master = "Standard".to_string();
            }
        }
        if master.is_empty() {
            return;
        }
        self.applyMaster(doc, &doc.borrow().sectionAt(0).clone(), &master);
    }
    /// A section laid out as a master page says. ODF puts the header INSIDE
    /// the page margin and Word puts it in the margin, so a page with a header
    /// has the header's height added to its top margin (and the footer's to
    /// the bottom) to start the text where it started.
    pub fn applyMaster(&self, doc: &Rc<RefCell<RichDocument>>, sec: &Rc<RefCell<DocSection>>, master: &str) {
        let mut hd: Option<Rc<RefCell<OdtNode>>> = self.masterHeader.get(master)
            .cloned();
        let mut ft: Option<Rc<RefCell<OdtNode>>> = self.masterFooter.get(master)
            .cloned();
        let pl_name: Option<String> = self.masterLayout.get(master).cloned();
        if pl_name.is_some() {
            let mut pl: Option<Rc<RefCell<OdtPageLayout>>> = self.pageLayouts.get(&pl_name.clone().unwrap())
                .cloned();
            if pl.is_some() {
                let mut l: Rc<RefCell<OdtPageLayout>> = pl.clone().unwrap();
                let _tmp_4199 = OdtReader::twips(l.borrow().widthPt.clone());
                sec.borrow_mut().pageWTwips = _tmp_4199;
                let _tmp_4201 = OdtReader::twips(l.borrow().heightPt.clone());
                sec.borrow_mut().pageHTwips = _tmp_4201;
                let mut top: f64 = l.borrow().marginTop;
                let mut bottom: f64 = l.borrow().marginBottom;
                if hd.is_some() {
                    top += l.borrow().headerExtra;
                }
                if ft.is_some() {
                    bottom += l.borrow().footerExtra;
                }
                sec.borrow_mut().marginTopTwips = OdtReader::twips(top);
                sec.borrow_mut().marginBottomTwips = OdtReader::twips(bottom);
                let _tmp_4205 = OdtReader::twips(l.borrow().marginLeft.clone());
                sec.borrow_mut().marginLeftTwips = _tmp_4205;
                let _tmp_4207 = OdtReader::twips(l.borrow().marginRight.clone());
                sec.borrow_mut().marginRightTwips = _tmp_4207;
                sec.borrow_mut().landscape = l.borrow().landscape;
            }
        }
        if hd.is_some() {
            let mut __arg_0 = sec.borrow().headerParas.clone();
            self.readHeaderFooter(doc, &hd.clone().unwrap(), &mut __arg_0);
            sec.borrow_mut().headerParas = __arg_0;
        }
        if ft.is_some() {
            let mut __arg_0 = sec.borrow().footerParas.clone();
            self.readHeaderFooter(doc, &ft.clone().unwrap(), &mut __arg_0);
            sec.borrow_mut().footerParas = __arg_0;
        }
    }
    pub fn readHeaderFooter(&self, doc: &Rc<RefCell<RichDocument>>, node: &Rc<RefCell<OdtNode>>, mut into: &mut Vec<Rc<RefCell<Paragraph>>>) {
        let __it_c = node.borrow().children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                if c.borrow().name == "text:p" || c.borrow().name == "text:h" {
                    let mut p: Rc<RefCell<Paragraph>> = doc.borrow_mut().addParagraphOnly();
                    self.fillParagraph(doc, &p, &c);
                    into.push(p.clone());
                }
            }
        }
    }
}
