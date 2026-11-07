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

/// Names of all running apps.
pub fn running_app_names() -> Vec<String> {
    NSWorkspace::sharedWorkspace().runningApplications().iter().filter_map(|a| a.localizedName().map(|n| n.to_string())).collect()
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

/// kVK_ANSI_V: where V sits on QWERTY. Also the answer for layouts with no Latin "v" (Arabic, Russian), where
/// macOS matches ⌘ shortcuts by QWERTY position.
const KEY_V_QWERTY: u16 = 9;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    static kTISPropertyUnicodeKeyLayoutData: CFTypeRef;
    fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
    fn TISGetInputSourceProperty(source: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn LMGetKbdType() -> u8;
    fn UCKeyTranslate(
        layout: *const u8, key: u16, action: u16, modifiers: u32, kbd_type: u32, options: u32,
        dead_keys: *mut u32, max_len: usize, len: *mut usize, out: *mut u16,
    ) -> i32;
}

#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    static _dispatch_main_q: std::ffi::c_void;
    fn dispatch_sync_f(queue: *const std::ffi::c_void, ctx: *mut std::ffi::c_void, work: extern "C" fn(*mut std::ffi::c_void));
}

/// The first key code whose character is `want`, else QWERTY V.
fn keycode_for(want: char, char_of: impl Fn(u16) -> Option<char>) -> u16 {
    (0..128u16).find(|&k| char_of(k).is_some_and(|c| c.eq_ignore_ascii_case(&want))).unwrap_or(KEY_V_QWERTY)
}

/// The key that types "v" with ⌘ held in the current layout, so ⌘V is right on Dvorak, Colemak or AZERTY.
/// Asking with ⌘ held follows layouts like "Dvorak - QWERTY ⌘" that switch to QWERTY for shortcuts.
fn key_v_now() -> u16 {
    use core_foundation::base::{CFType, TCFType};
    unsafe {
        let src = TISCopyCurrentKeyboardLayoutInputSource();
        if src.is_null() {
            return KEY_V_QWERTY;
        }
        let src = CFType::wrap_under_create_rule(src);
        key_v_in(src.as_CFTypeRef())
    }
}

unsafe fn key_v_in(src: CFTypeRef) -> u16 {
    use core_foundation::base::TCFType;
    use core_foundation::data::CFData;
    unsafe {
        let data = TISGetInputSourceProperty(src, kTISPropertyUnicodeKeyLayoutData);
        if data.is_null() {
            return KEY_V_QWERTY;
        }
        let data = CFData::wrap_under_get_rule(data as _);
        let layout = data.bytes().as_ptr();
        let kbd = LMGetKbdType() as u32;
        keycode_for('v', |key| {
            let (mut dead, mut len, mut buf) = (0u32, 0usize, [0u16; 4]);
            // modifiers = cmdKey >> 8; option 1 = kUCKeyTranslateNoDeadKeysMask
            let ok = UCKeyTranslate(layout, key, 0, 1, kbd, 1, &mut dead, buf.len(), &mut len, buf.as_mut_ptr()) == 0;
            if ok && len == 1 { char::from_u32(buf[0] as u32) } else { None }
        })
    }
}

/// Text Input Sources must be read on the main thread (macOS 14+ traps otherwise); paste runs on a worker.
fn key_v() -> u16 {
    if objc2::MainThreadMarker::new().is_some() {
        return key_v_now();
    }
    extern "C" fn work(ctx: *mut std::ffi::c_void) {
        unsafe { *(ctx as *mut u16) = key_v_now() };
    }
    let mut out = KEY_V_QWERTY;
    unsafe { dispatch_sync_f(&_dispatch_main_q, &mut out as *mut u16 as _, work) };
    out
}

fn post_cmd_v() -> Result<(), &'static str> {
    let src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| "event source")?;
    let key = key_v();
    #[cfg(debug_assertions)]
    eprintln!("[paste] keycode for v = {key}");
    for down in [true, false] {
        let ev = CGEvent::new_keyboard_event(src.clone(), key, down).map_err(|_| "key event")?;
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
    // Never press ⌘V on whatever was there before: if our text did not land, say so and paste nothing.
    if pb.stringForType(unsafe { NSPasteboardTypeString }).map(|s| s.to_string()).as_deref() != Some(text) {
        restore(&pb, saved);
        return PasteResult::Failed("clipboard_write");
    }
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

    #[test]
    fn finds_v_on_other_layouts() {
        // Dvorak puts "v" on the QWERTY "." key (47); a non-Latin layout has no "v" at all.
        let dvorak = |k: u16| if k == 47 { Some('v') } else if k == 9 { Some('k') } else { Some('x') };
        assert_eq!(keycode_for('v', dvorak), 47);
        assert_eq!(keycode_for('v', |_| Some('ب')), KEY_V_QWERTY);
        assert_eq!(keycode_for('v', |k| (k == 9).then_some('v')), 9);
    }

    /// Reads the real layout. Run: cargo test --lib current_layout_v -- --ignored --nocapture
    #[test]
    #[ignore]
    fn current_layout_v() {
        use core_foundation::base::TCFType;
        use core_foundation::dictionary::CFDictionary;
        use core_foundation::string::CFString;
        #[link(name = "Carbon", kind = "framework")]
        unsafe extern "C" {
            static kTISPropertyInputSourceID: CFTypeRef;
            fn TISCreateInputSourceList(props: CFTypeRef, all: bool) -> CFTypeRef;
        }
        println!("current layout: v is key {}", key_v_now());
        for (id, want) in [("com.apple.keylayout.Dvorak", 47), ("com.apple.keylayout.DVORAK-QWERTYCMD", 9), ("com.apple.keylayout.Colemak", 9), ("com.apple.keylayout.French", 9), ("com.apple.keylayout.Arabic", 9), ("com.apple.keylayout.US", 9)] {
            unsafe {
                let key = CFString::wrap_under_get_rule(kTISPropertyInputSourceID as _);
                let props = CFDictionary::from_CFType_pairs(&[(key, CFString::new(id))]);
                let list = TISCreateInputSourceList(props.as_CFTypeRef(), true);
                let list = core_foundation::array::CFArray::<core_foundation::base::CFType>::wrap_under_create_rule(list as _);
                let Some(src) = list.get(0) else { println!("{id}: not installed"); continue };
                let got = key_v_in(src.as_CFTypeRef());
                println!("{id}: v is key {got} (expect {want})");
                assert_eq!(got, want);
            }
        }
    }

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
