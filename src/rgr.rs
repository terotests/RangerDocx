//! The .rgr classes the crate's modules use, imported once.

#[allow(dead_code, unused, non_snake_case)]
mod rgr {
    ranger::import_rgr!("../../../lib/evg/EVGA11yTree");
    ranger::import_rgr!("../../../lib/evg/EVGCodepoint");
    ranger::import_rgr!("../../game_engine/ui/UITextRenderer");
    ranger::import_rgr!("../../office/text/OfficeFont");
    ranger::import_rgr!("../../office/text/OfficeStyle");
    ranger::import_rgr!("../../office/text/OfficeTextMetrics");
    ranger::import_rgr!("../../office/text/OfficeTextRun");
    ranger::import_rgr!("../../ooxml/OoxmlText");
    ranger::import_rgr!("../../ooxml/SourceRef");
    ranger::import_rgr!("../../text_editor/src/TextEditCore");
    ranger::import_rgr!("DxNative");
    ranger::import_rgr!("RichDocument");
}
pub use rgr::{DocumentPosition, DxNative, EVGA11yNode, EVGA11yRole, EVGA11yTree, EVGCodepoint, OfficeFont, OfficeTextMetrics, OfficeTextRun, OfficeTextStyle, OoxmlText, Paragraph, SourceRef, StyleFlag, UITextRenderer};
