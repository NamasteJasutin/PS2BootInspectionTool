import Foundation
import simd

/// `rom0:PS2LOGO` — the "PlayStation 2" logo program run before a disc boots
/// (see `notes/ps2logo.md`). Everything here is read from the user's BIOS; the lettering
/// bitmap itself lives on the game disc (`DiscLogo`).
public struct LogoAssets {
    public struct Node {
        public let type: Int              // 0 move, 1 line, 2 cubic, 3 end
        public var f: [Float]             // 6 floats: control points / end point
    }
    public typealias Shape = [[Node]]      // polylines of nodes

    public struct Object {
        public let type: Int               // 0 = line strips, 1 = ribbon
        public let layerA: Bool, layerB: Bool
        public let keys: [(t: Int, shape: Shape)]
        public let colours: [(t: Int, r: Float, g: Float, b: Float, a: Float)]
    }

    public struct SoundEffect {
        public let volL: Int, volR: Int, pitch: Int
        public let adsr1: UInt16, adsr2: UInt16
        public let reverb: Bool
        public let sampleOffset: Int
    }

    public let objects: [VideoMode: [Object]]       // index 0...3
    public let ribbonMultipliers: [Float]
    public let ribbonDelta: [Float]                 // per object 0...3
    public let ribbonRate: [VideoMode: Float]
    public let palYScale: Float
    public let effects: [SoundEffect]
    public let effectMasterVolume: Int
    public let sampleBody: Data

    static let base = 0x100000

    public init(biosURL: URL) throws {
        let rom = try ROMDirectory(data: try Data(contentsOf: biosURL))
        let module = try rom.module("PS2LOGO")
        let img = try OSDCompression.unpack(module, at: 0x1000)
        try self.init(image: img)
    }

    public init(image img: Data) throws {
        func at(_ a: Int) -> Int { a - Self.base }
        guard img.count >= 0x30B00 else { throw BIOSError.corrupt("PS2LOGO image too short") }
        func u32(_ a: Int) -> Int { Int(img.u32(at(a))) }
        func i32(_ a: Int) -> Int { Int(img.i32(at(a))) }
        func f32(_ a: Int) -> Float { img.f32(at(a)) }

        func shape(_ ptr: Int, _ count: Int) -> Shape {
            (0 ..< count).map { j in
                var a = u32(ptr + 4 * j), nodes: [Node] = []
                while true {
                    let ty = i32(a + 0x20)
                    var f = (0 ..< 6).map { f32(a + 4 * $0) }
                    // One-time fix-up at init: authoring canvas -> centred coordinates.
                    if ty == 0 || ty == 1 { f[0] -= 128; f[1] += 128 }
                    if ty == 2 { for i in stride(from: 0, to: 6, by: 2) { f[i] -= 128; f[i + 1] += 128 } }
                    nodes.append(Node(type: ty, f: f))
                    a += 0x30
                    if ty == 3 || nodes.count > 4096 { break }
                }
                return nodes
            }
        }
        var objs: [VideoMode: [Object]] = [:]
        for (mode, shapeTable, colourTable) in [(VideoMode.ntsc, 0x12AFF8, 0x12B538), (.pal, 0x12AFD8, 0x12B518)] {
            objs[mode] = (0 ..< 4).map { o in
                let kp = u32(shapeTable + o * 8), kn = u32(shapeTable + o * 8 + 4)
                let keys = (0 ..< min(kn, 16)).map { k in
                    (t: i32(kp + k * 12), shape: shape(u32(kp + k * 12 + 4), i32(kp + k * 12 + 8)))
                }
                let cp = u32(colourTable + o * 8), cn = u32(colourTable + o * 8 + 4)
                let colours = (0 ..< min(cn, 32)).map { k in
                    (t: i32(cp + k * 20), r: Float(i32(cp + k * 20 + 4)), g: Float(i32(cp + k * 20 + 8)),
                     b: Float(i32(cp + k * 20 + 12)), a: Float(i32(cp + k * 20 + 16)))
                }
                return Object(type: u32(0x121770 + 4 * (3 - o)), layerA: u32(0x121790 + 4 * (3 - o)) != 0,
                              layerB: u32(0x1217B0 + 4 * (3 - o)) != 0, keys: keys, colours: colours)
            }
        }
        objects = objs
        ribbonMultipliers = (0 ..< 5).map { f32(0x12F228 + 4 * $0) }
        ribbonDelta = [f32(0x130A28), f32(0x130A24), f32(0x130A24), 0]
        ribbonRate = [.ntsc: f32(0x130A34), .pal: f32(0x130A38)]
        palYScale = f32(0x130A2C) / f32(0x130A30)

        // Embedded sound bank: an SShd header with only a sound-effect table, one sample.
        let hd = 0x12B5C0
        let se = hd + u32(hd + 0x2C)
        effectMasterVolume = u32(se)
        let last = u32(se + 4)
        effects = (0 ... min(last, 15)).map { i in
            let e = at(se + 0x20 + i * 0x40)
            return SoundEffect(volL: Int(img.u16(e)), volR: Int(img.u16(e + 2)), pitch: Int(img.u16(e + 4)),
                               adsr1: img.u16(e + 8), adsr2: img.u16(e + 10), reverb: img.u16(e + 14) & 0x80 != 0,
                               sampleOffset: Int(img.u32(e + 16)))
        }
        sampleBody = img.bytes(at(0x12B7C0), 0x2C90)
    }

