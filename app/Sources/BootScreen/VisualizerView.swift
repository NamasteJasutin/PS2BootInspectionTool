import SwiftUI

/// Published once per display frame so only the visualiser redraws.
final class VisualizerState: ObservableObject {
    @Published var snapshot = VisualizerSnapshot()
    @Published var mode: VisualizerMode = .equalizer
}

struct VisualizerView: View {
    @ObservedObject var state: VisualizerState

    var body: some View {
        if state.mode != .off {
            Canvas { ctx, size in
                let s = state.snapshot
                ctx.fill(Path(CGRect(origin: .zero, size: size)), with: .color(.black.opacity(0.55)))
                switch state.mode {
                case .volume: drawMeters(ctx, size, s)
                case .wave: drawWave(ctx, size, s)
                case .equalizer: drawBars(ctx, size, s)
                case .off: break
                }
            }
            .frame(height: 96)
            .allowsHitTesting(false)
        }
    }

    private let accent = Color(red: 0.45, green: 0.6, blue: 1.0)

    private func drawMeters(_ ctx: GraphicsContext, _ size: CGSize, _ s: VisualizerSnapshot) {
        let rows: [(String, Float, Float, Float)] = [("L", s.rmsDB.0, s.peakDB.0, s.peakHoldDB.0),
                                                     ("R", s.rmsDB.1, s.peakDB.1, s.peakHoldDB.1)]
        let left: CGFloat = 36, right = size.width - 16, barH: CGFloat = 22
        func x(_ db: Float) -> CGFloat { left + (right - left) * CGFloat(max(0, min(1, (db + 60) / 60))) }
        for (i, (name, rms, peak, hold)) in rows.enumerated() {
            let y = 16 + CGFloat(i) * 36
            ctx.draw(Text(name).font(.system(size: 12, weight: .semibold, design: .monospaced)).foregroundColor(.white),
                     at: CGPoint(x: 18, y: y + barH / 2))
            ctx.fill(Path(CGRect(x: left, y: y, width: right - left, height: barH)), with: .color(.white.opacity(0.08)))
            ctx.fill(Path(CGRect(x: left, y: y, width: x(peak) - left, height: barH)), with: .color(accent.opacity(0.45)))
            ctx.fill(Path(CGRect(x: left, y: y, width: x(rms) - left, height: barH)), with: .color(accent))
            ctx.fill(Path(CGRect(x: x(hold) - 1.5, y: y, width: 3, height: barH)), with: .color(.white))
            for tick in stride(from: -60, through: 0, by: 12) {
                let tx = x(Float(tick))
                ctx.stroke(Path { $0.move(to: CGPoint(x: tx, y: y + barH)); $0.addLine(to: CGPoint(x: tx, y: y + barH + 4)) },
                           with: .color(.white.opacity(0.5)), lineWidth: 1)
            }
        }
        ctx.draw(Text("dBFS, RMS over 50 ms with peak hold").font(.system(size: 9)).foregroundColor(.white.opacity(0.6)),
                 at: CGPoint(x: size.width - 110, y: size.height - 8))
    }

    private func drawWave(_ ctx: GraphicsContext, _ size: CGSize, _ s: VisualizerSnapshot) {
        guard s.wave.count > 1 else { return }
        let mid = size.height / 2
        ctx.stroke(Path { $0.move(to: CGPoint(x: 0, y: mid)); $0.addLine(to: CGPoint(x: size.width, y: mid)) },
                   with: .color(.white.opacity(0.15)), lineWidth: 1)
        var path = Path()
        for (i, v) in s.wave.enumerated() {
            let p = CGPoint(x: size.width * CGFloat(i) / CGFloat(s.wave.count - 1), y: mid - CGFloat(v) * (size.height / 2 - 4))
            if i == 0 { path.move(to: p) } else { path.addLine(to: p) }
        }
        ctx.stroke(path, with: .color(accent), lineWidth: 1.5)
        ctx.draw(Text("last 21 ms").font(.system(size: 9)).foregroundColor(.white.opacity(0.6)),
                 at: CGPoint(x: size.width - 40, y: size.height - 8))
    }

    private func drawBars(_ ctx: GraphicsContext, _ size: CGSize, _ s: VisualizerSnapshot) {
        guard !s.bands.isEmpty else { return }
        let n = s.bands.count
        let gap: CGFloat = 3
        let w = (size.width - 16 - gap * CGFloat(n - 1)) / CGFloat(n)
        let top: CGFloat = 8, bottom = size.height - 14
        for (i, v) in s.bands.enumerated() {
            let x = 8 + CGFloat(i) * (w + gap)
            let h = (bottom - top) * CGFloat(v)
            ctx.fill(Path(roundedRect: CGRect(x: x, y: bottom - h, width: w, height: h), cornerRadius: 2),
                     with: .linearGradient(Gradient(colors: [accent, .white]), startPoint: CGPoint(x: x, y: bottom),
                                           endPoint: CGPoint(x: x, y: top)))
            let ph = (bottom - top) * CGFloat(s.bandPeaks[i])
            ctx.fill(Path(CGRect(x: x, y: bottom - ph - 2, width: w, height: 2)), with: .color(.white.opacity(0.9)))
        }
        for (label, i) in [("40 Hz", 0), ("300", 11), ("1k", 17), ("3k", 23), ("16k", n - 1)] {
            ctx.draw(Text(label).font(.system(size: 9)).foregroundColor(.white.opacity(0.6)),
                     at: CGPoint(x: 8 + CGFloat(i) * (w + gap) + w / 2, y: size.height - 6))
        }
    }
}
