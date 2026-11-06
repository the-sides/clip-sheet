//! An ephemeral clipboard sheet in a native macOS menu bar popover.
mod sheet;
mod smoke;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::*;
use objc2_foundation::{
    NSNotification, NSObjectProtocol, NSPoint, NSRect, NSRectEdge, NSSize, NSString,
};
use sheet::Sheet;
use std::cell::{OnceCell, RefCell};

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}
fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

// A real button preserves keyboard/accessibility behavior, with a separate
// secondary-click action that never copies the cell first.
define_class!(
    #[unsafe(super(NSButton))]
    #[thread_kind = MainThreadOnly]
    #[name = "ClipSheetCell"]
    struct CellButton;
    impl CellButton {
        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, _event: &NSEvent) {
            if let Some(target) = self.target() {
                unsafe { let _: () = msg_send![&target, editCell: self]; }
            }
        }
    }
);

type Editor = (usize, usize, Retained<NSButton>, Retained<NSTextField>);
#[derive(Default)]
struct Ivars {
    item: OnceCell<Retained<NSStatusItem>>,
    popover: OnceCell<Retained<NSPopover>>,
    content: OnceCell<Retained<NSView>>,
    status: OnceCell<Retained<NSTextField>>,
    sheet: RefCell<Option<Sheet>>,
    clipboard: OnceCell<Retained<NSPasteboard>>,
    editor: RefCell<Option<Editor>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ClipSheetDelegate"]
    #[ivars = Ivars]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish(&self, _: &NSNotification) {
            if std::env::args().any(|a| a == "--smoke-test") {
                smoke::run(self);
                NSApplication::sharedApplication(self.mtm()).terminate(None);
            } else { self.build_ui(); }
        }
    }
    unsafe impl NSControlTextEditingDelegate for Delegate {}
    unsafe impl NSTextFieldDelegate for Delegate {}
    impl Delegate {
        #[unsafe(method(toggle:))]
        fn toggle(&self, _: Option<&AnyObject>) {
            let p = self.ivars().popover.get().unwrap();
            if p.isShown() { self.finish_edit(); p.close(); return; }
            let button = self.ivars().item.get().unwrap().button(self.mtm()).unwrap();
            NSApplication::sharedApplication(self.mtm()).activate();
            p.showRelativeToRect_ofView_preferredEdge(button.bounds(), &button, NSRectEdge::MinY);
        }
        #[unsafe(method(importSheet:))]
        fn import_sheet(&self, _: Option<&AnyObject>) {
            let pb = self.clipboard();
            let text = pb.stringForType(unsafe { NSPasteboardTypeString }).map(|s| s.to_string()).unwrap_or_default();
            match Sheet::parse(&text) {
                Ok(sheet) => {
                    self.finish_edit();
                    *self.ivars().sheet.borrow_mut() = Some(sheet);
                    self.render();
                    self.status("Click a cell to copy · Right-click to edit");
                }
                Err(e) => self.status(&e),
            }
        }
        #[unsafe(method(clearSheet:))]
        fn clear_sheet(&self, _: Option<&AnyObject>) {
            self.finish_edit();
            *self.ivars().sheet.borrow_mut() = None;
            self.render();
            self.status("Paste a CSV, Markdown table, or spreadsheet cells");
        }
        #[unsafe(method(addRow:))]
        fn add_row(&self, _: Option<&AnyObject>) {
            self.finish_edit();
            if let Some(s) = self.ivars().sheet.borrow_mut().as_mut() { s.add_row(); }
            self.render();
        }
        #[unsafe(method(addColumn:))]
        fn add_column(&self, _: Option<&AnyObject>) {
            self.finish_edit();
            if let Some(s) = self.ivars().sheet.borrow_mut().as_mut() { s.add_col(); }
            self.render();
        }
        #[unsafe(method(copyCell:))]
        fn copy_cell(&self, sender: &NSButton) {
            self.finish_edit();
            if let Some(s) = self.ivars().sheet.borrow().as_ref() {
                let index = sender.tag() as usize;
                let pb = self.clipboard();
                pb.clearContents();
                let ok = pb.setString_forType(&ns(s.get(index / s.cols(), index % s.cols())), unsafe { NSPasteboardTypeString });
                self.status(if ok { "✓ Copied to clipboard" } else { "Couldn't write to the clipboard" });
            }
        }
        #[unsafe(method(editCell:))]
        fn edit_cell(&self, sender: &NSButton) {
            self.finish_edit();
            let (r,c,value) = {
                let sheet = self.ivars().sheet.borrow();
                let Some(s) = sheet.as_ref() else { return };
                let i = sender.tag() as usize;
                (i / s.cols(), i % s.cols(), s.get(i / s.cols(), i % s.cols()).to_string())
            };
            let field = NSTextField::initWithFrame(NSTextField::alloc(self.mtm()), sender.frame());
            field.setStringValue(&ns(&value));
            field.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(12.0,0.0)));
            unsafe { field.setDelegate(Some(ProtocolObject::from_ref(self))); }
            if let Some(parent) = unsafe { sender.superview() } { parent.addSubview(&field); }
            sender.setHidden(true);
            *self.ivars().editor.borrow_mut() = Some((r,c,sender.retain(),field.clone()));
            if let Some(window) = field.window() { window.makeFirstResponder(Some(&field)); }
            unsafe { field.selectText(None); }
            self.status("Editing · Return or click away to finish");
        }
        #[unsafe(method(controlTextDidChange:))]
        fn text_changed(&self, _: &NSNotification) { self.save_edit(); }
        #[unsafe(method(controlTextDidEndEditing:))]
        fn text_ended(&self, _: &NSNotification) { self.finish_edit(); }
        #[unsafe(method(quit:))]
        fn quit(&self, _: Option<&AnyObject>) { NSApplication::sharedApplication(self.mtm()).terminate(None); }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        unsafe { msg_send![super(this), init] }
    }
    fn clipboard(&self) -> &NSPasteboard {
        self.ivars()
            .clipboard
            .get_or_init(NSPasteboard::generalPasteboard)
    }
    fn status(&self, text: &str) {
        self.ivars().status.get().unwrap().setStringValue(&ns(text));
    }
    fn save_edit(&self) {
        if let Some((r, c, button, field)) = self.ivars().editor.borrow().as_ref() {
            let value = field.stringValue();
            if let Some(s) = self.ivars().sheet.borrow_mut().as_mut() {
                s.set(*r, *c, value.to_string());
            }
            button.setTitle(&value);
            button.setToolTip(Some(&value));
            button.setAccessibilityLabel(Some(&ns(&format!(
                "Row {}, column {}: {}",
                r + 1,
                c + 1,
                value
            ))));
        }
    }
    fn finish_edit(&self) {
        self.save_edit();
        let editor = self.ivars().editor.borrow_mut().take();
        if let Some((_, _, button, field)) = editor {
            // Take the editor before resigning first responder: AppKit sends
            // the end-edit notification synchronously.
            if let Some(w) = field.window() {
                w.makeFirstResponder(None);
            }
            field.removeFromSuperview();
            button.setHidden(false);
        }
    }
    fn button(&self, title: &str, action: objc2::runtime::Sel, frame: NSRect, parent: &NSView) {
        let b = unsafe {
            NSButton::buttonWithTitle_target_action(
                &ns(title),
                Some(self.as_ref()),
                Some(action),
                self.mtm(),
            )
        };
        b.setFrame(frame);
        parent.addSubview(&b);
    }
    fn render(&self) {
        let root = self.ivars().content.get().unwrap();
        for child in root.subviews().iter() {
            child.removeFromSuperview();
        }
        let sheet = self.ivars().sheet.borrow();
        let Some(s) = sheet.as_ref() else {
            self.button(
                "Paste",
                sel!(importSheet:),
                rect(250.0, 190.0, 140.0, 36.0),
                root,
            );
            return;
        };
        self.button("+ Row", sel!(addRow:), rect(12.0, 370.0, 80.0, 28.0), root);
        self.button(
            "+ Column",
            sel!(addColumn:),
            rect(96.0, 370.0, 100.0, 28.0),
            root,
        );
        self.button(
            "Clear",
            sel!(clearSheet:),
            rect(548.0, 370.0, 80.0, 28.0),
            root,
        );
        let scroll = NSScrollView::initWithFrame(
            NSScrollView::alloc(self.mtm()),
            rect(12.0, 12.0, 616.0, 348.0),
        );
        scroll.setHasVerticalScroller(true);
        scroll.setHasHorizontalScroller(true);
        scroll.setAutohidesScrollers(true);
        let widths: Vec<f64> = (0..s.cols())
            .map(|c| {
                let n = (0..s.rows())
                    .map(|r| s.get(r, c).chars().count())
                    .max()
                    .unwrap_or(0);
                (n as f64 * 7.4 + 24.0).clamp(140.0, 300.0)
            })
            .collect();
        let height = (s.rows() as f64 * 32.0).max(348.0);
        let width = widths.iter().sum::<f64>().max(616.0);
        let doc = NSView::initWithFrame(NSView::alloc(self.mtm()), rect(0.0, 0.0, width, height));
        for r in 0..s.rows() {
            let mut x = 0.0;
            for (c, w) in widths.iter().enumerate() {
                let b: Retained<CellButton> = unsafe {
                    msg_send![CellButton::alloc(self.mtm()), initWithFrame: rect(x,height-(r+1) as f64*32.0,*w-2.0,30.0)]
                };
                b.setTitle(&ns(s.get(r, c)));
                b.setTag((r * s.cols() + c) as isize);
                b.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(12.0, 0.0)));
                b.setAlignment(NSTextAlignment::Left);
                b.setToolTip(Some(&ns(s.get(r, c))));
                b.setAccessibilityLabel(Some(&ns(&format!(
                    "Row {}, column {}: {}",
                    r + 1,
                    c + 1,
                    s.get(r, c)
                ))));
                unsafe {
                    b.setTarget(Some(self.as_ref()));
                    b.setAction(Some(sel!(copyCell:)));
                }
                doc.addSubview(&b);
                x += w;
            }
        }
        scroll.setDocumentView(Some(&doc));
        root.addSubview(&scroll);
        doc.scrollPoint(NSPoint::new(0.0, height - 348.0));
    }
    fn build_ui(&self) {
        let mtm = self.mtm();
        NSApplication::sharedApplication(mtm).setMainMenu(Some(&edit_menu(mtm)));
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(b) = item.button(mtm) {
            if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &ns("tablecells"),
                Some(&ns("Clip Sheet")),
            ) {
                image.setTemplate(true);
                b.setImage(Some(&image));
            } else {
                b.setTitle(&ns("▦"));
            }
            unsafe {
                b.setTarget(Some(self.as_ref()));
                b.setAction(Some(sel!(toggle:)));
            }
        }
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 640.0, 460.0));
        let title = NSTextField::labelWithString(&ns("Clip Sheet"), mtm);
        title.setFont(Some(&NSFont::boldSystemFontOfSize(14.0)));
        title.setFrame(rect(16.0, 430.0, 200.0, 20.0));
        view.addSubview(&title);
        self.button("Quit", sel!(quit:), rect(570.0, 426.0, 56.0, 28.0), &view);
        let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 24.0, 640.0, 402.0));
        view.addSubview(&content);
        let status = NSTextField::labelWithString(
            &ns("Paste a CSV, Markdown table, or spreadsheet cells"),
            mtm,
        );
        status.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        status.setTextColor(Some(&NSColor::secondaryLabelColor()));
        status.setFrame(rect(16.0, 4.0, 610.0, 18.0));
        view.addSubview(&status);
        let vc = NSViewController::new(mtm);
        vc.setView(&view);
        let popover = NSPopover::new(mtm);
        popover.setContentViewController(Some(&vc));
        popover.setContentSize(NSSize::new(640.0, 460.0));
        popover.setBehavior(NSPopoverBehavior::Transient);
        popover.setAnimates(false);
        let _ = self.ivars().item.set(item);
        let _ = self.ivars().popover.set(popover);
        let _ = self.ivars().content.set(content);
        let _ = self.ivars().status.set(status);
        self.render();
    }
}
fn edit_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let main = NSMenu::new(mtm);
    let edit = NSMenu::new(mtm);
    for (title, action, key) in [
        ("Cut", sel!(cut:), "x"),
        ("Copy", sel!(copy:), "c"),
        ("Paste", sel!(paste:), "v"),
        ("Select All", sel!(selectAll:), "a"),
        ("Undo", sel!(undo:), "z"),
    ] {
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &ns(title),
                Some(action),
                &ns(key),
            )
        };
        edit.addItem(&item);
    }
    let holder = NSMenuItem::new(mtm);
    holder.setSubmenu(Some(&edit));
    main.addItem(&holder);
    main
}
fn main() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let delegate = Delegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
