// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::DxNative;
use crate::rgr::{OoxmlText};

/// ==============================================================================
/// OdtXml - the element tree the .odt reader walks
/// ==============================================================================
///
/// `gallery/xml/XmlCore.rgr` is the parser the .odp reader uses, and this is
/// the same parser with its classes renamed. It is not imported because of what
/// its names collide with: XmlCore and the spreadsheet's `XmlLite` both define
/// `XmlAttr` and `XmlTag`, so a program that holds the Word editor AND a
/// spreadsheet (ChartPasteTest, a chart pasted from the grid) would not compile
/// once the Word editor imported XmlCore. When those two are reconciled in
/// Ranger, this file goes and OdtReader imports XmlCore.
///
/// ODF's name policy only: prefixes are kept (`text:p`, `draw:frame`), and
/// white space is content inside the elements that hold text.
/// ==============================================================================
#[derive(Clone)]
pub struct OdtNode {
    pub name: String,
    pub attrNames: Vec<String>,
    pub attrValues: Vec<String>,
    pub children: Vec<Rc<RefCell<OdtNode>>>,
    pub text: String,
    pub isText: bool,
}

impl OdtNode {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            attrNames: Vec::new(),
            attrValues: Vec::new(),
            children: Vec::new(),
            text: String::new(),
            isText: false,
        }
    }
    pub fn attr(&self, n: &str) -> String {
        let mut i: i64 = 0;
        while i < (self.attrNames.len() as i64) {
            if self.attrNames[i as usize].clone() == n {
                return self.attrValues[i as usize].clone();
            }
            i += 1;
        }
        String::new()
    }
    pub fn firstChild(&self, local_name: &str) -> Option<Rc<RefCell<OdtNode>>> {
        let __it_c = self.children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText && c.borrow().name == local_name {
                let mut hit: Option<Rc<RefCell<OdtNode>>> = Some(c.clone());
                return hit;
            }
        }
        let mut missing: Option<Rc<RefCell<OdtNode>>> = None;
        missing
    }
    /// Depth-first, this node first.
    pub fn find(&self, local_name: &str) -> Option<Rc<RefCell<OdtNode>>> {
        if self.name == local_name {
            let mut __self: Option<Rc<RefCell<OdtNode>>> = Some(self.clone());
            return __self;
        }
        let __it_c = self.children.clone();
        for c in __it_c.iter().cloned() {
            if !c.borrow().isText {
                let mut hit: Option<Rc<RefCell<OdtNode>>> = c.borrow().find(local_name);
                if hit.is_some() {
                    return hit;
                }
            }
        }
        let mut missing: Option<Rc<RefCell<OdtNode>>> = None;
        missing
    }
    pub fn textContent(&self) -> String {
        if self.isText {
            return self.text.clone();
        }
        let mut out: String = String::new();
        let __it_c = self.children.clone();
        for c in __it_c.iter().cloned() {
            out = format!("{}{}", out, c.borrow().textContent());
        }
        out
    }
}

#[derive(Clone)]
pub struct OdtXmlParser {
    pub src: String,
    pub pos: i64,
    pub __len: i64,
    pub maxNodes: i64,
    pub maxDepth: i64,
    pub nodeCount: i64,
    pub aborted: bool,
}

