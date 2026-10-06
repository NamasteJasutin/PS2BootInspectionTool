import Accelerate
import AVFoundation
import Foundation
import PS2Kit

/// Plays the synthesised chime in step with the opening's clock. The audio thread advances
/// on its own at the playback speed; the UI only re-seats it when it drifts or jumps.
final class AudioPlayer {
    private let engine = AVAudioEngine()
    private var node: AVAudioSourceNode!
    private var lock = os_unfair_lock()
    /// A piece of audio placed on the sequence clock.
    struct Clip {
        var start: Int            // stereo frame
        var pcm: [Float]
        var loop = false
        var fadeStart: Int?       // stop command with release 0xF: 1.37 s linear fade
    }
    private var clips: [Clip] = []
    private var position = 0.0        // stereo frames into the sequence
    private var rate = 1.0
    private var playing = false
    private var gain: Float = 1
    private(set) var ready = false
    let sampleRate = Double(SequenceSynth.sampleRate)
    private(set) var sounds: BootSound?
    private(set) var logoChime: [Float] = []

    init() {
        let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 2)!
        node = AVAudioSourceNode(format: format) { [unowned self] _, _, frameCount, audioBufferList -> OSStatus in
            self.render(Int(frameCount), UnsafeMutableAudioBufferListPointer(audioBufferList))
            return noErr
        }
        engine.attach(node)
        engine.connect(node, to: engine.mainMixerNode, format: format)
        engine.prepare()
    }

    func load(_ sound: BootSound) { sounds = sound; ready = true }
    func loadLogoChime(_ pcm: [Float]) { logoChime = pcm }

    /// Places the clips for a scene; frames are converted with `fps`.
    func arrange(scene: SceneKind, fps: Float, diveFrame: Int, logoStart: Int, openingStart: Int) {
        func at(_ frame: Int) -> Int { Int(Double(frame) / Double(fps) * sampleRate) }
        var list: [Clip] = []
        switch scene {
        case .boot, .full:
            if let s = sounds {
                list.append(Clip(start: at(openingStart), pcm: s.chime))
                list.append(Clip(start: at(diveFrame), pcm: s.cue))
            }
            if scene == .full { list.append(Clip(start: at(logoStart), pcm: logoChime)) }
        case .warning:
            if let s = sounds { list.append(Clip(start: 0, pcm: s.warning, loop: true, fadeStart: at(diveFrame))) }
        case .logo:
            list.append(Clip(start: 0, pcm: logoChime))
        }
        os_unfair_lock_lock(&lock)
        clips = list
        os_unfair_lock_unlock(&lock)
    }

    /// Mixed sample at a stereo frame index (for the visualiser), or silence.
    func sample(_ frame: Int, right: Bool) -> Float {
        let c = right ? 1 : 0
        var v: Float = 0
        for clip in clips {
            var k = frame - clip.start
            guard k >= 0, !clip.pcm.isEmpty else { continue }
            if clip.loop { k %= clip.pcm.count / 2 } else if k * 2 + c >= clip.pcm.count { continue }
            var s = clip.pcm[k * 2 + c]
            if let f = clip.fadeStart, frame > f { s *= max(0, 1 - Float(frame - f) / Float(sampleRate * 1.37)) }
            v += s
        }
        return v * gain
    }

    /// Called once per display frame with the sequence clock.
    func sync(frame: Float, speed: Double, playing: Bool, fps: Float, enabled: Bool, volume: Float) {
        let target = Double(frame) / Double(fps) * sampleRate
        os_unfair_lock_lock(&lock)
        rate = speed
        gain = volume
        let wasPlaying = self.playing
        self.playing = playing && enabled
        if !wasPlaying || abs(position - target) > sampleRate * 0.06 { position = target }
        os_unfair_lock_unlock(&lock)
        if self.playing, !engine.isRunning { try? engine.start() }
        if !self.playing, engine.isRunning, !enabled { engine.pause() }
    }

    private func render(_ frames: Int, _ buffers: UnsafeMutableAudioBufferListPointer) {
        let left = buffers[0].mData!.assumingMemoryBound(to: Float.self)
        let right = buffers.count > 1 ? buffers[1].mData!.assumingMemoryBound(to: Float.self) : left
        os_unfair_lock_lock(&lock)
        defer { os_unfair_lock_unlock(&lock) }
        for i in 0 ..< frames {
            guard playing else { left[i] = 0; right[i] = 0; continue }
            let p = Int(position.rounded(.down)), f = Float(position - Double(p))
            left[i] = (sample(p, right: false) * (1 - f) + sample(p + 1, right: false) * f)
            right[i] = (sample(p, right: true) * (1 - f) + sample(p + 1, right: true) * f)
            position += rate
        }
    }
}

