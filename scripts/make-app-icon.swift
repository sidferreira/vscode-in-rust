// Compose the rscode app icon: the Code-OSS icon with Ferris the crab in the
// bottom-right corner. Re-run when either source image changes.
//
//   swift scripts/make-app-icon.swift <code-oss-1024.png> <ferris.png> <out-1024.png>
//
// Sources:
//   - Code-OSS icon: vscode-original/resources/darwin/code.icns (MIT, Microsoft).
//     NOT the trademarked "Visual Studio Code" product logo.
//   - Ferris: https://rustacean.net/assets/rustacean-flat-happy.png (CC0, Karen Rustad Tölva).
//
// upstream: resources/darwin/code.icns @ 50e1350fc78d
import AppKit
import CoreGraphics

let args = CommandLine.arguments
guard args.count == 4 else {
    FileHandle.standardError.write("usage: make-app-icon.swift <base.png> <ferris.png> <out.png>\n".data(using: .utf8)!)
    exit(2)
}

func cgImage(_ path: String) -> CGImage {
    guard let img = NSImage(contentsOfFile: path),
          let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
        FileHandle.standardError.write("cannot load \(path)\n".data(using: .utf8)!)
        exit(1)
    }
    return cg
}

let base = cgImage(args[1])
let ferris = cgImage(args[2])
let size = 1024

let ctx = CGContext(
    data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
    space: CGColorSpace(name: CGColorSpace.sRGB)!,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
)!
ctx.interpolationQuality = .high
ctx.draw(base, in: CGRect(x: 0, y: 0, width: size, height: size))

// Ferris: ~52% of the icon width, sitting on the bottom-right of the squircle
// (macOS icon grid leaves ~100px margin), with a soft shadow for contrast.
let fw = Double(size) * 0.52
let fh = fw * Double(ferris.height) / Double(ferris.width)
let rect = CGRect(x: Double(size) - fw - 70, y: 80, width: fw, height: fh)
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 24,
              color: CGColor(gray: 0, alpha: 0.45))
ctx.draw(ferris, in: rect)

let out = ctx.makeImage()!
let rep = NSBitmapImageRep(cgImage: out)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[3]))
print("wrote \(args[3])")
