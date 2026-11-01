//! The .rgr classes the crate's modules use, imported once.

#[allow(dead_code, unused, non_snake_case)]
mod rgr {
    ranger::import_rgr!("../../../lib/evg/EVGA11yTree");
    ranger::import_rgr!("../../../lib/evg/EVGCodepoint");
    ranger::import_rgr!("../../../lib/evg/EVGColor");
    ranger::import_rgr!("../../../lib/evg/EVGDisplayList");
    ranger::import_rgr!("../../../lib/evg/EVGElement");
    ranger::import_rgr!("../../../lib/evg/EVGToolbar");
    ranger::import_rgr!("../../../lib/image/ImageBuffer");
    ranger::import_rgr!("../../../lib/image/JPEGDecoder");
    ranger::import_rgr!("../../../lib/zip/Inflate");
    ranger::import_rgr!("../../datagrid/src/ChartDoc");
    ranger::import_rgr!("../../datagrid/src/GridChart");
    ranger::import_rgr!("../../evg_window/EVGToolbarView");
    ranger::import_rgr!("../../game_engine/ui/UIContext");
    ranger::import_rgr!("../../game_engine/ui/UITextRenderer");
    ranger::import_rgr!("../../office/text/OfficeCaretMotion");
    ranger::import_rgr!("../../office/text/OfficeFont");
    ranger::import_rgr!("../../office/text/OfficeLineNav");
    ranger::import_rgr!("../../office/text/OfficeStyle");
    ranger::import_rgr!("../../office/text/OfficeText");
    ranger::import_rgr!("../../office/text/OfficeTextMetrics");
    ranger::import_rgr!("../../office/text/OfficeTextRun");
    ranger::import_rgr!("../../ooxml/OoxmlText");
    ranger::import_rgr!("../../ooxml/OpcPackage");
    ranger::import_rgr!("../../ooxml/SourceRef");
    ranger::import_rgr!("../../pdf_writer/src/core/Utf8");
    ranger::import_rgr!("../../text_editor/src/TextEditCore");
    ranger::import_rgr!("../../zip/ZipWriter");
    ranger::import_rgr!("DocEditHistory");
    ranger::import_rgr!("DxNative");
}
pub use rgr::{ChartDoc, ChartDocCodec, ChartKind, ChartStyle, DocEditHistory, DocEditKind, DocEditOp, DocumentPosition, DocumentRange, DxNative, EVGA11yNode, EVGA11yRole, EVGA11yTree, EVGCodepoint, EVGColor, EVGDisplayList, EVGDrawCmd, EVGElement, EVGToolbarTheme, EVGToolbarView, ImageBuffer, Inflate, JPEGDecoder, OfficeCaretColumn, OfficeCaretMotion, OfficeFont, OfficeLineNav, OfficeText, OfficeTextMetrics, OfficeTextRun, OfficeTextStyle, OoxmlText, OpcPackage, SourceRef, StyleFlag, ToolIcon, ToolItem, UIContext, UITextRenderer, Utf8, ZipWriter};
