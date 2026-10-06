import Foundation
import Metal
import PS2Kit
import simd

/// What to draw; every stage of the console's frame can be switched off for study.
struct RenderOptions: Equatable {
    var towers = true
    var trails = true           // blend with previous frames
    var fog = true
    var orbs = true
    var glass = true
    var defocus = true
    var fade = true
    var lettering = true
    var letterbox = true
    /// Let tower-cap colours overflow 8 bits instead of saturating (see notes, open question).
    var colourWrap = false
    /// Draw the scripted camera's route and its current frustum (best seen with the free camera).
    var cameraPath = false
    var orbSeed: Float = 4000
    /// Texture name of the warning text in the current language (e.g. TEXOPNGE).
    var warningTexture: String? = "TEXOPNGE"
}

/// A camera in the opening's world: +Z is into the screen, +Y is down the screen.
struct ViewCamera: Equatable {
    var position: SIMD3<Float>
    var forward: SIMD3<Float>
    var up: SIMD3<Float>

    init(position: SIMD3<Float>, forward: SIMD3<Float>, up: SIMD3<Float>) {
        self.position = position; self.forward = forward; self.up = up
    }

    init(_ s: CameraState) {
        self.init(position: s.position, forward: s.forward, up: s.up)
    }

    /// Screen-right, screen-down and view axes (same construction as the console's camera matrix).
    var basis: (x: SIMD3<Float>, y: SIMD3<Float>, z: SIMD3<Float>) {
        let z = normalize(forward)
        let x = normalize(cross(up, z))
        return (x, cross(z, x), z)
    }

    /// Clip-space position. Scale factors reproduce the console projection (screen distance
    /// 1024 on a 640x224 field with a 0.4576 line aspect) shown on a 4:3 picture.
    func project(_ p: SIMD3<Float>) -> SIMD4<Float> {
        let b = basis, d = p - position
        let v = SIMD3(dot(d, b.x), dot(d, b.y), dot(d, b.z))
        let near: Float = 1, far: Float = 4000
        return SIMD4(v.x * 3.2, -v.y * (1024 * 0.4576 / 112), (v.z - near) * far / (far - near), v.z)
    }
}

struct Vertex {
    var pos: SIMD4<Float>
    var uv: SIMD2<Float>
    var color: SIMD4<Float>
}

struct GlassVertex {
    var pos: SIMD4<Float>
    var a: SIMD4<Float>      // refraction offset (uv), reflection uv
    var b: SIMD4<Float>      // noise uv, magnification, rim
    var c: SIMD4<Float>      // projected cube centre (uv), reflection strength
    var tint: SIMD4<Float>   // multiplies the refracted frame
    var reflTint: SIMD4<Float>
}

enum Blend: Hashable { case opaque, add, addSrcAlpha, alpha }
enum DepthMode: Hashable { case none, test, write }

struct Batch {
    var texture: String
    var blend: Blend
    var depth: DepthMode = .none
    var repeatUV = false
    var range: Range<Int>
}

final class Renderer {
    let device: MTLDevice
    let queue: MTLCommandQueue
    private let library: MTLLibrary
    private var pipelines: [String: MTLRenderPipelineState] = [:]
    private var depthStates: [DepthMode: MTLDepthStencilState] = [:]
    private let samplerClamp: MTLSamplerState
    private let samplerRepeat: MTLSamplerState
    var textures: [String: MTLTexture] = [:]
    var targets: [String: MTLTexture] = [:]
    private(set) var size = (width: 1280, height: 960)

    private(set) var assets: OpeningAssets?

    init(device: MTLDevice) throws {
        self.device = device
        queue = device.makeCommandQueue()!
        library = try device.makeLibrary(source: Self.shaderSource, options: nil)

        let sd = MTLSamplerDescriptor()
        sd.minFilter = .linear; sd.magFilter = .linear; sd.mipFilter = .linear
        samplerClamp = device.makeSamplerState(descriptor: sd)!
        sd.sAddressMode = .repeat; sd.tAddressMode = .repeat
        samplerRepeat = device.makeSamplerState(descriptor: sd)!

        for mode in [DepthMode.none, .test, .write] {
            let d = MTLDepthStencilDescriptor()
            d.depthCompareFunction = mode == .none ? .always : .lessEqual
            d.isDepthWriteEnabled = mode == .write
            depthStates[mode] = device.makeDepthStencilState(descriptor: d)
        }
        textures["white"] = makeTexture(width: 1, height: 1, rgba: [255, 255, 255, 255], mips: false)
        resize(width: size.width, height: size.height)
    }

    func setAssets(_ a: OpeningAssets) {
        assets = a
        for t in a.textures.values {
            textures[t.name] = makeTexture(width: t.width, height: t.height, rgba: t.rgba, mips: t.mipLevels > 0)
        }
    }

