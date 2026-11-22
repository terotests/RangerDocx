// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::DxNative;

/// ==============================================================================
/// DocxTyping - what typing does besides putting the characters in.
/// ==============================================================================
///
/// AutoFormat as you type (a paragraph that starts `* ` or `1. ` becomes a
/// list) and AutoCorrect (`teh` → `the`, a capital at the start of a
/// sentence, `TWo` → `Two`, `1/2` → `½`, `:)` → `☺`). The rules are Word's
/// defaults as WordCraft's `typing-parity.md` lists them; the code is our own.
///
/// Everything here is a pure function of strings, so it is tested without a
/// document. `DocxEditController.afterTyping` applies the answers, each fix as
/// its own undo step.
/// ==============================================================================
#[derive(Clone)]
pub struct DocxTyping {
}

impl DocxTyping {
    pub fn new() -> Self {
        Self {
        }
    }
    /// The list look a paragraph start turns into when a space follows it, as
    /// `WordNumbering.newList` takes it, or "" for none.
    pub fn listLook(prefix: &str) -> String {
        if prefix == "*" {
            return "bullet:•".to_string();
        }
        if prefix == "-" {
            return "bullet:–".to_string();
        }
        if prefix == ">" {
            return "bullet:➢".to_string();
        }
        if prefix == "->" {
            return "bullet:➔".to_string();
        }
        if prefix == "=>" {
            return "bullet:⇨".to_string();
        }
        if prefix == "1." {
            return "decimal:%1.".to_string();
        }
        if prefix == "1)" {
            return "decimal:%1)".to_string();
        }
        if prefix == "a." {
            return "lowerLetter:%1.".to_string();
        }
        if prefix == "a)" {
            return "lowerLetter:%1)".to_string();
        }
        if prefix == "A." {
            return "upperLetter:%1.".to_string();
        }
        if prefix == "A)" {
            return "upperLetter:%1)".to_string();
        }
        if prefix == "i." {
            return "lowerRoman:%1.".to_string();
        }
        if prefix == "I." {
            return "upperRoman:%1.".to_string();
        }
        String::new()
    }
    pub fn isUpper(c: i64) -> bool {
        (c >= 65 && c <= 90) || (c >= 192 && c <= 222 && c != 215)
    }
    pub fn isLower(c: i64) -> bool {
        (c >= 97 && c <= 122) || (c >= 223 && c <= 255 && c != 247)
    }
    pub fn isLetter(c: i64) -> bool {
        DocxTyping::isUpper(c) || DocxTyping::isLower(c)
    }
    pub fn isSpace(c: i64) -> bool {
        c == 32 || c == 9 || c == 10 || c == 160
    }
    /// Characters that end a word and set AutoCorrect off.
    pub fn isTrigger(s: &str) -> bool {
        s == " " || s == "." || s == "," || s == ";" || s == ":" || s == "!" || s == "?" || s == "\t"
    }
    pub fn isClosingPunct(c: i64) -> bool {
        c == 46 || c == 44 || c == 59 || c == 58 || c == 33 || c == 63 || c == 34 || c == 41 || c == 39 || c == 93
    }
    pub fn isOpeningPunct(c: i64) -> bool {
        c == 40 || c == 34 || c == 39 || c == 91
    }
    /// Where the last word of `before` starts: after the last space, and past
    /// any opening bracket or quote.
    pub fn wordStart(before: &str) -> i64 {
        let mut i: i64 = DxNative::str_len(&before);
        while i > 0 {
            if DocxTyping::isSpace(DxNative::char_at(&before, i - 1)) {
                break;
            }
            i -= 1;
        }
        let n: i64 = DxNative::str_len(&before);
        while i < n && DocxTyping::isOpeningPunct(DxNative::char_at(&before, i)) {
            i += 1;
        }
        i
    }
    /// Where the word that starts at `from` ends, before closing punctuation.
    pub fn wordEnd(before: &str, from: i64) -> i64 {
        let mut e: i64 = DxNative::str_len(&before);
        while e > from && DocxTyping::isClosingPunct(DxNative::char_at(&before, e - 1)) {
            e -= 1;
        }
        e
    }
    /// Abbreviations after which a full stop does not start a sentence.
    pub fn isAbbreviation(token: &str) -> bool {
        let t: String = token.to_lowercase();
        if t == "e.g." || t == "i.e." || t == "etc." || t == "dr." || t == "mr." || t == "mrs." || t == "ms." || t == "prof." || t == "vs." || t == "no." || t == "st." || t == "jr." || t == "sr." || t == "cf." || t == "approx." || t == "fig." {
            return true;
        }
        // An initial: one letter and its full stop (J. R. R. Tolkien).
        if (DxNative::str_len(&t)) == 2 && DocxTyping::isLetter(DxNative::char_at(&t, 0)) && DxNative::char_at(&t, 1) == 46 {
            return true;
        }
        false
    }
    /// Does a word that starts at `at` in `text` start a sentence? At the start
    /// of the paragraph, or after `. ! ?` and a space, unless the word before
    /// is an abbreviation.
    pub fn startsSentence(text: &str, at: i64) -> bool {
        let mut i: i64 = at;
        while i > 0 && DocxTyping::isSpace(DxNative::char_at(&text, i - 1)) {
            i -= 1;
        }
        if i == 0 {
            return true;
        }
        if i == at {
            return false;
        }
        let c: i64 = DxNative::char_at(&text, i - 1);
        if c == 33 || c == 63 {
            return true;
        }
        if c != 46 {
            return false;
        }
        let prev_start: i64 = DocxTyping::wordStart(&DxNative::substr(&text, 0, i));
        let prev: String = DxNative::substr(&text, prev_start, i);
        if DocxTyping::isAbbreviation(&prev) {
            return false;
        }
        // "..." is not the end of a sentence either.
        if DxNative::index_of(&prev, "..") >= 0 {
            return false;
        }
        true
    }
    pub fn upperFirst(w: &str) -> String {
        let n: i64 = DxNative::str_len(&w);
        if n == 0 {
            return String::new();
        }
        format!("{}{}", DxNative::substr(&w, 0, 1).to_uppercase(), DxNative::substr(&w, 1, n))
    }
    /// The built-in replacement list (English).
    pub fn replacement(w: &str) -> String {
        if w == "i" {
            return "I".to_string();
        }
        if w == "teh" {
            return "the".to_string();
        }
        if w == "adn" {
            return "and".to_string();
        }
        if w == "dont" {
            return "don't".to_string();
        }
        if w == "doesnt" {
            return "doesn't".to_string();
        }
        if w == "didnt" {
            return "didn't".to_string();
        }
        if w == "cant" {
            return "can't".to_string();
        }
        if w == "wont" {
            return "won't".to_string();
        }
        if w == "isnt" {
            return "isn't".to_string();
        }
        if w == "recieve" {
            return "receive".to_string();
        }
        if w == "recieved" {
            return "received".to_string();
        }
        if w == "seperate" {
            return "separate".to_string();
        }
        if w == "definately" {
            return "definitely".to_string();
        }
        if w == "occured" {
            return "occurred".to_string();
        }
        if w == "untill" {
            return "until".to_string();
        }
        if w == "wich" {
            return "which".to_string();
        }
        if w == "becuase" {
            return "because".to_string();
        }
        if w == "thier" {
            return "their".to_string();
        }
        if w == "(c)" {
            return "©".to_string();
        }
        if w == "(r)" {
            return "®".to_string();
        }
        if w == "(tm)" {
            return "™".to_string();
        }
        if w == "1/2" {
            return "½".to_string();
        }
        if w == "1/4" {
            return "¼".to_string();
        }
        if w == "3/4" {
            return "¾".to_string();
        }
        String::new()
    }
    pub fn dayName(w: &str) -> bool {
        w == "monday" || w == "tuesday" || w == "wednesday" || w == "thursday" || w == "friday" || w == "saturday" || w == "sunday"
    }
    /// `TWo` → `Two`: three or more letters, the first two capitals, the rest
    /// lower case.
    pub fn twoInitialCaps(w: &str) -> String {
        let n: i64 = DxNative::str_len(&w);
        if n < 3 {
            return String::new();
        }
        if !DocxTyping::isUpper(DxNative::char_at(&w, 0)) || !DocxTyping::isUpper(DxNative::char_at(&w, 1)) {
            return String::new();
        }
        // Plurals of abbreviations: IDs, CDs, PCs.
        if n == 3 && DxNative::char_at(&w, 2) == 115 {
            return String::new();
        }
        let mut i: i64 = 2;
        while i < n {
            if !DocxTyping::isLower(DxNative::char_at(&w, i)) {
                return String::new();
            }
            i += 1;
        }
        format!("{}{}", DxNative::substr(&w, 0, 1), DxNative::substr(&w, 1, n).to_lowercase())
    }
    /// `wORD` → `Word`: Caps Lock was on and Shift held for the first letter.
    pub fn capsLock(w: &str) -> String {
        let n: i64 = DxNative::str_len(&w);
        if n < 3 {
            return String::new();
        }
        if w == "iOS" || w == "mRNA" || w == "tRNA" || w == "eID" || w == "iPSC" || w == "pH" {
            return String::new();
        }
        if !DocxTyping::isLower(DxNative::char_at(&w, 0)) {
            return String::new();
        }
        let mut i: i64 = 1;
        while i < n {
            if !DocxTyping::isUpper(DxNative::char_at(&w, i)) {
                return String::new();
            }
            i += 1;
        }
        format!("{}{}", DxNative::substr(&w, 0, 1).to_uppercase(), DxNative::substr(&w, 1, n).to_lowercase())
    }
    /// What AutoCorrect makes of a finished word, or "" to leave it.
    /// `sentence` says the word starts a sentence.
    pub fn fixWord(w: &str, sentence: bool) -> String {
        if w.is_empty() {
            return String::new();
        }
        let mut out: String = w.to_string();
        let two: String = DocxTyping::twoInitialCaps(&out);
        if !two.is_empty() {
            out = two.clone();
        }
        let caps: String = DocxTyping::capsLock(&out);
        if !caps.is_empty() {
            out = caps.clone();
        }
        let mut rep: String = DocxTyping::replacement(&out);
        if rep.is_empty() && DocxTyping::isUpper(DxNative::char_at(&out, 0)) {
            // `Teh` at the start of a sentence is still `The`.
            let low: String = out.to_lowercase();
            if low != "i" && DocxTyping::upperFirst(&low) == out {
                let r2: String = DocxTyping::replacement(&low);
                if !r2.is_empty() {
                    rep = DocxTyping::upperFirst(&r2);
                }
            }
        }
        if !rep.is_empty() {
            out = rep.clone();
        }
        if DocxTyping::dayName(&out) {
            out = DocxTyping::upperFirst(&out);
        }
        if sentence && DocxTyping::isLower(DxNative::char_at(&out, 0)) && out == out.to_lowercase() {
            // Not a web address or an e-mail: those stay as typed.
            if DxNative::index_of(&out, "www.") != 0 && DxNative::index_of(&out, "://") < 0 && DxNative::index_of(&out, "@") < 0 {
                out = DocxTyping::upperFirst(&out);
            }
        }
        if out == w {
            return String::new();
        }
        out
    }
    /// A smiley at the end of `before`: how many characters it takes, or 0.
    pub fn smileyLength(before: &str) -> i64 {
        let n: i64 = DxNative::str_len(&before);
        let mut k: i64 = 3;
        while k >= 2 {
            if n >= k {
                let tail: String = DxNative::substr(&before, n - k, n);
                if !DocxTyping::smiley(&tail).is_empty() {
                    return k;
                }
            }
            k -= 1;
        }
        0
    }
    pub fn smiley(s: &str) -> String {
        if s == ":)" || s == ":-)" {
            return "☺".to_string();
        }
        if s == ":(" || s == ":-(" {
            return "☹".to_string();
        }
        if s == ":|" || s == ":-|" {
            return "😐".to_string();
        }
        String::new()
    }
    /// `1st 22nd 13th`: how many characters of `w` are the ordinal suffix to
    /// raise, or 0.
    pub fn ordinalSuffix(w: &str) -> i64 {
        let n: i64 = DxNative::str_len(&w);
        if n < 3 {
            return 0;
        }
        let suf: String = DxNative::substr(&w, n - 2, n);
        let mut i: i64 = 0;
        while i < n - 2 {
            let c: i64 = DxNative::char_at(&w, i);
            if c < 48 || c > 57 {
                return 0;
            }
            i += 1;
        }
        let last: i64 = DxNative::char_at(&w, n - 3) - 48;
        let mut tens: i64 = -1;
        if n >= 4 {
            tens = DxNative::char_at(&w, n - 4) - 48;
        }
        let mut want: String = "th".to_string();
        if tens != 1 {
            if last == 1 {
                want = "st".to_string();
            }
            if last == 2 {
                want = "nd".to_string();
            }
            if last == 3 {
                want = "rd".to_string();
            }
        }
        if suf == want {
            return 2;
        }
        0
    }
    /// A web address that becomes a link when it is finished.
    pub fn isWebAddress(w: &str) -> bool {
        let n: i64 = DxNative::str_len(&w);
        if n < 5 {
            return false;
        }
        if DxNative::index_of(&w, "http://") == 0 || DxNative::index_of(&w, "https://") == 0 {
            return n > 8;
        }
        if DxNative::index_of(&w, "www.") == 0 {
            return DxNative::index_of_from(&w, ".", 4) > 4;
        }
        false
    }
    /// The character for Alt+X: the hex digits at the end of `before` (up to
    /// six, `U+` optional), as [count of characters taken, code], or [0, 0].
    pub fn hexBefore(before: &str) -> Vec<i64> {
        let mut out: Vec<i64> = Vec::new();
        let n: i64 = DxNative::str_len(&before);
        let mut i: i64 = n;
        while i > 0 && n - i < 6 {
            let c: i64 = DxNative::char_at(&before, i - 1);
            let hex: bool = (c >= 48 && c <= 57) || (c >= 65 && c <= 70) || (c >= 97 && c <= 102);
            if !hex {
                break;
            }
            i -= 1;
        }
        if i == n {
            out.push(0);
            out.push(0);
            return out;
        }
        let digits: String = DxNative::substr(&before, i, n);
        let mut code: i64 = 0;
        let mut k: i64 = 0;
        while k < (DxNative::str_len(&digits)) {
            let c: i64 = DxNative::char_at(&digits, k);
            let mut v: i64 = c - 48;
            if c >= 65 && c <= 70 {
                v = c - 55;
            }
            if c >= 97 && c <= 102 {
                v = c - 87;
            }
            code = code * 16 + v;
            k += 1;
        }
        let mut taken: i64 = n - i;
        if i >= 2 {
            let pre: String = DxNative::substr(&before, i - 2, i);
            if pre == "U+" || pre == "u+" {
                taken += 2;
            }
        }
        if code <= 0 || code > 1114111 || (code >= 55296 && code <= 57343) {
            out.push(0);
            out.push(0);
            return out;
        }
        out.push(taken);
        out.push(code);
        out
    }
    pub fn charOf(code: i64) -> String {
        format!("{}", char::from_u32(code as u32).unwrap_or('\0'))
    }
    /// The hex Alt+X turns a character back into: `00e9` style, at least four
    /// digits, upper case as Word writes it.
    pub fn hexOf(code: i64) -> String {
        let digits: String = "0123456789ABCDEF".to_string();
        let mut s: String = String::new();
        let mut v: i64 = code;
        while v > 0 {
            let d: i64 = v - (v / 16) * 16;
            s = format!("{}{}", DxNative::substr(&digits, d, d + 1), s);
            v = v / 16;
        }
        while (DxNative::str_len(&s)) < 4 {
            s = format!("0{}", s);
        }
        s
    }
}
