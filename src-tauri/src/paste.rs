use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting, NSWorkspace};
use objc2_foundation::{NSArray, NSData, NSString};
use std::time::Duration;

type CFTypeRef = *const std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFTypeRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementIsAttributeSettable(element: CFTypeRef, attribute: CFTypeRef, settable: *mut u8) -> i32;
    fn AXUIElementCopyAttributeNames(element: CFTypeRef, names: *mut CFTypeRef) -> i32;
}

/// Accessibility permission: needed to post the synthetic Cmd+V.
pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub struct FrontApp {
    pub bundle_id: Option<String>,
    pub name: Option<String>,
    pub pid: Option<i32>,
}

pub fn frontmost_app() -> FrontApp {
    let ws = NSWorkspace::sharedWorkspace();
    match ws.frontmostApplication() {
        Some(app) => FrontApp {
            bundle_id: app.bundleIdentifier().map(|s| s.to_string()),
            name: app.localizedName().map(|s| s.to_string()),
            pid: Some(app.processIdentifier()),
        },
        None => FrontApp { bundle_id: None, name: None, pid: None },
    }
}

/// Hands keyboard focus back to `pid`. A click on the bar panel does not activate Utter, but macOS still routes
/// keys to Utter's process afterwards, so the paste would go nowhere.
pub fn give_focus(pid: Option<i32>) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    let Some(app) = pid.and_then(|p| NSRunningApplication::runningApplicationWithProcessIdentifier(p)) else { return };
    #[allow(deprecated)] // the replacement (activate(from:)) needs macOS 14; this app supports 12
    app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    std::thread::sleep(Duration::from_millis(150));
}

/// Does the app's focused element take text? Some(false) only when macOS clearly reports a non-text element
/// (Finder's file list, a plain window). None when it cannot tell: Chrome and some Electron apps hide their
/// accessibility tree until an assistive app asks, so unknown must never block a paste.
pub fn focused_accepts_text(pid: Option<i32>) -> Option<bool> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::string::CFString;
    let pid = pid?;
    let attr = |name: &'static str| CFString::from_static_string(name);
    unsafe {
        let app = AXUIElementCreateApplication(pid);
        if app.is_null() {
            return None;
        }
        let app = CFType::wrap_under_create_rule(app);
        let mut focused: CFTypeRef = std::ptr::null();
        let name = attr("AXFocusedUIElement");
        if AXUIElementCopyAttributeValue(app.as_CFTypeRef(), name.as_CFTypeRef(), &mut focused) != 0 || focused.is_null() {
            return None;
        }
        let focused = CFType::wrap_under_create_rule(focused);
        let el = focused.as_CFTypeRef();

        let mut role: CFTypeRef = std::ptr::null();
        let role_attr = attr("AXRole");
        if AXUIElementCopyAttributeValue(el, role_attr.as_CFTypeRef(), &mut role) == 0 && !role.is_null() {
            let role = CFType::wrap_under_create_rule(role);
            if let Some(r) = role.downcast::<CFString>() {
                if matches!(r.to_string().as_str(), "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField" | "AXWebArea") {
                    return Some(true);
                }
            }
        }
        // Anything that edits text lists a selection range. Only trust the attribute list: Finder answers a
        // direct AXSelectedTextRange query with an empty range although its file list takes no text.
        let mut names: CFTypeRef = std::ptr::null();
        if AXUIElementCopyAttributeNames(el, &mut names) == 0 && !names.is_null() {
            let names = core_foundation::array::CFArray::<CFString>::wrap_under_create_rule(names as _);
            if names.iter().any(|n| n.to_string() == "AXSelectedTextRange") {
                return Some(true);
            }
        }
        let mut settable: u8 = 0;
        let value_attr = attr("AXValue");
        if AXUIElementIsAttributeSettable(el, value_attr.as_CFTypeRef(), &mut settable) == 0 && settable != 0 {
            return Some(true);
        }
        Some(false)
    }
}

