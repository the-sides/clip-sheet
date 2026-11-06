//! Native AppKit action smoke test. Uses a private pasteboard, never the user's.
use super::*;

fn find_button(view: &NSView, title: &str) -> Option<Retained<NSButton>> {
    for child in view.subviews().iter() {
        if let Some(b) = child.downcast_ref::<NSButton>()
            && b.title().to_string() == title
        {
            return Some(b.retain());
        }
        if let Some(b) = find_button(&child, title) {
            return Some(b);
        }
    }
    None
}
fn press(d: &Delegate, title: &str) {
    let b = find_button(d.ivars().content.get().unwrap(), title).expect(title);
    unsafe {
        b.performClick(None);
    }
}

pub(super) fn run(d: &Delegate) {
    let pb = NSPasteboard::pasteboardWithUniqueName();
    let _ = d.ivars().clipboard.set(pb.clone());
    d.build_ui();
    let root = d.ivars().content.get().unwrap();
    assert!(find_button(root, "Paste").is_some());
    pb.setString_forType(&ns("Role,Email,Password\nAdmin,admin@example.test, secret \nReader,reader@example.test,read-only"), unsafe { NSPasteboardTypeString });
    press(d, "Paste");
    assert_eq!(d.ivars().sheet.borrow().as_ref().unwrap().rows(), 3);
    assert_eq!(d.ivars().sheet.borrow().as_ref().unwrap().cols(), 3);
    press(d, " secret ");
    assert_eq!(
        pb.stringForType(unsafe { NSPasteboardTypeString })
            .unwrap()
            .to_string(),
        " secret "
    );
    let cell = find_button(root, "admin@example.test").unwrap();
    let event=NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::RightMouseDown, NSPoint::new(0.0,0.0), NSEventModifierFlags::empty(), 0.0,0,None,0,1,1.0
    ).unwrap();
    cell.rightMouseDown(&event);
    let field = d.ivars().editor.borrow().as_ref().unwrap().3.clone();
    assert!(cell.isHidden());
    field.setStringValue(&ns("edited@example.test"));
    d.save_edit();
    press(d, "+ Row"); // also commits and tears down the live editor
    assert!(d.ivars().editor.borrow().is_none());
    assert_eq!(
        d.ivars().sheet.borrow().as_ref().unwrap().get(1, 1),
        "edited@example.test"
    );
    assert_eq!(d.ivars().sheet.borrow().as_ref().unwrap().rows(), 4);
    press(d, "+ Column");
    assert_eq!(d.ivars().sheet.borrow().as_ref().unwrap().cols(), 4);
    press(d, "edited@example.test");
    assert_eq!(
        pb.stringForType(unsafe { NSPasteboardTypeString })
            .unwrap()
            .to_string(),
        "edited@example.test"
    );
    d.render();
    assert!(find_button(root, "edited@example.test").is_some());
    press(d, "Clear");
    assert!(d.ivars().sheet.borrow().is_none());
    assert!(find_button(root, "Paste").is_some());
    pb.clearContents();
    pb.setString_forType(&ns("| A | B |\n|---|---|\n| one | two |"), unsafe {
        NSPasteboardTypeString
    });
    press(d, "Paste");
    assert_eq!(d.ivars().sheet.borrow().as_ref().unwrap().get(1, 1), "two");
    press(d, "Clear");
    println!(
        "AppKit smoke passed: CSV/Markdown import, exact copy, right-click editing, edit commit, row/column growth, redraw, clear/reimport. (Visual popover/keyboard checks require an unlocked desktop.)"
    );
}
