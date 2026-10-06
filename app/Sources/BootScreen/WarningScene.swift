import Foundation
import Metal
import PS2Kit
import simd

/// The second opening scene: the red "Please insert a PlayStation or PlayStation 2 format
/// disc" screen (see `notes/opening_scene1.md`).
extension Renderer {
    func renderWarning(frame: Float, timeline: Timeline, freeCamera: ViewCamera?, options: RenderOptions,
                       commandBuffer cb: MTLCommandBuffer) -> MTLTexture {
        let scripted = timeline.camera(at: frame)
        let camera = freeCamera ?? ViewCamera(scripted)
        func cameraAt(_ f: Float) -> ViewCamera { freeCamera ?? ViewCamera(timeline.camera(at: f)) }
        let sceneTarget = targets["scene"]!

        // 1. The light source (seven spinning discs + flares), fed back at 87.5 % per frame.
        let history = options.trails ? 20 : 1
        var first = true
        for k in stride(from: history - 1, through: 0, by: -1) {
            let f = frame - Float(k)
            if f < 1 && k != 0 { continue }
            var verts: [Vertex] = []
            var batches: [Batch] = []
            if options.orbs, f >= 1 {
                let b = OpeningMotion.warningBrightness(z: timeline.camera(at: f).z)
                let start = verts.count
                appendLightDiscs(&verts, frame: f, brightness: b, camera: cameraAt(f))
                batches.append(Batch(texture: "white", blend: .add, range: start ..< verts.count))
                let fs = verts.count
                appendFlares(&verts, frame: f, brightness: b, camera: cameraAt(f))
                batches.append(Batch(texture: "TEXOFLAR", blend: .add, range: fs ..< verts.count))
            }
            pass(cb, color: "sub", clear: true, depth: .clear) { enc in
                self.draw(enc, verts, batches, format: .rgba8Unorm, hasDepth: true)
            }
            let weight: Float = history == 1 ? 1 : (k == history - 1 ? pow(0.875, Float(k)) : 0.125 * pow(0.875, Float(k)))
            var quad: [Vertex] = []
            appendQuad(&quad, color: SIMD4(repeating: weight))
            pass(cb, color: "accum", clear: first, depth: .none) { enc in
                self.draw(enc, quad, [Batch(texture: "@sub", blend: .add, range: 0 ..< 6)], format: .rgba16Float, hasDepth: false)
            }
            first = false
        }

        // 2. Composite, then the smoke.
        var verts: [Vertex] = []
        var batches: [Batch] = []
        func batch(_ texture: String, _ blend: Blend, _ build: (inout [Vertex]) -> Void) {
            let start = verts.count
            build(&verts)
            if verts.count > start { batches.append(Batch(texture: texture, blend: blend, range: start ..< verts.count)) }
        }
        batch("@accum", .opaque) { self.appendQuad(&$0) }
        if options.fog { batch("TEXOFOG0", .add) { self.appendSmoke(&$0, frame: frame, cameraZ: scripted.z, camera: camera) } }
        pass(cb, color: "scene", clear: true, depth: .load) { enc in
            self.draw(enc, verts, batches, format: .rgba8Unorm, hasDepth: true)
        }

        // 3. Prisms: the glass renderer with red reflections.
        if options.glass, let positions = assets?.prismPositions {
            blit(cb, from: "scene", to: "copy")
            for front in [false, true] {
                var gv: [GlassVertex] = []
                for (i, p) in positions.enumerated() {
                    appendGlassCube(&gv, centre: SIMD3(p.x, p.y, (p.z - 2.5) * 128 + 788),
                                    rotation: OpeningMotion.prismRotation(i, frame: frame), half: 1.2,
                                    tint: SIMD3(repeating: 1), reflectTint: SIMD3(1, 1.0 / 3, 1.0 / 3), refraction: 0.8,
                                    reflection: (0, 0.5), camera: camera, front: front)
                }
                guard !gv.isEmpty else { continue }
                pass(cb, color: front ? "scene" : "copy", clear: false, depth: .none) { enc in
                    self.drawGlass(enc, gv, source: front ? "copy" : "scene")
                }
            }
        }

        // 4. Overlays: fade in from black, the text, the exit fade, letterbox.
        verts = []; batches = []
        let exitFrames = max(0, frame - Float(timeline.diveFrame))
        if options.fade {
            var a = frame < 1 ? 1 : OpeningMotion.warningFadeIn(z: scripted.z)
            if frame >= Float(timeline.diveFrame) { a = max(a, min(1, exitFrames / 128)) }
            if a > 0 { batch("white", .alpha) { self.appendQuad(&$0, color: SIMD4(0, 0, 0, a)) } }
        }
        if options.lettering {
            let since = frame - Float(timeline.frame(passing: 800))
            var counter = min(112, max(0, since))
            if frame >= Float(timeline.diveFrame) { counter = max(0, counter - exitFrames) }
            let a = counter / 128
            if a > 0, let name = options.warningTexture, textures[name] != nil {
                batch(name, .add) { self.appendSprite(&$0, x: 64, y: 88, w: 512, h: 64, u: 0, v: 0, uw: 512, vh: 128,
                                                      texture: (512, 128), alpha: 1, rgb: a) }
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

    /// Seven discs in two sizes, each a 16-segment fan with a coloured centre and black rim,
    /// re-oriented so fast every frame that only the smeared glow reads.
    private func appendLightDiscs(_ out: inout [Vertex], frame: Float, brightness b: Float, camera: ViewCamera) {
        let scale = min(1, b / 64)
        let sets: [(radius: (Float) -> Float, centre: SIMD3<Float>)] = [
            ({ _ in 8 }, SIMD3(0x80, 0x10, 0x28)), ({ i in 28 + 8 * i }, SIMD3(0x30, 0x08, 0x0C)),
        ]
        for (radius, colour) in sets {
            let c = colour * scale / 255
            for i in 0 ..< 7 {
                let (position, r) = OpeningMotion.warningDisc(i, frame: frame)
                let rot = float3x3(simd_quatf(angle: r.x, axis: SIMD3(1, 0, 0)))
                    * float3x3(simd_quatf(angle: r.y, axis: SIMD3(0, 1, 0)))
                    * float3x3(simd_quatf(angle: r.z, axis: SIMD3(0, 0, 1)))
                let centre = camera.project(position)
                guard centre.w > 1 else { continue }
                let rim = (0 ... 16).map { k -> SIMD4<Float> in
                    let a = Float(k % 16) * 2 * .pi / 16
                    return camera.project(position + rot * SIMD3(cos(a) * radius(Float(i)), sin(a) * radius(Float(i)), 0))
                }
                guard rim.allSatisfy({ $0.w > 1 }) else { continue }
                let cv = Vertex(pos: centre, uv: .zero, color: SIMD4(c.x, c.y, c.z, 1))
                for k in 0 ..< 16 {
                    out.append(cv)
                    out.append(Vertex(pos: rim[k], uv: .zero, color: SIMD4(0, 0, 0, 1)))
                    out.append(Vertex(pos: rim[k + 1], uv: .zero, color: SIMD4(0, 0, 0, 1)))
                }
            }
        }
    }

    /// Five additive flare sprites around the light source's centre.
    private func appendFlares(_ out: inout [Vertex], frame: Float, brightness b: Float, camera: ViewCamera) {
        let n = Float(min(Int(b / 16), 8))
        let angle = (Float(Int(frame) & 31) + 49) * 0.1
        let centre = SIMD3<Float>(0.196 * cos(angle), 0.196 * sin(angle), 1160)
        let clip = camera.project(centre)
        guard clip.w > 1 else { return }
        let sx = clip.x / clip.w * 320, sy = -clip.y / clip.w * 112
        let flares: [(scale: Float, half: Float, fix: Float, colour: SIMD3<Float>)] = [
            (1, 0x70, n + 2, SIMD3(0x80, 0x40, 0x40)), (1, 0xAA, n + 2, SIMD3(0x80, 0x40, 0x40)),
            (1, 0x100, n + 2, SIMD3(0x80, 0x40, 0x40)), (1, 0x1C0, n + 2, SIMD3(0x80, 0x40, 0x40)),
            (0.9, Float(Int(b / 8 + 420)), min(255, 3 * n + b / 2), SIMD3(0x80, 0x70, 0x60)),
        ]
        for f in flares {
            let k = f.colour / 128 * (f.fix / 128)
            appendSprite(&out, x: sx * f.scale + 320 - f.half, y: sy * f.scale + 112 - f.half / 2, w: 2 * f.half, h: f.half,
                         u: 0, v: 0, uw: 128, vh: 128, texture: (128, 128), alpha: 1, rgb: 1)
            for i in out.count - 6 ..< out.count { out[i].color = SIMD4(k.x, k.y, k.z, 1) }
        }
    }

    /// 128 drifting puffs: 3x3 quads of ±15 units with a bright centre vertex, recycled in a
    /// band from 32 to 805 units in front of the camera.
    private func appendSmoke(_ out: inout [Vertex], frame: Float, cameraZ: Float, camera: ViewCamera) {
        var rng = SplitMix64(seed: 0x5EED)
        for k in 0 ..< 128 {
            let x = Float(Int(rng.next() % 4800) - 2400) * 0.01, y = Float(Int(rng.next() % 4800) - 2400) * 0.01
            let z0 = 477 + Float(rng.next() % 805)
            let r = Float(64 + rng.next() % 64)
            let speed = Float(Int(Float(k + 1) * 0.02 + 1.2))
            // Position relative to the parked camera, wrapped in the recycling band.
            let band: Float = 805 - 32
            var rel = (z0 - 672 - 32 - speed * frame).truncatingRemainder(dividingBy: band)
            if rel < 0 { rel += band }
            let z = cameraZ + 32 + rel
            let fade = min(64, min((cameraZ + 805 - z) / 3, (z - cameraZ - 32) / 3))
            guard fade > 0 else { continue }
            let fix = fade / 128
            let centreColour = SIMD4<Float>(r, 0.75 * r, 0.75 * r, 128) / 128 * fix
            func v(_ ox: Float, _ oy: Float, _ u: Float, _ w: Float, centre: Bool) -> Vertex {
                Vertex(pos: camera.project(SIMD3(x + ox, y + oy, z)), uv: SIMD2(u, w),
                       color: centre ? centreColour : SIMD4(0, 0, 0, 1))
            }
            let g: [Float] = [-15, 0, 15]
            for qy in 0 ..< 2 {
                for qx in 0 ..< 2 {
                    let q = [v(g[qx], g[qy], Float(qx) * 0.5, Float(qy) * 0.5, centre: qx == 1 && qy == 1),
                             v(g[qx + 1], g[qy], Float(qx + 1) * 0.5, Float(qy) * 0.5, centre: qx == 0 && qy == 1),
                             v(g[qx], g[qy + 1], Float(qx) * 0.5, Float(qy + 1) * 0.5, centre: qx == 1 && qy == 0),
                             v(g[qx + 1], g[qy + 1], Float(qx + 1) * 0.5, Float(qy + 1) * 0.5, centre: qx == 0 && qy == 0)]
                    guard q.allSatisfy({ $0.pos.w > 1 }) else { continue }
                    out.append(contentsOf: [q[0], q[1], q[2], q[2], q[1], q[3]])
                }
            }
        }
    }
}

struct SplitMix64 {
    var state: UInt64
    init(seed: UInt64) { state = seed }
    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ z >> 30) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ z >> 27) &* 0x94D0_49BB_1331_11EB
        return z ^ z >> 31
    }
}
