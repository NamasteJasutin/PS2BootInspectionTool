import CoreGraphics
import Foundation
import ImageIO
import Metal
import PS2Kit
import UniformTypeIdentifiers

/// `BootScreen --render <frame> <out.png> --bios <file> [--card <file> | --titles N --launches N]
///  [--free x,y,z,yaw,pitch] [--disc seconds] [--no-<layer>]...` — renders one frame without a window.
enum OfflineRender {
    static func run(_ args: [String]) -> Int32 {
        var positional: [String] = []
        var named: [String: String] = [:]
        var options = RenderOptions()
        var i = 0
        while i < args.count {
            let a = args[i]
            if a.hasPrefix("--no-") {
                switch a.dropFirst(5) {
                case "towers": options.towers = false
                case "trails": options.trails = false
                case "fog": options.fog = false
                case "orbs": options.orbs = false
                case "glass": options.glass = false
                case "defocus": options.defocus = false
                case "fade": options.fade = false
                case "lettering": options.lettering = false
                case "letterbox": options.letterbox = false
                default: return fail("unknown layer \(a)")
                }
            } else if a == "--wrap" {
                options.colourWrap = true
            } else if a == "--show-path" {
                options.cameraPath = true
            } else if a == "--pal" {
                named["video"] = "pal"
            } else if a.hasPrefix("--"), i + 1 < args.count {
                named[String(a.dropFirst(2))] = args[i + 1]
                i += 1
            } else {
                positional.append(a)
            }
            i += 1
        }
        guard positional.count == 2, let frame = Float(positional[0]), let bios = named["bios"] else {
            return fail("usage: BootScreen --render <frame> <out.png> --bios <file> [--card <file> | --titles N --launches N]")
        }
        do {
            let assets = try OpeningAssets(biosURL: URL(fileURLWithPath: bios))
            var history = PlayHistory()
            if let card = named["card"] {
                history = try PlayHistory(card: try MemoryCard(url: URL(fileURLWithPath: card)))
            } else if let titles = named["titles"].flatMap({ Int($0) }) {
                history = .synthetic(launches: Array(repeating: named["launches"].flatMap { Int($0) } ?? 30, count: titles))
            }
            guard let device = MTLCreateSystemDefaultDevice() else { return fail("no Metal device") }
            let renderer = try Renderer(device: device)
            renderer.setAssets(assets)
            var free: ViewCamera?
            if let f = named["free"]?.split(separator: ",").compactMap({ Float($0) }), f.count == 5 {
                free = FreeCamera(position: SIMD3(f[0], f[1], f[2]), yaw: f[3], pitch: f[4]).view
            }
            let video: VideoMode = named["video"] == "pal" ? .pal : .ntsc
            let fps = video.framesPerSecond
            let timeline = named["scene"] == "warning"
                ? Timeline.warning(exitFrame: Int((named["exit"].flatMap { Float($0) } ?? 10) * fps), video: video)
                : Timeline(discSettledFrame: Int((named["disc"].flatMap { Float($0) } ?? 0) * fps), video: video)
            guard let cb = renderer.queue.makeCommandBuffer() else { return fail("no command buffer") }
            let scene = OpeningScene(assets: assets, history: history)
            renderer.render(frame: frame, scene: scene, timeline: timeline, freeCamera: free, options: options, commandBuffer: cb)
            cb.commit()
            cb.waitUntilCompleted()
            guard let image = renderer.readFrame() else { return fail("read-back failed") }
            try writePNG(image.rgba, width: image.width, height: image.height, to: URL(fileURLWithPath: positional[1]))
            let c = timeline.camera(at: frame)
            print("frame \(frame): camera z \(c.z), \(scene.towers.count) towers, scene ends at frame \(timeline.endFrame)")
            return 0
        } catch {
            return fail("error: \(error)")
        }
    }

    static func chime(bios: String, out: String, diveFrame: Int) -> Int32 {
        do {
            let sound = try BootSound(biosURL: URL(fileURLWithPath: bios))
            let pcm = sound.mixed(diveFrame: diveFrame)
            try writeWAV(pcm, sampleRate: sound.sampleRate, to: URL(fileURLWithPath: out))
            let peak = pcm.map(abs).max() ?? 0
            print("\(pcm.count / 2) frames (\(Double(pcm.count / 2) / Double(sound.sampleRate)) s), peak \(peak)")
            return 0
        } catch {
            return fail("error: \(error)")
        }
    }

    private static func writeWAV(_ pcm: [Float], sampleRate: Int, to url: URL) throws {
        var data = Data()
        func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { data.append(contentsOf: $0) } }
        func u16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { data.append(contentsOf: $0) } }
        let bytes = pcm.count * 2
        data.append(contentsOf: Array("RIFF".utf8)); u32(UInt32(36 + bytes)); data.append(contentsOf: Array("WAVE".utf8))
        data.append(contentsOf: Array("fmt ".utf8)); u32(16); u16(1); u16(2); u32(UInt32(sampleRate)); u32(UInt32(sampleRate * 4)); u16(4); u16(16)
        data.append(contentsOf: Array("data".utf8)); u32(UInt32(bytes))
        var samples = [Int16](repeating: 0, count: pcm.count)
        for (i, v) in pcm.enumerated() { samples[i] = Int16(max(-32768, min(32767, (v * 32767).rounded()))) }
        samples.withUnsafeBytes { data.append(contentsOf: $0) }
        try data.write(to: url)
    }

    private static func fail(_ message: String) -> Int32 {
        FileHandle.standardError.write(Data((message + "\n").utf8))
        return 1
    }

    private static func writePNG(_ rgba: [UInt8], width: Int, height: Int, to url: URL) throws {
        let provider = CGDataProvider(data: Data(rgba) as CFData)!
        let image = CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32, bytesPerRow: width * 4,
                            space: CGColorSpace(name: CGColorSpace.sRGB)!,
                            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
                            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
        guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else {
            throw CocoaError(.fileWriteUnknown)
        }
        CGImageDestinationAddImage(dest, image, nil)
        guard CGImageDestinationFinalize(dest) else { throw CocoaError(.fileWriteUnknown) }
    }
}