    func resize(width: Int, height: Int) {
        size = (width, height)
        func target(_ name: String, _ format: MTLPixelFormat) {
            let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: format, width: width, height: height, mipmapped: false)
            d.usage = [.renderTarget, .shaderRead]
            d.storageMode = .private
            targets[name] = device.makeTexture(descriptor: d)
        }
        target("sub", .rgba8Unorm)
        target("accum", .rgba16Float)
        target("scene", .rgba8Unorm)
        target("copy", .rgba8Unorm)
        target("temp", .rgba8Unorm)
        target("depth", .depth32Float)
    }

    private func makeTexture(width: Int, height: Int, rgba: [UInt8], mips: Bool) -> MTLTexture {
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: width, height: height, mipmapped: mips)
        d.usage = mips ? [.shaderRead, .renderTarget] : [.shaderRead]
        let t = device.makeTexture(descriptor: d)!
        rgba.withUnsafeBytes {
            t.replace(region: MTLRegionMake2D(0, 0, width, height), mipmapLevel: 0, withBytes: $0.baseAddress!, bytesPerRow: width * 4)
        }
        if mips, let cb = queue.makeCommandBuffer(), let blit = cb.makeBlitCommandEncoder() {
            blit.generateMipmaps(for: t)
            blit.endEncoding()
            cb.commit()
        }
        return t
    }

    // MARK: - Frame

    /// Renders one frame of the opening into the "scene" target and returns it.
    @discardableResult
    func render(frame: Float, scene: OpeningScene, timeline: Timeline, freeCamera: ViewCamera?,
                options: RenderOptions, commandBuffer cb: MTLCommandBuffer) -> MTLTexture {
        if timeline.kind == .warning {
            return renderWarning(frame: frame, timeline: timeline, freeCamera: freeCamera, options: options, commandBuffer: cb)
        }
        let scripted = timeline.camera(at: frame)
        let camera = freeCamera ?? ViewCamera(scripted)
        func cameraAt(_ f: Float) -> ViewCamera { freeCamera ?? ViewCamera(timeline.camera(at: f)) }
        let sceneTarget = targets["scene"]!

        // 1. Towers, blended over the previous frames: frame = 0.375 * current + 0.625 * previous.
        let history = options.trails ? 12 : 1
        var first = true
        for k in stride(from: history - 1, through: 0, by: -1) {
            let f = frame - Float(k)
            if f < 0 && k != 0 { continue }
            var verts: [Vertex] = []
            if options.towers, f >= 0 { appendTowers(&verts, scene: scene, frame: f, camera: cameraAt(f), wrap: options.colourWrap) }
            pass(cb, color: "sub", clear: true, depth: .clear) { enc in
                self.draw(enc, verts, [Batch(texture: "TEXOWAL0", blend: .opaque, depth: .write, repeatUV: true, range: 0 ..< verts.count)],
                          format: .rgba8Unorm, hasDepth: true)
            }
            let weight: Float = history == 1 ? 1 : (k == history - 1 ? pow(0.625, Float(k)) : 0.375 * pow(0.625, Float(k)))
            var quad: [Vertex] = []
            appendQuad(&quad, color: SIMD4(repeating: weight))
            pass(cb, color: "accum", clear: first, depth: .none) { enc in
                self.draw(enc, quad, [Batch(texture: "@sub", blend: .add, range: 0 ..< 6)], format: .rgba16Float, hasDepth: false)
            }
            first = false
        }

        // 2. Composite, fog, orbs. Depth comes from the current frame's towers.
        var verts: [Vertex] = []
        var batches: [Batch] = []
        func batch(_ texture: String, _ blend: Blend, depth: DepthMode = .none, repeatUV: Bool = false, _ build: (inout [Vertex]) -> Void) {
            let start = verts.count
            build(&verts)
            if verts.count > start { batches.append(Batch(texture: texture, blend: blend, depth: depth, repeatUV: repeatUV, range: start ..< verts.count)) }
        }
        batch("@accum", .opaque) { self.appendQuad(&$0) }
        if options.fog {
            let fogTextures = ["TEXOFOG4", "TEXOFOG2", "TEXOFOG1", "TEXOFOG4", "TEXOFOG2", "TEXOFOG1"]
            for i in 0 ..< 6 {
                batch(fogTextures[i], .add, depth: .test, repeatUV: true) { self.appendFogLayer(&$0, layer: i, frame: frame, camera: camera) }
            }
        }
        let nearObjects = scripted.z < 73
        if options.orbs, nearObjects, let colours = assets?.orbColours {
            batch("TEXOCRBL", .addSrcAlpha) { self.appendOrbs(&$0, frame: frame, seed: options.orbSeed, colours: colours, camera: camera) }
            batch("white", .addSrcAlpha) {
                self.appendOrbTrails(&$0, frame: frame, seed: options.orbSeed, colours: colours, cameraAt: cameraAt)
            }
        }
        pass(cb, color: "scene", clear: true, depth: .load) { enc in
            self.draw(enc, verts, batches, format: .rgba8Unorm, hasDepth: true)
        }

        // 3. Glass cubes: back faces into a copy of the frame, front faces refract that copy.
        if options.glass, nearObjects, let positions = assets?.cubePositions {
            blit(cb, from: "scene", to: "copy")
            for front in [false, true] {
                var gv: [GlassVertex] = []
                for (i, p) in positions.enumerated() {
                    appendGlassCube(&gv, centre: SIMD3(p.x * 3.5, p.y * 3.5, p.z * -15 + 150),
                                    rotation: OpeningMotion.cubeRotation(i, frame: frame), half: 1.8,
                                    tint: SIMD3(112, 112, 152) / 128, reflectTint: SIMD3(repeating: 1), refraction: 1,
                                    reflection: (0.25, 0.5), camera: camera, front: front)
                }
                guard !gv.isEmpty else { continue }
                pass(cb, color: front ? "scene" : "copy", clear: false, depth: .none) { enc in
                    self.drawGlass(enc, gv, source: front ? "copy" : "scene")
                }
            }
        }

        // 4. Defocus: squeeze the picture into its top-left corner and stretch it back.
        if options.defocus {
            let n = OpeningMotion.defocusPasses(z: scripted.z)
            for i in 0 ..< n {
                let shrink = Float(i * (n - 1))
                let fx = (640 * 7 / 8 - 1 - shrink) / 640, fy = (224 * 7 / 8 - 1 - shrink) / 224
                var down: [Vertex] = [], up: [Vertex] = []
                appendQuad(&down, x0: -1, y0: 1 - 2 * fy, x1: -1 + 2 * fx, y1: 1)
                appendQuad(&up, u0: 0, v0: 0, u1: fx, v1: fy)
                pass(cb, color: "temp", clear: true, depth: .none) { enc in
                    self.draw(enc, down, [Batch(texture: "@scene", blend: .opaque, range: 0 ..< 6)], format: .rgba8Unorm, hasDepth: false)
                }
                pass(cb, color: "scene", clear: false, depth: .none) { enc in
                    self.draw(enc, up, [Batch(texture: "@temp", blend: .opaque, range: 0 ..< 6)], format: .rgba8Unorm, hasDepth: false)
                }
            }
        }

        // 5. Overlays: fade, lettering, letterbox.
        verts = []; batches = []
        if options.fade {
            let a = frame < 2 ? 1 : OpeningMotion.fadeAlpha(z: scripted.z)
            if a > 0 { batch("white", .alpha) { self.appendQuad(&$0, color: SIMD4(0, 0, 0, a)) } }
        }
        if options.lettering {
            let a = OpeningMotion.letteringAlpha(framesSinceTrigger: frame - Float(timeline.frame(passing: 18)))
            if a > 0 {
                batch("TEXOSCE", .alpha) {
                    self.appendSprite(&$0, x: 120, y: 105, w: 256, h: 16, u: 0, v: 1, uw: 256, vh: 30, texture: (256, 64), alpha: a)
                    self.appendSprite(&$0, x: 326, y: 105, w: 256, h: 16, u: 0, v: 33, uw: 256, vh: 30, texture: (256, 64), alpha: a)
                }
            }
        }
        if options.letterbox {
            batch("white", .opaque) {
                self.appendSprite(&$0, x: 0, y: 0, w: 640, h: 30, u: 0, v: 0, uw: 1, vh: 1, texture: (1, 1), alpha: 1, rgb: 0)
                self.appendSprite(&$0, x: 0, y: 194, w: 640, h: 30, u: 0, v: 0, uw: 1, vh: 1, texture: (1, 1), alpha: 1, rgb: 0)
            }
        }
        if !batches.isEmpty {
            pass(cb, color: "scene", clear: false, depth: .none) { enc in
                self.draw(enc, verts, batches, format: .rgba8Unorm, hasDepth: false)
            }
        }
        // 6. Study aid: the scripted camera's route, depth-tested against the towers.
        if options.cameraPath {
            var lines: [Vertex] = []
            appendCameraPath(&lines, timeline: timeline, frame: frame, camera: camera, showFrustum: freeCamera != nil)
            pass(cb, color: "scene", clear: false, depth: .load) { enc in
                self.draw(enc, lines, [Batch(texture: "white", blend: .alpha, depth: .test, range: 0 ..< lines.count)],
                          format: .rgba8Unorm, hasDepth: true)
            }
        }
        return sceneTarget
    }

    /// Draws the finished frame into `target` (e.g. a drawable), letterboxed to 4:3.
    func present(_ cb: MTLCommandBuffer, to target: MTLTexture) {
        let rp = MTLRenderPassDescriptor()
        rp.colorAttachments[0].texture = target
        rp.colorAttachments[0].loadAction = .clear
        rp.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        rp.colorAttachments[0].storeAction = .store
        guard let enc = cb.makeRenderCommandEncoder(descriptor: rp) else { return }
        let tw = Float(target.width), th = Float(target.height)
        let scale = min(tw / 4, th / 3)
        let w = 4 * scale / tw, h = 3 * scale / th
        var quad: [Vertex] = []
        appendQuad(&quad, x0: -w, y0: -h, x1: w, y1: h)
        draw(enc, quad, [Batch(texture: "@scene", blend: .opaque, range: 0 ..< 6)], format: target.pixelFormat, hasDepth: false)
        enc.endEncoding()
    }

    /// Copies the finished frame out of the GPU as RGBA8 (for writing image files).
    func readFrame() -> (width: Int, height: Int, rgba: [UInt8])? {
        guard let src = targets["scene"] else { return nil }
        let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: src.width, height: src.height, mipmapped: false)
        d.storageMode = .shared
        guard let dst = device.makeTexture(descriptor: d), let cb = queue.makeCommandBuffer(),
              let blit = cb.makeBlitCommandEncoder() else { return nil }
        blit.copy(from: src, to: dst)
        blit.endEncoding()
        cb.commit()
        cb.waitUntilCompleted()
        var out = [UInt8](repeating: 0, count: src.width * src.height * 4)
        dst.getBytes(&out, bytesPerRow: src.width * 4, from: MTLRegionMake2D(0, 0, src.width, src.height), mipmapLevel: 0)
        return (src.width, src.height, out)
    }

    // MARK: - Geometry

    private static let towerFaces: [(corners: [(Float, Float, Float)], normal: SIMD3<Float>)] = [
        ([(2, -2, -1), (-2, -2, -1), (2, 2, -1), (-2, 2, -1)], SIMD3(0, 0, 1)),     // near cap
        ([(2, -2, -1), (2, -2, 1), (-2, -2, -1), (-2, -2, 1)], SIMD3(0, 1, 0)),
        ([(2, 2, 1), (2, 2, -1), (-2, 2, 1), (-2, 2, -1)], SIMD3(0, -1, 0)),
        ([(2, -2, 1), (2, 2, 1), (2, -2, -1), (2, 2, -1)], SIMD3(-1, 0, 0)),
        ([(-2, -2, -1), (-2, 2, -1), (-2, -2, 1), (-2, 2, 1)], SIMD3(1, 0, 0)),
    ]
    private static let towerUV: [SIMD2<Float>] = [SIMD2(0.01, 0.01), SIMD2(0.24, 0.01), SIMD2(0.01, 0.24), SIMD2(0.24, 0.24)]
    /// Directions the three lights shine from, with their strengths; ambient is 0.4.
    private static let lights: [(SIMD3<Float>, Float)] = [
        (SIMD3(0, 0, 1), 1.0), (normalize(SIMD3(-0.5, -0.5, 0)), 0.8), (normalize(SIMD3(0.5, 0.5, 0)), 0.8),
    ]

    private func appendTowers(_ out: inout [Vertex], scene: OpeningScene, frame: Float, camera: ViewCamera, wrap: Bool) {
        let sway = OpeningMotion.sway(frame: frame)
        for t in scene.towers {
            let angle = Float(t.quarterTurns) * .pi / 2 + (t.growing ? sway : 0)
            let c = cos(angle), s = sin(angle)
            func rotate(_ v: SIMD3<Float>) -> SIMD3<Float> { SIMD3(v.x * c - v.y * s, v.x * s + v.y * c, v.z) }
            for (fi, face) in Self.towerFaces.enumerated() {
                let n = rotate(face.normal)
                var lit: Float = 0.4
                for (dir, strength) in Self.lights { lit += max(0, dot(n, dir)) * strength }
                var quad: [Vertex] = []
                for (vi, corner) in face.corners.enumerated() {
                    let local = SIMD3(corner.0, corner.1, corner.2 * t.halfLength)
                    let base: Float = corner.2 > 0 ? 0 : (fi == 0 ? t.brightness : t.brightness * 0.8)
                    var value = Int(base * lit)
                    value = wrap ? value & 255 : min(value, 255)
                    let shade = Float(value) / 128
                    quad.append(Vertex(pos: camera.project(t.centre + rotate(local)),
                                       uv: Self.towerUV[vi] + SIMD2(repeating: t.uvOffset),
                                       color: SIMD4(shade, shade, shade, 1)))
                }
                out.append(contentsOf: [quad[0], quad[1], quad[2], quad[2], quad[1], quad[3]])
            }
        }
    }

    private func appendFogLayer(_ out: inout [Vertex], layer: Int, frame: Float, camera: ViewCamera) {
        let radius = Float(5202).squareRoot()
        let z = 134 - 5 * Float(layer)
        let scroll = OpeningMotion.fogScroll(layer, frame: frame)
        func vertex(_ a: Int, _ b: Int) -> Vertex {
            let d = hypotf(-5.1 - (Float(a - 8) * 6 + 3), -(Float(b - 8) * 6 + 3))
            let c = min(127, max(0, (radius - 4 * d) * 96 / radius))
            let k: Float = 0x14 / 128.0 / 128.0      // constant blend factor x modulate scale
            return Vertex(pos: camera.project(SIMD3(Float(a) * 6 - 46, Float(b) * 6 - 48, z)),
                          uv: SIMD2(Float(a) * 0.5 - scroll, Float(b) * 0.5),
                          color: SIMD4(Float(Int(c) / 4) * k, Float(Int(c) * 2 / 5) * k, Float(Int(c)) * k, 1))
        }
        for b in 0 ..< 16 {
            for a in 0 ..< 16 {
                let q = [vertex(a, b), vertex(a + 1, b), vertex(a, b + 1), vertex(a + 1, b + 1)]
                if q.allSatisfy({ $0.color.z == 0 }) { continue }
                out.append(contentsOf: [q[0], q[1], q[2], q[2], q[1], q[3]])
            }
        }
    }

    func appendBillboard(_ out: inout [Vertex], centre: SIMD3<Float>, half: Float, color: SIMD4<Float>, camera: ViewCamera) {
        let b = camera.basis
        let corners: [(Float, Float)] = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
        let q = corners.map { c in
            Vertex(pos: camera.project(centre + b.x * (c.0 * half) + b.y * (c.1 * half)),
                   uv: SIMD2((c.0 + 1) / 2, (c.1 + 1) / 2), color: color)
        }
        guard q.allSatisfy({ $0.pos.w > 1 }) else { return }
        out.append(contentsOf: [q[0], q[1], q[2], q[2], q[1], q[3]])
    }

    private func appendOrbs(_ out: inout [Vertex], frame: Float, seed: Float, colours: [SIMD3<Float>], camera: ViewCamera) {
        for i in 0 ..< 4 {
            for ghost in 1 ... 4 {      // the last four frames, newest brightest
                let f = frame - Float(4 - ghost)
                guard f >= 0 else { continue }
                let p = OpeningMotion.orbPosition(i, frame: f, seed: seed)
                let rgb = colours[i] * 0.5 / 128
                appendBillboard(&out, centre: p, half: 0.8, color: SIMD4(rgb.x, rgb.y, rgb.z, Float(24 * ghost / 5) / 128), camera: camera)
                appendBillboard(&out, centre: p, half: 0.25, color: SIMD4(1, 1, 1, Float(12 * ghost / 5) / 128), camera: camera)
            }
        }
    }

    /// The console keeps the last 128 screen positions of each orb and joins every eighth
    /// one with a line that fades towards the tail.
    private func appendOrbTrails(_ out: inout [Vertex], frame: Float, seed: Float, colours: [SIMD3<Float>],
                                 cameraAt: (Float) -> ViewCamera) {
        let length: Float = 127
        let halfWidth = SIMD2<Float>(1.0 / 640, 1.0 / 448)      // half a console pixel, in NDC units x2
        for i in 0 ..< 4 {
            var points: [(SIMD2<Float>, SIMD4<Float>)] = []
            for k in stride(from: 0, to: 128, by: 8) {
                let f = frame - Float(k)
                guard f >= 0 else { break }
                let clip = cameraAt(f).project(OpeningMotion.orbPosition(i, frame: f, seed: seed))
                guard clip.w > 1 else { break }
                let fade = Float(Int((length - Float(k)) * 64 / length))
                let rgb = colours[i] * fade / 128 / 255
                points.append((SIMD2(clip.x, clip.y) / clip.w, SIMD4(rgb.x, rgb.y, rgb.z, min(1, fade * 2 / 128))))
            }
            guard points.count > 1 else { continue }
            for j in 0 ..< points.count - 1 {
                let (p0, c0) = points[j], (p1, c1) = points[j + 1]
                let dir = p1 - p0
                guard length_squared(dir) > 0 else { continue }
                let n = normalize(SIMD2(-dir.y * 448 / 640, dir.x * 640 / 448)) * halfWidth * 2
                func v(_ p: SIMD2<Float>, _ c: SIMD4<Float>) -> Vertex { Vertex(pos: SIMD4(p.x, p.y, 0, 1), uv: .zero, color: c) }
                out.append(contentsOf: [v(p0 - n, c0), v(p0 + n, c0), v(p1 - n, c1), v(p1 - n, c1), v(p0 + n, c0), v(p1 + n, c1)])
            }
        }
    }

    /// A world-space line drawn `width` target pixels wide.
    func appendLine(_ out: inout [Vertex], _ a: SIMD3<Float>, _ b: SIMD3<Float>, color: SIMD4<Float>,
                            width: Float = 3, camera: ViewCamera) {
        var ca = camera.project(a), cb = camera.project(b)
        let near: Float = 1.05
        if ca.w < near, cb.w < near { return }
        if ca.w < near { ca = cb + (ca - cb) * ((cb.w - near) / (cb.w - ca.w)) }
        if cb.w < near { cb = ca + (cb - ca) * ((ca.w - near) / (ca.w - cb.w)) }
        let pa = SIMD3(ca.x, ca.y, ca.z) / ca.w, pb = SIMD3(cb.x, cb.y, cb.z) / cb.w
        let pixel = SIMD2<Float>(2 / Float(size.width), 2 / Float(size.height))
        let d = SIMD2(pb.x - pa.x, pb.y - pa.y) / pixel
        guard length_squared(d) > 1e-6 else { return }
        let n = normalize(SIMD2(-d.y, d.x)) * pixel * width * 0.5
        func v(_ p: SIMD3<Float>, _ s: Float) -> Vertex {
            Vertex(pos: SIMD4(p.x + n.x * s, p.y + n.y * s, p.z, 1), uv: .zero, color: color)
        }
        out.append(contentsOf: [v(pa, -1), v(pa, 1), v(pb, -1), v(pb, -1), v(pa, 1), v(pb, 1)])
    }

    /// The scripted camera's route: a spine through every frame's position, a rung every
    /// 10 frames pointing along the camera's screen-up (so the roll shows as a twist), gates
    /// where the scene's events fire, and the frustum at the current frame.
    func appendCameraPath(_ out: inout [Vertex], timeline: Timeline, frame: Float, camera: ViewCamera, showFrustum: Bool) {
        let travelled = SIMD4<Float>(1.0, 0.82, 0.25, 0.95), ahead = SIMD4<Float>(1.0, 0.82, 0.25, 0.35)
        let now = Int(frame)
        for f in 0 ..< timeline.endFrame {
            appendLine(&out, timeline.states[f].position, timeline.states[f + 1].position,
                       color: f < now ? travelled : ahead, camera: camera)
        }
        for f in stride(from: 0, through: timeline.endFrame, by: 10) {
            let cam = ViewCamera(timeline.states[f])
            let b = cam.basis
            let second = f % 60 == 0
            let colour = SIMD4<Float>(1, 1, 1, f <= now ? 0.9 : 0.35)
            appendLine(&out, cam.position, cam.position - b.y * (second ? 2.4 : 1.2), color: colour, width: second ? 3 : 2, camera: camera)
            appendLine(&out, cam.position - b.x * 0.6, cam.position + b.x * 0.6, color: colour, width: 2, camera: camera)
        }
        // Gates: lettering, dive start, defocus, fade, end of scene.
        let gates: [(Int, SIMD3<Float>)] = [
            (timeline.frame(passing: 18), SIMD3(0.3, 0.9, 1.0)),
            (timeline.diveFrame, SIMD3(0.3, 1.0, 0.4)),
            (timeline.frame(passing: 56), SIMD3(1.0, 0.4, 1.0)),
            (timeline.frame(passing: 72), SIMD3(1.0, 0.55, 0.15)),
            (timeline.endFrame, SIMD3(1.0, 0.25, 0.25)),
        ]
        for (f, rgb) in gates {
            let cam = ViewCamera(timeline.states[min(f, timeline.endFrame)])
            let b = cam.basis, r: Float = 3
            let c = [cam.position - b.x * r - b.y * r, cam.position + b.x * r - b.y * r,
                     cam.position + b.x * r + b.y * r, cam.position - b.x * r + b.y * r]
            for i in 0 ..< 4 { appendLine(&out, c[i], c[(i + 1) % 4], color: SIMD4(rgb.x, rgb.y, rgb.z, 0.9), camera: camera) }
        }
        guard showFrustum else { return }
        let cam = ViewCamera(timeline.camera(at: frame))
        let b = cam.basis, depth: Float = 10
        let halfW = depth / 3.2, halfH = depth / (1024 * 0.4576 / 112)
        let centre = cam.position + b.z * depth
        let corners = [centre - b.x * halfW - b.y * halfH, centre + b.x * halfW - b.y * halfH,
                       centre + b.x * halfW + b.y * halfH, centre - b.x * halfW + b.y * halfH]
        let white = SIMD4<Float>(1, 1, 1, 1)
        for i in 0 ..< 4 {
            appendLine(&out, cam.position, corners[i], color: white, camera: camera)
            appendLine(&out, corners[i], corners[(i + 1) % 4], color: white, camera: camera)
        }
        // A small roof on the top edge marks which way is up on screen.
        let apex = centre - b.y * (halfH * 1.35)
        appendLine(&out, corners[0], apex, color: white, camera: camera)
        appendLine(&out, corners[1], apex, color: white, camera: camera)
    }

    private static let cubeCorners: [SIMD3<Float>] = [
        SIMD3(-1, -1, -1), SIMD3(1, -1, -1), SIMD3(-1, 1, -1), SIMD3(1, 1, -1),
        SIMD3(-1, -1, 1), SIMD3(1, -1, 1), SIMD3(-1, 1, 1), SIMD3(1, 1, 1),
    ]
    private static let cubeFaces: [([Int], SIMD3<Float>)] = [
        ([0, 1, 2, 3], SIMD3(0, 0, -1)), ([5, 4, 7, 6], SIMD3(0, 0, 1)), ([4, 0, 6, 2], SIMD3(-1, 0, 0)),
        ([2, 3, 6, 7], SIMD3(0, 1, 0)), ([1, 5, 3, 7], SIMD3(1, 0, 0)), ([4, 5, 0, 1], SIMD3(0, -1, 0)),
    ]

    /// Approximation of the console's ten-pass glass: per face, a refracted copy of the frame
    /// multiplied by `tint`, plus the reflection map (times `reflectTint`) masked by the noise
    /// texture. `reflection` gives the back/front reflection strengths.
    func appendGlassCube(_ out: inout [GlassVertex], centre: SIMD3<Float>, rotation r: SIMD3<Float>, half: Float,
                         tint: SIMD3<Float>, reflectTint: SIMD3<Float>, refraction: Float,
                         reflection: (back: Float, front: Float), camera: ViewCamera, front: Bool) {
        let rot = float3x3(simd_quatf(angle: r.x, axis: SIMD3(1, 0, 0)))
            * float3x3(simd_quatf(angle: r.y, axis: SIMD3(0, 1, 0)))
            * float3x3(simd_quatf(angle: r.z, axis: SIMD3(0, 0, 1)))
        let world = Self.cubeCorners.map { centre + rot * ($0 * half) }
        let clip = world.map(camera.project)
        guard clip.allSatisfy({ $0.w > 1 }) else { return }
        func screenUV(_ c: SIMD4<Float>) -> SIMD2<Float> { SIMD2(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5) }
        let centreUV = screenUV(camera.project(centre))
        let basis = camera.basis
        let corners: [SIMD2<Float>] = [SIMD2(0, 0), SIMD2(1, 0), SIMD2(0, 1), SIMD2(1, 1)]
        for (indices, normal) in Self.cubeFaces {
            let n = rot * normal
            let faceCentre = indices.reduce(SIMD3<Float>.zero) { $0 + world[$1] } / 4
            let facing = dot(n, faceCentre - camera.position) < 0
            guard facing == front else { continue }
            let nView = SIMD2(dot(n, basis.x), dot(n, basis.y))
            var quad: [GlassVertex] = []
            for (vi, ci) in indices.enumerated() {
                let toEye = normalize(camera.position - world[ci])
                let rim = pow(1 - abs(dot(n, toEye)), 2) * 0.5 * 32 / 128
                // Offset the lookup along the face normal, weaker with distance (2/w in uv units).
                let refract = nView * 2 * refraction / clip[ci].w
                let reflectScale: Float = front ? 0.5 : -0.25
                let reflUV = corners[vi] * 0.5 + SIMD2(repeating: 0.25) + nView * reflectScale
                let noiseShift: Float = front ? 0.0075 : 0.00375
                quad.append(GlassVertex(
                    pos: clip[ci],
                    a: SIMD4(refract.x, refract.y, reflUV.x, reflUV.y),
                    b: SIMD4(corners[vi].x + noiseShift, corners[vi].y - noiseShift, front ? -0.084 : 0, rim),
                    c: SIMD4(centreUV.x, centreUV.y, front ? reflection.front : reflection.back, 0),
                    tint: SIMD4(tint.x, tint.y, tint.z, 1),
                    reflTint: SIMD4(reflectTint.x, reflectTint.y, reflectTint.z, 1)))
            }
            out.append(contentsOf: [quad[0], quad[1], quad[2], quad[2], quad[1], quad[3]])
        }
    }

    /// Full-target quad by default; coordinates are NDC, uv origin top-left.
    func appendQuad(_ out: inout [Vertex], x0: Float = -1, y0: Float = -1, x1: Float = 1, y1: Float = 1,
                            u0: Float = 0, v0: Float = 0, u1: Float = 1, v1: Float = 1,
                            color: SIMD4<Float> = SIMD4(1, 1, 1, 1)) {
        func v(_ x: Float, _ y: Float, _ u: Float, _ w: Float) -> Vertex { Vertex(pos: SIMD4(x, y, 0, 1), uv: SIMD2(u, w), color: color) }
        let tl = v(x0, y1, u0, v0), tr = v(x1, y1, u1, v0), bl = v(x0, y0, u0, v1), br = v(x1, y0, u1, v1)
        out.append(contentsOf: [tl, tr, bl, bl, tr, br])
    }

    /// A sprite in the console's 640x224 field coordinates.
    func appendSprite(_ out: inout [Vertex], x: Float, y: Float, w: Float, h: Float,
                              u: Float, v: Float, uw: Float, vh: Float, texture: (Float, Float), alpha: Float, rgb: Float = 1) {
        appendQuad(&out, x0: x / 320 - 1, y0: 1 - (y + h) / 112, x1: (x + w) / 320 - 1, y1: 1 - y / 112,
                   u0: u / texture.0, v0: v / texture.1, u1: (u + uw) / texture.0, v1: (v + vh) / texture.1,
                   color: SIMD4(rgb, rgb, rgb, alpha))
    }

    // MARK: - Metal plumbing

    enum DepthAction { case none, clear, load }

    func pass(_ cb: MTLCommandBuffer, color: String, clear: Bool, depth: DepthAction, _ body: (MTLRenderCommandEncoder) -> Void) {
        let rp = MTLRenderPassDescriptor()
        rp.colorAttachments[0].texture = targets[color]
        rp.colorAttachments[0].loadAction = clear ? .clear : .load
        rp.colorAttachments[0].clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        rp.colorAttachments[0].storeAction = .store
        if depth != .none {
            rp.depthAttachment.texture = targets["depth"]
            rp.depthAttachment.loadAction = depth == .clear ? .clear : .load
            rp.depthAttachment.clearDepth = 1
            rp.depthAttachment.storeAction = .store
        }
        guard let enc = cb.makeRenderCommandEncoder(descriptor: rp) else { return }
        body(enc)
        enc.endEncoding()
    }

    func blit(_ cb: MTLCommandBuffer, from: String, to: String) {
        guard let enc = cb.makeBlitCommandEncoder(), let src = targets[from], let dst = targets[to] else { return }
        enc.copy(from: src, to: dst)
        enc.endEncoding()
    }

    func texture(_ name: String) -> MTLTexture? {
        name.hasPrefix("@") ? targets[String(name.dropFirst())] : (textures[name] ?? textures["white"])
    }

    private func pipeline(vertex: String, fragment: String, format: MTLPixelFormat, hasDepth: Bool, blend: Blend) -> MTLRenderPipelineState {
        let key = "\(vertex)/\(fragment)/\(format.rawValue)/\(hasDepth)/\(blend)"
        if let p = pipelines[key] { return p }
        let d = MTLRenderPipelineDescriptor()
        d.vertexFunction = library.makeFunction(name: vertex)
        d.fragmentFunction = library.makeFunction(name: fragment)
        let a = d.colorAttachments[0]!
        a.pixelFormat = format
        if hasDepth { d.depthAttachmentPixelFormat = .depth32Float }
        if blend != .opaque {
            a.isBlendingEnabled = true
            a.rgbBlendOperation = .add
            a.alphaBlendOperation = .add
            a.destinationAlphaBlendFactor = .one
            a.sourceAlphaBlendFactor = .zero
            switch blend {
            case .add: a.sourceRGBBlendFactor = .one; a.destinationRGBBlendFactor = .one
            case .addSrcAlpha: a.sourceRGBBlendFactor = .sourceAlpha; a.destinationRGBBlendFactor = .one
            case .alpha: a.sourceRGBBlendFactor = .sourceAlpha; a.destinationRGBBlendFactor = .oneMinusSourceAlpha
            case .opaque: break
            }
        }
        let p = try! device.makeRenderPipelineState(descriptor: d)
        pipelines[key] = p
        return p
    }

    func draw(_ enc: MTLRenderCommandEncoder, _ verts: [Vertex], _ batches: [Batch], format: MTLPixelFormat, hasDepth: Bool) {
        guard !verts.isEmpty,
              let buffer = device.makeBuffer(bytes: verts, length: verts.count * MemoryLayout<Vertex>.stride, options: .storageModeShared)
        else { return }
        enc.setVertexBuffer(buffer, offset: 0, index: 0)
        for b in batches where !b.range.isEmpty {
            enc.setRenderPipelineState(pipeline(vertex: "v_main", fragment: "f_tex", format: format, hasDepth: hasDepth, blend: b.blend))
            enc.setDepthStencilState(depthStates[hasDepth ? b.depth : .none])
            enc.setFragmentTexture(texture(b.texture), index: 0)
            enc.setFragmentSamplerState(b.repeatUV ? samplerRepeat : samplerClamp, index: 0)
            enc.drawPrimitives(type: .triangle, vertexStart: b.range.lowerBound, vertexCount: b.range.count)
        }
    }

    func drawGlass(_ enc: MTLRenderCommandEncoder, _ verts: [GlassVertex], source: String) {
        guard let buffer = device.makeBuffer(bytes: verts, length: verts.count * MemoryLayout<GlassVertex>.stride, options: .storageModeShared)
        else { return }
        enc.setRenderPipelineState(pipeline(vertex: "v_glass", fragment: "f_glass", format: .rgba8Unorm, hasDepth: false, blend: .opaque))
        enc.setDepthStencilState(depthStates[.none])
        enc.setVertexBuffer(buffer, offset: 0, index: 0)
        enc.setFragmentTexture(targets[source], index: 0)
        enc.setFragmentTexture(texture("TEXOREF"), index: 1)
        enc.setFragmentTexture(texture("TEXOBLP"), index: 2)
        enc.setFragmentSamplerState(samplerClamp, index: 0)
        enc.setFragmentSamplerState(samplerRepeat, index: 1)
        enc.drawPrimitives(type: .triangle, vertexStart: 0, vertexCount: verts.count)
    }

    private static let shaderSource = """
    #include <metal_stdlib>
    using namespace metal;

    struct Vertex { float4 pos; float2 uv; float2 pad; float4 color; };
    struct VOut { float4 pos [[position]]; float2 uv; float4 color; };

    vertex VOut v_main(uint vid [[vertex_id]], const device Vertex* v [[buffer(0)]]) {
        VOut o;
        o.pos = v[vid].pos; o.uv = v[vid].uv; o.color = v[vid].color;
        return o;
    }

    fragment float4 f_tex(VOut in [[stage_in]], texture2d<float> t [[texture(0)]], sampler s [[sampler(0)]]) {
        return t.sample(s, in.uv) * in.color;
    }

    struct GlassVertex { float4 pos; float4 a; float4 b; float4 c; float4 tint; float4 reflTint; };
    struct GOut { float4 pos [[position]]; float4 a; float4 b; float4 c; float4 tint; float4 reflTint; };

    vertex GOut v_glass(uint vid [[vertex_id]], const device GlassVertex* v [[buffer(0)]]) {
        GOut o;
        o.pos = v[vid].pos; o.a = v[vid].a; o.b = v[vid].b; o.c = v[vid].c; o.tint = v[vid].tint; o.reflTint = v[vid].reflTint;
        return o;
    }

    fragment float4 f_glass(GOut in [[stage_in]], texture2d<float> scene [[texture(0)]], texture2d<float> refl [[texture(1)]],
                            texture2d<float> noise [[texture(2)]], sampler clampS [[sampler(0)]], sampler repeatS [[sampler(1)]]) {
        float2 uv = in.pos.xy / float2(scene.get_width(), scene.get_height());
        uv += (uv - in.c.xy) * in.b.z + in.a.xy;
        float3 col = scene.sample(clampS, uv).rgb * in.tint.rgb + in.b.w;
        col += refl.sample(repeatS, in.a.zw).rgb * in.reflTint.rgb * noise.sample(repeatS, in.b.xy).a * 2.0 * in.c.z;
        return float4(col, 1.0);
    }
    """
}
