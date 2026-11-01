// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::DxNative;

/// ==============================================================================
/// ClipboardTable - OS clipboard HTML / TSV → a grid of cells.
/// ==============================================================================
///
/// What a spreadsheet puts on the clipboard when you press Ctrl+C is two
/// flavours of the same selection: `text/plain` TSV and `text/html` holding a
/// real <table>. Word pastes the HTML flavour as a table; this is the reader for
/// that, so a range copied in the Ranger DataGrid (or in Excel) lands in the DOCX
/// editor as a DocumentTable rather than as tab-separated text.
///
/// The scanner is deliberately tolerant, not an XML parser: real clipboard HTML
/// carries <!--StartFragment--> comments, a <style> block, unquoted or
/// single-quoted attributes and void <col>/<br> tags, none of which are
/// well-formed XML.
/// ==============================================================================
#[derive(Clone)]
pub struct ClipCell {
    pub text: String,
    pub bold: bool,
    pub fillHex: String,
    pub align: String,
    pub gridSpan: i64,
}

impl ClipCell {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            bold: false,
            fillHex: String::new(),
            align: String::new(),
            gridSpan: 1,
        }
    }
}

#[derive(Clone)]
pub struct ClipRow {
    pub cells: Vec<Rc<RefCell<ClipCell>>>,
}

