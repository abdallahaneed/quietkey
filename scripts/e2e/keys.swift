// Usage: keys fn-down | fn-up | space | left | esc | ctrlcmd-v | ctrlcmd-c | cmd-a | cmd-c | click | click-bar:<dx> | sleep:<s> ...
// click-bar clicks the dictation pill, dx points right of its centre. The bar window is looked up, so it works on any display.
// Events are posted like real HID input.
import CoreGraphics
import Foundation
let src = CGEventSource(stateID: .hidSystemState)
var fnHeld = false
func key(_ code: CGKeyCode, _ flags: CGEventFlags) {
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down)!
        e.flags = flags
        e.post(tap: .cghidEventTap)
    }
}
func click(_ at: CGPoint? = nil) { // left click, in the middle of the main screen by default
    let b = CGDisplayBounds(CGMainDisplayID())
    let p = at ?? CGPoint(x: b.midX, y: b.midY)
    for t in [CGEventType.leftMouseDown, .leftMouseUp] {
        CGEvent(mouseEventSource: src, mouseType: t, mouseCursorPosition: p, mouseButton: .left)!.post(tap: .cghidEventTap)
    }
}
/// The on-screen 220x56 window of the Quietkey process (tauri.conf.json "bar"), in global top-left coordinates.
/// Titles need Screen Recording permission, so match owner and size.
func barFrame() -> CGRect {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
    for w in list {
        guard (w[kCGWindowOwnerName as String] as? String)?.lowercased().contains("quietkey") == true,
              let d = w[kCGWindowBounds as String] as? NSDictionary,
              let r = CGRect(dictionaryRepresentation: d), r.width == 220, r.height == 56 else { continue }
        return r
    }
    FileHandle.standardError.write("click-bar: bar window not found\n".data(using: .utf8)!)
    exit(2)
}
func fn(_ down: Bool) {
    let e = CGEvent(keyboardEventSource: src, virtualKey: 63, keyDown: down)!
    e.type = .flagsChanged
    e.flags = down ? .maskSecondaryFn : []
    e.post(tap: .cghidEventTap)
    fnHeld = down
}
for a in CommandLine.arguments.dropFirst() {
    switch a {
    case "fn-down": fn(true)
    case "fn-up": fn(false)
    case "space": key(49, fnHeld ? .maskSecondaryFn : [])
    case "esc": key(53, [])
    case "left": key(123, fnHeld ? .maskSecondaryFn : [])
    case "ctrlcmd-v": key(9, [.maskControl, .maskCommand])
    case "ctrlcmd-c": key(8, [.maskControl, .maskCommand])
    case "cmd-a": key(0, .maskCommand)
    case "cmd-c": key(8, .maskCommand)
    case "click": click()
    case _ where a.hasPrefix("click-bar:"):
        // pill centre: 8 pt padding plus half of the 36 pt pill below the window top
        let b = barFrame()
        click(CGPoint(x: b.midX + (Double(a.dropFirst(10)) ?? 0), y: b.minY + 8 + 18))
    default:
        if a.hasPrefix("sleep:"), let s = Double(a.dropFirst(6)) { Thread.sleep(forTimeInterval: s) }
    }
    Thread.sleep(forTimeInterval: 0.05)
}
