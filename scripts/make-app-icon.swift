// Draw the rscode app icon: an original "ribbon" mark in Rust orange (a chevron
// folding into a tall panel, evoking a code editor) with Ferris the crab.
// Original artwork: no Microsoft logo or Code-OSS artwork is used.
//
//   swift scripts/make-app-icon.swift <ferris.png> <out-1024.png>
//
// Ferris: https://rustacean.net/assets/rustacean-flat-happy.png (CC0, Karen Rustad Tölva).
import AppKit
import CoreGraphics

let args = CommandLine.arguments
guard args.count == 3 else {
    FileHandle.standardError.write("usage: make-app-icon.swift <ferris.png> <out.png>\n".data(using: .utf8)!)
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

func rgb(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255, alpha: a)
}

let ferris = cgImage(args[1])
let size = 1024
let space = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                    space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
ctx.interpolationQuality = .high

func linear(_ path: CGPath, _ c0: CGColor, _ c1: CGColor, from p0: CGPoint, to p1: CGPoint) {
    ctx.saveGState()
    ctx.addPath(path)
    ctx.clip()
    let g = CGGradient(colorsSpace: space, colors: [c0, c1] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(g, start: p0, end: p1, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
    ctx.restoreGState()
}

// 1. macOS-style tile (824px squircle-ish rounded rect on the 1024 grid) with drop shadow.
let tileRect = CGRect(x: 100, y: 100, width: 824, height: 824)
let tile = CGPath(roundedRect: tileRect, cornerWidth: 185, cornerHeight: 185, transform: nil)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: rgb(0x000000, 0.35))
ctx.addPath(tile)
ctx.setFillColor(rgb(0xFFFFFF))
ctx.fillPath()
ctx.restoreGState()
linear(tile, rgb(0xFFFDFB), rgb(0xF1E8E2), from: CGPoint(x: 512, y: 924), to: CGPoint(x: 512, y: 100))

// 2. Chevron "<": two thick arms meeting at the left point. Drawn as filled quads so
//    the folds can get separate shading.
let tip = CGPoint(x: 268, y: 540)
let armTopEnd = CGPoint(x: 640, y: 790)
let armBotEnd = CGPoint(x: 640, y: 290)
let thick: CGFloat = 118

func arm(_ end: CGPoint) -> CGPath {
    // Quad from the tip to `end`, `thick` wide, offset perpendicular to the arm.
    let dx = end.x - tip.x, dy = end.y - tip.y
    let len = (dx * dx + dy * dy).squareRoot()
    let nx = -dy / len * thick / 2, ny = dx / len * thick / 2
    let p = CGMutablePath()
    p.move(to: CGPoint(x: tip.x + nx, y: tip.y + ny))
    p.addLine(to: CGPoint(x: end.x + nx, y: end.y + ny))
    p.addLine(to: CGPoint(x: end.x - nx, y: end.y - ny))
    p.addLine(to: CGPoint(x: tip.x - nx, y: tip.y - ny))
    p.closeSubpath()
    return p
}
// Lower arm first (it sits "behind" the fold), slightly darker.
linear(arm(armBotEnd), rgb(0x8E300D), rgb(0x6A2209), from: tip, to: armBotEnd)
linear(arm(armTopEnd), rgb(0xC9561F), rgb(0xA8431A), from: tip, to: armTopEnd)
// Rounded cap at the tip so the chevron reads as one ribbon.
let cap = CGPath(ellipseIn: CGRect(x: tip.x - thick / 2, y: tip.y - thick / 2, width: thick, height: thick), transform: nil)
linear(cap, rgb(0xC9561F), rgb(0x9C3B12), from: CGPoint(x: tip.x, y: tip.y + 60), to: CGPoint(x: tip.x, y: tip.y - 60))

// 3. Tall panel on the right: a slanted-top/bottom slab the arms tuck into.
let panel = CGMutablePath()
panel.move(to: CGPoint(x: 600, y: 838))
panel.addLine(to: CGPoint(x: 770, y: 760))
panel.addLine(to: CGPoint(x: 770, y: 264))
panel.addLine(to: CGPoint(x: 600, y: 186))
panel.closeSubpath()
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: -10, height: 0), blur: 22, color: rgb(0x5A1A05, 0.45))
ctx.addPath(panel)
ctx.setFillColor(rgb(0xA8431A))
ctx.fillPath()
ctx.restoreGState()
linear(panel, rgb(0xD0612A), rgb(0x7A2A0C), from: CGPoint(x: 600, y: 838), to: CGPoint(x: 770, y: 186))
// Highlight strip on the panel's left edge (the fold).
let fold = CGMutablePath()
fold.move(to: CGPoint(x: 600, y: 838))
fold.addLine(to: CGPoint(x: 628, y: 825))
fold.addLine(to: CGPoint(x: 628, y: 199))
fold.addLine(to: CGPoint(x: 600, y: 186))
fold.closeSubpath()
ctx.addPath(fold)
ctx.setFillColor(rgb(0xFFFFFF, 0.22))
ctx.fillPath()

// 4. Ferris, bottom-right, overlapping the panel, with a soft shadow.
let fw: CGFloat = 470
let fh = fw * CGFloat(ferris.height) / CGFloat(ferris.width)
let ferrisRect = CGRect(x: 924 - fw - 30, y: 112, width: fw, height: fh)
// White outline: stamp a white silhouette (Ferris' alpha as a mask) around him.
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -8), blur: 20, color: rgb(0x000000, 0.35))
ctx.beginTransparencyLayer(auxiliaryInfo: nil)
for i in 0..<16 {
    let a = Double(i) / 16 * 2 * Double.pi
    let r = ferrisRect.offsetBy(dx: CGFloat(cos(a)) * 9, dy: CGFloat(sin(a)) * 9)
    ctx.saveGState()
    ctx.clip(to: r, mask: ferris)
    ctx.setFillColor(rgb(0xFFFFFF))
    ctx.fill(r)
    ctx.restoreGState()
}
ctx.endTransparencyLayer()
ctx.restoreGState()
ctx.draw(ferris, in: ferrisRect)

let out = ctx.makeImage()!
try! NSBitmapImageRep(cgImage: out).representation(using: .png, properties: [:])!
    .write(to: URL(fileURLWithPath: args[2]))
print("wrote \(args[2])")
