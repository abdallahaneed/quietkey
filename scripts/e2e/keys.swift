// Usage: keys fn-down | fn-up | space | left | esc | ctrlcmd-v | ctrlcmd-c | cmd-a | cmd-c | click | click-bar:<dx> | sleep:<s> ...
// click-bar clicks the dictation pill, dx points right of its centre (the bar sits at the bottom centre, see lib.rs place_bar).
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
        // pill centre: 88 pt above the screen bottom to the window, 8 pt padding, half of the 36 pt pill
        let b = CGDisplayBounds(CGMainDisplayID())
        click(CGPoint(x: b.midX + (Double(a.dropFirst(10)) ?? 0), y: b.maxY - 88 - 8 - 18))
    default:
        if a.hasPrefix("sleep:"), let s = Double(a.dropFirst(6)) { Thread.sleep(forTimeInterval: s) }
    }
    Thread.sleep(forTimeInterval: 0.05)
}