    /// The five-voice chime (`cmd 0x5200` for effects 0...4), 48 kHz interleaved stereo,
    /// with the SE master volume OSDSYS/PS2LOGO sets (18).
    public func chime(masterVolume: Int = 0x12) -> [Float] {
        var mix: [Float] = []
        for e in effects.prefix(5) {
            let (pcm, _) = SoundBank.decodeADPCM(sampleBody, at: e.sampleOffset)
            guard pcm.count > 1 else { continue }
            let step = Double(e.pitch * 44100 / 48000) / 4096
            let n = Int(Double(pcm.count) / step) + 1
            let env = SequenceSynth.envelope(adsr1: e.adsr1, adsr2: e.adsr2, onSamples: n, total: n)
            let master = Float(0x3FFF) / 16384
            let gl = Float((e.volL * masterVolume) >> 7) / 16384 * master
            let gr = Float((e.volR * masterVolume) >> 7) / 16384 * master
            if mix.count < n * 2 { mix.append(contentsOf: [Float](repeating: 0, count: n * 2 - mix.count)) }
            var pos = 0.0
            for k in 0 ..< n {
                let i0 = Int(pos), i1 = i0 + 1
                guard i1 < pcm.count else { break }
                let f = Float(pos - Double(i0))
                let s = (pcm[i0] * (1 - f) + pcm[i1] * f) * env[k] / 32768 / 32768
                mix[k * 2] += s * gl
                mix[k * 2 + 1] += s * gr
                pos += step
            }
        }
        return mix
    }
}

/// Evaluates the keyframed vector objects of the logo animation.
public struct LogoAnimation {
    public let assets: LogoAssets
    public let video: VideoMode
    public var objects: [LogoAssets.Object] { assets.objects[video] ?? [] }

    public init(assets: LogoAssets, video: VideoMode) { self.assets = assets; self.video = video }

    /// First field index the program renders, the last animated field, and the hold afterwards.
    public var firstField: Int { video == .pal ? 14 : 17 }
    public var lastField: Int { video == .pal ? 35 : 42 }
    public static let holdFields = 120

    public func logoBlurIterations(field t: Int) -> Int {
        video == .pal ? max(0, min(240, (28 - t) * 240 / 28)) : max(0, min(240, (33 - t) * 240 / 33))
    }
    public func screenBlurActive(field t: Int) -> Bool { video == .pal ? t < 29 : t < 34 }
    public func feedbackAlpha(field t: Int) -> Int {
        if video == .pal { return t <= 21 ? 112 : max(0, min(112, (34 - t) * 112 / 12)) }
        return t <= 25 ? 112 : max(0, min(112, (41 - t) * 112 / 15))
    }
    public var logoRect: (x: Float, y: Float, w: Float, h: Float) {
        video == .pal ? (149.5, 216.5, 384, 77) : (149.5, 223.5, 384, 64)
    }

    static func keyPair<T>(_ keys: [T], time: (T) -> Int, t: Int) -> (Int, Int, Float) {
        var i = 0
        for (k, key) in keys.enumerated() where time(key) <= t { i = k }
        if i >= keys.count - 1 { return (i, i, 1) }
        let t0 = time(keys[i]), t1 = time(keys[i + 1])
        return (i, i + 1, Float(t1 - t) / Float(t1 - t0))
    }