/// Every item and every type on the pasteboard, so images and rich text survive, not just plain text.
struct Saved(Vec<Vec<(Retained<NSString>, Retained<NSData>)>>);

fn save(pb: &NSPasteboard) -> Saved {
    let mut items = Vec::new();
    if let Some(list) = pb.pasteboardItems() {
        for item in list.iter() {
            let mut entries = Vec::new();
            for ty in item.types().iter() {
                if let Some(data) = item.dataForType(&ty) {
                    entries.push((ty.clone(), data));
                }
            }
            items.push(entries);
        }
    }
    Saved(items)
}

fn restore(pb: &NSPasteboard, saved: Saved) {
    pb.clearContents();
    if saved.0.is_empty() {
        return;
    }
    let objs: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = saved
        .0
        .into_iter()
        .map(|entries| {
            let item = NSPasteboardItem::new();
            for (ty, data) in entries {
                item.setData_forType(&data, &ty);
            }
            ProtocolObject::from_retained(item)
        })
        .collect();
    pb.writeObjects(&NSArray::from_retained_slice(&objs));
}

pub fn set_clipboard_text(text: &str) {
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    pb.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
}

// TODO: kVK_ANSI_V (9) is the V key position on QWERTY only; on Dvorak and similar layouts this sends the
// wrong key. Upgrade by looking up the keycode for "v" with TISCopyCurrentKeyboardLayoutInputSource.
const KEY_V: u16 = 9;

fn post_cmd_v() -> Result<(), &'static str> {
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| "event source")?;
    for down in [true, false] {
        let ev = CGEvent::new_keyboard_event(src.clone(), KEY_V, down).map_err(|_| "key event")?;
        ev.set_flags(CGEventFlags::CGEventFlagCommand);
        // Tag our own keystroke so the fn listener does not mistake it for the user typing fn+V.
        ev.set_integer_value_field(core_graphics::event::EventField::EVENT_SOURCE_USER_DATA, crate::hotkey::SYNTHETIC_TAG);
        ev.post(CGEventTapLocation::HID);
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum PasteResult {
    Pasted,
    NoPermission,
    Failed(&'static str),
}

/// Puts `text` on the pasteboard, presses Cmd+V, then restores what the user had copied.
/// The restore is skipped if anything else wrote to the pasteboard in the meantime (the user copied something).
pub fn paste_text(text: &str) -> PasteResult {
    if !accessibility_trusted() {
        // Leave the text on the clipboard so the user can still paste it by hand.
        set_clipboard_text(text);
        return PasteResult::NoPermission;
    }
    let pb = NSPasteboard::generalPasteboard();
    let saved = save(&pb);
    set_clipboard_text(text);
    let ours = pb.changeCount();
    if let Err(e) = post_cmd_v() {
        return PasteResult::Failed(e);
    }
    // Slow targets (Chrome, Electron) read the pasteboard late; restoring too early pastes the old content.
    // TODO: fixed 500 ms wait, upgrade to per-app timing if pastes go missing in specific apps.
    std::thread::sleep(Duration::from_millis(500));
    let now = pb.changeCount();
    #[cfg(debug_assertions)]
    eprintln!("[paste] changeCount ours={ours} now={now} saved_items={}", saved.0.len());
    if now == ours {
        restore(&pb, saved);
    }
    PasteResult::Pasted
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real clipboard. Run: cargo test --lib clipboard_restore -- --ignored --nocapture
    #[test]
    #[ignore]
    fn clipboard_restore_roundtrip() {
        set_clipboard_text("ORIGINAL");
        let pb = NSPasteboard::generalPasteboard();
        let saved = save(&pb);
        println!("saved items: {}, types in first: {}", saved.0.len(), saved.0.first().map(|i| i.len()).unwrap_or(0));
        set_clipboard_text("TEMP");
        restore(&pb, saved);
        let now = pb.stringForType(unsafe { NSPasteboardTypeString }).map(|s| s.to_string());
        assert_eq!(now.as_deref(), Some("ORIGINAL"));
    }
}