enum VisualizerMode: String, CaseIterable, Identifiable {
    case off = "Off", volume = "Volume", wave = "Wave", equalizer = "Equalizer"
    var id: String { rawValue }
}

/// What the visualiser draws this frame.
struct VisualizerSnapshot {
    var rmsDB: (Float, Float) = (-90, -90)
    var peakDB: (Float, Float) = (-90, -90)
    var peakHoldDB: (Float, Float) = (-90, -90)
    var wave: [Float] = []            // mono, -1...1
    var bands: [Float] = []           // 0...1 per band
    var bandPeaks: [Float] = []
}

/// Measures the audio around the current time; keeps a little state for peak-hold decay.
final class VisualizerAnalysis {
    static let bandCount = 32
    private let fftSize = 2048
    private let fft: vDSP.FFT<DSPSplitComplex>
    private let window: [Float]
    private var holdL: Float = -90, holdR: Float = -90
    private var bandPeaks = [Float](repeating: 0, count: VisualizerAnalysis.bandCount)
    private var bandSmooth = [Float](repeating: 0, count: VisualizerAnalysis.bandCount)
    private let bandEdges: [Int]

    init() {
        fft = vDSP.FFT(log2n: vDSP_Length(log2(Double(2048))), radix: .radix2, ofType: DSPSplitComplex.self)!
        window = vDSP.window(ofType: Float.self, usingSequence: .hanningDenormalized, count: fftSize, isHalfWindow: false)
        // Log-spaced bands from 40 Hz to 16 kHz over 2048-point bins at 48 kHz.
        let binHz = 48000.0 / Double(fftSize)
        bandEdges = (0 ... Self.bandCount).map { i in
            Int((40 * pow(16000 / 40, Double(i) / Double(Self.bandCount))) / binHz)
        }
    }

    func measure(player: AudioPlayer, frame: Int, mode: VisualizerMode) -> VisualizerSnapshot {
        var snap = VisualizerSnapshot()
        guard mode != .off else { return snap }
        let n = fftSize
        var mono = [Float](repeating: 0, count: n)
        var sumL: Float = 0, sumR: Float = 0, peakL: Float = 0, peakR: Float = 0
        for i in 0 ..< n {
            let l = player.sample(frame - n + i, right: false), r = player.sample(frame - n + i, right: true)
            mono[i] = (l + r) * 0.5
            if i >= n - 2400 {      // last 50 ms for the meters
                sumL += l * l; sumR += r * r
                peakL = max(peakL, abs(l)); peakR = max(peakR, abs(r))
            }
        }
        func db(_ v: Float) -> Float { max(-90, 20 * log10(max(v, 1e-6))) }
        snap.rmsDB = (db((sumL / 2400).squareRoot()), db((sumR / 2400).squareRoot()))
        snap.peakDB = (db(peakL), db(peakR))
        holdL = max(snap.peakDB.0, holdL - 0.4)
        holdR = max(snap.peakDB.1, holdR - 0.4)
        snap.peakHoldDB = (holdL, holdR)
        if mode == .wave {
            snap.wave = Array(mono.suffix(1024))
        }
        if mode == .equalizer {
            var windowed = [Float](repeating: 0, count: n)
            vDSP.multiply(mono, window, result: &windowed)
            var real = [Float](repeating: 0, count: n / 2), imag = [Float](repeating: 0, count: n / 2)
            real.withUnsafeMutableBufferPointer { rp in
                imag.withUnsafeMutableBufferPointer { ip in
                    var split = DSPSplitComplex(realp: rp.baseAddress!, imagp: ip.baseAddress!)
                    windowed.withUnsafeBufferPointer { wp in
                        wp.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: n / 2) {
                            vDSP_ctoz($0, 2, &split, 1, vDSP_Length(n / 2))
                        }
                    }
                    fft.forward(input: split, output: &split)
                }
            }
            var mags = [Float](repeating: 0, count: n / 2)
            for i in 0 ..< n / 2 { mags[i] = (real[i] * real[i] + imag[i] * imag[i]).squareRoot() / Float(n) }
            var bands = [Float](repeating: 0, count: Self.bandCount)
            for b in 0 ..< Self.bandCount {
                let lo = max(1, bandEdges[b]), hi = max(lo + 1, bandEdges[b + 1])
                let m = mags[lo ..< min(hi, n / 2)].max() ?? 0
                bands[b] = max(0, (db(m) + 72) / 72)         // -72 dB ... 0 dB -> 0...1
                bandSmooth[b] = bands[b] > bandSmooth[b] ? bands[b] : bandSmooth[b] * 0.85
                bandPeaks[b] = max(bandSmooth[b], bandPeaks[b] - 0.012)
            }
            snap.bands = bandSmooth
            snap.bandPeaks = bandPeaks
        }
        return snap
    }
}
