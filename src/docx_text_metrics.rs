// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rich_document::{Paragraph, TextSpan};
use crate::rgr::DxNative;
use crate::rgr::{OfficeFont, OfficeTextMetrics, OfficeTextRun, UITextRenderer};

/// ==============================================================================
/// DocxTextMetrics - ONE measurement authority for paragraph text.
/// ==============================================================================
///
/// The painter draws a visual line run by run, switching the font face per
/// TextSpan (regular vs bold). Anything that asks "how far along the line is
/// offset N?" — caret X, click hit-test, selection rectangles — has to measure
/// the SAME way, or a click inside a bold run lands on the wrong character and
/// the selection box stops short of the glyphs it is meant to cover.
///
/// Every one of those callers goes through here, so paint and measure cannot
/// drift apart. Face selection mirrors DocxView.setPaintFace exactly.
///
/// Note: like the painter, a run is measured at the visual line's size. Per-span
/// font SIZE is not honoured yet — layout wraps the paragraph at one size, so
/// measuring at another would put the caret outside the painted text. Per-span
/// FAMILY is not honoured for the same reason: `tr.wrap` breaks the paragraph in
/// one face, and measuring the pieces in another would put the line ends
/// somewhere the breaks are not. A PARAGRAPH's family is honoured: its first
/// run's (`useParagraph`), for the wrap, the paint and the measure alike.
///
/// ITALIC now is. It used to be neither measured nor painted: `applyFace` took
/// one argument, `bold`, so every italic run in every document opened here was
/// drawn upright — and Word's italic button and its Ctrl+I changed the model and
/// nothing on the page. The face naming and the walk when a face is missing are
/// `OfficeFont`'s, shared with the .pptx reader.
/// ==============================================================================
#[derive(Clone)]
pub struct DocxTextMetrics {
}