    public func shape(of obj: LogoAssets.Object, field t: Int) -> LogoAssets.Shape {
        let (i, j, w) = Self.keyPair(obj.keys, time: { $0.t }, t: max(t, 0))
        if i == j { return obj.keys[i].shape }
        let a = obj.keys[i].shape, b = obj.keys[j].shape
        return zip(a, b).map { pa, pb in
            zip(pa, pb).map { na, nb in LogoAssets.Node(type: na.type, f: zip(na.f, nb.f).map { $0 * w + $1 * (1 - w) }) }
        }
    }

    /// Drawn colour (0...255 per channel) of an object at field `t`: rgb · min(a, 255) / 128.
    public func colour(of obj: LogoAssets.Object, field t: Int) -> SIMD3<Float> {
        let (i, j, w) = Self.keyPair(obj.colours, time: { $0.t }, t: t)
        let ci = obj.colours[i], cj = obj.colours[j]
        let a = min(255, ci.a * w + cj.a * (1 - w))
        let rgb = SIMD3(ci.r, ci.g, ci.b) * w + SIMD3(cj.r, cj.g, cj.b) * (1 - w)
        return simd_min(rgb * a / 128, SIMD3(repeating: 255))
    }

    /// Subdivision count of one cubic: 4 steps, or a straight line for very bent curves.
    static func subdivisions(from p0: SIMD2<Float>, _ f: [Float]) -> Int {
        let c1 = SIMD2(f[0], f[1]), c2 = SIMD2(f[2], f[3]), p3 = SIMD2(f[4], f[5])
        let chord = simd_length(p3 - p0)
        if chord == 0 { return 0 }
        let m01 = (p0 + c1) / 2, m12 = (c1 + c2) / 2, m23 = (c2 + p3) / 2
        let m012 = (m01 + m12) / 2, m123 = (m12 + m23) / 2, mid = (m012 + m123) / 2
        let pts = [p0, m01, m012, mid, m123, m23, p3]
        let polygon = Float(Int((0 ..< 6).reduce(Float(0)) { $0 + simd_length(pts[$1 + 1] - pts[$1]) }))
        return (chord * 3.5 < polygon || chord > 2000) ? 1 : 4
    }

    /// Flattens a shape into point lists. `counts` reuses subdivision counts (ribbons);
    /// without it, samples closer than 1 px are dropped (line strips).
    public func flatten(_ shape: LogoAssets.Shape, counts: [Int]? = nil) -> [[SIMD2<Float>]] {
        var ci = 0
        return shape.map { nodes in
            var pts: [SIMD2<Float>] = []
            var prev = SIMD2<Float>(0, 0)
            for n in nodes {
                switch n.type {
                case 0: prev = SIMD2(n.f[0], n.f[1]); pts.append(prev)
                case 1: pts.append(prev); prev = SIMD2(n.f[0], n.f[1])
                case 2:
                    pts.append(prev)
                    let steps: Int
                    if let counts { steps = ci < counts.count ? counts[ci] : 4; ci += 1 } else { steps = Self.subdivisions(from: prev, n.f) }
                    let c1 = SIMD2(n.f[0], n.f[1]), c2 = SIMD2(n.f[2], n.f[3]), p3 = SIMD2(n.f[4], n.f[5])
                    var last = prev, acc: Float = 0
                    if steps > 1 {
                        for i in 1 ..< steps {
                            let s = Float(i) / Float(steps), u = 1 - s
                            let p = prev * (u * u * u) + c1 * (3 * u * u * s) + c2 * (3 * u * s * s) + p3 * (s * s * s)
                            if counts != nil { pts.append(p) } else {
                                acc += simd_length(p - last)
                                if acc > 1 { pts.append(p); acc = 0 }
                            }
                            last = p
                        }
                    }
                    prev = p3
                case 3: pts.append(prev)
                default: break
                }
            }
            return pts
        }
    }

    public func segmentCounts(of obj: LogoAssets.Object, field t: Int) -> [Int] {
        var counts: [Int] = []
        for pl in shape(of: obj, field: t) {
            var prev = SIMD2<Float>(0, 0)
            for n in pl {
                if n.type == 0 || n.type == 1 { prev = SIMD2(n.f[0], n.f[1]) }
                if n.type == 2 { counts.append(Self.subdivisions(from: prev, n.f)); prev = SIMD2(n.f[4], n.f[5]) }
            }
        }
        return counts
    }

