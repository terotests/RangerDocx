// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;

use crate::rgr::DxNative;
use crate::rgr::{OoxmlText};

/// ==============================================================================
/// WordXml - tiny WordprocessingML string helpers (no full DOM).
/// ==============================================================================
///
/// Sufficient for the viewer subset: paragraphs, runs, text, rPr/pPr attrs.
/// ==============================================================================
#[derive(Clone)]
pub struct WordXml {
}

impl WordXml {
    pub fn new() -> Self {
        Self {
        }
    }
    /// One decoder for all three OOXML readers. This one used to know only the
    /// five named entities, so a document written with `&#228;` showed the user
    /// the digits, and it copied a character at a time, which is quadratic
    /// where a string is mutable.
    pub fn decodeEntities(s: &str) -> String {
        OoxmlText::decodeEntities(s)
    }
    pub fn attr(tag: &str, name: &str) -> String {
        let key: String = format!("{}=\"", name);
        let mut i: i64 = DxNative::index_of(&tag, &key);
        if i < 0 {
            let key2: String = format!("{}='", name);
            i = DxNative::index_of(&tag, &key2);
            if i < 0 {
                return String::new();
            }
            let a: i64 = i + (DxNative::str_len(&key2));
            let mut b: i64 = a;
            let n: i64 = DxNative::str_len(&tag);
            while b < n {
                if DxNative::char_at(&tag, b) == 39 {
                    return DxNative::substr(&tag, a, b);
                }
                b += 1;
            }
            return String::new();
        }
        let a2: i64 = i + (DxNative::str_len(&key));
        let mut b2: i64 = a2;
        let n2: i64 = DxNative::str_len(&tag);
        while b2 < n2 {
            if DxNative::char_at(&tag, b2) == 34 {
                return DxNative::substr(&tag, a2, b2);
            }
            b2 += 1;
        }
        String::new()
    }
    /// `indexOfFrom`, not `indexOf` over a tail SUBSTRING. The difference does
    /// not show on the fixtures: `(indexOf (substring xml i n) open)` copies
    /// everything from i to the end of the document before looking at any of it,
    /// so one search costs the length of what is left. The body walk searches
    /// three times per paragraph, which made opening a 20,000-paragraph document
    /// a 4.6-million-character copy sixty thousand times over — 58 seconds, for
    /// a file the spreadsheet's loader would have read in well under one.
    pub fn findNext(xml: &str, tag_name: &str, from: i64) -> i64 {
        let open: String = format!("<{}", tag_name);
        let mut i: i64 = from;
        let n: i64 = DxNative::str_len(&xml);
        while i < n {
            let j: i64 = DxNative::index_of_from(&xml, &open, i);
            if j < 0 {
                return -1;
            }
            let after: i64 = j + (DxNative::str_len(&open));
            if after >= n {
                return -1;
            }
            let ch: i64 = DxNative::char_at(&xml, after);
            // word tag boundary: space, /, or >
            if ((((ch == 32 || ch == 47) || ch == 62) || ch == 9) || ch == 10) || ch == 13 {
                return j;
            }
            i = after + 1;
        }
        -1
    }
    /// Extract inner XML of first <tagName ...>...</tagName> starting at from.
    /// Returns empty on failure. Sets outEnd to index after closing tag via side
    /// channel is awkward in Ranger — caller rescans. Here we return the element
    /// slice including tags via a packed convention: use extractElement.
    pub fn elementEnd(xml: &str, tag_name: &str, start: i64) -> i64 {
        let n: i64 = DxNative::str_len(&xml);
        if start < 0 {
            return -1;
        }
        if start >= n {
            return -1;
        }
        // Self-closing?
        let mut gt: i64 = start;
        while gt < n {
            if DxNative::char_at(&xml, gt) == 62 {
                if gt > start {
                    if DxNative::char_at(&xml, gt - 1) == 47 {
                        return gt + 1;
                    }
                }
                gt += 1;
                return WordXml::scanClose(xml, tag_name, gt);
            }
            gt += 1;
        }
        -1
    }
    pub fn scanClose(xml: &str, tag_name: &str, from: i64) -> i64 {
        // unused:  let open: String = format!("<{}", tag_name);
        let close: String = format!("</{}>", tag_name);
        let mut depth: i64 = 1;
        let mut i: i64 = from;
        let n: i64 = DxNative::str_len(&xml);
        while i < n {
            let cpos: i64 = DxNative::index_of_from(&xml, &close, i);
            let opos: i64 = WordXml::findNext(xml, tag_name, i);
            if cpos < 0 {
                return -1;
            }
            if opos >= 0 && opos < cpos {
                // nested open
                let oe: i64 = WordXml::elementEnd(xml, tag_name, opos);
                if oe < 0 {
                    return -1;
                }
                // If self-closing nested, depth unchanged for self-close; for
                // real open we already recurse via elementEnd. Simpler: bump.
                let mut ogt: i64 = opos;
                while ogt < n {
                    if DxNative::char_at(&xml, ogt) == 62 {
                        if DxNative::char_at(&xml, ogt - 1) == 47 {
                            i = ogt + 1;
                        } else {
                            depth += 1;
                            i = ogt + 1;
                        }
                        ogt = n;
                    } else {
                        ogt += 1;
                    }
                }
            } else {
                depth -= 1;
                let after: i64 = cpos + (DxNative::str_len(&close));
                if depth == 0 {
                    return after;
                }
                i = after;
            }
        }
        -1
    }
    pub fn sliceElement(xml: &str, tag_name: &str, start: i64) -> String {
        let end: i64 = WordXml::elementEnd(xml, tag_name, start);
        if end < 0 {
            return String::new();
        }
        DxNative::substr(&xml, start, end)
    }
    pub fn innerOf(el: &str, tag_name: &str) -> String {
        let open_end: i64 = DxNative::index_of(&el, ">");
        if open_end < 0 {
            return String::new();
        }
        if DxNative::char_at(&el, open_end - 1) == 47 {
            return String::new();
        }
        let close: String = format!("</{}>", tag_name);
        let c: i64 = DxNative::index_of(&el, &close);
        if c < 0 {
            return String::new();
        }
        DxNative::substr(&el, open_end + 1, c)
    }
    pub fn openTag(el: &str) -> String {
        let gt: i64 = DxNative::index_of(&el, ">");
        if gt < 0 {
            return el.to_string();
        }
        DxNative::substr(&el, 0, gt + 1)
    }
    pub fn hasFlag(xml: &str, tag: &str) -> bool {
        // true if <w:b/> or <w:b> or <w:b w:val="1|true|on"> present;
        // false if explicitly val=0|false|off.
        let i: i64 = WordXml::findNext(xml, tag, 0);
        if i < 0 {
            return false;
        }
        let el: String = WordXml::sliceElement(xml, tag, i);
        let ot: String = WordXml::openTag(&el);
        let v: String = WordXml::attr(&ot, "w:val");
        if v.is_empty() {
            return true;
        }
        if (v == "0" || v == "false") || v == "off" {
            return false;
        }
        true
    }
    pub fn twipsAttr(tag: &str, name: &str, fallback: i64) -> i64 {
        let v: String = WordXml::attr(tag, name);
        if v.is_empty() {
            return fallback;
        }
        let n: Option<i64> = v.parse::<i64>().ok();
        if n.is_none() {
            return fallback;
        }
        n.unwrap()
    }
    /// Text for an attribute value or element content: the four characters
    /// XML gives a meaning escaped.
    pub fn escapeAttr(s: &str) -> String {
        let mut r: String = String::new();
        let n: i64 = DxNative::str_len(&s);
        let mut from: i64 = 0;
        let mut i: i64 = 0;
        while i < n {
            let c: i64 = DxNative::char_at(&s, i);
            let mut rep: String = "";
            if c == 38 {
                rep = "&amp;";
            }
            if c == 60 {
                rep = "&lt;";
            }
            if c == 62 {
                rep = "&gt;";
            }
            if c == 34 {
                rep = "&quot;";
            }
            if !rep.is_empty() {
                r = format!("{}{}{}", r, DxNative::substr(&s, from, i), rep);
                from = i + 1;
            }
            i += 1;
        }
        format!("{}{}", r, DxNative::substr(&s, from, n))
    }
}
