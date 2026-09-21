// Draws the macOS app icon on Apple's template: a 1024 canvas, an 824 body
// centred in it (100 each side) with continuous corners, a soft shadow under
// it. The body is NEO's ground -- lavender with the painter's grid -- and the
// site's cucumber slice sits in the middle.
import AppKit
import CoreGraphics

let size = 1024.0
let body = 824.0
let inset = (size - body) / 2

// A superellipse (n = 5) is how the continuous corner is approximated: the
// curvature eases in rather than switching from straight to arc.
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

// The shadow Apple's template puts under the body.
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 20, color: color(0x000000, 0.28))
ctx.addPath(shape)
ctx.setFillColor(color(0xccccff))
ctx.fillPath()
ctx.restoreGState()

// The ground, lit a little from above, and the painter's grid over it.
ctx.saveGState()
ctx.addPath(shape)
ctx.clip()
let gradient = CGGradient(colorsSpace: space,
                          colors: [color(0xdadaff), color(0xc4c4fb)] as CFArray,
                          locations: [0, 1])!
ctx.drawLinearGradient(gradient, start: CGPoint(x: 0, y: inset + body),
                       end: CGPoint(x: 0, y: inset), options: [])
ctx.setStrokeColor(color(0xb4b4f6))
ctx.setLineWidth(3)
let step = body / 12
var p = inset + step
while p < inset + body {
    ctx.move(to: CGPoint(x: p, y: inset)); ctx.addLine(to: CGPoint(x: p, y: inset + body))
    ctx.move(to: CGPoint(x: inset, y: p)); ctx.addLine(to: CGPoint(x: inset + body, y: p))
    p += step
}
ctx.strokePath()
// A hairline of light just inside the edge, as the template's bodies have.
ctx.addPath(shape)
ctx.setStrokeColor(color(0xffffff, 0.35))
ctx.setLineWidth(4)
ctx.strokePath()
ctx.restoreGState()

// The cucumber slice, with the shadow of something lying on the ground.
let markURL = URL(fileURLWithPath: CommandLine.arguments[1])
let markSource = CGImageSourceCreateWithURL(markURL as CFURL, nil)!
let mark = CGImageSourceCreateImageAtIndex(markSource, 0, nil)!
let markSize = 540.0
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 24, color: color(0x1c2811, 0.35))
ctx.interpolationQuality = .high
ctx.draw(mark, in: CGRect(x: (size - markSize) / 2, y: (size - markSize) / 2 - 6,
                          width: markSize, height: markSize))
ctx.restoreGState()

let image = ctx.makeImage()!
let out = URL(fileURLWithPath: CommandLine.arguments[2])
let dest = CGImageDestinationCreateWithURL(out as CFURL, "public.png" as CFString, 1, nil)!
CGImageDestinationAddImage(dest, image, nil)
CGImageDestinationFinalize(dest)