    /// Centred coordinates -> 640x512 buffer coordinates.
    public func toScreen(_ p: SIMD2<Float>) -> SIMD2<Float> {
        SIMD2(p.x + 320, 256 - (video == .pal ? p.y * assets.palYScale : p.y))
    }

    /// Field offsets of the five ribbon copies for object `index` (oldest first).
    public func ribbonOffsets(object index: Int) -> [Int] {
        let d = assets.ribbonDelta[index], r = assets.ribbonRate[video] ?? 1
        return (0 ... 4).map { Int(d * Float(4 - $0) * r) }
    }
}

/// The lettering bitmap every licensed disc carries in its first 12 sectors.
public struct DiscLogo {
    public let width: Int, height: Int
    public let pixels: [UInt8]           // row-major 8-bit grey
    public let region: String?           // "E" or "J" when the checksum matched

    /// Reads sectors 0-11 of a plain ISO image and descrambles them as the drive does.
    public init(discImageURL url: URL) throws {
        let raw = try SectorReader(url: url).read(lba: 0, count: 12)
        guard raw.count == 12 * 2048 else { throw BIOSError.corrupt("disc image is too short for the logo sectors") }
        let key = raw[raw.startIndex]
        var out = [UInt8](repeating: 0, count: raw.count)
        for (i, b) in raw.enumerated() {
            let x = b ^ key
            out[i] = (x << 3) | (x >> 5)
        }
        var sum: UInt32 = 0
        for i in stride(from: 0, to: out.count, by: 4) {
            sum &+= UInt32(out[i]) | UInt32(out[i + 1]) << 8 | UInt32(out[i + 2]) << 16 | UInt32(out[i + 3]) << 24
        }
        region = sum == 0x7813_4705 ? "E" : (sum == 0x62DB_1E66 ? "J" : nil)
        pixels = out
        width = 384
        height = 64
    }

    /// The image laid out for a video mode: 384x64 (NTSC) or the 344x71 PAL picture in a
    /// 384x77 frame starting at row 3.
    public func bitmap(for video: VideoMode) -> (width: Int, height: Int, grey: [UInt8]) {
        if video == .pal {
            var out = [UInt8](repeating: 0, count: 384 * 77)
            for r in 0 ..< 71 { for c in 0 ..< 344 { out[(r + 3) * 384 + c] = pixels[r * 344 + c] } }
            return (384, 77, out)
        }
        return (384, 64, Array(pixels.prefix(384 * 64)))
    }

    /// Fallback when no disc is available: the lettering outline from the BIOS, filled.
    public static func synthesised(from animation: LogoAnimation) -> (width: Int, height: Int, grey: [UInt8]) {
        let rect = animation.logoRect
        let w = 384, h = Int(rect.h)
        var out = [UInt8](repeating: 0, count: w * h)
        guard let outline = animation.objects.indices.contains(3) ? animation.objects[3].keys.last?.shape : nil else { return (w, h, out) }
        let polys = animation.flatten(outline).map { $0.map { animation.toScreen($0) - SIMD2(rect.x, rect.y) } }
        let sub = 4
        for py in 0 ..< h {
            var coverage = [Float](repeating: 0, count: w)
            for s in 0 ..< sub {
                let y = Float(py) + (Float(s) + 0.5) / Float(sub)
                var xs: [Float] = []
                for poly in polys where poly.count > 2 {
                    for i in 0 ..< poly.count {
                        let a = poly[i], b = poly[(i + 1) % poly.count]
                        if (a.y <= y) != (b.y <= y) { xs.append(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x)) }
                    }
                }
                xs.sort()
                for k in stride(from: 0, to: xs.count - 1, by: 2) {
                    let x0 = max(0, xs[k]), x1 = min(Float(w), xs[k + 1])
                    guard x1 > x0 else { continue }
                    for px in Int(x0) ... min(w - 1, Int(x1)) {
                        coverage[px] += max(0, min(Float(px + 1), x1) - max(Float(px), x0)) / Float(sub)
                    }
                }
            }
            for px in 0 ..< w { out[py * w + px] = UInt8(min(255, coverage[px] * 255)) }
        }
        return (w, h, out)
    }
}
