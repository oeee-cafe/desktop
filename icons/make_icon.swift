// Draws the app icon at 1024: a squircle of NEO's ground -- the lavender
// and 14px grid the site draws behind every page, here ten cells across --
// with the site's cucumber slice lying on it. The same design as the iOS,
// Mac and Android icons.
//
//   swift icons/make_icon.swift <slice.png> <out.png>
//
// <slice.png> is the 1024px slice on transparency, padding and all:
// oeee-cafe-apple's scripts/app-icon/slice.png.
import AppKit
import CoreGraphics

let size = 1024.0
// The whole canvas, as Windows icons are: the taskbar shows the icon at 24
// and 32px, where any margin only makes it smaller than its neighbours. And
// no shadow under it, which Windows icons don't have and which at those
// sizes reads as more margin.
let body = 1024.0
let inset = (size - body) / 2
let cells = 10.0
let lineWeight = 0.07  // of a cell, the site's 1px in 14

// A superellipse (n = 5) approximates the continuous corner: the curvature
// eases in rather than switching from straight to arc.
func squircle(in rect: CGRect, n: Double = 5.0) -> CGPath {
    let path = CGMutablePath()
    let a = rect.width / 2, b = rect.height / 2
    let cx = rect.midX, cy = rect.midY
    let steps = 720
    for i in 0...steps {
        let t = Double(i) / Double(steps) * 2 * Double.pi
        let c = cos(t), s = sin(t)
        let x = cx + a * copysign(pow(abs(c), 2 / n), c)
        let y = cy + b * copysign(pow(abs(s), 2 / n), s)
        if i == 0 { path.move(to: CGPoint(x: x, y: y)) } else { path.addLine(to: CGPoint(x: x, y: y)) }
    }
    path.closeSubpath()
    return path
}

func color(_ hex: UInt32, _ alpha: Double = 1) -> CGColor {
    CGColor(red: Double((hex >> 16) & 0xff) / 255, green: Double((hex >> 8) & 0xff) / 255,
            blue: Double(hex & 0xff) / 255, alpha: alpha)
}

let space = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(data: nil, width: Int(size), height: Int(size), bitsPerComponent: 8,
                    bytesPerRow: 0, space: space,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
let rect = CGRect(x: inset, y: inset, width: body, height: body)
let shape = squircle(in: rect)

// The ground and its grid, lines on the cell boundaries so the grid is
// symmetric about the centre.
ctx.saveGState()
ctx.addPath(shape)
ctx.clip()
ctx.setFillColor(color(0xccccff))
ctx.fill(rect)
ctx.setFillColor(color(0xbbbbff))
let cell = body / cells
let line = cell * lineWeight
for k in 1..<Int(cells) {
    let p = inset + Double(k) * cell - line / 2
    ctx.fill(CGRect(x: p, y: inset, width: line, height: body))
    ctx.fill(CGRect(x: inset, y: p, width: body, height: line))
}
// A hairline of light just inside the edge.
ctx.addPath(shape)
ctx.setStrokeColor(color(0xffffff, 0.35))
ctx.setLineWidth(4)
ctx.strokePath()

// The slice, drawn the body's size so its own padding leaves it at about
// 70% across, as on the other platforms, with the shadow of something
// lying on the ground.
let markURL = URL(fileURLWithPath: CommandLine.arguments[1])
let markSource = CGImageSourceCreateWithURL(markURL as CFURL, nil)!
let mark = CGImageSourceCreateImageAtIndex(markSource, 0, nil)!
ctx.setShadow(offset: CGSize(width: 0, height: -body * 0.024),
              blur: body * 0.048, color: color(0x1c2811, 0.34))
ctx.interpolationQuality = .high
ctx.draw(mark, in: rect)
ctx.restoreGState()

let image = ctx.makeImage()!
let out = URL(fileURLWithPath: CommandLine.arguments[2])
let dest = CGImageDestinationCreateWithURL(out as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(dest, image, nil)
CGImageDestinationFinalize(dest)
