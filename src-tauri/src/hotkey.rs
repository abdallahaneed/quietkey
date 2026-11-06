//! Global Fn-key listener. Fn never reaches normal hotkey APIs, so this uses a CGEventTap on flagsChanged
//! (needs Input Monitoring + Accessibility). Runs its own CFRunLoop thread and sends events over a channel.

use core_foundation::base::TCFType;
use core_foundation::runloop::CFRunLoop;
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HotkeyEvent {
    FnDown,
    FnUp,
    /// Space pressed while Fn is held: lock into hands-free.
    FnSpace,
    /// Any other key while Fn is held (Fn+arrow, Fn+F-key): the user is not dictating.
    FnCombo,
    Escape,
    /// ⌃⌘V: paste the last dictation again (recovery when a paste went missing).
    PasteLast,
    /// ⌃⌘C: copy the last dictation.
    CopyLast,
}

const KC_FN: i64 = 63; // kVK_Function
const KC_SPACE: i64 = 49;
const KC_ESC: i64 = 53;
const KC_V: i64 = 9;
const KC_C: i64 = 8;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapEnable(tap: *mut std::ffi::c_void, enable: bool);
}

pub fn input_monitoring_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

pub fn request_input_monitoring() -> bool {
    unsafe { CGRequestListenEventAccess() }
}

/// macOS can make fn start Apple Dictation, the emoji picker or switch input source. Any of those fires on
/// every dictation press, so Setup warns about it. 0 = "Do Nothing".
pub fn fn_key_conflict() -> bool {
    std::process::Command::new("defaults")
        .args(["read", "com.apple.HIToolbox", "AppleFnUsageType"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|v| v.trim() != "0")
        .unwrap_or(false) // key missing = system default, which on recent macOS is "Do Nothing" or emoji; do not nag
}

static TAP_PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// Pure decision logic, kept apart from the OS callback so it can be tested.
pub fn classify(etype: CGEventType, keycode: i64, flags: CGEventFlags, fn_held: &AtomicBool) -> Option<HotkeyEvent> {
    match etype {
        CGEventType::FlagsChanged if keycode == KC_FN => {
            let down = flags.contains(CGEventFlags::CGEventFlagSecondaryFn);
            let was = fn_held.swap(down, Ordering::SeqCst);
            match (was, down) {
                (false, true) => Some(HotkeyEvent::FnDown),
                (true, false) => Some(HotkeyEvent::FnUp),
                _ => None,
            }
        }
        CGEventType::KeyDown if keycode == KC_ESC => Some(HotkeyEvent::Escape),
        CGEventType::KeyDown
            if flags.contains(CGEventFlags::CGEventFlagControl | CGEventFlags::CGEventFlagCommand)
                && (keycode == KC_V || keycode == KC_C) =>
        {
            Some(if keycode == KC_V { HotkeyEvent::PasteLast } else { HotkeyEvent::CopyLast })
        }
        CGEventType::KeyDown if fn_held.load(Ordering::SeqCst) => {
            Some(if keycode == KC_SPACE { HotkeyEvent::FnSpace } else { HotkeyEvent::FnCombo })
        }
        _ => None,
    }
}

/// Spawns the listener thread. Returns Err right away if macOS refuses the tap (permission missing).
pub fn spawn(on_event: impl Fn(HotkeyEvent) + Send + 'static) -> anyhow::Result<()> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), ()>>();
    std::thread::Builder::new().name("hotkey-tap".into()).spawn(move || {
        let fn_held = AtomicBool::new(false);
        let tap = CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::ListenOnly,
            vec![CGEventType::FlagsChanged, CGEventType::KeyDown],
            move |_proxy, etype, event| {
                // macOS disables a tap that is slow or after secure input; turn it straight back on.
                if matches!(etype, CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput) {
                    let port = TAP_PORT.load(Ordering::SeqCst);
                    if !port.is_null() {
                        unsafe { CGEventTapEnable(port, true) };
                    }
                    return CallbackResult::Keep;
                }
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                if let Some(ev) = classify(etype, keycode, event.get_flags(), &fn_held) {
                    on_event(ev);
                }
                CallbackResult::Keep
            },
        );
        let Ok(tap) = tap else {
            let _ = ready_tx.send(Err(()));
            return;
        };
        TAP_PORT.store(tap.mach_port().as_concrete_TypeRef() as *mut _, Ordering::SeqCst);
        let source = tap.mach_port().create_runloop_source(0).expect("runloop source");
        CFRunLoop::get_current().add_source(&source, unsafe { core_foundation::runloop::kCFRunLoopCommonModes });
        tap.enable();
        let _ = ready_tx.send(Ok(()));
        CFRunLoop::run_current(); // runs for the app's lifetime
    })?;
    ready_rx
        .recv()
        .map_err(|_| anyhow::anyhow!("hotkey thread died"))?
        .map_err(|_| anyhow::anyhow!("event_tap_denied"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fn_press_release_and_combos() {
        let held = AtomicBool::new(false);
        let fnf = CGEventFlags::CGEventFlagSecondaryFn;
        let none = CGEventFlags::CGEventFlagNull;
        assert_eq!(classify(CGEventType::FlagsChanged, KC_FN, fnf, &held), Some(HotkeyEvent::FnDown));
        // repeated flagsChanged with Fn still down is ignored
        assert_eq!(classify(CGEventType::FlagsChanged, KC_FN, fnf, &held), None);
        assert_eq!(classify(CGEventType::KeyDown, KC_SPACE, fnf, &held), Some(HotkeyEvent::FnSpace));
        assert_eq!(classify(CGEventType::KeyDown, 123, fnf, &held), Some(HotkeyEvent::FnCombo));
        assert_eq!(classify(CGEventType::FlagsChanged, KC_FN, none, &held), Some(HotkeyEvent::FnUp));
        // plain typing without Fn is ignored, Esc always reported
        assert_eq!(classify(CGEventType::KeyDown, 0, none, &held), None);
        assert_eq!(classify(CGEventType::KeyDown, KC_ESC, none, &held), Some(HotkeyEvent::Escape));
        let cc = CGEventFlags::CGEventFlagControl | CGEventFlags::CGEventFlagCommand;
        assert_eq!(classify(CGEventType::KeyDown, KC_V, cc, &held), Some(HotkeyEvent::PasteLast));
        assert_eq!(classify(CGEventType::KeyDown, KC_C, cc, &held), Some(HotkeyEvent::CopyLast));
        assert_eq!(classify(CGEventType::KeyDown, KC_V, CGEventFlags::CGEventFlagCommand, &held), None); // plain ⌘V
        // other modifier changes (Shift) are ignored
        assert_eq!(classify(CGEventType::FlagsChanged, 56, none, &held), None);
    }
}
