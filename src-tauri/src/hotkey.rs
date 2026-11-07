//! Global Fn-key listener. Fn never reaches normal hotkey APIs, so this uses a CGEventTap on flagsChanged
//! (needs Input Monitoring + Accessibility). Runs its own CFRunLoop thread and sends events over a channel.

use core_foundation::base::TCFType;
use core_foundation::runloop::CFRunLoop;
use core_graphics::event::{
    CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};

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

/// Marks key events this app posts itself (the paste keystroke), so the listener can skip them.
pub const SYNTHETIC_TAG: i64 = 0x5574_7465; // "Utte"

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

/// Other push-to-talk dictation apps, by the name they show in the Dock. Two apps listening for the same keys
/// both record and both paste.
const OTHER_DICTATION_APPS: &[&str] = &["wispr flow", "superwhisper", "macwhisper", "voiceink", "aqua voice", "willow"];

/// Names of running apps that likely listen for a dictation shortcut too.
pub fn other_dictation_apps(running: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = running
        .into_iter()
        .filter(|n| OTHER_DICTATION_APPS.iter().any(|o| n.to_lowercase().starts_with(o)))
        .collect();
    out.sort();
    out.dedup();
    out
}

static TAP_PORT: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

/// The push-to-talk keys as a mask of modifier flag bits. Fn by default.
static SHORTCUT: AtomicU64 = AtomicU64::new(FN_BIT);
/// Set while the shortcut editor is recording keys, so pressing them does not start a dictation.
static PAUSED: AtomicBool = AtomicBool::new(false);

const FN_BIT: u64 = 0x80_0000; // NX_SECONDARYFNMASK
/// Modifier keys a shortcut can use: name, keycode, and the device-dependent flag bit that tells left from right.
/// shortcut: modifier keys only. The tap is listen-only, so a letter key in the shortcut would also be typed
/// into the app; supporting one needs an active tap that swallows the key.
const MOD_KEYS: [(&str, i64, u64); 9] = [
    ("Fn", KC_FN, FN_BIT),
    ("LeftCtrl", 59, 0x1),
    ("RightCtrl", 62, 0x2000),
    ("LeftShift", 56, 0x2),
    ("RightShift", 60, 0x4),
    ("LeftCmd", 55, 0x8),
    ("RightCmd", 54, 0x10),
    ("LeftOpt", 58, 0x20),
    ("RightOpt", 61, 0x40),
];
const ALL_MODS: u64 = FN_BIT | 0x1 | 0x2000 | 0x2 | 0x4 | 0x8 | 0x10 | 0x20 | 0x40;

/// Validates key names from the UI and turns them into a mask. 1 to 3 modifier keys; a single left-side key is
/// refused because it is held all the time while typing (capitals, ⌘C, ⌥ letters).
pub fn parse_shortcut(keys: &[String]) -> Result<u64, String> {
    if keys.is_empty() || keys.len() > 3 {
        return Err("Pick 1 to 3 keys".into());
    }
    let mut mask = 0;
    for k in keys {
        let (_, _, bit) = MOD_KEYS.iter().find(|(n, _, _)| n == k).ok_or(format!("{k} cannot be used, pick modifier keys like fn, ⌃, ⌥ or ⌘"))?;
        if mask & bit != 0 {
            return Err("Each key can only be used once".into());
        }
        mask |= bit;
    }
    if keys.len() == 1 && keys[0].starts_with("Left") {
        return Err("This key is used while typing. Pick fn, a right-side key, or two keys together".into());
    }
    if mask & !(0x2 | 0x4) == 0 {
        return Err("Shift alone is used while typing. Add another key".into());
    }
    Ok(mask)
}

pub fn set_shortcut(mask: u64) {
    SHORTCUT.store(mask, Ordering::SeqCst);
}

pub fn set_paused(on: bool) {
    PAUSED.store(on, Ordering::SeqCst);
}

/// What the listener remembers between events.
#[derive(Default)]
pub struct Tracker {
    /// The shortcut is held right now.
    active: AtomicBool,
    /// An extra key joined the shortcut; ignore it until all its keys are released.
    spoiled: AtomicBool,
}

/// Pure decision logic, kept apart from the OS callback so it can be tested.
pub fn classify(etype: CGEventType, keycode: i64, flags: CGEventFlags, shortcut: u64, st: &Tracker) -> Option<HotkeyEvent> {
    match etype {
        CGEventType::FlagsChanged if MOD_KEYS.iter().any(|(_, kc, _)| *kc == keycode) => {
            let held = flags.bits() & ALL_MODS;
            if held & shortcut == 0 {
                st.spoiled.store(false, Ordering::SeqCst);
            }
            let was = st.active.load(Ordering::SeqCst);
            if !was && held == shortcut && !st.spoiled.load(Ordering::SeqCst) {
                st.active.store(true, Ordering::SeqCst);
                Some(HotkeyEvent::FnDown)
            } else if was && held != shortcut {
                st.active.store(false, Ordering::SeqCst);
                if held & shortcut == shortcut {
                    st.spoiled.store(true, Ordering::SeqCst); // another modifier joined: a shortcut, not dictation
                    Some(HotkeyEvent::FnCombo)
                } else {
                    Some(HotkeyEvent::FnUp)
                }
            } else {
                None
            }
        }
        CGEventType::KeyDown if keycode == KC_ESC => Some(HotkeyEvent::Escape),
        CGEventType::KeyDown
            if flags.contains(CGEventFlags::CGEventFlagControl | CGEventFlags::CGEventFlagCommand)
                && (keycode == KC_V || keycode == KC_C) =>
        {
            Some(if keycode == KC_V { HotkeyEvent::PasteLast } else { HotkeyEvent::CopyLast })
        }
        CGEventType::KeyDown if st.active.load(Ordering::SeqCst) => {
            Some(if keycode == KC_SPACE { HotkeyEvent::FnSpace } else { HotkeyEvent::FnCombo })
        }
        _ => None,
    }
}