impl ClipRow {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct ClipboardTable {
    pub rows: Vec<Rc<RefCell<ClipRow>>>,
    pub ok: bool,
}

impl ClipboardTable {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            ok: false,
        }
    }
    /// --- small string helpers -------------------------------------------------
    pub fn lower(s: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&s);
        while i < n {
            let mut c: i64 = DxNative::char_at(&s, i);
            if (c >= 65 && c <= 90) {
                c += 32;
            }
            out = format!("{}{}", out, char::from_u32(c as u32).unwrap_or('\0'));
            i += 1;
        }
        out
    }
    pub fn hasAt(hay: &str, pos: i64, needle: &str) -> bool {
        let m: i64 = DxNative::str_len(&needle);
        if m == 0 {
            return true;
        }
        if pos + m > (DxNative::str_len(&hay)) {
            return false;
        }
        DxNative::substr(&hay, pos, pos + m) == needle
    }
    pub fn findFrom(hay: &str, needle: &str, from: i64) -> i64 {
        let n: i64 = DxNative::str_len(&hay);
        let m: i64 = DxNative::str_len(&needle);
        if m == 0 {
            return from;
        }
        let mut i: i64 = from;
        if i < 0 {
            i = 0;
        }
        while i <= n - m {
            if DxNative::substr(&hay, i, i + m) == needle {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn isSpace(c: i64) -> bool {
        if c == 32 {
            return true;
        }
        if c == 9 {
            return true;
        }
        if c == 10 {
            return true;
        }
        if c == 13 {
            return true;
        }
        false
    }
    pub fn trimWs(s: &str) -> String {
        let mut a: i64 = 0;
        let mut b: i64 = DxNative::str_len(&s);
        while a < b {
            if ClipboardTable::isSpace(DxNative::char_at(&s, a)) {
                a += 1;
            } else {
                break;
            }
        }
        while b > a {
            if ClipboardTable::isSpace(DxNative::char_at(&s, b - 1)) {
                b -= 1;
            } else {
                break;
            }
        }
        DxNative::substr(&s, a, b)
    }
    /// Collapse runs of whitespace: clipboard HTML is pretty-printed.
    pub fn squeeze(s: &str) -> String {
        let mut out: String = String::new();
        let mut prev_space: bool = false;
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&s);
        while i < n {
            let c: i64 = DxNative::char_at(&s, i);
            if ClipboardTable::isSpace(c) {
                if !prev_space {
                    out = format!("{} ", out);
                }
                prev_space = true;
            } else {
                out = format!("{}{}", out, char::from_u32(c as u32).unwrap_or('\0'));
                prev_space = false;
            }
            i += 1;
        }
        ClipboardTable::trimWs(&out)
    }
    pub fn decodeEntities(s: &str) -> String {
        let mut out: String = String::new();
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&s);
        while i < n {
            let c: i64 = DxNative::char_at(&s, i);
            if c == 38 {
                let semi: i64 = ClipboardTable::findFrom(s, ";", i);
                let mut handled: bool = false;
                if semi > i {
                    if semi - i <= 10 {
                        let name: String = DxNative::substr(&s, i + 1, semi);
                        let low: String = ClipboardTable::lower(&name);
                        if low == "amp" {
                            out = format!("{}&", out);
                            handled = true;
                        }
                        if low == "lt" {
                            out = format!("{}<", out);
                            handled = true;
                        }
                        if low == "gt" {
                            out = format!("{}>", out);
                            handled = true;
                        }
                        if low == "quot" {
                            out = format!("{}\"", out);
                            handled = true;
                        }
                        if low == "apos" {
                            out = format!("{}'", out);
                            handled = true;
                        }
                        if low == "nbsp" {
                            out = format!("{} ", out);
                            handled = true;
                        }
                        if low == "#39" {
                            out = format!("{}'", out);
                            handled = true;
                        }
                        if low == "#160" {
                            out = format!("{} ", out);
                            handled = true;
                        }
                        if handled {
                            i = semi + 1;
                        }
                    }
                }
                if !handled {
                    out = format!("{}&", out);
                    i += 1;
                }
            } else {
                out = format!("{}{}", out, char::from_u32(c as u32).unwrap_or('\0'));
                i += 1;
            }
        }
        out
    }
    /// Attribute value from a raw tag body, quoted ("x" / 'x') or bare.
    pub fn attrValue(raw: &str, name: &str) -> String {
        let low: String = ClipboardTable::lower(raw);
        let n: i64 = DxNative::str_len(&raw);
        let mut from: i64 = 0;
        while from < n {
            let at: i64 = ClipboardTable::findFrom(&low, name, from);
            if at < 0 {
                return String::new();
            }
            // must start at a word boundary
            let mut ok_start: bool = true;
            if at > 0 {
                let prev: i64 = DxNative::char_at(&low, at - 1);
                if !ClipboardTable::isSpace(prev) {
                    ok_start = false;
                }
            }
            let mut i: i64 = at + (DxNative::str_len(&name));
            if ok_start {
                while i < n {
                    if ClipboardTable::isSpace(DxNative::char_at(&raw, i)) {
                        i += 1;
                    } else {
                        break;
                    }
                }
                if i < n {
                    if DxNative::char_at(&raw, i) == 61 {
                        i += 1;
                        while i < n {
                            if ClipboardTable::isSpace(DxNative::char_at(&raw, i)) {
                                i += 1;
                            } else {
                                break;
                            }
                        }
                        if i < n {
                            let q: i64 = DxNative::char_at(&raw, i);
                            if q == 34 || q == 39 {
                                let mut end: i64 = i + 1;
                                while end < n {
                                    if DxNative::char_at(&raw, end) == q {
                                        break;
                                    }
                                    end += 1;
                                }
                                return DxNative::substr(&raw, i + 1, end);
                            }
                            let mut e2: i64 = i;
                            while e2 < n {
                                if ClipboardTable::isSpace(DxNative::char_at(&raw, e2)) {
                                    break;
                                }
                                e2 += 1;
                            }
                            return DxNative::substr(&raw, i, e2);
                        }
                    }
                }
            }
            from = at + 1;
        }
        String::new()
    }
    /// CSS declaration value, e.g. cssValue("a:1;color:red" "color") → "red".
    pub fn cssValue(style: &str, prop: &str) -> String {
        let low: String = ClipboardTable::lower(style);
        let key: String = format!("{}:", prop);
        let at: i64 = ClipboardTable::findFrom(&low, &key, 0);
        if at < 0 {
            return String::new();
        }
        let i: i64 = at + (DxNative::str_len(&key));
        let n: i64 = DxNative::str_len(&style);
        let mut end: i64 = i;
        while end < n {
            if DxNative::char_at(&style, end) == 59 {
                break;
            }
            end += 1;
        }
        ClipboardTable::trimWs(&DxNative::substr(&style, i, end))
    }
    pub fn isHex6(s: &str) -> bool {
        if (DxNative::str_len(&s)) != 6 {
            return false;
        }
        let mut i: i64 = 0;
        while i < 6 {
            let c: i64 = DxNative::char_at(&s, i);
            let mut okc: bool = false;
            if (c >= 48 && c <= 57) {
                okc = true;
            }
            if (c >= 97 && c <= 102) {
                okc = true;
            }
            if (c >= 65 && c <= 70) {
                okc = true;
            }
            if !okc {
                return false;
            }
            i += 1;
        }
        true
    }
    /// "#RRGGBB" → "RRGGBB"; anything else (named colours, rgb()) is dropped.
    pub fn hexOf(v: &str) -> String {
        let s: String = ClipboardTable::trimWs(v);
        if (DxNative::str_len(&s)) == 7 {
            if DxNative::char_at(&s, 0) == 35 {
                let h: String = DxNative::substr(&s, 1, 7);
                if ClipboardTable::isHex6(&h) {
                    return h;
                }
            }
        }
        if (DxNative::str_len(&s)) == 6 {
            if ClipboardTable::isHex6(&s) {
                return s;
            }
        }
        String::new()
    }
    /// Tag name: letters right after "<" or "</".
    pub fn tagName(tag: &str) -> String {
        let mut i: i64 = 0;
        let n: i64 = DxNative::str_len(&tag);
        if i < n {
            if DxNative::char_at(&tag, i) == 47 {
                i += 1;
            }
        }
        let mut out: String = String::new();
        while i < n {
            let c: i64 = DxNative::char_at(&tag, i);
            let mut is_letter: bool = false;
            if (c >= 97 && c <= 122) {
                is_letter = true;
            }
            if (c >= 65 && c <= 90) {
                is_letter = true;
            }
            if (c >= 48 && c <= 57) {
                is_letter = true;
            }
            if !is_letter {
                break;
            }
            out = format!("{}{}", out, char::from_u32(c as u32).unwrap_or('\0'));
            i += 1;
        }
        ClipboardTable::lower(&out)
    }
    pub fn isClosing(tag: &str) -> bool {
        if tag.is_empty() {
            return false;
        }
        DxNative::char_at(&tag, 0) == 47
    }
    /// Plain text worth pasting as a grid: at least one tab or several lines.
    pub fn looksTabular(text: &str) -> bool {
        if ClipboardTable::findFrom(text, "\t", 0) >= 0 {
            return true;
        }
        false
    }
    pub fn rowCount(&self) -> i64 {
        self.rows.len() as i64
    }
    /// Widest row, counting a colspan as the columns it covers.
    pub fn columnCount(&self) -> i64 {
        let mut best: i64 = 0;
        let mut i: i64 = 0;
        while i < (self.rows.len() as i64) {
            let mut r: Rc<RefCell<ClipRow>> = self.rows[i as usize].clone();
            let mut w: i64 = 0;
            let mut j: i64 = 0;
            while j < (r.borrow().cells.len() as i64) {
                let mut c: Rc<RefCell<ClipCell>> = r.borrow().cells[j as usize].clone();
                let mut gs: i64 = c.borrow().gridSpan;
                if gs < 1 {
                    gs = 1;
                }
                w += gs;
                j += 1;
            }
            if w > best {
                best = w;
            }
            i += 1;
        }
        best
    }
    pub fn rowAt(&self, i: i64) -> Rc<RefCell<ClipRow>> {
        self.rows[i as usize].clone()
    }
    pub fn applyCellAttrs(&self, cell: &Rc<RefCell<ClipCell>>, raw: &str) {
        let span: String = ClipboardTable::attrValue(raw, "colspan");
        if !span.is_empty() {
            let gv: Option<i64> = span.parse::<i64>().ok();
            if gv.is_some() {
                let g: i64 = gv.unwrap();
                if g > 0 {
                    cell.borrow_mut().gridSpan = g;
                }
            }
        }
        let bg: String = ClipboardTable::attrValue(raw, "bgcolor");
        if !bg.is_empty() {
            cell.borrow_mut().fillHex = ClipboardTable::hexOf(&bg);
        }
        let style: String = ClipboardTable::attrValue(raw, "style");
        if !style.is_empty() {
            let mut fill: String = ClipboardTable::cssValue(&style, "background-color");
            if fill.is_empty() {
                fill = ClipboardTable::cssValue(&style, "background");
            }
            let hx: String = ClipboardTable::hexOf(&fill);
            if !hx.is_empty() {
                cell.borrow_mut().fillHex = hx.clone();
            }
            let weight: String = ClipboardTable::lower(&ClipboardTable::cssValue(&style, "font-weight"));
            if weight == "bold" {
                cell.borrow_mut().bold = true;
            }
            if weight == "700" {
                cell.borrow_mut().bold = true;
            }
            let al: String = ClipboardTable::lower(&ClipboardTable::cssValue(&style, "text-align"));
            if al == "center" || al == "right" {
                cell.borrow_mut().align = al.clone();
            }
        }
    }
    /// --- HTML ----------------------------------------------------------------
    /// Reads the FIRST <table> in the document and stops at its </table>.
    pub fn parseHtml(&mut self, html: &str) -> bool {
        self.rows.clear();
        self.ok = false;
        let low: String = ClipboardTable::lower(html);
        let n: i64 = DxNative::str_len(&html);
        let start: i64 = ClipboardTable::findFrom(&low, "<table", 0);
        if start < 0 {
            return false;
        }
        let mut i: i64 = start;
        let mut in_row: bool = false;
        let mut in_cell: bool = false;
        let mut buf: String = String::new();
        let mut cur_row: Rc<RefCell<ClipRow>> = Rc::new(RefCell::new(ClipRow::new()));
        let mut cur_cell: Rc<RefCell<ClipCell>> = Rc::new(RefCell::new(ClipCell::new()));
        let mut running: bool = true;
        while running && i < n {
            let ch: i64 = DxNative::char_at(&html, i);
            if ch == 60 {
                if ClipboardTable::hasAt(&low, i, "<!--") {
                    let ce: i64 = ClipboardTable::findFrom(&low, "-->", i);
                    if ce < 0 {
                        i = n;
                    } else {
                        i = ce + 3;
                    }
                } else {
                    let close: i64 = ClipboardTable::findFrom(html, ">", i);
                    if close < 0 {
                        i = n;
                    } else {
                        let raw: String = DxNative::substr(&html, i + 1, close);
                        let name: String = ClipboardTable::tagName(&raw);
                        let closing: bool = ClipboardTable::isClosing(&raw);
                        i = close + 1;
                        if name == "style" || name == "script" {
                            if !closing {
                                let end_tag: String = format!("</{}>", name);
                                let se: i64 = ClipboardTable::findFrom(&low, &end_tag, i);
                                if se < 0 {
                                    i = n;
                                } else {
                                    i = se + (DxNative::str_len(&end_tag));
                                }
                            }
                        }
                        if name == "table" {
                            if closing {
                                running = false;
                            }
                        }
                        if name == "tr" {
                            if closing {
                                if in_cell {
                                    let _tmp_5 = ClipboardTable::squeeze(&ClipboardTable::decodeEntities(&buf));
                                    cur_cell.borrow_mut().text = _tmp_5;
                                    cur_row.borrow_mut().cells.push(cur_cell.clone());
                                    in_cell = false;
                                    buf = String::new();
                                }
                                if in_row {
                                    self.rows.push(cur_row.clone());
                                    in_row = false;
                                }
                            } else {
                                if in_row {
                                    self.rows.push(cur_row.clone());
                                }
                                cur_row = Rc::new(RefCell::new(ClipRow::new()));
                                in_row = true;
                                in_cell = false;
                                buf = String::new();
                            }
                        }
                        if name == "td" || name == "th" {
                            if closing {
                                if in_cell {
                                    let _tmp_7 = ClipboardTable::squeeze(&ClipboardTable::decodeEntities(&buf));
                                    cur_cell.borrow_mut().text = _tmp_7;
                                    cur_row.borrow_mut().cells.push(cur_cell.clone());
                                    in_cell = false;
                                    buf = String::new();
                                }
                            } else {
                                if in_cell {
                                    let _tmp_9 = ClipboardTable::squeeze(&ClipboardTable::decodeEntities(&buf));
                                    cur_cell.borrow_mut().text = _tmp_9;
                                    cur_row.borrow_mut().cells.push(cur_cell.clone());
                                }
                                if !in_row {
                                    cur_row = Rc::new(RefCell::new(ClipRow::new()));
                                    in_row = true;
                                }
                                cur_cell = Rc::new(RefCell::new(ClipCell::new()));
                                if name == "th" {
                                    cur_cell.borrow_mut().bold = true;
                                }
                                self.applyCellAttrs(&cur_cell, &raw);
                                in_cell = true;
                                buf = String::new();
                            }
                        }
                        if in_cell {
                            if !closing {
                                if name == "b" || name == "strong" {
                                    cur_cell.borrow_mut().bold = true;
                                }
                                if name == "br" || name == "p" {
                                    buf = format!("{} ", buf);
                                }
                            }
                        }
                    }
                }
            } else {
                if in_cell {
                    buf = format!("{}{}", buf, char::from_u32(ch as u32).unwrap_or('\0'));
                }
                i += 1;
            }
        }
        if in_cell {
            let _tmp_11 = ClipboardTable::squeeze(&ClipboardTable::decodeEntities(&buf));
            cur_cell.borrow_mut().text = _tmp_11;
            cur_row.borrow_mut().cells.push(cur_cell.clone());
        }
        if in_row {
            self.rows.push(cur_row.clone());
        }
        // Drop trailing rows with no cells (stray <tr> in fragment markup).
        while !self.rows.is_empty() {
            let mut last_row: Rc<RefCell<ClipRow>> = self.rows[((self.rows.len() as i64) - 1) as usize].clone();
            if last_row.borrow().cells.is_empty() {
                self.rows.pop();
            } else {
                break;
            }
        }
        self.ok = !self.rows.is_empty();
        self.ok
    }
    /// --- TSV (text/plain flavour) --------------------------------------------
    pub fn parseTsv(&mut self, text: &str) -> bool {
        self.rows.clear();
        self.ok = false;
        if text.is_empty() {
            return false;
        }
        let mut lines: Vec<String> = text.split("\n").map(|s| s.to_string()).collect::<Vec<String>>();
        let mut li: i64 = 0;
        let n: i64 = lines.len() as i64;
        while li < n {
            let mut line: String = lines[li as usize].clone();
            let __len: i64 = DxNative::str_len(&line);
            if __len > 0 {
                if DxNative::char_at(&line, __len - 1) == 13 {
                    line = DxNative::substr(&line, 0, __len - 1);
                }
            }
            // A single trailing newline should not make an empty last row.
            let mut skip: bool = false;
            if li == n - 1 {
                if line.is_empty() {
                    skip = true;
                }
            }
            if !skip {
                let mut row: Rc<RefCell<ClipRow>> = Rc::new(RefCell::new(ClipRow::new()));
                let mut cols: Vec<String> = line.split("\t")
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>();
                let mut ci: i64 = 0;
                while ci < (cols.len() as i64) {
                    let mut cell: Rc<RefCell<ClipCell>> = Rc::new(RefCell::new(ClipCell::new()));
                    cell.borrow_mut().text = cols[ci as usize].clone();
                    row.borrow_mut().cells.push(cell.clone());
                    ci += 1;
                }
                self.rows.push(row.clone());
            }
            li += 1;
        }
        self.ok = !self.rows.is_empty();
        self.ok
    }
}
