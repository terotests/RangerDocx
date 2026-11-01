// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::rich_document::{DocBlock, DocBlockKind, Paragraph, RichDocument};
use crate::docx_layout::DocxLayout;
use crate::docx_layout::LaidLine;
use crate::docx_layout::LaidPage;
use crate::rgr::DxNative;
use crate::rgr::{EVGColor, EVGDisplayList, EVGToolbarTheme, EVGToolbarView, ToolIcon, ToolItem, UIContext, UITextRenderer};

/// ==============================================================================
/// DocxChrome.rgr — the word processor's frame: title bar, ribbon, navigation
/// pane and status bar.
/// ==============================================================================
///
/// `DocxToolbar` is one wrapping strip of icons, the same strip the spreadsheet
/// and the slides have. A word processor is read differently: the commands sit
/// in named GROUPS (Clipboard, Font, Paragraph, Styles, Editing), the big
/// buttons are the ones a page is for, and a group's name sits under it. This
/// is that layout, painted with the shared toolbar's icons and text so the
/// three editors still draw a bold button the same way.
///
/// The model and the geometry live here; `DocxApp` owns the commands, the
/// document and the decision of what is pressed.
///
/// A button whose command does not exist yet is DRAWN, greyed and inert. The
/// ribbon is also the parity checklist: what is grey is what is missing, and a
/// button that looks live and does nothing would hide exactly that.
/// ==============================================================================
/// The colours of the frame. One place, so the frame reads as one design
/// rather than as whatever each painter picked.
pub struct DocxChromeTheme {
    window: Rc<RefCell<EVGColor>>,
    titleBar: Rc<RefCell<EVGColor>>,
    ribbon: Rc<RefCell<EVGColor>>,
    desk: Rc<RefCell<EVGColor>>,
    line: Rc<RefCell<EVGColor>>,
    ink: Rc<RefCell<EVGColor>>,
    dim: Rc<RefCell<EVGColor>>,
    faint: Rc<RefCell<EVGColor>>,
    off: Rc<RefCell<EVGColor>>,
    accent: Rc<RefCell<EVGColor>>,
    accentInk: Rc<RefCell<EVGColor>>,
    accentSoft: Rc<RefCell<EVGColor>>,
    hover: Rc<RefCell<EVGColor>>,
    pressed: Rc<RefCell<EVGColor>>,
    field: Rc<RefCell<EVGColor>>,
    heading: Rc<RefCell<EVGColor>>,
    /// The vertical ruler's band beside the paper.
    rulerPage: Rc<RefCell<EVGColor>>,
}

impl DocxChromeTheme {
    pub fn new() -> Self {
        Self {
            window: EVGColor::rgb(243, 244, 247),
            titleBar: EVGColor::rgb(248, 249, 251),
            ribbon: EVGColor::rgb(255, 255, 255),
            desk: EVGColor::rgb(233, 235, 240),
            line: EVGColor::rgb(222, 225, 231),
            ink: EVGColor::rgb(32, 36, 44),
            dim: EVGColor::rgb(96, 104, 118),
            faint: EVGColor::rgb(160, 166, 178),
            off: EVGColor::rgb(184, 189, 199),
            accent: EVGColor::rgb(47, 91, 211),
            accentInk: EVGColor::rgb(255, 255, 255),
            accentSoft: EVGColor::rgb(226, 233, 252),
            hover: EVGColor::rgb(236, 239, 245),
            pressed: EVGColor::rgb(216, 226, 250),
            field: EVGColor::rgb(255, 255, 255),
            heading: EVGColor::rgb(31, 78, 121),
            rulerPage: EVGColor::rgb(214, 218, 226),
        }
    }
    /// The dark palette, for a host whose interface is dark: the frame turns,
    /// the paper does not — a document is printed on white.
    pub fn setDark(&mut self, on: bool) {
        if on {
            self.window = EVGColor::rgb(31, 35, 42);
            self.titleBar = EVGColor::rgb(31, 35, 42);
            self.ribbon = EVGColor::rgb(31, 35, 42);
            self.desk = EVGColor::rgb(21, 24, 29);
            self.line = EVGColor::rgb(54, 61, 72);
            self.ink = EVGColor::rgb(230, 232, 235);
            self.dim = EVGColor::rgb(161, 169, 181);
            self.faint = EVGColor::rgb(111, 120, 132);
            self.off = EVGColor::rgb(90, 98, 110);
            self.accent = EVGColor::rgb(110, 168, 254);
            self.accentInk = EVGColor::rgb(15, 23, 42);
            self.accentSoft = EVGColor::rgb(30, 58, 95);
            self.hover = EVGColor::rgb(43, 49, 58);
            self.pressed = EVGColor::rgb(54, 61, 72);
            self.field = EVGColor::rgb(27, 31, 37);
            self.heading = EVGColor::rgb(160, 190, 240);
            self.rulerPage = EVGColor::rgb(45, 51, 60);
            return;
        }
        let mut light: Rc<RefCell<DocxChromeTheme>> = Rc::new(RefCell::new(DocxChromeTheme::new()));
        self.window = light.borrow().window.clone();
        self.titleBar = light.borrow().titleBar.clone();
        self.ribbon = light.borrow().ribbon.clone();
        self.desk = light.borrow().desk.clone();
        self.line = light.borrow().line.clone();
        self.ink = light.borrow().ink.clone();
        self.dim = light.borrow().dim.clone();
        self.faint = light.borrow().faint.clone();
        self.off = light.borrow().off.clone();
        self.accent = light.borrow().accent.clone();
        self.accentInk = light.borrow().accentInk.clone();
        self.accentSoft = light.borrow().accentSoft.clone();
        self.hover = light.borrow().hover.clone();
        self.pressed = light.borrow().pressed.clone();
        self.field = light.borrow().field.clone();
        self.heading = light.borrow().heading.clone();
        self.rulerPage = light.borrow().rulerPage.clone();
    }
}

/// One button, combo box or style tile on the ribbon.
#[derive(Clone)]
pub struct DocxRibbonItem {
    pub command: String,
    pub arg: String,
    pub label: String,
    pub tip: String,
    pub icon: i64,
    /// Drawn instead of an icon when set: "x2", "Aa", "¶" — marks every suite
    /// draws as type rather than as a picture.
    pub glyph: String,
    /// 0 small icon, 1 large, 2 small with its name beside it, 3 combo box,
    /// 4 style tile.
    pub kind: i64,
    pub enabled: bool,
    pub pressed: bool,
    /// What a toggle asks the editor ("bold"), so the app can say whether the
    /// run under the caret has it.
    pub toggleOp: String,
    /// 0 a plain button, 1 the whole button opens a list, 2 a split button
    /// whose arrow opens the list and whose face runs the command.
    pub menu: i64,
    /// A combo box's text and width.
    pub text: String,
    pub fieldW: i64,
    /// A style tile's style id, and how its sample is drawn.
    pub styleId: String,
    pub sampleSize: f64,
    pub sampleBold: bool,
    pub sampleItalic: bool,
    pub sampleColor: String,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl DocxRibbonItem {
    pub fn new() -> Self {
        Self {
            command: String::new(),
            arg: String::new(),
            label: String::new(),
            tip: String::new(),
            icon: 0,
            glyph: String::new(),
            kind: 0,
            enabled: true,
            pressed: false,
            toggleOp: String::new(),
            menu: 0,
            text: String::new(),
            fieldW: 0,
            styleId: String::new(),
            sampleSize: 13.0,
            sampleBold: false,
            sampleItalic: false,
            sampleColor: String::new(),
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        }
    }
    pub fn contains(&self, px: i64, py: i64) -> bool {
        if self.w <= 0 {
            return false;
        }
        ((px >= self.x && px < self.x + self.w) && py >= self.y) && py < self.y + self.h
    }
    /// Does a press here open the item's list rather than run it?
    pub fn opensMenu(&self, px: i64) -> bool {
        if self.menu == 1 {
            return true;
        }
        if self.menu == 2 {
            return px >= (self.x + self.w) - 12;
        }
        false
    }
}

/// A line in a ribbon dropdown.
#[derive(Clone)]
pub struct DocxMenuEntry {
    pub label: String,
    pub command: String,
    pub arg: String,
    pub checked: bool,
    /// Drawn in front of the label: the bullet or number a list would get.
    pub sample: String,
    /// A rule above this line.
    pub rule: bool,
}

impl DocxMenuEntry {
    pub fn new() -> Self {
        Self {
            label: String::new(),
            command: String::new(),
            arg: String::new(),
            checked: false,
            sample: String::new(),
            rule: false,
        }
    }
}

/// The list a dropdown button opens, hanging under it.
#[derive(Clone)]
pub struct DocxRibbonMenu {
    pub open: bool,
    pub owner: String,
    pub entries: Vec<Rc<RefCell<DocxMenuEntry>>>,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub hot: i64,
}

impl DocxRibbonMenu {
    pub fn new() -> Self {
        Self {
            open: false,
            owner: String::new(),
            entries: Vec::new(),
            x: 0,
            y: 0,
            w: 180,
            hot: -1,
        }
    }
    pub fn reset(&mut self, owner0: String) {
        self.entries.clear();
        self.owner = owner0.clone();
        self.hot = -1;
        self.open = true;
    }
    pub fn close(&mut self) {
        self.open = false;
        self.owner = String::new();
        self.hot = -1;
    }
    pub fn add(&mut self, label: String, command: String, arg: String, checked: bool) -> Rc<RefCell<DocxMenuEntry>> {
        let mut e: Rc<RefCell<DocxMenuEntry>> = Rc::new(RefCell::new(DocxMenuEntry::new()));
        e.borrow_mut().label = label.clone();
        e.borrow_mut().command = command.clone();
        e.borrow_mut().arg = arg.clone();
        e.borrow_mut().checked = checked;
        self.entries.push(e.clone());
        e
    }
    pub fn rowH(&self) -> i64 {
        28
    }
    pub fn height(&self) -> i64 {
        (self.entries.len() as i64) * self.rowH() + 10
    }
    pub fn contains(&self, px: i64, py: i64) -> bool {
        ((px >= self.x && px < self.x + self.w) && py >= self.y) && py < self.y + self.height()
    }
    pub fn hit(&self, px: i64, py: i64) -> i64 {
        if !self.open || !self.contains(px, py) {
            return -1;
        }
        let k: i64 = ((py - self.y) - 5) / (self.rowH());
        if k < 0 || k >= (self.entries.len() as i64) {
            return -1;
        }
        k
    }
    pub fn at(&self, i: i64) -> Rc<RefCell<DocxMenuEntry>> {
        self.entries[i as usize].clone()
    }
}

/// A column of the ribbon: one large button, or a stack of rows of small ones.
#[derive(Clone)]
pub struct DocxRibbonCell {
    pub large: bool,
    /// Item indices into the ribbon's list, row by row; `rowEnds` says where
    /// each row stops.
    pub members: Vec<i64>,
    pub rowEnds: Vec<i64>,
    pub x: i64,
    pub w: i64,
}

impl DocxRibbonCell {
    pub fn new() -> Self {
        Self {
            large: false,
            members: Vec::new(),
            rowEnds: Vec::new(),
            x: 0,
            w: 0,
        }
    }
}

#[derive(Clone)]
pub struct DocxRibbonGroup {
    pub name: String,
    pub cells: Vec<Rc<RefCell<DocxRibbonCell>>>,
    pub x: i64,
    pub w: i64,
    /// How many rows the stacks in this group have: two rows sit lower and
    /// wider apart than three, the way Font and Paragraph do.
    pub rows: i64,
}

impl DocxRibbonGroup {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            cells: Vec::new(),
            x: 0,
            w: 0,
            rows: 3,
        }
    }
}

