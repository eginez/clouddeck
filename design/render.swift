// Renders the shots in shots.json from main-window.html to PNG files, using the
// WebKit that ships with macOS (no downloads, no browser install).
//
//   swiftc -swift-version 5 -o /tmp/clouddeck-render design/render.swift
//   /tmp/clouddeck-render design/shots.json design/main-window.html design/images [name-prefix]
//
// Each shot reloads the page, runs `prelude` + `setup` (plain JS, may use await),
// then snapshots the element matched by `clip` at `scale`x. Re-run it after
// changing main-window.html or shots.json.
import Cocoa
import WebKit

struct Shot: Decodable {
    let name: String
    let description: String
    let setup: String
    let clip: String
    let scale: Double
}
struct Manifest: Decodable {
    let prelude: String
    let shots: [Shot]
}

@MainActor
final class Renderer: NSObject, WKNavigationDelegate {
    let web: WKWebView
    let window: NSWindow
    var loaded: CheckedContinuation<Void, Never>?

    override init() {
        let frame = NSRect(x: 0, y: 0, width: 1380, height: 1700)
        web = WKWebView(frame: frame, configuration: WKWebViewConfiguration())
        window = NSWindow(contentRect: frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = web
        window.isReleasedWhenClosed = false
        super.init()
        web.navigationDelegate = self
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        window.makeFirstResponder(web)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        loaded?.resume()
        loaded = nil
    }

    func load(_ html: URL) async {
        await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
            loaded = c
            web.loadFileURL(html, allowingReadAccessTo: html.deletingLastPathComponent())
        }
    }

    @discardableResult
    func js(_ code: String) async throws -> Any? {
        try await web.callAsyncJavaScript(code, arguments: [:], in: nil, contentWorld: .page)
    }

    func render(_ shot: Shot, prelude: String, html: URL, out: URL) async throws {
        await load(html)
        window.makeFirstResponder(web)
        try await js(prelude + shot.setup + "\nawait new Promise(r=>setTimeout(r,250)); return null;")
        let r = try await js("""
            const el=document.querySelector(\(jsString(shot.clip)));
            if(!el) return null;
            el.scrollIntoView({block:'start'});
            const b=el.getBoundingClientRect();
            return [b.left,b.top,b.width,b.height];
            """) as? [Double]
        guard let r, r.count == 4 else {
            throw NSError(domain: "render", code: 1, userInfo: [NSLocalizedDescriptionKey: "no element for \(shot.clip)"])
        }
        try await Task.sleep(nanoseconds: 120_000_000)
        let cfg = WKSnapshotConfiguration()
        cfg.rect = CGRect(x: r[0], y: r[1], width: r[2], height: r[3])
        cfg.snapshotWidth = NSNumber(value: r[2] * shot.scale)
        let image = try await web.takeSnapshot(configuration: cfg)
        let pw = Int((r[2] * shot.scale).rounded()), ph = Int((r[3] * shot.scale).rounded())
        guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pw, pixelsHigh: ph, bitsPerSample: 8,
                                         samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                         colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0) else { return }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        image.draw(in: NSRect(x: 0, y: 0, width: pw, height: ph))
        NSGraphicsContext.restoreGraphicsState()
        guard let png = rep.representation(using: .png, properties: [:]) else { return }
        try png.write(to: out.appendingPathComponent(shot.name + ".png"))
    }
}

func jsString(_ s: String) -> String {
    let data = try! JSONSerialization.data(withJSONObject: [s])
    let arr = String(data: data, encoding: .utf8)!
    return String(arr.dropFirst().dropLast())
}

let args = CommandLine.arguments
guard args.count >= 4 else {
    print("usage: render <shots.json> <main-window.html> <out-dir> [name-prefix]")
    exit(2)
}
let manifest = try JSONDecoder().decode(Manifest.self, from: Data(contentsOf: URL(fileURLWithPath: args[1])))
let html = URL(fileURLWithPath: args[2])
let outDir = URL(fileURLWithPath: args[3])
let only = args.count > 4 ? args[4] : ""
try FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)

let app = NSApplication.shared
app.setActivationPolicy(.regular)
Task { @MainActor in
    let renderer = Renderer()
    var failed = 0
    for shot in manifest.shots where only.isEmpty || shot.name.hasPrefix(only) {
        do {
            try await renderer.render(shot, prelude: manifest.prelude, html: html, out: outDir)
            print("ok   \(shot.name)")
        } catch {
            failed += 1
            print("FAIL \(shot.name): \(error)")
        }
    }
    exit(failed == 0 ? 0 : 1)
}
app.run()
