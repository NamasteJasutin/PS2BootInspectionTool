import Foundation
import Metal
import PS2Kit
import simd

/// The "PlayStation 2" logo (`rom0:PS2LOGO`), rendered field by field in its 640x512 buffer
/// coordinates. The feedback pass depends on the previous field, so fields are produced in
/// order and cached; a scrub re-runs the sequence from the first field.
extension Renderer {
    func setLogo(animation: LogoAnimation?, bitmap: (width: Int, height: Int, grey: [UInt8])?) {
        logoAnimation = animation
        logoCachedField = nil
        if let bitmap {
            var rgba = [UInt8](repeating: 0, count: 512 * 128 * 4)
            for y in 0 ..< bitmap.height {
                for x in 0 ..< bitmap.width {
                    let v = bitmap.grey[y * bitmap.width + x], o = (y * 512 + x) * 4
                    rgba[o] = v; rgba[o + 1] = v; rgba[o + 2] = v; rgba[o + 3] = v
                }
            }
            targets["logo"] = makeTexture(width: 512, height: 128, rgba: rgba, mips: false)
        }
        // The logo blur ping-pongs between two textures covering only the 384x96 logo region.
        let scale = Float(size.width) / 640
        for name in ["blurA", "blurB"] where targets[name] == nil {
            let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: Int(384 * scale), height: Int(96 * scale), mipmapped: false)
            d.usage = [.renderTarget, .shaderRead]
            d.storageMode = .private
            targets[name] = device.makeTexture(descriptor: d)
        }
        if targets["logoPrev"] == nil {
            let d = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .rgba8Unorm, width: size.width, height: size.height, mipmapped: false)
            d.usage = [.renderTarget, .shaderRead]
            d.storageMode = .private
            targets["logoPrev"] = device.makeTexture(descriptor: d)
        }
    }

    func renderLogo(frame: Float, timeline: Timeline, options: RenderOptions, commandBuffer cb: MTLCommandBuffer) -> MTLTexture {
        let scene = targets["scene"]!
        guard let anim = logoAnimation, anim.video == timeline.video || true else {
            pass(cb, color: "scene", clear: true, depth: .none) { _ in }
            return scene
        }
        let animated = anim.lastField - anim.firstField
        let f = min(max(Int(frame), 0), animated)
        if let cached = logoCachedField, cached == f { return scene }
        // Fields depend on the previous one: continue from the cache when moving forward
        // (catching up over skipped fields), restart only when scrubbing backwards.
        let start: Int
        if let cached = logoCachedField, cached < f {
            start = cached + 1
        } else {
            start = 0
            pass(cb, color: "logoPrev", clear: true, depth: .none) { _ in }
        }
        for field in start ... f {
            renderLogoField(anim.firstField + field, animation: anim, options: options, cb: cb)
            blit(cb, from: "scene", to: "logoPrev")
        }
        logoCachedField = f
        return scene
    }

    /// A textured quad between two targets, each addressed in its own logical pixel space
    /// (the frame is 640x512; the blur textures are the 384x96 logo region).
    private func resample(_ cb: MTLCommandBuffer, from: String, fromSize: (Float, Float), to: String, toSize: (Float, Float),
                          src: (Float, Float, Float, Float), dst: (Float, Float, Float, Float), clear: Bool,
                          color: SIMD4<Float> = SIMD4(1, 1, 1, 1), blend: Blend = .opaque) {
        var v: [Vertex] = []
        appendQuad(&v, x0: dst.0 / toSize.0 * 2 - 1, y0: 1 - dst.3 / toSize.1 * 2, x1: dst.2 / toSize.0 * 2 - 1, y1: 1 - dst.1 / toSize.1 * 2,
                   u0: src.0 / fromSize.0, v0: src.1 / fromSize.1, u1: src.2 / fromSize.0, v1: src.3 / fromSize.1, color: color)
        pass(cb, color: to, clear: clear, depth: .none) { enc in
            self.draw(enc, v, [Batch(texture: "@" + from, blend: blend, range: 0 ..< 6)], format: .rgba8Unorm, hasDepth: false)
        }
    }

    private var frameSize: (Float, Float) { (640, 512) }
    private var regionSize: (Float, Float) { (384, 96) }
    private var regionOrigin: (Float, Float) { (120.5, 200.5) }

    private func renderLogoField(_ t: Int, animation anim: LogoAnimation, options: RenderOptions, cb: MTLCommandBuffer) {
        let pal = anim.video == .pal
        // 1. The logo bitmap, opaque.
        var v: [Vertex] = []
        let r = anim.logoRect
        if options.towers {
            appendQuad(&v, x0: r.x / 320 - 1, y0: 1 - (r.y + r.h) / 256, x1: (r.x + r.w) / 320 - 1, y1: 1 - r.y / 256,
                       u0: 0, v0: 0, u1: r.w / 512, v1: r.h / 128)
        }
        pass(cb, color: "scene", clear: true, depth: .none) { enc in
            self.draw(enc, v, [Batch(texture: "@logo", blend: .opaque, range: 0 ..< v.count)], format: .rgba8Unorm, hasDepth: false)
        }
        // 2. Progressive blur of the logo neighbourhood: n down/up resamples through the aux buffer.
        if options.defocus {
            let n = anim.logoBlurIterations(field: t)
            if n > 0 {
                let auxH: Float = pal ? 84.25 : 71.25
                let region = (regionOrigin.0, regionOrigin.1, regionOrigin.0 + regionSize.0, regionOrigin.1 + regionSize.1)
                let whole = (Float(0), Float(0), regionSize.0, regionSize.1)
                resample(cb, from: "scene", fromSize: frameSize, to: "blurA", toSize: regionSize, src: region, dst: whole, clear: true)
                for i in 0 ..< n {
                    let s = Float(i) * 0.5
                    // The console's rectangles carry GS half-pixel offsets; Metal's texel centres differ,
                    // so the down/up pair is kept exactly symmetric to avoid a per-iteration drift.
                    resample(cb, from: "blurA", fromSize: regionSize, to: "blurB", toSize: regionSize, src: whole, dst: (0, 0, 239.25 - s, auxH - s), clear: true)
                    resample(cb, from: "blurB", fromSize: regionSize, to: "blurA", toSize: regionSize, src: (0, 0, 239.25 - s, auxH - s), dst: whole, clear: false)
                }
                resample(cb, from: "blurA", fromSize: regionSize, to: "scene", toSize: frameSize, src: whole, dst: region, clear: false)
            }
        }
        // 3. Layer A (outline + one ribbon), soft blur, layer B, feedback, soft blur.
        if options.orbs { drawLayers(cb, anim, field: t, layerA: true) }
        if options.fog, anim.screenBlurActive(field: t) { screenBlur(cb, pal: pal) }
        if options.orbs { drawLayers(cb, anim, field: t, layerA: false) }
        if options.trails {
            let a = Float(anim.feedbackAlpha(field: t)) / 128
            if a > 0 {
                let k: Float = Float(0x84) / 128
                resample(cb, from: "logoPrev", fromSize: frameSize, to: "scene", toSize: frameSize, src: (0.5, 0.5, 640.5, 512.5),
                         dst: (2, 2, 638, 510), clear: false, color: SIMD4(k, k, k, a), blend: .alpha)
            }
        }
        if options.fog, anim.screenBlurActive(field: t) { screenBlur(cb, pal: pal) }
    }

    private func screenBlur(_ cb: MTLCommandBuffer, pal: Bool) {
        for i in 0 ..< 2 {
            let s = Float(i) * 0.5
            resample(cb, from: "scene", fromSize: frameSize, to: "temp", toSize: frameSize, src: (0, 0, 640, 512), dst: (0, 0, 479.25 - s, 385.25 - s), clear: true)
            resample(cb, from: "temp", fromSize: frameSize, to: "scene", toSize: frameSize, src: (0, 0, 479.25 - s, 385.25 - s), dst: (0, 0, 640, 512), clear: false)
        }
    }

    /// Additive vector layers: anti-aliased line strips (type 0) and five-copy ribbons (type 1).
    private func drawLayers(_ cb: MTLCommandBuffer, _ anim: LogoAnimation, field t: Int, layerA: Bool) {
        var v: [Vertex] = []
        for index in stride(from: 3, through: 0, by: -1) {
            let obj = anim.objects[index]
            guard layerA ? obj.layerA : obj.layerB else { continue }
            let colour = anim.colour(of: obj, field: t) / 255
            if obj.type == 0 {
                for strip in anim.flatten(anim.shape(of: obj, field: t)) {
                    let pts = strip.map(anim.toScreen)
                    for i in 0 ..< max(0, pts.count - 1) { appendLine2D(&v, pts[i], pts[i + 1], color: SIMD4(colour.x, colour.y, colour.z, 1)) }
                }
            } else {
                let counts = anim.segmentCounts(of: obj, field: t)
                let offsets = anim.ribbonOffsets(object: index)
                let curves = offsets.map { d in anim.flatten(anim.shape(of: obj, field: t - d), counts: counts).map { $0.map(anim.toScreen) } }
                for k in 1 ..< 5 {
                    let ca = colour * anim.assets.ribbonMultipliers[k - 1], cb2 = colour * anim.assets.ribbonMultipliers[k]
                    for (pa, pb) in zip(curves[k - 1], curves[k]) {
                        let n = min(pa.count, pb.count)
                        guard n > 1 else { continue }
                        func vert(_ p: SIMD2<Float>, _ c: SIMD3<Float>) -> Vertex {
                            Vertex(pos: SIMD4(p.x / 320 - 1, 1 - p.y / 256, 0, 1), uv: .zero, color: SIMD4(c.x, c.y, c.z, 1))
                        }
                        for i in 0 ..< n - 1 {
                            v.append(contentsOf: [vert(pa[i], ca), vert(pb[i], cb2), vert(pa[i + 1], ca),
                                                  vert(pa[i + 1], ca), vert(pb[i], cb2), vert(pb[i + 1], cb2)])
                        }
                    }
                }
            }
        }
        guard !v.isEmpty else { return }
        pass(cb, color: "scene", clear: false, depth: .none) { enc in
            self.draw(enc, v, [Batch(texture: "white", blend: .add, range: 0 ..< v.count)], format: .rgba8Unorm, hasDepth: false)
        }
    }

    /// A one-buffer-pixel-wide line between two 640x512 buffer points.
    private func appendLine2D(_ out: inout [Vertex], _ a: SIMD2<Float>, _ b: SIMD2<Float>, color: SIMD4<Float>) {
        let d = b - a
        guard simd_length_squared(d) > 1e-6 else { return }
        let n = simd_normalize(SIMD2(-d.y, d.x)) * 0.5
        func vert(_ p: SIMD2<Float>) -> Vertex { Vertex(pos: SIMD4(p.x / 320 - 1, 1 - p.y / 256, 0, 1), uv: .zero, color: color) }
        out.append(contentsOf: [vert(a - n), vert(a + n), vert(b - n), vert(b - n), vert(a + n), vert(b + n)])
    }
}
