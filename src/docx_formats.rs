// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::docx_viewer::DocxViewer;
use crate::odt_reader::OdtReader;
use crate::rgr::DxNative;
use crate::rich_document::RichDocument;
use crate::rtf_reader::RtfReader;

/// ==============================================================================
/// DocxFormats - open a .docx, .odt or .rtf into a DocxViewer
/// ==============================================================================
///
/// What a file is comes from its bytes, not its name: an .odt is a ZIP like a
/// .docx, and an .rtf starts with `{\rtf`. Anything that is neither of the
/// other two goes to the .docx reader, which says what is wrong with it.
///
/// Kept out of DocxViewer on purpose. The viewer is imported by programs that
/// only ever see a .docx (a chart pasted from the spreadsheet, the markdown
/// page), and the two readers are a large part of the compiler's memory for a
/// program that holds the Word editor and the spreadsheet together.
/// ==============================================================================
#[derive(Clone)]
pub struct DocxFormats {
}

impl DocxFormats {
    pub fn new() -> Self {
        Self {
        }
    }
    /// "odt", "rtf" or "docx": the reader these bytes are for.
    pub fn kindOf(data: &[u8]) -> String {
        if OdtReader::isOdt(data) {
            return "odt".to_string();
        }
        if RtfReader::isRtf(data) {
            return "rtf".to_string();
        }
        "docx".to_string()
    }
    pub fn openBytes(viewer: &Rc<RefCell<DocxViewer>>, data: &[u8], name: &str) -> bool {
        let kind: String = DocxFormats::kindOf(data);
        if kind == "odt" {
            let mut odt: Rc<RefCell<OdtReader>> = Rc::new(RefCell::new(OdtReader::new()));
            let mut read: Option<Rc<RefCell<RichDocument>>> = odt.borrow_mut().readBytes(data, name.to_string());
            if read.is_none() {
                viewer.borrow_mut().lastError = odt.borrow().lastError.clone();
                return false;
            }
            viewer.borrow_mut().lastError = String::new();
            viewer.borrow_mut().adoptForeign(read.clone().unwrap(), odt.borrow().numbering.clone(), name.to_string());
            return true;
        }
        if kind == "rtf" {
            let mut rtf: Rc<RefCell<RtfReader>> = Rc::new(RefCell::new(RtfReader::new()));
            let mut rd: Option<Rc<RefCell<RichDocument>>> = rtf.borrow_mut().readBytes(data, name.to_string());
            if rd.is_none() {
                viewer.borrow_mut().lastError = rtf.borrow().lastError.clone();
                return false;
            }
            viewer.borrow_mut().lastError = String::new();
            viewer.borrow_mut().adoptForeign(rd.clone().unwrap(), rtf.borrow().numbering.clone(), name.to_string());
            return true;
        }
        return viewer.borrow_mut().openBytes(data, name.to_string());
    }
    /// From a file, for a host with a file system (the tests, the tools).
    pub fn openFile(viewer: &Rc<RefCell<DocxViewer>>, dir: &str, file: &str) -> bool {
        let lower: String = file.to_lowercase();
        if DocxFormats::endsWith(&lower, ".docx") || DocxFormats::endsWith(&lower, ".docm") {
            return viewer.borrow_mut().openFile(dir, file.to_string());
        }
        DocxFormats::openBytes(
            viewer,
            &DxNative::read_bytes(dir, file),
            file
        )
    }
    pub fn endsWith(s: &str, suffix: &str) -> bool {
        let n: i64 = DxNative::str_len(&s);
        let m: i64 = DxNative::str_len(&suffix);
        if m > n {
            return false;
        }
        DxNative::substr(&s, n - m, n) == suffix
    }
}
