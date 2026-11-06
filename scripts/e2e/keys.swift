// Usage: keys fn-down | fn-up | space | esc | ctrlcmd-v | sleep:<s> ...   (posted like real HID input)
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
    case "ctrlcmd-v": key(9, [.maskControl, .maskCommand])
    default:
        if a.hasPrefix("sleep:"), let s = Double(a.dropFirst(6)) { Thread.sleep(forTimeInterval: s) }
    }
    Thread.sleep(forTimeInterval: 0.05)
}
