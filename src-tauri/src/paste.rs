use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting, NSWorkspace};
use objc2_foundation::{NSArray, NSData, NSString};
use std::time::Duration;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Accessibility permission: needed to post the synthetic Cmd+V.
pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

pub struct FrontApp {
    pub bundle_id: Option<String>,
    pub name: Option<String>,
}

pub fn frontmost_app() -> FrontApp {
    let ws = NSWorkspace::sharedWorkspace();
    match ws.frontmostApplication() {
        Some(app) => FrontApp {
            bundle_id: app.bundleIdentifier().map(|s| s.to_string()),
            name: app.localizedName().map(|s| s.to_string()),
        },
        None => FrontApp { bundle_id: None, name: None },
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