impl DocxTextMetrics {
    pub fn new() -> Self {
        Self {
        }
    }
    /// Select the face for the next draw or measure.
    ///
    /// It must go through `fontFamily`, NOT tr.rt.setFont: UITextRenderer calls
    /// applyFace() at the top of measureWidth / wrap / getCachedLine, which resets
    /// the rasterizer from `fontFamily`. Poking rt directly is undone by the very
    /// next call — which is why bold runs used to be measured, and drawn, in the
    /// regular face. The raster cache key already includes fontFamily, so no cache
    /// flush is needed here.
    /// The family a paragraph is set in: its first run's, as the document named
    /// it (`WordStyleResolver.mapFont`), when this renderer has a face of it;
    /// Open Sans otherwise. One family per paragraph, not per run: `tr.wrap`
    /// breaks a paragraph in one face, and measuring its pieces in another
    /// would put the line ends somewhere the breaks are not.
    ///
    /// It matters for where lines break. Open Sans is about 15 % wider than
    /// Cambria, so a Cambria paragraph wrapped in it broke a word earlier
    /// than Word (and M-Files' own preview) does on almost every line.
    fn paragraphFamily(tr: &Rc<RefCell<UITextRenderer>>, p: &Rc<RefCell<Paragraph>>) -> String {
        let mut fam: String = String::new();
        if !p.borrow().spans.is_empty() {
            let mut s0: Rc<RefCell<TextSpan>> = p.borrow().spans[0].clone();
            fam = s0.borrow().fontFamily.clone();
        }
        DocxTextMetrics::loadedFamily(tr, &fam)
    }
    /// `fam` when a face of it is loaded, else "Open Sans".
    fn loadedFamily(tr: &Rc<RefCell<UITextRenderer>>, fam: &str) -> String {
        if fam.is_empty() {
            return "Open Sans".to_string();
        }
        if (DxNative::str_len(&OfficeFont::nearestInFamily(&Rc::clone(&tr.borrow().fm), fam, false, false))) == 0 {
            return "Open Sans".to_string();
        }
        fam.to_string()
    }
    /// The family the renderer is set to now: the face name up to its first
    /// dash ("Caladea-Bold Italic" -> "Caladea", see `OfficeFont`).
    fn family(tr: &Rc<RefCell<UITextRenderer>>) -> String {
        let face: String = tr.borrow().fontFamily.clone();
        let dash: i64 = DxNative::index_of(&face, "-");
        if dash > 0 {
            return DxNative::substr(&face, 0, dash);
        }
        face
    }
    /// Set the renderer to paragraph p's family, upright and regular. Every
    /// later `applyStyle` keeps the family and changes only weight and slant,
    /// so layout, paint and hit-testing call this once per paragraph or line.
    fn useParagraph(tr: &Rc<RefCell<UITextRenderer>>, p: &Rc<RefCell<Paragraph>>) {
        DocxTextMetrics::useFamily(tr, &DocxTextMetrics::paragraphFamily(tr, p));
    }
    fn useFamily(tr: &Rc<RefCell<UITextRenderer>>, fam: &str) {
        let f: String = DocxTextMetrics::loadedFamily(tr, fam);
        UITextRenderer::useFace(&tr, &f, false, false, "Open Sans");
    }
    fn applyFace(tr: &Rc<RefCell<UITextRenderer>>, bold: bool) {
        DocxTextMetrics::applyStyle(tr, bold, false);
    }
    fn applyStyle(tr: &Rc<RefCell<UITextRenderer>>, bold: bool, italic: bool) {
        let fam: String = DocxTextMetrics::family(tr);
        UITextRenderer::useFace(&tr, &fam, bold, italic, "Open Sans");
    }
    pub fn safeSize(px: f64) -> f64 {
        if px < 8.0 {
            return 8.0;
        }
        px
    }
    /// The span covering `off`, or the nearest one. Spans are half-open
    /// [start,end); an offset past the last span belongs to the last span, which
    /// is what puts a caret at the end of a paragraph in the style of the text
    /// in front of it.
    fn spanAt(p: &Rc<RefCell<Paragraph>>, off: i64) -> Option<Rc<RefCell<TextSpan>>> {
        let mut i: i64 = 0;
        let n: i64 = p.borrow().spans.len() as i64;
        while i < n {
            let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            if off >= sp.borrow().start && off < sp.borrow().end {
                let mut hit: Option<Rc<RefCell<TextSpan>>> = Some(sp.clone());
                return hit;
            }
            i += 1;
        }
        if n > 0 {
            let mut last: Rc<RefCell<TextSpan>> = p.borrow().spans[(n - 1) as usize].clone();
            if off >= last.borrow().end {
                let mut last_hit: Option<Rc<RefCell<TextSpan>>> = Some(last.clone());
                return last_hit;
            }
            let mut first_hit: Option<Rc<RefCell<TextSpan>>> = Some(p.borrow().spans[0].clone());
            return first_hit;
        }
        let mut none: Option<Rc<RefCell<TextSpan>>> = None;
        none
    }
    /// Is the character at `off` painted bold?
    fn boldAt(p: &Rc<RefCell<Paragraph>>, off: i64) -> bool {
        let mut hit: Option<Rc<RefCell<TextSpan>>> = DocxTextMetrics::spanAt(p, off);
        if hit.is_none() {
            return false;
        }
        let mut sp: Rc<RefCell<TextSpan>> = hit.clone().unwrap();
        return sp.borrow().bold;
    }
    /// ...and slanted? This is what was missing: without it a run switched face
    /// only where the WEIGHT changed, so an italic word inside plain text was
    /// measured and drawn in the same upright face as its neighbours.
    fn italicAt(p: &Rc<RefCell<Paragraph>>, off: i64) -> bool {
        let mut hit: Option<Rc<RefCell<TextSpan>>> = DocxTextMetrics::spanAt(p, off);
        if hit.is_none() {
            return false;
        }
        let mut sp: Rc<RefCell<TextSpan>> = hit.clone().unwrap();
        return sp.borrow().italic;
    }
    /// One number for "which face draws this character", so a run break can be
    /// a single comparison rather than one per style bit — and so a bit added
    /// later cannot be forgotten in one of the two walks below.
    fn faceKeyAt(p: &Rc<RefCell<Paragraph>>, off: i64) -> i64 {
        let mut key: i64 = 0;
        if DocxTextMetrics::boldAt(p, off) {
            key += 1;
        }
        if DocxTextMetrics::italicAt(p, off) {
            key += 2;
        }
        key
    }
    pub fn keyBold(key: i64) -> bool {
        key % 2 == 1
    }
    pub fn keyItalic(key: i64) -> bool {
        (key / 2) % 2 == 1
    }
    /// A paragraph's spans as the shared measurer wants them. Word lays a line
    /// out at ONE size — `tr.wrap` broke it at one size — so no run states a
    /// size of its own here: measuring the pieces at another would put the line
    /// ends somewhere the breaks are not.
    fn runsOf(p: &Rc<RefCell<Paragraph>>) -> Vec<Rc<RefCell<OfficeTextRun>>> {
        let mut out: Vec<Rc<RefCell<OfficeTextRun>>> = Vec::new();
        let n: i64 = p.borrow().spans.len() as i64;
        if n == 0 {
            out.push(OfficeTextRun::of(0, (DxNative::str_len(&p.borrow().text)), false, false));
            return out;
        }
        let mut i: i64 = 0;
        while i < n {
            let mut sp: Rc<RefCell<TextSpan>> = p.borrow().spans[i as usize].clone();
            out.push(OfficeTextRun::of(
                sp.borrow().start,
                sp.borrow().end,
                sp.borrow().bold,
                sp.borrow().italic
            ));
            i += 1;
        }
        // The tail beyond the last span belongs to the last span's face, which
        // is what puts a caret at the end of a paragraph in the style of the
        // text in front of it.
        let mut last: Rc<RefCell<TextSpan>> = p.borrow().spans[(n - 1) as usize].clone();
        if last.borrow().end < (DxNative::str_len(&p.borrow().text)) {
            out.push(OfficeTextRun::of(
                last.borrow().end,
                (DxNative::str_len(&p.borrow().text)),
                last.borrow().bold,
                last.borrow().italic
            ));
        }
        out
    }
    /// Width of p.text[from,to) as painted: contiguous same-face runs, each
    /// measured with that face active.
    ///
    /// The walk is `OfficeTextMetrics`', shared with the slide reader. What is
    /// NOT shared is the rounding: Word's ink advances x by an integer per run,
    /// so this has to truncate the same way — summing in full precision and
    /// rounding once gives a different answer, and the difference is where the
    /// caret ends up.
    fn measureRange(tr: &Rc<RefCell<UITextRenderer>>, p: &Rc<RefCell<Paragraph>>, line_px: f64, from: i64, to: i64) -> i64 {
        let size: f64 = DocxTextMetrics::safeSize(line_px);
        let __len: i64 = DxNative::str_len(&p.borrow().text);
        let mut a: i64 = from;
        let mut b: i64 = to;
        if a < 0 {
            a = 0;
        }
        if b > __len {
            b = __len;
        }
        if a >= b {
            return 0;
        }
        DocxTextMetrics::useParagraph(tr, p);
        let mut _tmp_1281 = p.borrow().text.clone();
        return (OfficeTextMetrics::measureRange(
            Rc::clone(&UITextRenderer::metrics(&tr)),
            &_tmp_1281,
            &DocxTextMetrics::runsOf(p),
            a,
            b,
            &DocxTextMetrics::family(tr),
            size,
            true
        )).floor() as i64;
    }
    /// Inverse of measureRange: the offset in [from,to] whose painted position is
    /// nearest targetX (measured from the start of `from`). Splits on glyph
    /// midpoints so a click lands on the closer side of a character.
    ///
    /// The same walk, and the same rounding, as `measureRange` — because these
    /// two have to be each other's inverse. Two copies of one walk is exactly
    /// how they stop being.
    fn offsetAtX(tr: &Rc<RefCell<UITextRenderer>>, p: &Rc<RefCell<Paragraph>>, line_px: f64, from: i64, to: i64, target_x: i64) -> i64 {
        let size: f64 = DocxTextMetrics::safeSize(line_px);
        DocxTextMetrics::useParagraph(tr, p);
        let mut _tmp_1282 = p.borrow().text.clone();
        return OfficeTextMetrics::offsetAtX(
            Rc::clone(&UITextRenderer::metrics(&tr)),
            &_tmp_1282,
            &DocxTextMetrics::runsOf(p),
            from,
            to,
            target_x as f64,
            &DocxTextMetrics::family(tr),
            size,
            true
        );
    }
    /// List markers / chrome are painted with the line's own face, not a span.
    fn measurePlain(tr: &Rc<RefCell<UITextRenderer>>, text: &str, line_px: f64, bold: bool) -> i64 {
        DocxTextMetrics::measureStyled(tr, text, line_px, bold, false)
    }
    fn measureStyled(tr: &Rc<RefCell<UITextRenderer>>, text: &str, line_px: f64, bold: bool, italic: bool) -> i64 {
        if text.is_empty() {
            return 0;
        }
        DocxTextMetrics::applyStyle(tr, bold, italic);
        return (UITextRenderer::measureWidth(
            &tr,
            text,
            DocxTextMetrics::safeSize(line_px)
        )).floor() as i64;
    }
}