impl OdtXmlParser {
    pub fn new() -> Self {
        Self {
            src: String::new(),
            pos: 0,
            __len: 0,
            maxNodes: 2000000,
            maxDepth: 256,
            nodeCount: 0,
            aborted: false,
        }
    }
    pub fn isSpace(c: i64) -> bool {
        ((c == 32 || c == 9) || c == 10) || c == 13
    }
    /// Elements whose white-space-only text is content: the space between two
    /// spans is the space between two words.
    pub fn keepsSpace(name: &str) -> bool {
        ((name == "text:p" || name == "text:h") || name == "text:span") || name == "text:a"
    }
    pub fn parse(&mut self, source: String) -> Option<Rc<RefCell<OdtNode>>> {
        self.src = source.clone();
        self.pos = 0;
        self.__len = DxNative::str_len(&source);
        self.nodeCount = 0;
        self.aborted = false;
        // the prolog, comments and declarations before the root
        while self.pos < self.__len {
            self.skipSpace();
            if self.pos + 1 < self.__len && DxNative::char_at(&self.src, self.pos) == 60 {
                let n: i64 = DxNative::char_at(&self.src, self.pos + 1);
                if n == 63 || n == 33 {
                    self.skipMarkup();
                } else {
                    return self.readElement(0);
                }
            } else {
                self.pos = self.__len;
            }
        }
        let mut missing: Option<Rc<RefCell<OdtNode>>> = None;
        missing
    }
    pub fn skipSpace(&mut self) {
        while self.pos < self.__len && OdtXmlParser::isSpace(DxNative::char_at(&self.src, self.pos)) {
            self.pos += 1;
        }
    }
    /// `<?…?>`, `<!--…-->`, `<![CDATA[…]]>` or `<!…>` at `pos`, stepped over.
    pub fn skipMarkup(&mut self) {
        let n: i64 = DxNative::char_at(&self.src, self.pos + 1);
        let mut end_at: i64 = -1;
        let mut end_len: i64 = 1;
        if n == 63 {
            end_at = DxNative::index_of_from(&self.src, "?>", self.pos);
            end_len = 2;
        } else {
            if DxNative::index_of_from(&self.src, "<!--", self.pos) == self.pos {
                end_at = DxNative::index_of_from(&self.src, "-->", self.pos);
                end_len = 3;
            } else {
                end_at = DxNative::index_of_from(&self.src, ">", self.pos);
            }
        }
        if end_at < 0 {
            self.pos = self.__len;
        } else {
            self.pos = end_at + end_len;
        }
    }
    pub fn readName(&mut self) -> String {
        let start: i64 = self.pos;
        while self.pos < self.__len {
            let c: i64 = DxNative::char_at(&self.src, self.pos);
            if ((OdtXmlParser::isSpace(c) || c == 62) || c == 47) || c == 61 {
                break;
            }
            self.pos += 1;
        }
        DxNative::substr(&self.src, start, self.pos)
    }
    /// An element whose `<` is at `pos`, with everything inside it.
    pub fn readElement(&mut self, depth: i64) -> Option<Rc<RefCell<OdtNode>>> {
        let mut missing: Option<Rc<RefCell<OdtNode>>> = None;
        if depth > self.maxDepth || self.nodeCount >= self.maxNodes {
            self.aborted = true;
            return missing;
        }
        self.pos += 1;
        let mut node: Rc<RefCell<OdtNode>> = Rc::new(RefCell::new(OdtNode::new()));
        node.borrow_mut().name = self.readName();
        self.nodeCount += 1;
        // attributes
        while self.pos < self.__len {
            self.skipSpace();
            if self.pos >= self.__len {
                return missing;
            }
            let ch: i64 = DxNative::char_at(&self.src, self.pos);
            if ch == 47 {
                let gt: i64 = DxNative::index_of_from(&self.src, ">", self.pos);
                if gt < 0 {
                    self.pos = self.__len;
                } else {
                    self.pos = gt + 1;
                }
                let mut self_closed: Option<Rc<RefCell<OdtNode>>> = Some(node.clone());
                return self_closed;
            }
            if ch == 62 {
                self.pos += 1;
                break;
            }
            let an: String = self.readName();
            if an.is_empty() {
                self.pos += 1;
            } else {
                self.skipSpace();
                let mut value: String = String::new();
                if self.pos < self.__len && DxNative::char_at(&self.src, self.pos) == 61 {
                    self.pos += 1;
                    self.skipSpace();
                    if self.pos < self.__len {
                        let q: i64 = DxNative::char_at(&self.src, self.pos);
                        if q == 34 || q == 39 {
                            let ve: i64 = DxNative::index_of_from(&self.src, &char::from_u32(q as u32).unwrap_or('\0').to_string(), self.pos + 1);
                            if ve < 0 {
                                self.pos = self.__len;
                                return missing;
                            }
                            value = OoxmlText::decodeEntities(&DxNative::substr(&self.src, self.pos + 1, ve));
                            self.pos = ve + 1;
                        }
                    }
                }
                node.borrow_mut().attrNames.push(an.clone());
                node.borrow_mut().attrValues.push(value.clone());
            }
        }
        // content, to the matching end tag
        while self.pos < self.__len && !self.aborted {
            let c: i64 = DxNative::char_at(&self.src, self.pos);
            if c == 60 {
                if self.pos + 1 < self.__len && DxNative::char_at(&self.src, self.pos + 1) == 47 {
                    let gt2: i64 = DxNative::index_of_from(&self.src, ">", self.pos);
                    if gt2 < 0 {
                        self.pos = self.__len;
                    } else {
                        self.pos = gt2 + 1;
                    }
                    let mut closed: Option<Rc<RefCell<OdtNode>>> = Some(node.clone());
                    return closed;
                }
                let mut n2: i64 = 0;
                if self.pos + 1 < self.__len {
                    n2 = DxNative::char_at(&self.src, self.pos + 1);
                }
                if n2 == 33 || n2 == 63 {
                    if DxNative::index_of_from(&self.src, "<![CDATA[", self.pos) == self.pos {
                        let mut ce: i64 = DxNative::index_of_from(&self.src, "]]>", self.pos);
                        if ce < 0 {
                            ce = self.__len;
                        }
                        self.addText(&node, DxNative::substr(&self.src, self.pos + 9, ce).clone(), false);
                        self.pos = ce + 3;
                    } else {
                        self.skipMarkup();
                    }
                } else {
                    let mut child: Option<Rc<RefCell<OdtNode>>> = self.readElement(depth + 1);
                    if child.is_some() {
                        node.borrow_mut().children.push(child.clone().unwrap());
                    }
                }
            } else {
                let ts: i64 = self.pos;
                let mut lt: i64 = DxNative::index_of_from(&self.src, "<", self.pos);
                if lt < 0 {
                    lt = self.__len;
                }
                self.pos = lt;
                self.addText(&node, DxNative::substr(&self.src, ts, lt).clone(), true);
            }
        }
        let mut unclosed: Option<Rc<RefCell<OdtNode>>> = Some(node.clone());
        unclosed
    }
    pub fn addText(&self, node: &Rc<RefCell<OdtNode>>, raw: String, decode: bool) {
        let mut only_space: bool = true;
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&raw);
        while i < n {
            if !OdtXmlParser::isSpace(DxNative::char_at(&raw, i)) {
                only_space = false;
                i = n;
            }
            i += 1;
        }
        if only_space && !OdtXmlParser::keepsSpace(&node.borrow().name.clone()) {
            return;
        }
        let mut t: Rc<RefCell<OdtNode>> = Rc::new(RefCell::new(OdtNode::new()));
        t.borrow_mut().isText = true;
        if decode {
            t.borrow_mut().text = OoxmlText::decodeEntities(&raw);
        } else {
            t.borrow_mut().text = raw.clone();
        }
        node.borrow_mut().children.push(t.clone());
    }
}
