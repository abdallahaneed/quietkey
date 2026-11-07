// Puts the pointer on each display, starts and cancels a dictation, prints whether the bar landed on that display.
// Used by suite.sh F03-H4 (B-bar-multidisplay). Prints one "inside=true|false" line per display.
import CoreGraphics
import Foundation
func bar() -> CGRect {
    let l = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
    for w in l where (w[kCGWindowOwnerName as String] as? String ?? "").lowercased().contains("quietkey") {
        let r = CGRect(dictionaryRepresentation: w[kCGWindowBounds as String] as! NSDictionary)!
        if r.width == 220 { return r }
    }
    return .zero
}
var ids = [CGDirectDisplayID](repeating: 0, count: 4); var n: UInt32 = 0
CGGetActiveDisplayList(4, &ids, &n)
for d in ids.prefix(Int(n)) {
    let b = CGDisplayBounds(d)
    CGWarpMouseCursorPosition(CGPoint(x: b.midX, y: b.midY))
    Thread.sleep(forTimeInterval: 0.3)
    let p = Process(); p.executableURL = URL(fileURLWithPath: "./keys"); p.arguments = ["fn-down", "sleep:0.6", "esc", "fn-up", "sleep:1"]
    try! p.run(); p.waitUntilExit()
    let r = bar()
    print("display \(b) -> bar \(r) inside=\(b.contains(r))")
}
