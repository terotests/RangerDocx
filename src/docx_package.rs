// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::{OpcPackage};

/// ==============================================================================
/// DocxPackage — what a .docx calls its parts.
/// ==============================================================================
///
/// The container is not Word's. A .docx, a .xlsx and a .pptx are all OPC
/// packages, and that layer now lives once in `gallery/ooxml/OpcPackage.rgr`.
/// What is left here is the part of a Word package that is actually about Word:
/// that the document is at `word/document.xml`, that its styles are beside it,
/// and that a relationship id in `document.xml` is resolved against
/// `document.xml`.
///
/// WHAT THIS FIXES BY DELEGATING.
///
/// The relationship table this class used to keep was flat — one table for the
/// whole document, read out of `word/_rels/document.xml.rels` and nothing else —
/// and every target was forced under `word/`:
///
///     "media/image1.png"     → "word/media/image1.png"        correct
///     "../media/image1.png"  → "word/../media/image1.png"     names no part
///     "/word/media/x.png"    → "word/media/x.png"             correct
///     "https://example.com/" → "word/https://example.com/"    names no part
///
/// The first form is what Word writes for a picture in the body, which is why
/// this held up for as long as it did. The second is what a part in a
/// subdirectory writes, the fourth is every hyperlink in the document, and a
/// header's own relationships were not read at all, so a picture in a header had
/// no path to resolve against.
///
/// `OpcPackage` resolves a target against the part that names it, which is what
/// the standard says and what makes all four forms come out right.
/// ==============================================================================
pub struct DocxPackage {
    opc: Rc<RefCell<OpcPackage>>,
    pub openOk: bool,
    pub pathDir: String,
    pub pathFile: String,
    /// The part relationship ids are resolved against. Read from `_rels/.rels`
    /// when the package opens, rather than assumed, and only falling back to the
    /// conventional name when a file does not say.
    pub mainPart: String,
}

impl DocxPackage {
    pub fn new() -> Self {
        Self {
            opc: Rc::new(RefCell::new(OpcPackage::new())),
            openOk: false,
            pathDir: String::new(),
            pathFile: String::new(),
            mainPart: "word/document.xml".to_string(),
        }
    }
    pub fn open(&mut self, dir: String, file: String) -> bool {
        self.pathDir = dir.clone();
        self.pathFile = file.clone();
        self.openOk = OpcPackage::openParts(&self.opc, dir.clone(), file.clone());
        if self.openOk {
            self.findMainPart();
        }
        self.openOk
    }
    /// The package from bytes — what a browser has, and what a dropped file is.
    pub fn openBytes(&mut self, data: &[u8], name: String) -> bool {
        self.pathDir = String::new();
        self.pathFile = name.clone();
        self.openOk = OpcPackage::openBytes(&self.opc, data, name.clone());
        if self.openOk {
            self.findMainPart();
        }
        self.openOk
    }
    pub fn findMainPart(&mut self) {
        self.mainPart = "word/document.xml".to_string();
        let named: String = OpcPackage::mainDocumentPart(&self.opc);
        if !named.is_empty() {
            self.mainPart = named.clone();
        }
    }
    pub fn close(&mut self) {
        OpcPackage::close(&self.opc.clone());
        self.openOk = false;
    }
    pub fn hasPart(&self, name: &str) -> bool {
        return OpcPackage::hasPart(&self.opc, name);
    }
    pub fn readPartBytes(&self, name: &str) -> Vec<u8> {
        return OpcPackage::readBinary(&self.opc, name);
    }
    pub fn readPart(&self, name: &str) -> String {
        return OpcPackage::readXml(&self.opc, name);
    }
    pub fn documentXml(&self) -> String {
        return OpcPackage::readXml(&self.opc, &self.mainPart.clone());
    }
    /// Styles and numbering are found through the document's own relationships,
    /// so a package that puts them somewhere other than beside `document.xml`
    /// still reads. The conventional path is the fallback, not the rule.
    pub fn stylesXml(&self) -> String {
        self.readRelatedPart("/styles", "word/styles.xml")
    }
    /// The theme (fonts and colours) — "" when the package has none.
    pub fn themeXml(&self) -> String {
        self.readRelatedPart("/theme", "word/theme/theme1.xml")
    }
    pub fn numberingXml(&self) -> String {
        self.readRelatedPart("/numbering", "word/numbering.xml")
    }
    pub fn readRelatedPart(&self, type_suffix: &str, fallback: &str) -> String {
        let mut _tmp_132 = self.mainPart.clone();
        let part: String = OpcPackage::findRelationshipByType(
            &self.opc,
            &_tmp_132,
            type_suffix
        );
        if !part.is_empty() {
            return OpcPackage::readXml(&self.opc, &part);
        }
        return OpcPackage::readXml(&self.opc, fallback);
    }
    /// Where numbering.xml is, or "" when the document has none.
    pub fn numberingPart(&self) -> String {
        let mut _tmp_136 = self.mainPart.clone();
        return OpcPackage::findRelationshipByType(
            &self.opc,
            &_tmp_136,
            "/numbering"
        ).clone();
    }
    pub fn documentRelsXml(&self) -> String {
        return OpcPackage::readXml(&self.opc, &OpcPackage::relsPathFor(&self.mainPart.clone()));
    }
    /// rId → package path, resolved against `word/document.xml`. A hyperlink
    /// answers "" here because it is not a part; `relTarget` hands back the URL.
    pub fn resolveRel(&self, rel_id: &str) -> String {
        return OpcPackage::resolveRelationship(&self.opc, &self.mainPart.clone(), rel_id);
    }
    /// The same, for a relationship named by some other part — a picture in a
    /// header, whose `_rels` are the header's and not the document's.
    pub fn resolveRelIn(&self, part: &str, rel_id: &str) -> String {
        return OpcPackage::resolveRelationship(&self.opc, part, rel_id);
    }
    pub fn relType(&self, rel_id: &str) -> String {
        return OpcPackage::relationshipType(&self.opc, &self.mainPart.clone(), rel_id);
    }
    /// The target as written: a URL for an external relationship, and the
    /// unresolved relative path for an internal one.
    pub fn relTarget(&self, rel_id: &str) -> String {
        return OpcPackage::relationshipTarget(&self.opc, &self.mainPart.clone(), rel_id);
    }
    pub fn isExternalRel(&self, rel_id: &str) -> bool {
        return OpcPackage::isExternalRelationship(&self.opc, &self.mainPart.clone(), rel_id);
    }
    pub fn listParts(&self) -> Vec<String> {
        return OpcPackage::listParts(&self.opc);
    }
    pub fn contentTypeFor(&self, part: &str) -> String {
        return OpcPackage::contentTypeFor(&self.opc, part);
    }
    /// The package underneath, for a caller that needs the container rather than
    /// Word's names for things.
    fn opcPackage(&self) -> Rc<RefCell<OpcPackage>> {
        self.opc.clone()
    }
}
