// A minimal paste target for the e2e tests: one window with a focused text view.
// It writes its text to the file given as the first argument every 200 ms and quits when killed.
// Unlike TextEdit it never autosaves to iCloud, never renames itself and leaves nothing behind.
// Usage: QuietkeyTestTarget /tmp/out.txt
import AppKit

let outPath = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "/tmp/quietkey-target.txt"

final class Delegate: NSObject, NSApplicationDelegate {
    var window: NSWindow!
    var textView: NSTextView!

    func applicationDidFinishLaunching(_ notification: Notification) {
        // ⌘V / ⌘C / ⌘A only work through menu key equivalents, so a bare app needs an Edit menu.
        let main = NSMenu()
        let appItem = NSMenuItem(); main.addItem(appItem)
        appItem.submenu = NSMenu()
        appItem.submenu!.addItem(withTitle: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let editItem = NSMenuItem(); main.addItem(editItem)
        let edit = NSMenu(title: "Edit")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = edit
        NSApp.mainMenu = main

        window = NSWindow(contentRect: NSRect(x: 240, y: 240, width: 640, height: 420),
                          styleMask: [.titled], backing: .buffered, defer: false)
        window.title = "Quietkey test target"
        let scroll = NSScrollView(frame: window.contentView!.bounds)
        scroll.autoresizingMask = [.width, .height]
        textView = NSTextView(frame: scroll.bounds)
        textView.autoresizingMask = [.width, .height]
        textView.isRichText = false
        scroll.documentView = textView
        window.contentView!.addSubview(scroll)
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(textView)
        NSApp.activate(ignoringOtherApps: true)
        try? "".write(toFile: outPath, atomically: true, encoding: .utf8)
        Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in
            guard let text = self?.textView.string else { return }
            try? text.write(toFile: outPath, atomically: true, encoding: .utf8)
        }
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let delegate = Delegate()
app.delegate = delegate
app.run()