/// Spawns the listener thread. Returns Err right away if macOS refuses the tap (permission missing).
pub fn spawn(on_event: impl Fn(HotkeyEvent) + Send + 'static) -> anyhow::Result<()> {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), ()>>();
    std::thread::Builder::new().name("hotkey-tap".into()).spawn(move || {
        let tracker = Tracker::default();
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
                if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == SYNTHETIC_TAG {
                    return CallbackResult::Keep; // our own paste keystroke, not the user
                }
                if PAUSED.load(Ordering::SeqCst) {
                    return CallbackResult::Keep;
                }
                let keycode = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                if let Some(ev) = classify(etype, keycode, event.get_flags(), SHORTCUT.load(Ordering::SeqCst), &tracker) {
                    #[cfg(debug_assertions)]
                    if ev == HotkeyEvent::FnCombo {
                        eprintln!("[hotkey] fn combo from keycode {keycode} type {etype:?}");
                    }
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

    #[test]
    fn finds_other_dictation_apps() {
        let running = ["Finder", "Wispr Flow", "superwhisper", "Flow", "Safari", "Wispr Flow"].map(String::from);
        assert_eq!(other_dictation_apps(running), vec!["Wispr Flow".to_string(), "superwhisper".to_string()]);
        assert!(other_dictation_apps(Vec::<String>::new()).is_empty());
    }

    use super::*;

    fn f(bits: u64) -> CGEventFlags {
        CGEventFlags::from_bits_retain(bits)
    }

    #[test]
    fn fn_press_release_and_combos() {
        let st = Tracker::default();
        let c = |t, k, fl| classify(t, k, fl, FN_BIT, &st);
        let fnf = f(FN_BIT);
        let none = CGEventFlags::CGEventFlagNull;
        assert_eq!(c(CGEventType::FlagsChanged, KC_FN, fnf), Some(HotkeyEvent::FnDown));
        // repeated flagsChanged with Fn still down is ignored
        assert_eq!(c(CGEventType::FlagsChanged, KC_FN, fnf), None);
        assert_eq!(c(CGEventType::KeyDown, KC_SPACE, fnf), Some(HotkeyEvent::FnSpace));
        assert_eq!(c(CGEventType::KeyDown, 123, fnf), Some(HotkeyEvent::FnCombo));
        assert_eq!(c(CGEventType::FlagsChanged, KC_FN, none), Some(HotkeyEvent::FnUp));
        // plain typing without Fn is ignored, Esc always reported
        assert_eq!(c(CGEventType::KeyDown, 0, none), None);
        assert_eq!(c(CGEventType::KeyDown, KC_ESC, none), Some(HotkeyEvent::Escape));
        let cc = CGEventFlags::CGEventFlagControl | CGEventFlags::CGEventFlagCommand;
        assert_eq!(c(CGEventType::KeyDown, KC_V, cc), Some(HotkeyEvent::PasteLast));
        assert_eq!(c(CGEventType::KeyDown, KC_C, cc), Some(HotkeyEvent::CopyLast));
        assert_eq!(c(CGEventType::KeyDown, KC_V, CGEventFlags::CGEventFlagCommand), None); // plain ⌘V
        // other modifier changes (Shift) are ignored
        assert_eq!(c(CGEventType::FlagsChanged, 56, none), None);
    }

    #[test]
    fn custom_two_key_shortcut() {
        let st = Tracker::default();
        let sc = parse_shortcut(&["RightCtrl".into(), "RightOpt".into()]).unwrap();
        let c = |k, bits| classify(CGEventType::FlagsChanged, k, f(bits), sc, &st);
        assert_eq!(c(62, 0x2000), None); // first key alone
        assert_eq!(c(61, 0x2000 | 0x40), Some(HotkeyEvent::FnDown));
        assert_eq!(c(61, 0x2000), Some(HotkeyEvent::FnUp));
        // an extra modifier turns it into a combo, and it stays ignored until fully released
        assert_eq!(c(61, 0x2000 | 0x40), Some(HotkeyEvent::FnDown));
        assert_eq!(c(55, 0x2000 | 0x40 | 0x8), Some(HotkeyEvent::FnCombo));
        assert_eq!(c(55, 0x2000 | 0x40), None);
        assert_eq!(c(61, 0x2000), None);
        assert_eq!(c(62, 0), None);
        assert_eq!(c(62, 0x2000), None);
        assert_eq!(c(61, 0x2000 | 0x40), Some(HotkeyEvent::FnDown));
        // the left keys do not match the right ones
        let st2 = Tracker::default();
        assert_eq!(classify(CGEventType::FlagsChanged, 58, f(0x2000 | 0x20), sc, &st2), None);
    }

    #[test]
    fn shortcut_validation() {
        let p = |k: &[&str]| parse_shortcut(&k.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(p(&["Fn"]), Ok(FN_BIT));
        assert!(p(&["RightOpt"]).is_ok());
        assert!(p(&["LeftCtrl", "LeftOpt"]).is_ok());
        assert!(p(&[]).is_err());
        assert!(p(&["LeftCmd"]).is_err()); // used while typing
        assert!(p(&["LeftShift", "RightShift"]).is_err());
        assert!(p(&["Fn", "Fn"]).is_err());
        assert!(p(&["Fn", "LeftCtrl", "LeftOpt", "LeftCmd"]).is_err());
        assert!(p(&["Space"]).is_err());
    }
}