#[derive(Clone)]
pub struct DocxRibbon {
    pub tabs: Vec<String>,
    pub active: i64,
    pub items: Vec<Rc<RefCell<DocxRibbonItem>>>,
    pub groups: Vec<Rc<RefCell<DocxRibbonGroup>>>,
    pub hot: i64,
    pub hotTab: i64,
    /// Geometry, set by `layout`.
    pub tabX: Vec<i64>,
    pub tabW: Vec<i64>,
    pub x0: i64,
    pub y0: i64,
    pub width: i64,
    pub tabRowH: i64,
    pub bodyH: i64,
    pub built: String,
    pub menu: Rc<RefCell<DocxRibbonMenu>>,
}

impl DocxRibbon {
    pub fn new() -> Self {
        let mut me = Self {
            tabs: Vec::new(),
            active: 1,
            items: Vec::new(),
            groups: Vec::new(),
            hot: -1,
            hotTab: -1,
            tabX: Vec::new(),
            tabW: Vec::new(),
            x0: 0,
            y0: 0,
            width: 0,
            tabRowH: 36,
            bodyH: 108,
            built: String::new(),
            menu: Rc::new(RefCell::new(DocxRibbonMenu::new())),
        };
        me.tabs.push("File".to_string().to_string());
        me.tabs.push("Home".to_string().to_string());
        me.tabs.push("Insert".to_string().to_string());
        me.tabs.push("Layout".to_string().to_string());
        me.tabs.push("References".to_string().to_string());
        me.tabs.push("Review".to_string().to_string());
        me.tabs.push("View".to_string().to_string());
        me
    }
    pub fn height(&self) -> i64 {
        self.tabRowH + self.bodyH
    }
    pub fn tabName(&self) -> String {
        self.tabs[(self.active) as usize].clone()
    }
    pub fn selectTab(&mut self, i: i64) {
        if i >= 0 && i < (self.tabs.len() as i64) {
            self.active = i;
        }
    }
    pub fn tabIndex(&self, name: &str) -> i64 {
        let mut i: i64 = 0;
        while i < (self.tabs.len() as i64) {
            if self.tabs[i as usize].clone() == name {
                return i;
            }
            i += 1;
        }
        -1
    }
    /// --- building a page --------------------------------------------------------
    pub fn group(&mut self, name: String, rows: i64) {
        let mut g: Rc<RefCell<DocxRibbonGroup>> = Rc::new(RefCell::new(DocxRibbonGroup::new()));
        g.borrow_mut().name = name.clone();
        g.borrow_mut().rows = rows;
        self.groups.push(g.clone());
    }
    pub fn lastGroup(&self) -> Rc<RefCell<DocxRibbonGroup>> {
        self.groups[((self.groups.len() as i64) - 1) as usize].clone()
            .clone()
    }
    pub fn put(&mut self, it: Rc<RefCell<DocxRibbonItem>>) -> i64 {
        self.items.push(it.clone());
        (self.items.len() as i64) - 1
    }
    pub fn item(&self, command: String, label: String, icon: i64) -> Rc<RefCell<DocxRibbonItem>> {
        let mut it: Rc<RefCell<DocxRibbonItem>> = Rc::new(RefCell::new(DocxRibbonItem::new()));
        it.borrow_mut().command = command.clone();
        it.borrow_mut().label = label.clone();
        it.borrow_mut().tip = label.clone();
        it.borrow_mut().icon = icon;
        it
    }
    /// A big button: its own column.
    pub fn large(&mut self, command: &str, label: &str, icon: i64) -> Rc<RefCell<DocxRibbonItem>> {
        let mut it: Rc<RefCell<DocxRibbonItem>> = self.item(command.to_string(),
            label.to_string(),
            icon
        );
        it.borrow_mut().kind = 1;
        let mut c: Rc<RefCell<DocxRibbonCell>> = Rc::new(RefCell::new(DocxRibbonCell::new()));
        c.borrow_mut().large = true;
        c.borrow_mut().members.push(self.put(it.clone()));
        c.borrow_mut().rowEnds.push(c.borrow().members.len() as i64);
        let mut g: Rc<RefCell<DocxRibbonGroup>> = self.lastGroup();
        g.borrow_mut().cells.push(c.clone());
        it
    }
    /// A new stack of rows in the current group; `small` and friends fill it.
    pub fn stack(&self) {
        let mut c: Rc<RefCell<DocxRibbonCell>> = Rc::new(RefCell::new(DocxRibbonCell::new()));
        let mut g: Rc<RefCell<DocxRibbonGroup>> = self.lastGroup();
        g.borrow_mut().cells.push(c.clone());
    }
    pub fn lastCell(&self) -> Rc<RefCell<DocxRibbonCell>> {
        let mut g: Rc<RefCell<DocxRibbonGroup>> = self.lastGroup();
        return g.borrow().cells[((g.borrow().cells.len() as i64) - 1) as usize].clone();
    }
    /// Ends the row being filled; the next item starts a new one.
    pub fn row(&self) {
        let mut c: Rc<RefCell<DocxRibbonCell>> = self.lastCell();
        c.borrow_mut().rowEnds.push(c.borrow().members.len() as i64);
    }
    pub fn addToRow(&mut self, it: Rc<RefCell<DocxRibbonItem>>) -> Rc<RefCell<DocxRibbonItem>> {
        let mut c: Rc<RefCell<DocxRibbonCell>> = self.lastCell();
        c.borrow_mut().members.push(self.put(it.clone()));
        it
    }
    pub fn small(&mut self, command: &str, tip: &str, icon: i64) -> Rc<RefCell<DocxRibbonItem>> {
        let mut it: Rc<RefCell<DocxRibbonItem>> = self.item(command.to_string(),
            tip.to_string(),
            icon
        );
        it.borrow_mut().kind = 0;
        self.addToRow(it.clone())
    }
    pub fn glyphButton(&mut self, command: &str, tip: &str, glyph: String) -> Rc<RefCell<DocxRibbonItem>> {
        let mut it: Rc<RefCell<DocxRibbonItem>> = self.item(command.to_string(),
            tip.to_string(),
            0
        );
        it.borrow_mut().glyph = glyph.clone();
        it.borrow_mut().kind = 0;
        self.addToRow(it.clone())
    }
    pub fn labeled(&mut self, command: &str, label: &str, icon: i64) -> Rc<RefCell<DocxRibbonItem>> {
        let mut it: Rc<RefCell<DocxRibbonItem>> = self.item(command.to_string(),
            label.to_string(),
            icon
        );
        it.borrow_mut().kind = 2;
        self.addToRow(it.clone())
    }
    pub fn combo(&mut self, command: &str, tip: &str, w: i64) -> Rc<RefCell<DocxRibbonItem>> {
        let mut it: Rc<RefCell<DocxRibbonItem>> = self.item(command.to_string(),
            tip.to_string(),
            0
        );
        it.borrow_mut().kind = 3;
        it.borrow_mut().fieldW = w;
        self.addToRow(it.clone())
    }
    pub fn tile(&mut self, style_id: String, label: &str, size: f64, bold: bool, italic: bool, color: String) -> Rc<RefCell<DocxRibbonItem>> {
        let mut it: Rc<RefCell<DocxRibbonItem>> = self.item("format.style".to_string(),
            label.to_string(),
            0
        );
        it.borrow_mut().kind = 4;
        it.borrow_mut().arg = style_id.clone();
        it.borrow_mut().styleId = style_id.clone();
        it.borrow_mut().sampleSize = size;
        it.borrow_mut().sampleBold = bold;
        it.borrow_mut().sampleItalic = italic;
        it.borrow_mut().sampleColor = color.clone();
        let mut c: Rc<RefCell<DocxRibbonCell>> = Rc::new(RefCell::new(DocxRibbonCell::new()));
        c.borrow_mut().large = true;
        c.borrow_mut().members.push(self.put(it.clone()));
        c.borrow_mut().rowEnds.push(c.borrow().members.len() as i64);
        let mut g: Rc<RefCell<DocxRibbonGroup>> = self.lastGroup();
        g.borrow_mut().cells.push(c.clone());
        it
    }
    /// Builds the page that is showing. Cheap enough to do on a tab change and
    /// nowhere else; `built` says which page the items are.
    pub fn build(&mut self) {
        let name: String = self.tabName();
        if name == self.built {
            return;
        }
        self.built = name.clone();
        self.menu.clone().borrow_mut().close();
        self.items.clear();
        self.groups.clear();
        self.hot = -1;
        if name == "File" {
            self.buildFile();
        }
        if name == "Home" {
            self.buildHome();
        }
        if name == "Insert" {
            self.buildInsert();
        }
        if name == "Layout" {
            self.buildLayout();
        }
        if name == "References" {
            self.buildReferences();
        }
        if name == "Review" {
            self.buildReview();
        }
        if name == "View" {
            self.buildView();
        }
    }
    pub fn rebuild(&mut self) {
        self.built = String::new();
        self.build();
    }
    pub fn buildFile(&mut self) {
        self.group("Document".to_string(), 3);
        self.large("file.open", "Open", ToolIcon::openFile());
        self.large("file.save", "Save a copy", ToolIcon::save());
        self.large("file.print", "Print", ToolIcon::printDoc());
        self.group("Info".to_string(), 3);
        let mut a: Rc<RefCell<DocxRibbonItem>> = self.large("file.properties",
            "Properties",
            ToolIcon::report()
        );
        a.borrow_mut().enabled = false;
    }
    pub fn buildHome(&mut self) {
        self.group("Clipboard".to_string(), 3);
        self.large("edit.paste", "Paste", ToolIcon::paste());
        self.stack();
        self.labeled("edit.cut", "Cut", ToolIcon::cut());
        self.row();
        self.labeled("edit.copy", "Copy", ToolIcon::copy());
        self.row();
        self.labeled("format.painter", "Format", ToolIcon::paint());
        self.group("Font".to_string(), 2);
        self.stack();
        self.combo("format.font", "Font", 132);
        self.combo("format.size.pick", "Font size", 48);
        self.small("format.size.up", "Grow font", ToolIcon::sizeUp());
        self.small("format.size.down", "Shrink font", ToolIcon::sizeDown());
        let mut cc: Rc<RefCell<DocxRibbonItem>> = self.glyphButton("format.case",
            "Change case",
            "Aa".to_string()
        );
        cc.borrow_mut().menu = 1;
        self.small("format.clear", "Clear formatting", ToolIcon::clearFmt());
        self.row();
        let mut b: Rc<RefCell<DocxRibbonItem>> = self.small("format.bold",
            "Bold (Ctrl+B)",
            ToolIcon::bold()
        );
        b.borrow_mut().toggleOp = "bold".to_string();
        let mut i: Rc<RefCell<DocxRibbonItem>> = self.small("format.italic",
            "Italic (Ctrl+I)",
            ToolIcon::italic()
        );
        i.borrow_mut().toggleOp = "italic".to_string();
        let mut u: Rc<RefCell<DocxRibbonItem>> = self.small("format.underline",
            "Underline (Ctrl+U)",
            ToolIcon::underline()
        );
        u.borrow_mut().toggleOp = "underline".to_string();
        let mut s: Rc<RefCell<DocxRibbonItem>> = self.small("format.strike",
            "Strikethrough",
            ToolIcon::strike()
        );
        s.borrow_mut().toggleOp = "strike".to_string();
        let mut sub: Rc<RefCell<DocxRibbonItem>> = self.glyphButton("format.subscript",
            "Subscript (Ctrl+=)",
            "x₂".to_string()
        );
        sub.borrow_mut().toggleOp = "sub".to_string();
        let mut sup: Rc<RefCell<DocxRibbonItem>> = self.glyphButton("format.superscript",
            "Superscript (Ctrl+Shift+=)",
            "x²".to_string()
        );
        sup.borrow_mut().toggleOp = "super".to_string();
        let mut hl: Rc<RefCell<DocxRibbonItem>> = self.small("format.highlight",
            "Text highlight colour",
            ToolIcon::fill()
        );
        hl.borrow_mut().toggleOp = "highlight".to_string();
        hl.borrow_mut().menu = 2;
        let mut fc: Rc<RefCell<DocxRibbonItem>> = self.small("format.color",
            "Font colour",
            ToolIcon::textColor()
        );
        fc.borrow_mut().menu = 2;
        self.group("Paragraph".to_string(), 2);
        self.stack();
        let mut bl: Rc<RefCell<DocxRibbonItem>> = self.small("para.bullets",
            "Bullets",
            ToolIcon::bulletList()
        );
        bl.borrow_mut().menu = 2;
        let mut nl: Rc<RefCell<DocxRibbonItem>> = self.small("para.numbering",
            "Numbering",
            ToolIcon::numberList()
        );
        nl.borrow_mut().menu = 2;
        let mut ml: Rc<RefCell<DocxRibbonItem>> = self.small("para.multilevel",
            "Multilevel list",
            ToolIcon::outlineList()
        );
        ml.borrow_mut().enabled = false;
        self.small("text.outdent", "Decrease indent", ToolIcon::indentLess());
        self.small("text.indent", "Increase indent", ToolIcon::indentMore());
        let mut so: Rc<RefCell<DocxRibbonItem>> = self.small("para.sort",
            "Sort",
            ToolIcon::sortAsc()
        );
        so.borrow_mut().enabled = false;
        let mut pm: Rc<RefCell<DocxRibbonItem>> = self.glyphButton("view.marks",
            "Show paragraph marks",
            "¶".to_string()
        );
        pm.borrow_mut().enabled = false;
        self.row();
        let mut al: Rc<RefCell<DocxRibbonItem>> = self.small("para.align",
            "Align left",
            ToolIcon::alignLeft()
        );
        al.borrow_mut().arg = "0".to_string();
        let mut ac: Rc<RefCell<DocxRibbonItem>> = self.small("para.align",
            "Centre",
            ToolIcon::alignCenter()
        );
        ac.borrow_mut().arg = "1".to_string();
        let mut ar: Rc<RefCell<DocxRibbonItem>> = self.small("para.align",
            "Align right",
            ToolIcon::alignRight()
        );
        ar.borrow_mut().arg = "2".to_string();
        let mut aj: Rc<RefCell<DocxRibbonItem>> = self.small("para.align",
            "Justify",
            ToolIcon::textJustify()
        );
        aj.borrow_mut().arg = "3".to_string();
        let mut ls: Rc<RefCell<DocxRibbonItem>> = self.glyphButton("para.spacing",
            "Line and paragraph spacing",
            "↕".to_string()
        );
        ls.borrow_mut().menu = 1;
        let mut sh: Rc<RefCell<DocxRibbonItem>> = self.small("para.shading",
            "Shading",
            ToolIcon::fill()
        );
        sh.borrow_mut().enabled = false;
        let mut bo: Rc<RefCell<DocxRibbonItem>> = self.small("para.borders",
            "Borders",
            ToolIcon::borderAll()
        );
        bo.borrow_mut().enabled = false;
        self.group("Styles".to_string(), 3);
        self.tile("Normal".to_string(),
            "Normal",
            13.0,
            false,
            false,
            String::new()
        );
        self.tile("NoSpacing".to_string(),
            "No Spacing",
            13.0,
            false,
            false,
            String::new()
        );
        self.tile("Heading1".to_string(),
            "Heading 1",
            17.0,
            false,
            false,
            "2F5496".to_string()
        );
        self.tile("Title".to_string(), "Title", 22.0, false, false, String::new());
        self.tile("Heading2".to_string(),
            "Heading 2",
            15.0,
            false,
            false,
            "2F5496".to_string()
        );
        self.tile("Subtitle".to_string(),
            "Subtitle",
            13.0,
            false,
            false,
            "5A5A5A".to_string()
        );
        self.group("Editing".to_string(), 3);
        self.stack();
        self.labeled("edit.find", "Find", ToolIcon::search());
        self.row();
        let mut rp: Rc<RefCell<DocxRibbonItem>> = self.labeled("edit.replace",
            "Replace",
            ToolIcon::formatPaste()
        );
        rp.borrow_mut().enabled = false;
        self.row();
        self.labeled("edit.selectAll", "Select all", ToolIcon::editMode());
        self.group("Mode".to_string(), 3);
        self.large("edit.mode", "Editing", ToolIcon::editMode());
    }
    pub fn buildInsert(&mut self) {
        self.group("Pages".to_string(), 3);
        self.large("insert.pageBreak", "Page Break", ToolIcon::newFile());
        self.group("Tables".to_string(), 3);
        let mut tb: Rc<RefCell<DocxRibbonItem>> = self.large("insert.table",
            "Table",
            ToolIcon::borderAll()
        );
        tb.borrow_mut().menu = 1;
        self.group("Illustrations".to_string(), 3);
        let mut pic: Rc<RefCell<DocxRibbonItem>> = self.large("insert.picture",
            "Pictures",
            ToolIcon::picture()
        );
        pic.borrow_mut().enabled = false;
        let mut shp: Rc<RefCell<DocxRibbonItem>> = self.large("insert.shapes",
            "Shapes",
            ToolIcon::shapes()
        );
        shp.borrow_mut().enabled = false;
        let mut ch: Rc<RefCell<DocxRibbonItem>> = self.large("insert.chart",
            "Chart",
            ToolIcon::chart()
        );
        ch.borrow_mut().enabled = false;
        self.group("Links".to_string(), 3);
        let mut ln: Rc<RefCell<DocxRibbonItem>> = self.large("insert.link",
            "Link",
            ToolIcon::link()
        );
        ln.borrow_mut().enabled = false;
        self.group("Comments".to_string(), 3);
        let mut cm: Rc<RefCell<DocxRibbonItem>> = self.large("insert.comment",
            "Comment",
            ToolIcon::comment()
        );
        cm.borrow_mut().enabled = false;
        self.group("Text".to_string(), 3);
        let mut tx: Rc<RefCell<DocxRibbonItem>> = self.large("insert.textBox",
            "Text Box",
            ToolIcon::textBox()
        );
        tx.borrow_mut().enabled = false;
    }
    pub fn buildLayout(&mut self) {
        self.group("Page Setup".to_string(), 3);
        let mut mg: Rc<RefCell<DocxRibbonItem>> = self.large("layout.margins",
            "Margins",
            ToolIcon::borderOut()
        );
        mg.borrow_mut().enabled = false;
        let mut orr: Rc<RefCell<DocxRibbonItem>> = self.large("layout.orientation",
            "Orientation",
            ToolIcon::newFile()
        );
        orr.borrow_mut().enabled = false;
        let mut sz: Rc<RefCell<DocxRibbonItem>> = self.large("layout.size",
            "Size",
            ToolIcon::report()
        );
        sz.borrow_mut().enabled = false;
        self.group("Paragraph".to_string(), 3);
        self.stack();
        self.labeled("text.indent", "Indent more", ToolIcon::indentMore());
        self.row();
        self.labeled("text.outdent", "Indent less", ToolIcon::indentLess());
        self.group("Arrange".to_string(), 3);
        let mut al: Rc<RefCell<DocxRibbonItem>> = self.large("layout.align",
            "Align",
            ToolIcon::arrangeLeft()
        );
        al.borrow_mut().enabled = false;
    }
    pub fn buildReferences(&mut self) {
        self.group("Table of Contents".to_string(), 3);
        let mut toc: Rc<RefCell<DocxRibbonItem>> = self.large("ref.toc",
            "Table of Contents",
            ToolIcon::outlineList()
        );
        toc.borrow_mut().enabled = false;
        self.group("Footnotes".to_string(), 3);
        let mut r#fn: Rc<RefCell<DocxRibbonItem>> = self.large("ref.footnote",
            "Insert Footnote",
            ToolIcon::comment()
        );
        r#fn.borrow_mut().enabled = false;
    }
    pub fn buildReview(&mut self) {
        self.group("Proofing".to_string(), 3);
        let mut sp: Rc<RefCell<DocxRibbonItem>> = self.large("review.spelling",
            "Spelling",
            ToolIcon::validate()
        );
        sp.borrow_mut().enabled = false;
        self.group("Comments".to_string(), 3);
        let mut nc: Rc<RefCell<DocxRibbonItem>> = self.large("review.comment",
            "New Comment",
            ToolIcon::comment()
        );
        nc.borrow_mut().enabled = false;
        self.group("Tracking".to_string(), 3);
        let mut tc: Rc<RefCell<DocxRibbonItem>> = self.large("review.track",
            "Track Changes",
            ToolIcon::editMode()
        );
        tc.borrow_mut().enabled = false;
    }
    pub fn buildView(&mut self) {
        self.group("Views".to_string(), 3);
        self.large("view.continuous", "Print Layout", ToolIcon::report());
        self.group("Show".to_string(), 3);
        self.stack();
        self.labeled("view.ruler", "Ruler", ToolIcon::alignLeft());
        self.row();
        self.labeled("view.nav", "Navigation Pane", ToolIcon::outlineList());
        self.group("Zoom".to_string(), 3);
        self.large("view.zoom.in", "Zoom in", ToolIcon::sizeUp());
        self.large("view.zoom.out", "Zoom out", ToolIcon::sizeDown());
        self.large("view.zoom.reset", "100%", ToolIcon::search());
        self.group("Pages".to_string(), 3);
        self.stack();
        self.labeled("page.prev", "Previous page", ToolIcon::navPrev());
        self.row();
        self.labeled("page.next", "Next page", ToolIcon::navNext());
        self.group("Direction".to_string(), 3);
        self.large("view.rtl", "Right to left", ToolIcon::textRtl());
    }
    /// --- geometry ---------------------------------------------------------------
    /// Lays the tab row and the page out from (x, y) across `w`. `measure`
    /// gives the width of a label at a size, which only the painter can answer.
    fn layout(&mut self, x: i64, y: i64, w: i64, view: &Rc<RefCell<EVGToolbarView>>) {
        self.build();
        self.x0 = x;
        self.y0 = y;
        self.width = w;
        self.tabX.clear();
        self.tabW.clear();
        let mut tx: i64 = x + 12;
        let mut ti: i64 = 0;
        while ti < (self.tabs.len() as i64) {
            let nm: String = self.tabs[ti as usize].clone();
            let tw: i64 = ((UITextRenderer::measureWidth(
                &view.borrow().ctx.borrow().tr,
                &nm,
                13.0
            )).floor() as i64) + 22;
            self.tabX.push(tx);
            self.tabW.push(tw);
            tx += tw;
            ti += 1;
        }
        let top: i64 = (y + self.tabRowH) + 6;
        let mut gx: i64 = x + 10;
        let mut gi: i64 = 0;
        while gi < (self.groups.len() as i64) {
            let mut g: Rc<RefCell<DocxRibbonGroup>> = self.groups[gi as usize].clone();
            g.borrow_mut().x = gx;
            let mut cx: i64 = gx + 6;
            let mut ci: i64 = 0;
            while ci < (g.borrow().cells.len() as i64) {
                let mut c: Rc<RefCell<DocxRibbonCell>> = g.borrow().cells[ci as usize].clone();
                c.borrow_mut().x = cx;
                let _tmp_2672 = self.layoutCell(&c, cx, top, g.borrow().rows.clone(), view);
                c.borrow_mut().w = _tmp_2672;
                cx = (cx + c.borrow().w) + 6;
                ci += 1;
            }
            let mut _tmp_2673 = g.borrow().name.clone();
            let name_w: i64 = ((UITextRenderer::measureWidth(
                &view.borrow().ctx.borrow().tr,
                &_tmp_2673,
                11.0
            )).floor() as i64) + 16;
            let mut gw: i64 = (cx - gx) + 2;
            if gw < name_w {
                gw = name_w;
            }
            g.borrow_mut().w = gw;
            gx = (gx + gw) + 9;
            gi += 1;
        }
    }
    /// One column; answers its width.
    fn layoutCell(&self, c: &Rc<RefCell<DocxRibbonCell>>, x: i64, top: i64, rows: i64, view: &Rc<RefCell<EVGToolbarView>>) -> i64 {
        // The row being filled when the page was built has no end yet.
        let n: i64 = c.borrow().rowEnds.len() as i64;
        if n == 0 || c.borrow().rowEnds[(n - 1) as usize] < (c.borrow().members.len() as i64) {
            c.borrow_mut().rowEnds.push(c.borrow().members.len() as i64);
        }
        let mut widest: i64 = 0;
        let mut row_start: i64 = 0;
        let mut r: i64 = 0;
        while r < (c.borrow().rowEnds.len() as i64) {
            let row_end: i64 = c.borrow().rowEnds[r as usize];
            let mut ry: i64 = top + r * 25;
            let mut rh: i64 = 23;
            if rows == 2 {
                ry = (top + 6) + r * 32;
                rh = 28;
            }
            let mut rx: i64 = x;
            let mut k: i64 = row_start;
            while k < row_end {
                let mut it: Rc<RefCell<DocxRibbonItem>> = self.items[(c.borrow().members[k as usize]) as usize].clone();
                if it.borrow().kind == 1 {
                    it.borrow_mut().x = rx;
                    it.borrow_mut().y = top;
                    it.borrow_mut().h = 72;
                    let mut _tmp_2675 = it.borrow().label.clone();
                    let lw: i64 = ((UITextRenderer::measureWidth(
                        &view.borrow().ctx.borrow().tr,
                        &_tmp_2675,
                        11.5
                    )).floor() as i64) + 12;
                    it.borrow_mut().w = 52;
                    if lw > it.borrow().w {
                        it.borrow_mut().w = lw;
                    }
                }
                if it.borrow().kind == 4 {
                    it.borrow_mut().x = rx;
                    it.borrow_mut().y = top + 2;
                    it.borrow_mut().w = 84;
                    it.borrow_mut().h = 66;
                }
                if it.borrow().kind == 0 {
                    it.borrow_mut().x = rx;
                    it.borrow_mut().y = ry;
                    it.borrow_mut().w = 28;
                    if it.borrow().menu > 0 {
                        it.borrow_mut().w = 40;
                    }
                    it.borrow_mut().h = rh;
                }
                if it.borrow().kind == 2 {
                    it.borrow_mut().x = rx;
                    it.borrow_mut().y = ry;
                    it.borrow_mut().h = 23;
                    let mut _tmp_2676 = it.borrow().label.clone();
                    let _tmp_2677 = ((UITextRenderer::measureWidth(
                        &view.borrow().ctx.borrow().tr,
                        &_tmp_2676,
                        12.5
                    )).floor() as i64) + 34;
                    it.borrow_mut().w = _tmp_2677;
                }
                if it.borrow().kind == 3 {
                    it.borrow_mut().x = rx;
                    it.borrow_mut().y = ry + 1;
                    it.borrow_mut().h = rh - 2;
                    it.borrow_mut().w = it.borrow().fieldW;
                }
                rx = (rx + it.borrow().w) + 2;
                k += 1;
            }
            if rx - x > widest {
                widest = rx - x;
            }
            row_start = row_end;
            r += 1;
        }
        if widest > 0 {
            widest -= 2;
        }
        widest
    }
    pub fn hit(&self, px: i64, py: i64) -> i64 {
        let mut i: i64 = 0;
        while i < (self.items.len() as i64) {
            let mut it: Rc<RefCell<DocxRibbonItem>> = self.items[i as usize].clone();
            if it.borrow().contains(px, py) {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn hitTab(&self, px: i64, py: i64) -> i64 {
        if py < self.y0 || py >= self.y0 + self.tabRowH {
            return -1;
        }
        let mut i: i64 = 0;
        while i < (self.tabX.len() as i64) {
            let tx: i64 = self.tabX[i as usize];
            if px >= tx && px < tx + self.tabW[i as usize] {
                return i;
            }
            i += 1;
        }
        -1
    }
    pub fn at(&self, i: i64) -> Rc<RefCell<DocxRibbonItem>> {
        self.items[i as usize].clone()
    }
    pub fn indexOf(&self, command: &str, arg: &str) -> i64 {
        let mut i: i64 = 0;
        while i < (self.items.len() as i64) {
            let mut it: Rc<RefCell<DocxRibbonItem>> = self.items[i as usize].clone();
            if it.borrow().command == command && it.borrow().arg == arg {
                return i;
            }
            i += 1;
        }
        -1
    }
}

/// A heading in the navigation pane.
#[derive(Clone)]
pub struct NavEntry {
    pub paragraphId: i64,
    pub level: i64,
    pub text: String,
    pub page: i64,
    pub y: i64,
    pub h: i64,
}

impl NavEntry {
    pub fn new() -> Self {
        Self {
            paragraphId: 0,
            level: 1,
            text: String::new(),
            page: 0,
            y: 0,
            h: 0,
        }
    }
}

/// The pane at the left: the document's headings, its pages, or what a search
/// found — the three ways a reader gets around a long document without
/// scrolling through it.
#[derive(Clone)]
pub struct DocxNavPane {
    pub visible: bool,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    /// 0 headings, 1 pages, 2 results.
    pub mode: i64,
    pub query: String,
    pub searchFocused: bool,
    pub entries: Vec<Rc<RefCell<NavEntry>>>,
    pub hot: i64,
    pub current: i64,
    pub scroll: i64,
    pub builtRevision: i64,
    pub builtMode: i64,
    pub builtQuery: String,
    pub builtPages: i64,
    pub resultCount: i64,
}

impl DocxNavPane {
    pub fn new() -> Self {
        Self {
            visible: true,
            x: 0,
            y: 0,
            w: 280,
            h: 0,
            mode: 0,
            query: String::new(),
            searchFocused: false,
            entries: Vec::new(),
            hot: -1,
            current: -1,
            scroll: 0,
            builtRevision: -1,
            builtMode: -1,
            builtQuery: String::new(),
            builtPages: -1,
            resultCount: 0,
        }
    }
    /// The level of a heading style, or 0 for a paragraph that is not one.
    /// Word's ids are "Heading1".."Heading9"; files written by other tools say
    /// "Heading 1" or "heading 1".
    pub fn headingLevel(style_id: &str) -> i64 {
        let s: String = style_id.to_lowercase();
        let n: i64 = DxNative::str_len(&s);
        if n < 8 {
            return 0;
        }
        if DxNative::substr(&s, 0, 7) != "heading" {
            return 0;
        }
        let last: i64 = DxNative::char_at(&s, n - 1) - 48;
        if (last >= 1 && last <= 9) {
            return last;
        }
        0
    }
    pub fn markDirty(&mut self) {
        self.builtRevision = -1;
    }
    /// The entries for the current mode, rebuilt only when the document, the
    /// mode, the query or the page count changed.
    fn refresh(&mut self, doc: &Rc<RefCell<RichDocument>>, layout: &Rc<RefCell<DocxLayout>>) {
        let pages: i64 = layout.borrow().pageCount();
        if ((self.builtRevision == doc.borrow().revision && self.builtMode == self.mode) && self.builtQuery == self.query) && self.builtPages == pages {
            return;
        }
        self.builtRevision = doc.borrow().revision;
        self.builtMode = self.mode;
        self.builtQuery = self.query.clone();
        self.builtPages = pages;
        self.entries.clear();
        self.hot = -1;
        if self.mode == 1 {
            let mut p: i64 = 0;
            while p < pages {
                let mut e: Rc<RefCell<NavEntry>> = Rc::new(RefCell::new(NavEntry::new()));
                e.borrow_mut().page = p;
                e.borrow_mut().text = format!("Page {}", p + 1);
                e.borrow_mut().level = 1;
                self.entries.push(e.clone());
                p += 1;
            }
            return;
        }
        let mut page_of: HashMap<i64,i64> = HashMap::new();
        let mut pi: i64 = 0;
        while pi < pages {
            let mut pg: Rc<RefCell<LaidPage>> = layout.borrow().pageAt(pi);
            let mut li: i64 = 0;
            while li < (pg.borrow().lines.len() as i64) {
                let mut ln: Rc<RefCell<LaidLine>> = pg.borrow().lines[li as usize].clone();
                if !page_of.contains_key(&ln.borrow().paraIndex) {
                    page_of.insert(ln.borrow().paraIndex, pi);
                }
                li += 1;
            }
            pi += 1;
        }
        let want: String = self.query.to_lowercase();
        self.resultCount = 0;
        let mut bi: i64 = 0;
        while bi < (doc.borrow().blocks.len() as i64) {
            let mut b: Rc<RefCell<DocBlock>> = doc.borrow().blocks[bi as usize].clone();
            if b.borrow().kind == DocBlockKind::para() {
                let mut para: Rc<RefCell<Paragraph>> = b.borrow().paragraph.clone().clone();
                let lvl: i64 = DocxNavPane::headingLevel(&para.borrow().styleId.clone());
                let mut take: bool = false;
                if self.mode == 0 {
                    take = lvl > 0 && !para.borrow().text.trim().to_string().is_empty();
                }
                if self.mode == 2 {
                    if !want.is_empty() {
                        if DxNative::index_of(&para.borrow().text.to_lowercase(), &want) >= 0 {
                            take = true;
                            self.resultCount += 1;
                        }
                    }
                }
                if take {
                    let mut e_1: Rc<RefCell<NavEntry>> = Rc::new(RefCell::new(NavEntry::new()));
                    e_1.borrow_mut().paragraphId = para.borrow().id;
                    e_1.borrow_mut().level = lvl;
                    if self.mode == 2 {
                        e_1.borrow_mut().level = 1;
                    }
                    e_1.borrow_mut().text = para.borrow().text.trim().to_string();
                    e_1.borrow_mut().page = 0;
                    let pg_hit: Option<i64> = page_of.get(&para.borrow().id).cloned();
                    if pg_hit.is_some() {
                        e_1.borrow_mut().page = pg_hit.unwrap();
                    }
                    self.entries.push(e_1.clone());
                }
            }
            bi += 1;
        }
    }
    /// Which entry the reader is in: the last one at or before the page they
    /// are looking at (headings), or that page (pages).
    pub fn settleCurrent(&mut self, page: i64, caret_pid: i64) {
        self.current = -1;
        let mut i: i64 = 0;
        while i < (self.entries.len() as i64) {
            let mut e: Rc<RefCell<NavEntry>> = self.entries[i as usize].clone();
            if self.mode == 1 {
                if e.borrow().page == page {
                    self.current = i;
                }
            } else {
                if caret_pid > 0 {
                    if e.borrow().paragraphId == caret_pid {
                        self.current = i;
                    }
                }
                if self.current < 0 {
                    if e.borrow().page <= page {
                        self.current = i;
                    }
                }
            }
            i += 1;
        }
    }
    /// --- geometry ---------------------------------------------------------------
    pub fn searchY(&self) -> i64 {
        self.y + 46
    }
    pub fn tabsY(&self) -> i64 {
        self.y + 86
    }
    pub fn listY(&self) -> i64 {
        self.y + 126
    }
    pub fn closeX(&self) -> i64 {
        (self.x + self.w) - 40
    }
    /// 0 nothing, 1 close, 2 search box, 3.. tabs (3 + mode), 10 a list row.
    pub fn hitPart(&self, px: i64, py: i64) -> i64 {
        if !self.visible {
            return 0;
        }
        if ((px < self.x || px >= self.x + self.w) || py < self.y) || py >= self.y + self.h {
            return 0;
        }
        if (py >= self.y + 12 && py < self.y + 36) && px >= self.closeX() {
            return 1;
        }
        let sy: i64 = self.searchY();
        if py >= sy && py < sy + 30 {
            return 2;
        }
        let ty: i64 = self.tabsY();
        if py >= ty && py < ty + 28 {
            let k: i64 = self.tabAtX(px);
            if k >= 0 {
                return 3 + k;
            }
            return 0;
        }
        if py >= self.listY() {
            return 10;
        }
        0
    }
    pub fn tabLeft(&self, k: i64) -> i64 {
        if k == 0 {
            return self.x + 16;
        }
        if k == 1 {
            return self.x + 104;
        }
        self.x + 172
    }
    pub fn tabWidth(&self, k: i64) -> i64 {
        if k == 0 {
            return 82;
        }
        if k == 1 {
            return 62;
        }
        72
    }
    pub fn tabAtX(&self, px: i64) -> i64 {
        let mut k: i64 = 0;
        while k < 3 {
            let l: i64 = self.tabLeft(k);
            if px >= l && px < l + self.tabWidth(k) {
                return k;
            }
            k += 1;
        }
        -1
    }
    pub fn entryAt(&self, py: i64) -> i64 {
        let mut i: i64 = 0;
        while i < (self.entries.len() as i64) {
            let mut e: Rc<RefCell<NavEntry>> = self.entries[i as usize].clone();
            if (e.borrow().h > 0 && py >= e.borrow().y) && py < e.borrow().y + e.borrow().h {
                return i;
            }
            i += 1;
        }
        -1
    }
}

/// Draws the frame's parts. Holds the shared toolbar painter for its icons and
/// its text, so a glyph here is the glyph the other editors draw.
pub struct DocxChromePainter {
    view: Rc<RefCell<EVGToolbarView>>,
    pub theme: Rc<RefCell<DocxChromeTheme>>,
    offTheme: Rc<RefCell<EVGToolbarTheme>>,
    onTheme: Rc<RefCell<EVGToolbarTheme>>,
}

impl DocxChromePainter {
    pub fn new() -> Self {
        let mut me = Self {
            view: Rc::new(RefCell::new(EVGToolbarView::new())),
            theme: Rc::new(RefCell::new(DocxChromeTheme::new())),
            offTheme: Rc::new(RefCell::new(EVGToolbarTheme::new())),
            onTheme: Rc::new(RefCell::new(EVGToolbarTheme::new())),
        };
        me.offTheme.borrow_mut().ink = EVGColor::rgb(184, 189, 199);
        me.offTheme.borrow_mut().glyphInk = EVGColor::rgb(184, 189, 199);
        me.offTheme.borrow_mut().dim = EVGColor::rgb(196, 200, 208);
        me.offTheme.borrow_mut().glyph = EVGColor::rgb(206, 210, 218);
        me.offTheme.borrow_mut().accent = EVGColor::rgb(178, 190, 220);
        me.offTheme.borrow_mut().warn = EVGColor::rgb(226, 180, 178);
        me.offTheme.borrow_mut().link = EVGColor::rgb(178, 190, 220);
        me
    }
    pub fn roundPts(x: f64, y: f64, w: f64, h: f64, r0: f64) -> Vec<f64> {
        let mut pts: Vec<f64> = Vec::new();
        let mut r: f64 = r0;
        if r > w / 2.0 {
            r = w / 2.0;
        }
        if r > h / 2.0 {
            r = h / 2.0;
        }
        // Four quarter circles, five points each, clockwise from the top left.
        let mut cxs: Vec<f64> = Vec::new();
        cxs.push((x + w) - r);
        cxs.push((x + w) - r);
        cxs.push(x + r);
        cxs.push(x + r);
        let mut cys: Vec<f64> = Vec::new();
        cys.push(y + r);
        cys.push((y + h) - r);
        cys.push((y + h) - r);
        cys.push(y + r);
        let mut q: i64 = 0;
        while q < 4 {
            let mut s: i64 = 0;
            while s <= 4 {
                let a: f64 = (((q * 4 + s) as f64) - 4.0) * (3.14159265 / 8.0);
                pts.push(cxs[q as usize] + r * a.cos());
                pts.push(cys[q as usize] + r * a.sin());
                s += 1;
            }
            q += 1;
        }
        pts
    }
    /// A label cut to a width, with an ellipsis.
    pub fn fit(p: &Rc<RefCell<DocxChromePainter>>, s: &str, size: f64, max_w: i64) -> String {
        if ((p.borrow().textW(s, size)).floor() as i64) <= max_w {
            return s.to_string();
        }
        let mut n: i64 = DxNative::str_len(&s);
        while n > 1 {
            n -= 1;
            let cut: String = format!("{}…", DxNative::substr(&s, 0, n));
            if ((p.borrow().textW(&cut, size)).floor() as i64) <= max_w {
                return cut;
            }
        }
        "…".to_string()
    }
    fn attach(&self, ctx: Rc<RefCell<UIContext>>) {
        EVGToolbarView::attach(&self.view.clone(), ctx.clone(), "Open Sans".to_string());
    }
    /// --- primitives -------------------------------------------------------------
    fn rect(&self, dl: &Rc<RefCell<EVGDisplayList>>, x: i64, y: i64, w: i64, h: i64, col: &Rc<RefCell<EVGColor>>) {
        EVGDisplayList::addRect(&dl, x as f64, y as f64, w as f64, h as f64, col);
    }
    /// A rectangle with rounded corners, as a polygon: the display list has no
    /// rounded rectangle of its own and the frame is made of them.
    fn round(&self, dl: &Rc<RefCell<EVGDisplayList>>, x: i64, y: i64, w: i64, h: i64, r: i64, col: &Rc<RefCell<EVGColor>>) {
        let mut pts: Vec<f64> = DocxChromePainter::roundPts(
            x as f64,
            y as f64,
            w as f64,
            h as f64,
            r as f64
        );
        EVGDisplayList::addPolygon(&dl, &pts, col);
    }
    fn roundFrame(&self, dl: &Rc<RefCell<EVGDisplayList>>, x: i64, y: i64, w: i64, h: i64, r: i64, col: &Rc<RefCell<EVGColor>>) {
        let mut pts: Vec<f64> = DocxChromePainter::roundPts(
            (x as f64) + 0.5,
            (y as f64) + 0.5,
            (w as f64) - 1.0,
            (h as f64) - 1.0,
            r as f64
        );
        pts.push(pts[0]);
        pts.push(pts[1]);
        EVGDisplayList::addPolyline(&dl, &pts, 1.0, col);
    }
    pub fn textW(&self, s: &str, size: f64) -> f64 {
        return UITextRenderer::measureWidth(
            &self.view.borrow().ctx.borrow().tr,
            s,
            size
        );
    }
    /// Text with its top at y.
    fn text(&self, dl: &Rc<RefCell<EVGDisplayList>>, s: &str, x: i64, y: i64, size: f64, col: &Rc<RefCell<EVGColor>>, bold: bool) {
        let _tmp_2695 = self.view.clone();
        EVGToolbarView::pushStyledText(
            &_tmp_2695,
            dl,
            s,
            x as f64,
            y as f64,
            size,
            col,
            "",
            bold,
            false
        );
    }
    /// Text centred on (cx, cy).
    fn textMid(&self, dl: &Rc<RefCell<EVGDisplayList>>, s: &str, cx: i64, cy: i64, size: f64, col: &Rc<RefCell<EVGColor>>, bold: bool) {
        EVGToolbarView::pushToolText(&self.view.clone(), dl, s, cx, cy, size, col, bold);
    }
    fn icon(&self, dl: &Rc<RefCell<EVGDisplayList>>, k: i64, cx: i64, cy: i64, size: i64, enabled: bool) {
        let mut t: Rc<RefCell<ToolItem>> = Rc::new(RefCell::new(ToolItem::new()));
        t.borrow_mut().icon = k;
        t.borrow_mut().x = cx - 14;
        t.borrow_mut().y = cy - 14;
        t.borrow_mut().w = 28;
        t.borrow_mut().h = 28;
        self.view.borrow_mut().glyphSize = size;
        if enabled {
            self.view.borrow_mut().theme = self.onTheme.clone();
        } else {
            self.view.borrow_mut().theme = self.offTheme.clone();
        }
        EVGToolbarView::paintToolIcon(&self.view.clone(), dl, &t, 28);
        self.view.borrow_mut().theme = self.onTheme.clone();
        self.view.borrow_mut().glyphSize = 24;
    }
    fn chevron(&self, dl: &Rc<RefCell<EVGDisplayList>>, cx: i64, cy: i64, col: &Rc<RefCell<EVGColor>>) {
        let mut pts: Vec<f64> = Vec::new();
        pts.push((cx as f64) - 3.5);
        pts.push((cy as f64) - 1.5);
        pts.push(cx as f64);
        pts.push((cy as f64) + 2.0);
        pts.push((cx as f64) + 3.5);
        pts.push((cy as f64) - 1.5);
        EVGDisplayList::addPolyline(&dl, &pts, 1.3, col);
    }
    /// --- the title bar ----------------------------------------------------------
    fn paintTitleBar(&self, dl: &Rc<RefCell<EVGDisplayList>>, w: i64, h: i64, title: &str, dirty: bool, search_hot: bool) {
        self.rect(dl, 0, 0, w, h, &self.theme.borrow().titleBar.clone());
        // The program's mark: a page with a fold, in the accent colour.
        self.round(dl, 12, 9, 26, 26, 6, &self.theme.borrow().accent.clone());
        self.textMid(dl, "R", 25, 22, 14.0, &self.theme.borrow().accentInk.clone(), true);
        // Quick access: save, undo, redo.
        self.icon(dl, ToolIcon::save(), 62, 22, 18, true);
        self.icon(dl, ToolIcon::undo(), 92, 22, 18, true);
        self.icon(dl, ToolIcon::redo(), 122, 22, 18, true);
        // The document's name, and whether it has changed since it was opened.
        let mut t: String = title.to_string();
        if t.is_empty() {
            t = "Document".to_string();
        }
        let mut state: String = "Saved";
        if dirty {
            state = "Edited";
        }
        let full: String = format!("{}  ·  {}", t, state);
        let tw: i64 = (self.textW(&full, 13.0)).floor() as i64;
        let mut tx: i64 = (w - tw) / 2;
        if tx < 150 {
            tx = 150;
        }
        self.text(dl, &t, tx, 14, 13.0, &self.theme.borrow().ink.clone(), true);
        let name_w: i64 = (self.textW(&t, 13.0)).floor() as i64;
        let mut _tmp_2707 = self.theme.borrow().dim.clone();
        self.text(dl,
            &format!("  ·  {}", state),
            tx + name_w,
            14,
            13.0,
            &_tmp_2707,
            false
        );
        // The search field, on the right: it searches the document.
        let sw: i64 = 260;
        let sx: i64 = (w - sw) - 150;
        if sx > (tx + tw) + 24 {
            if search_hot {
                self.round(dl, sx, 8, sw, 28, 8, &self.theme.borrow().hover.clone());
            } else {
                self.round(dl, sx, 8, sw, 28, 8, &self.theme.borrow().field.clone());
            }
            self.roundFrame(dl, sx, 8, sw, 28, 8, &self.theme.borrow().line.clone());
            self.icon(dl, ToolIcon::search(), sx + 16, 22, 15, true);
            let mut _tmp_2711 = self.theme.borrow().faint.clone();
            self.text(dl,
                "Search in document",
                sx + 30,
                15,
                12.5,
                &_tmp_2711,
                false
            );
        }
        // Save, as the one coloured button: the thing a person in a browser
        // needs to do before closing the tab.
        self.round(dl, (w - 120) - 12, 8, 120, 28, 8, &self.theme.borrow().accent.clone());
        let mut _tmp_2713 = self.theme.borrow().accentInk.clone();
        self.textMid(dl,
            "Download",
            (w - 60) - 12,
            22,
            13.0,
            &_tmp_2713,
            true
        );
        self.rect(dl, 0, h - 1, w, 1, &self.theme.borrow().line.clone());
    }
    /// --- the ribbon ---------------------------------------------------------------
    fn paintRibbon(&self, dl: &Rc<RefCell<EVGDisplayList>>, rb: &Rc<RefCell<DocxRibbon>>, editing: bool) {
        let x0: i64 = rb.borrow().x0;
        let y0: i64 = rb.borrow().y0;
        let w: i64 = rb.borrow().width;
        let mut _tmp_2715 = rb.borrow().tabRowH.clone();
        self.rect(dl, x0, y0, w, _tmp_2715, &self.theme.borrow().titleBar.clone());
        let mut ti: i64 = 0;
        while ti < (rb.borrow().tabs.len() as i64) {
            let tx: i64 = rb.borrow().tabX[ti as usize];
            let tw: i64 = rb.borrow().tabW[ti as usize];
            let nm: String = rb.borrow().tabs[ti as usize].clone();
            let on: bool = ti == rb.borrow().active;
            if rb.borrow().hotTab == ti && !on {
                self.round(dl, tx + 2, y0 + 5, tw - 4, 26, 6, &self.theme.borrow().hover.clone());
            }
            let mut col: Rc<RefCell<EVGColor>> = self.theme.borrow().ink.clone().clone();
            if on {
                col = self.theme.borrow().accent.clone();
            }
            self.textMid(dl, &nm, tx + (tw / 2), y0 + 18, 13.0, &col, on);
            if on {
                let mut _tmp_2719 = self.theme.borrow().accent.clone();
                self.round(dl, tx + 8, (y0 + rb.borrow().tabRowH) - 4, tw - 16, 3, 1, &_tmp_2719);
            }
            ti += 1;
        }
        // The editing switch at the right end of the tab row, as Word puts it.
        let pill_w: i64 = 104;
        let px: i64 = (x0 + w) - (pill_w + 14);
        self.roundFrame(dl, px, y0 + 5, pill_w, 26, 6, &self.theme.borrow().line.clone());
        let mut mode: String = "Viewing";
        if editing {
            mode = "Editing";
        }
        self.icon(dl, ToolIcon::editMode(), px + 16, y0 + 18, 14, true);
        self.text(dl, mode, px + 30, y0 + 10, 12.5, &self.theme.borrow().ink.clone(), false);
        self.chevron(dl, (px + pill_w) - 14, y0 + 18, &self.theme.borrow().dim.clone());
        // The page of commands, on a white card with a hairline round it.
        let by: i64 = y0 + rb.borrow().tabRowH;
        let mut _tmp_2723 = rb.borrow().bodyH.clone();
        self.rect(dl, x0, by, w, _tmp_2723, &self.theme.borrow().window.clone());
        let mut _tmp_2725 = self.theme.borrow().ribbon.clone();
        self.round(dl, x0 + 6, by + 2, w - 12, rb.borrow().bodyH - 8, 10, &_tmp_2725);
        let mut _tmp_2726 = self.theme.borrow().line.clone();
        self.roundFrame(dl, x0 + 6, by + 2, w - 12, rb.borrow().bodyH - 8, 10, &_tmp_2726);
        let mut gi: i64 = 0;
        while gi < (rb.borrow().groups.len() as i64) {
            let mut g: Rc<RefCell<DocxRibbonGroup>> = rb.borrow().groups[gi as usize].clone();
            if g.borrow().x + g.borrow().w <= (x0 + w) - 8 {
                let mut _tmp_2727 = g.borrow().name.clone();
                let mut _tmp_2728 = self.theme.borrow().faint.clone();
                let __arg_0 = g.borrow().x + (g.borrow().w / 2);
                let __arg_1 = (by + rb.borrow().bodyH) - 15;
                self.textMid(dl,
                    &_tmp_2727,
                    __arg_0,
                    __arg_1,
                    11.0,
                    &_tmp_2728,
                    false
                );
                if gi < (rb.borrow().groups.len() as i64) - 1 {
                    let mut _tmp_2729 = self.theme.borrow().line.clone();
                    let __arg_0 = (g.borrow().x + g.borrow().w) + 4;
                    self.rect(dl, __arg_0, by + 10, 1, rb.borrow().bodyH - 24, &_tmp_2729);
                }
            }
            gi += 1;
        }
        let mut i: i64 = 0;
        while i < (rb.borrow().items.len() as i64) {
            let mut it: Rc<RefCell<DocxRibbonItem>> = rb.borrow().items[i as usize].clone();
            if it.borrow().x + it.borrow().w <= (x0 + w) - 8 {
                self.paintItem(dl, &it, i == rb.borrow().hot);
            }
            i += 1;
        }
        let mut _tmp_2730 = self.theme.borrow().line.clone();
        self.rect(dl, x0, (by + rb.borrow().bodyH) - 1, w, 1, &_tmp_2730);
    }
    fn paintItem(&self, dl: &Rc<RefCell<EVGDisplayList>>, it: &Rc<RefCell<DocxRibbonItem>>, hot: bool) {
        let mut ink: Rc<RefCell<EVGColor>> = self.theme.borrow().ink.clone().clone();
        if !it.borrow().enabled {
            ink = self.theme.borrow().off.clone();
        }
        if it.borrow().kind == 3 {
            let mut _tmp_2732 = it.borrow().x.clone();
            let mut _tmp_2733 = it.borrow().y.clone();
            let mut _tmp_2734 = it.borrow().w.clone();
            let mut _tmp_2735 = it.borrow().h.clone();
            let mut _tmp_2736 = self.theme.borrow().field.clone();
            self.round(dl,
                _tmp_2732,
                _tmp_2733,
                _tmp_2734,
                _tmp_2735,
                5,
                &_tmp_2736
            );
            let mut edge: Rc<RefCell<EVGColor>> = self.theme.borrow().line.clone().clone();
            if hot && it.borrow().enabled {
                edge = self.theme.borrow().accent.clone();
            }
            let mut _tmp_2738 = it.borrow().x.clone();
            let mut _tmp_2739 = it.borrow().y.clone();
            let mut _tmp_2740 = it.borrow().w.clone();
            let mut _tmp_2741 = it.borrow().h.clone();
            self.roundFrame(dl,
                _tmp_2738,
                _tmp_2739,
                _tmp_2740,
                _tmp_2741,
                5,
                &edge
            );
            let mut _tmp_2742 = it.borrow().text.clone();
            let __arg_0 = it.borrow().x + 7;
            self.text(dl, &_tmp_2742, __arg_0, it.borrow().y + 5, 12.5, &ink, false);
            let mut _tmp_2743 = self.theme.borrow().dim.clone();
            let __arg_0 = (it.borrow().x + it.borrow().w) - 10;
            self.chevron(dl, __arg_0, it.borrow().y + (it.borrow().h / 2), &_tmp_2743);
            return;
        }
        if it.borrow().kind == 4 {
            if it.borrow().pressed {
                let mut _tmp_2744 = it.borrow().x.clone();
                let mut _tmp_2745 = it.borrow().y.clone();
                let mut _tmp_2746 = it.borrow().w.clone();
                let mut _tmp_2747 = it.borrow().h.clone();
                let mut _tmp_2748 = self.theme.borrow().accentSoft.clone();
                self.round(dl,
                    _tmp_2744,
                    _tmp_2745,
                    _tmp_2746,
                    _tmp_2747,
                    6,
                    &_tmp_2748
                );
                let mut _tmp_2749 = it.borrow().x.clone();
                let mut _tmp_2750 = it.borrow().y.clone();
                let mut _tmp_2751 = it.borrow().w.clone();
                let mut _tmp_2752 = it.borrow().h.clone();
                let mut _tmp_2753 = self.theme.borrow().accent.clone();
                self.roundFrame(dl,
                    _tmp_2749,
                    _tmp_2750,
                    _tmp_2751,
                    _tmp_2752,
                    6,
                    &_tmp_2753
                );
            } else {
                if hot {
                    let mut _tmp_2754 = it.borrow().x.clone();
                    let mut _tmp_2755 = it.borrow().y.clone();
                    let mut _tmp_2756 = it.borrow().w.clone();
                    let mut _tmp_2757 = it.borrow().h.clone();
                    let mut _tmp_2758 = self.theme.borrow().hover.clone();
                    self.round(dl,
                        _tmp_2754,
                        _tmp_2755,
                        _tmp_2756,
                        _tmp_2757,
                        6,
                        &_tmp_2758
                    );
                }
            }
            let mut col: Rc<RefCell<EVGColor>> = ink.clone();
            if !it.borrow().sampleColor.is_empty() {
                let _tmp_2760 = EVGColor::parseHex(&it.borrow().sampleColor.clone());
                col = _tmp_2760;
            }
            let sample: String = "AaBbCc";
            let mut sz: f64 = it.borrow().sampleSize;
            let mut sw: i64 = (self.textW(sample, sz)).floor() as i64;
            while sw > it.borrow().w - 10 && sz > 9.0 {
                sz -= 1.0;
                sw = (self.textW(sample, sz)).floor() as i64;
            }
            let mut _tmp_2762 = it.borrow().sampleBold.clone();
            let mut _tmp_2763 = it.borrow().sampleItalic.clone();
            let __arg_0 = ((it.borrow().x + (it.borrow().w / 2)) - (sw / 2)) as f64;
            let __arg_1 = ((it.borrow().y + 24) - ((sz * 0.7).floor() as i64)) as f64;
            let _tmp_2764 = self.view.clone();
            EVGToolbarView::pushStyledText(
                &_tmp_2764,
                dl,
                sample,
                __arg_0,
                __arg_1,
                sz,
                &col,
                "",
                _tmp_2762,
                _tmp_2763
            );
            let mut _tmp_2765 = it.borrow().label.clone();
            let mut _tmp_2766 = self.theme.borrow().dim.clone();
            let __arg_0 = it.borrow().x + (it.borrow().w / 2);
            let __arg_1 = (it.borrow().y + it.borrow().h) - 12;
            self.textMid(dl,
                &_tmp_2765,
                __arg_0,
                __arg_1,
                11.5,
                &_tmp_2766,
                false
            );
            return;
        }
        if it.borrow().pressed {
            let mut _tmp_2767 = it.borrow().x.clone();
            let mut _tmp_2768 = it.borrow().y.clone();
            let mut _tmp_2769 = it.borrow().w.clone();
            let mut _tmp_2770 = it.borrow().h.clone();
            let mut _tmp_2771 = self.theme.borrow().pressed.clone();
            self.round(dl,
                _tmp_2767,
                _tmp_2768,
                _tmp_2769,
                _tmp_2770,
                5,
                &_tmp_2771
            );
        } else {
            if hot && it.borrow().enabled {
                let mut _tmp_2772 = it.borrow().x.clone();
                let mut _tmp_2773 = it.borrow().y.clone();
                let mut _tmp_2774 = it.borrow().w.clone();
                let mut _tmp_2775 = it.borrow().h.clone();
                let mut _tmp_2776 = self.theme.borrow().hover.clone();
                self.round(dl,
                    _tmp_2772,
                    _tmp_2773,
                    _tmp_2774,
                    _tmp_2775,
                    5,
                    &_tmp_2776
                );
            }
        }
        if it.borrow().kind == 1 {
            let cx: i64 = it.borrow().x + (it.borrow().w / 2);
            let mut _tmp_2777 = it.borrow().icon.clone();
            let mut _tmp_2778 = it.borrow().enabled.clone();
            self.icon(dl, _tmp_2777, cx, it.borrow().y + 22, 30, _tmp_2778);
            let mut _tmp_2779 = it.borrow().label.clone();
            self.textMid(dl, &_tmp_2779, cx, it.borrow().y + 56, 11.5, &ink, false);
            if it.borrow().menu > 0 {
                let mut _tmp_2780 = self.theme.borrow().dim.clone();
                self.chevron(dl, cx, it.borrow().y + 66, &_tmp_2780);
            }
            return;
        }
        let cy: i64 = it.borrow().y + (it.borrow().h / 2);
        if it.borrow().menu > 0 && it.borrow().kind == 0 {
            if it.borrow().menu == 2 && hot {
                let mut _tmp_2781 = self.theme.borrow().line.clone();
                let __arg_0 = (it.borrow().x + it.borrow().w) - 13;
                let __arg_1 = it.borrow().y + 4;
                self.rect(dl, __arg_0, __arg_1, 1, it.borrow().h - 8, &_tmp_2781);
            }
            let mut _tmp_2782 = self.theme.borrow().dim.clone();
            self.chevron(dl, (it.borrow().x + it.borrow().w) - 7, cy, &_tmp_2782);
        }
        if !it.borrow().glyph.is_empty() {
            let mut _tmp_2783 = it.borrow().glyph.clone();
            self.textMid(dl, &_tmp_2783, it.borrow().x + 14, cy, 14.0, &ink, false);
        } else {
            let mut _tmp_2784 = it.borrow().icon.clone();
            let mut _tmp_2785 = it.borrow().enabled.clone();
            self.icon(dl, _tmp_2784, it.borrow().x + 14, cy, 18, _tmp_2785);
        }
        if it.borrow().kind == 2 {
            let mut _tmp_2786 = it.borrow().label.clone();
            self.text(dl, &_tmp_2786, it.borrow().x + 30, cy - 9, 12.5, &ink, false);
        }
    }
    /// The name of the hot button, under it.
    fn paintTip(&self, dl: &Rc<RefCell<EVGDisplayList>>, rb: &Rc<RefCell<DocxRibbon>>, W: i64) {
        if rb.borrow().hot < 0 || rb.borrow().menu.borrow().open {
            return;
        }
        let mut it: Rc<RefCell<DocxRibbonItem>> = rb.borrow().at(rb.borrow().hot.clone());
        if it.borrow().tip.is_empty() {
            return;
        }
        let mut tip: String = it.borrow().tip.clone();
        if !it.borrow().enabled {
            tip = format!("{} (not yet)", tip);
        }
        let tw: i64 = ((self.textW(&tip, 12.0)).floor() as i64) + 16;
        let mut tx: i64 = it.borrow().x;
        if tx + tw > W - 4 {
            tx = (W - 4) - tw;
        }
        let ty: i64 = (it.borrow().y + it.borrow().h) + 6;
        self.round(dl, tx, ty, tw, 24, 5, &EVGColor::rgb(38, 43, 54).clone());
        self.text(dl, &tip, tx + 8, ty + 4, 12.0, &EVGColor::rgb(246, 248, 251).clone(), false);
    }
    /// An open dropdown: a card with a soft shadow, a tick on the line that
    /// is in effect, and the sample a list line would put in front.
    fn paintMenu(&self, dl: &Rc<RefCell<EVGDisplayList>>, m: &Rc<RefCell<DocxRibbonMenu>>) {
        if !m.borrow().open {
            return;
        }
        let h: i64 = m.borrow().height();
        let mut _tmp_2788 = m.borrow().w.clone();
        let __arg_0 = m.borrow().x + 1;
        let __arg_1 = m.borrow().y + 3;
        self.round(dl, __arg_0, __arg_1, _tmp_2788, h, 8, &EVGColor::rgba(20, 30, 50, 0.1).clone());
        let mut _tmp_2789 = m.borrow().x.clone();
        let mut _tmp_2790 = m.borrow().y.clone();
        let mut _tmp_2791 = m.borrow().w.clone();
        self.round(dl, _tmp_2789, _tmp_2790, _tmp_2791, h, 8, &self.theme.borrow().ribbon.clone());
        let mut _tmp_2793 = m.borrow().x.clone();
        let mut _tmp_2794 = m.borrow().y.clone();
        let mut _tmp_2795 = m.borrow().w.clone();
        let mut _tmp_2796 = self.theme.borrow().line.clone();
        self.roundFrame(dl,
            _tmp_2793,
            _tmp_2794,
            _tmp_2795,
            h,
            8,
            &_tmp_2796
        );
        let rh: i64 = m.borrow().rowH();
        let mut i: i64 = 0;
        while i < (m.borrow().entries.len() as i64) {
            let mut e: Rc<RefCell<DocxMenuEntry>> = m.borrow().entries[i as usize].clone();
            let ry: i64 = (m.borrow().y + 5) + i * rh;
            if e.borrow().rule {
                let mut _tmp_2797 = self.theme.borrow().line.clone();
                let __arg_0 = m.borrow().x + 8;
                self.rect(dl, __arg_0, ry, m.borrow().w - 16, 1, &_tmp_2797);
            }
            if i == m.borrow().hot {
                let mut _tmp_2798 = self.theme.borrow().hover.clone();
                let __arg_0 = m.borrow().x + 4;
                self.round(dl, __arg_0, ry + 1, m.borrow().w - 8, rh - 2, 5, &_tmp_2798);
            }
            if e.borrow().checked {
                let mut _tmp_2799 = self.theme.borrow().accent.clone();
                self.round(dl, m.borrow().x + 4, ry + 1, 4, rh - 2, 2, &_tmp_2799);
                let mut _tmp_2800 = self.theme.borrow().accent.clone();
                let __arg_0 = m.borrow().x + 18;
                self.textMid(dl,
                    "✓",
                    __arg_0,
                    ry + (rh / 2),
                    13.0,
                    &_tmp_2800,
                    true
                );
            }
            let mut lx: i64 = m.borrow().x + 32;
            if !e.borrow().sample.is_empty() {
                let mut _tmp_2801 = e.borrow().sample.clone();
                let mut _tmp_2802 = self.theme.borrow().ink.clone();
                let __arg_0 = m.borrow().x + 44;
                self.textMid(dl,
                    &_tmp_2801,
                    __arg_0,
                    ry + (rh / 2),
                    14.0,
                    &_tmp_2802,
                    false
                );
                lx = m.borrow().x + 62;
            }
            let mut _tmp_2803 = e.borrow().label.clone();
            let mut _tmp_2804 = self.theme.borrow().ink.clone();
            self.text(dl,
                &_tmp_2803,
                lx,
                (ry + (rh / 2)) - 8,
                12.5,
                &_tmp_2804,
                false
            );
            i += 1;
        }
    }
    /// --- the navigation pane ------------------------------------------------------
    fn paintNav(&self, dl: &Rc<RefCell<EVGDisplayList>>, nav: &Rc<RefCell<DocxNavPane>>) {
        let x: i64 = nav.borrow().x;
        let y: i64 = nav.borrow().y;
        let w: i64 = nav.borrow().w;
        let mut _tmp_2805 = nav.borrow().h.clone();
        self.rect(dl, x, y, w, _tmp_2805, &self.theme.borrow().titleBar.clone());
        let mut _tmp_2807 = nav.borrow().h.clone();
        self.rect(dl, (x + w) - 1, y, 1, _tmp_2807, &self.theme.borrow().line.clone());
        self.text(dl, "Navigation", x + 16, y + 14, 15.0, &self.theme.borrow().ink.clone(), true);
        // Close.
        let cx: i64 = nav.borrow().closeX() + 14;
        self.textMid(dl, "×", cx, y + 24, 16.0, &self.theme.borrow().dim.clone(), false);
        let sy: i64 = nav.borrow().searchY();
        self.round(dl, x + 14, sy, w - 28, 30, 7, &self.theme.borrow().field.clone());
        let mut edge: Rc<RefCell<EVGColor>> = self.theme.borrow().line.clone().clone();
        if nav.borrow().searchFocused {
            edge = self.theme.borrow().accent.clone();
        }
        self.roundFrame(dl, x + 14, sy, w - 28, 30, 7, &edge);
        self.icon(dl, ToolIcon::search(), x + 30, sy + 15, 15, true);
        if nav.borrow().query.is_empty() {
            if !nav.borrow().searchFocused {
                let mut _tmp_2813 = self.theme.borrow().faint.clone();
                self.text(dl,
                    "Search document",
                    x + 44,
                    sy + 7,
                    12.5,
                    &_tmp_2813,
                    false
                );
            }
        } else {
            let mut _tmp_2814 = nav.borrow().query.clone();
            let mut _tmp_2815 = self.theme.borrow().ink.clone();
            self.text(dl,
                &_tmp_2814,
                x + 44,
                sy + 7,
                12.5,
                &_tmp_2815,
                false
            );
        }
        if nav.borrow().searchFocused {
            let qw: i64 = (self.textW(&nav.borrow().query.clone(), 12.5)).floor() as i64;
            self.rect(dl, (x + 45) + qw, sy + 7, 1, 16, &self.theme.borrow().ink.clone());
        }
        let ty: i64 = nav.borrow().tabsY();
        let mut k: i64 = 0;
        while k < 3 {
            let l: i64 = nav.borrow().tabLeft(k);
            let tw: i64 = nav.borrow().tabWidth(k);
            let mut nm: String = "Headings";
            if k == 1 {
                nm = "Pages";
            }
            if k == 2 {
                nm = "Results";
            }
            let on: bool = k == nav.borrow().mode;
            if on {
                self.round(dl, l, ty, tw, 28, 7, &self.theme.borrow().accentSoft.clone());
            }
            let mut col: Rc<RefCell<EVGColor>> = self.theme.borrow().ink.clone().clone();
            if on {
                col = self.theme.borrow().accent.clone();
            }
            self.textMid(dl, nm, l + (tw / 2), ty + 14, 13.0, &col, on);
            k += 1;
        }
        let ly: i64 = nav.borrow().listY();
        self.rect(dl, x + 14, ly - 8, w - 28, 1, &self.theme.borrow().line.clone());
        EVGDisplayList::addClip(
            &dl,
            x as f64,
            ly as f64,
            (w - 1) as f64,
            ((y + nav.borrow().h) - ly) as f64
        );
        let mut row_y: i64 = ly - nav.borrow().scroll;
        let mut i: i64 = 0;
        while i < (nav.borrow().entries.len() as i64) {
            let mut e: Rc<RefCell<NavEntry>> = nav.borrow().entries[i as usize].clone();
            let rh: i64 = 30;
            e.borrow_mut().y = row_y;
            e.borrow_mut().h = rh;
            if row_y + rh >= ly && row_y < y + nav.borrow().h {
                if i == nav.borrow().current {
                    self.round(dl, x + 10, row_y, w - 20, rh, 6, &self.theme.borrow().accentSoft.clone());
                    self.rect(dl, x + 10, row_y + 6, 3, rh - 12, &self.theme.borrow().accent.clone());
                } else {
                    if i == nav.borrow().hot {
                        self.round(dl, x + 10, row_y, w - 20, rh, 6, &self.theme.borrow().hover.clone());
                    }
                }
                let indent: i64 = (e.borrow().level - 1) * 16;
                let mut bold: bool = e.borrow().level == 1;
                if nav.borrow().mode != 0 {
                    bold = false;
                }
                let mut _tmp_2824 = e.borrow().text.clone();
                let label: String = DocxChromePainter::fit(
                    self,
                    &_tmp_2824,
                    13.0,
                    (w - 44) - indent
                );
                let mut _tmp_2825 = self.theme.borrow().ink.clone();
                self.text(dl,
                    &label,
                    (x + 22) + indent,
                    row_y + 6,
                    13.0,
                    &_tmp_2825,
                    bold
                );
                if nav.borrow().mode == 2 {
                    let pg: String = format!("p. {}", e.borrow().page + 1);
                    let pw: i64 = (self.textW(&pg, 11.0)).floor() as i64;
                    let mut _tmp_2826 = self.theme.borrow().faint.clone();
                    self.text(dl,
                        &pg,
                        ((x + w) - 20) - pw,
                        row_y + 8,
                        11.0,
                        &_tmp_2826,
                        false
                    );
                }
            }
            row_y += rh;
            i += 1;
        }
        if nav.borrow().entries.is_empty() {
            let mut empty: String = "No headings in this document";
            if nav.borrow().mode == 2 {
                empty = "Type above to search the text";
                if !nav.borrow().query.is_empty() {
                    empty = "No results";
                }
            }
            self.text(dl, empty, x + 22, ly + 4, 12.5, &self.theme.borrow().faint.clone(), false);
        }
        EVGDisplayList::addClipEnd(&dl);
    }
    /// --- the vertical ruler -------------------------------------------------------
    /// The page's height in inches down the left of the paper, with the top and
    /// bottom margins shaded the way the horizontal one shades left and right.
    fn paintVRuler(&self, dl: &Rc<RefCell<EVGDisplayList>>, x: i64, y: i64, h: i64, page_top: i64, page_h: i64, margin_t: i64, margin_b: i64, zoom: f64, clip_top: i64) {
        let w: i64 = 18;
        self.rect(dl, x, y, w, h, &self.theme.borrow().window.clone());
        EVGDisplayList::addClip(&dl, x as f64, clip_top as f64, w as f64, ((y + h) - clip_top) as f64);
        self.rect(dl, x + 3, page_top, w - 6, page_h, &self.theme.borrow().rulerPage.clone());
        let mt: i64 = ((margin_t as f64) * zoom).floor() as i64;
        let mb: i64 = ((margin_b as f64) * zoom).floor() as i64;
        let mut _tmp_2830 = self.theme.borrow().field.clone();
        self.rect(dl,
            x + 3,
            page_top + mt,
            w - 6,
            (page_h - mt) - mb,
            &_tmp_2830
        );
        let unit: f64 = 96.0 * zoom;
        let steps: i64 = (((page_h as f64) / unit) * 8.0).floor() as i64;
        let mut s: i64 = 1;
        while s < steps {
            let ty: i64 = (page_top + mt) + ((((s as f64) * unit) / 8.0).floor() as i64);
            if ty < (page_top + page_h) - 2 {
                if s % 8 == 0 {
                    let mut _tmp_2831 = self.theme.borrow().dim.clone();
                    self.textMid(dl,
                        &(s / 8).to_string(),
                        x + 9,
                        ty,
                        9.0,
                        &_tmp_2831,
                        false
                    );
                } else {
                    let mut __len: i64 = 3;
                    if s % 4 == 0 {
                        __len = 6;
                    }
                    self.rect(dl, (x + 9) - (__len / 2), ty, __len, 1, &self.theme.borrow().faint.clone());
                }
            }
            s += 1;
        }
        EVGDisplayList::addClipEnd(&dl);
    }
    /// --- the status bar -----------------------------------------------------------
    fn paintStatus(&self, dl: &Rc<RefCell<EVGDisplayList>>, y: i64, w: i64, h: i64, left: &str, zoom: f64) {
        self.rect(dl, 0, y, w, h, &self.theme.borrow().titleBar.clone());
        self.rect(dl, 0, y, w, 1, &self.theme.borrow().line.clone());
        self.text(dl, left, 14, y + 6, 12.0, &self.theme.borrow().dim.clone(), false);
        // Zoom: minus, a slider, plus, and the percentage.
        let pct: String = format!("{}%", ((zoom * 100.0 + 0.5).floor() as i64).to_string());
        let sx: i64 = self.sliderX(w);
        let mut _tmp_2836 = self.theme.borrow().dim.clone();
        self.textMid(dl,
            "−",
            sx - 14,
            y + (h / 2),
            14.0,
            &_tmp_2836,
            false
        );
        self.rect(dl, sx, y + (h / 2), 120, 2, &self.theme.borrow().faint.clone());
        let k: i64 = self.sliderKnob(w, zoom);
        self.rect(dl, sx + 59, (y + (h / 2)) - 4, 1, 10, &self.theme.borrow().faint.clone());
        self.round(dl, k - 5, (y + (h / 2)) - 6, 10, 14, 3, &self.theme.borrow().field.clone());
        self.roundFrame(dl, k - 5, (y + (h / 2)) - 6, 10, 14, 3, &self.theme.borrow().dim.clone());
        let mut _tmp_2841 = self.theme.borrow().dim.clone();
        self.textMid(dl,
            "+",
            sx + 134,
            y + (h / 2),
            14.0,
            &_tmp_2841,
            false
        );
        self.text(dl, &pct, sx + 150, y + 6, 12.0, &self.theme.borrow().dim.clone(), false);
    }
    pub fn sliderX(&self, w: i64) -> i64 {
        w - 220
    }
    /// The slider is centred on 100%: the left half runs down to a quarter, the
    /// right half up to four times, each half linear.
    pub fn sliderKnob(&self, w: i64, zoom: f64) -> i64 {
        let mut t: f64 = 0.0;
        if zoom < 1.0 {
            t = (zoom - 1.0) / 0.75;
        } else {
            t = (zoom - 1.0) / 3.0;
        }
        if t < -1.0 {
            t = -1.0;
        }
        if t > 1.0 {
            t = 1.0;
        }
        (self.sliderX(w) + 60) + ((t * 60.0).floor() as i64)
    }
    pub fn zoomAtSlider(&self, w: i64, px: i64) -> f64 {
        let mut t: f64 = (((px - self.sliderX(w)) - 60) as f64) / 60.0;
        if t < -1.0 {
            t = -1.0;
        }
        if t > 1.0 {
            t = 1.0;
        }
        if t < 0.0 {
            return 1.0 + t * 0.75;
        }
        1.0 + t * 3.0
    }
}
