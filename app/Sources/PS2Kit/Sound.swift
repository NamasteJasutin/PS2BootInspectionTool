import Foundation

/// Sony's OSD sound data: an "SShd" bank header, a headerless ADPCM body and "SSsq"
/// sequences, played by the IOP driver the way `notes/sound.md` describes.
public enum SoundError: Error, CustomStringConvertible {
    case badBank(String)
    case badSequence(String)

    public var description: String {
        switch self {
        case .badBank(let m): return "sound bank: \(m)"
        case .badSequence(let m): return "sound sequence: \(m)"
        }
    }
}

public struct Tone {
    public let low: Int, high: Int, root: Int, fine: Int
    public let sampleOffset: Int          // byte offset into the body
    public let adsr1: UInt16, adsr2: UInt16
    public let volume: Int, pan: Int, bendRange: Int
    public let flags: UInt8
}

public struct Program {
    public enum Kind { case split, layer, drum }
    public let kind: Kind
    public let volume: Int
    public let drumBase: Int
    public let tones: [Tone]
}

public struct SoundBank {
    public let programs: [Int: Program]
    public let velocity: [Int]            // 128-entry curve
    public let body: Data

    public init(header: Data, body: Data) throws {
        guard header.count > 0x30, header.bytes(0xC, 4) == Data("SShd".utf8) else { throw SoundError.badBank("signature") }
        self.body = body
        var programs: [Int: Program] = [:]
        let base = Int(header.u32(0x10))
        if base != 0xFFFF_FFFF {
            let maxProgram = Int(header.u16(base))
            for p in 0 ... maxProgram {
                let off = Int(header.u16(base + 2 + 2 * p))
                if off == 0xFFFF { continue }
                let h = base + off
                guard h + 8 <= header.count else { throw SoundError.badBank("program \(p) out of range") }
                let mode = header.u8(h)
                let kind: Program.Kind = mode == 0xFF ? .drum : (mode & 0x80 != 0 ? .layer : .split)
                let count = kind == .drum ? 0 : Int(mode & 0x7F) + 1
                var tones: [Tone] = []
                for t in 0 ..< count {
                    let o = h + 8 + 16 * t
                    guard o + 16 <= header.count else { throw SoundError.badBank("tone out of range") }
                    tones.append(Tone(low: Int(header.u8(o)), high: Int(header.u8(o + 1)), root: Int(header.u8(o + 2)),
                                      fine: Int(Int8(bitPattern: header.u8(o + 3))), sampleOffset: Int(header.u16(o + 4)) * 8,
                                      adsr1: header.u16(o + 6), adsr2: header.u16(o + 8),
                                      volume: Int(header.u8(o + 11)), pan: Int(header.u8(o + 12)),
                                      bendRange: Int(header.u8(o + 13)), flags: header.u8(o + 15)))
                }
                programs[p] = Program(kind: kind, volume: Int(header.u8(h + 1)), drumBase: Int(header.u8(h + 6)), tones: tones)
            }
        }
        self.programs = programs
        let velOff = Int(header.u32(0x14))
        if velOff != 0xFFFF_FFFF, velOff + 2 + 128 <= header.count {
            velocity = (0 ..< 128).map { Int(header.u8(velOff + 2 + $0)) }
        } else {
            velocity = Array(0 ..< 128)
        }
    }

    /// Decodes one PS-ADPCM sample starting at `offset`: 16-byte blocks of a shift/filter
    /// byte, a flag byte and 28 nibbles. Returns PCM and the loop start, if it loops.
    public func decodeSample(at offset: Int) -> (pcm: [Float], loopStart: Int?) {
        let filters: [(Int32, Int32)] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)]
        var out: [Float] = []
        var s1: Int32 = 0, s2: Int32 = 0
        var loopStart: Int?
        var loops = false
        var pos = offset
        let b = [UInt8](body)
        while pos + 16 <= b.count {
            let shift = Int32(b[pos] & 0xF), filter = Int(b[pos] >> 4), flags = b[pos + 1]
            let (f0, f1) = filter < 5 ? filters[filter] : (0, 0)
            if flags & 4 != 0 { loopStart = out.count }
            for i in 0 ..< 28 {
                let byte = b[pos + 2 + i / 2]
                var nib = Int32(i & 1 == 1 ? byte >> 4 : byte & 0xF)
                if nib >= 8 { nib -= 16 }
                var s = shift <= 12 ? (nib << 12) >> shift : (nib << 12) >> 9
                s += (s1 * f0 + s2 * f1 + 32) >> 6
                s = max(-32768, min(32767, s))
                out.append(Float(s))
                s2 = s1; s1 = s
            }
            pos += 16
            if flags & 1 != 0 { loops = flags & 2 != 0; break }
        }
        return (out, loops ? loopStart : nil)
    }
}

public struct SequenceEvent {
    public enum Kind { case noteOn(note: Int, velocity: Int), noteOff(note: Int), control(Int, Int), program(Int), bend(Int), tempo(Int), end }
    public let kind: Kind
    public let channel: Int
    public let tick: Int
    /// Index of the driver's 60 Hz update at which the event fires.
    public var update: Int = 0
}

public struct SoundSequence {
    public struct Channel { public var program: Int; public var volume: Int; public var pan: Int; public var bend: Int }
    public let volume: Int
    public let resolution: Int
    public let bpm: Int
    public let channels: [Channel]
    public let events: [SequenceEvent]

    public init(data d: Data) throws {
        guard d.count > 0x110, d.bytes(0xC, 4) == Data("SSsq".utf8) else { throw SoundError.badSequence("signature") }
        volume = Int(d.u8(0))
        resolution = Int(d.u16(2))
        bpm = Int(d.u16(4))
        channels = (0 ..< 16).map { c in
            let o = 0x10 + 16 * c
            return Channel(program: Int(d.u8(o + 2)), volume: Int(d.u8(o + 3)), pan: Int(d.u8(o + 4)), bend: Int(d.u8(o + 10)))
        }
        var events: [SequenceEvent] = []
        var pos = 0x110, tick = 0, status: UInt8 = 0
        parse: while pos < d.count {
            if d.u8(pos) & 0x80 != 0 { status = d.u8(pos) } else { pos -= 1 }   // running status
            let d1 = pos + 1 < d.count ? Int(d.u8(pos + 1)) : 0
            let d2 = pos + 2 < d.count ? Int(d.u8(pos + 2)) : 0
            let hi = status & 0xF0, ch = Int(status & 0xF)
            let kind: SequenceEvent.Kind
            switch hi {
            case 0x90 where d2 != 0: kind = .noteOn(note: d1, velocity: d2); pos += 3
            case 0x80, 0x90: kind = .noteOff(note: d1); pos += 3
            case 0xB0: kind = .control(d1, d2); pos += 3
            case 0xC0: kind = .program(d1); pos += 2
            case 0xE0: kind = .bend(d1); pos += 2
            default:
                if status == 0xFF, d1 == 0x2F {
                    events.append(SequenceEvent(kind: .end, channel: 0, tick: tick))
                    break parse
                }
                if status == 0xFF, d1 == 0x51 { kind = .tempo(Int(d.u16(pos + 2))); pos += 4 } else {
                    throw SoundError.badSequence("unknown event \(status) at \(pos)")
                }
            }
            events.append(SequenceEvent(kind: kind, channel: ch, tick: tick))
            var delta = 0
            while pos < d.count {
                let b = Int(d.u8(pos)); pos += 1
                delta = delta << 7 | (b & 0x7F)
                if b & 0x80 == 0 { break }
            }
            tick += delta
        }
        // The driver keeps a 20.12 fixed-point countdown and fires events while it is <= 0,
        // subtracting (resolution * bpm << 12) / 60 / 60 per 60 Hz update.
        var bpmNow = bpm, acc = 0, update = 0, last = 0
        for i in events.indices {
            acc += (events[i].tick - last) << 12
            last = events[i].tick
            let step = (resolution * bpmNow << 12) / 60 / 60
            while acc >= 1 { acc -= step; update += 1 }
            events[i].update = update
            if case .tempo(let t) = events[i].kind { bpmNow = t }
        }
        self.events = events
    }
}

/// The driver's lookup tables, read from the IOP module when a layout is known.
public struct DriverTables {
    /// 608 entries, 16 steps per semitone, [208] = 0x1000.
    public let pitch: [Int]
    /// 32 (left, right) gain pairs indexed by pan >> 2.
    public let pan: [(Int, Int)]

    public static let computed = DriverTables(
        pitch: (0 ..< 608).map { Int((4096 * pow(2, Double($0 - 208) / 192)).rounded()) },
        pan: (0 ..< 32).map { i in
            // Constant-power-ish curve approximating the driver's table ends (128,0) ... (0,128).
            let a = Double(i) / 31 * .pi / 2
            return (Int((cos(a) * 128).rounded()), Int((sin(a) * 128).rounded()))
        })

    public init(pitch: [Int], pan: [(Int, Int)]) { self.pitch = pitch; self.pan = pan }

    init?(driverModule d: Data, pitchOffset: Int, panOffset: Int) {
        guard pitchOffset + 608 * 2 <= d.count, panOffset + 64 <= d.count else { return nil }
        pitch = (0 ..< 608).map { Int(d.u16(pitchOffset + $0 * 2)) }
        pan = (0 ..< 32).map { (Int(d.u8(panOffset + $0 * 2)), Int(d.u8(panOffset + $0 * 2 + 1))) }
    }
}

/// Renders a sequence through the bank into 48 kHz stereo float PCM, voice by voice, with
/// the driver's pitch and volume arithmetic and the SPU envelope generator.
public struct SequenceSynth {
    public static let sampleRate = 48000
    public static let updatesPerSecond = 60.0

    public let bank: SoundBank
    public let tables: DriverTables

    public init(bank: SoundBank, tables: DriverTables) { self.bank = bank; self.tables = tables }

    struct Voice {
        let start: Int           // output sample
        var off: Int?
        let tone: Tone
        let pitch: Int
        let volL: Int, volR: Int
    }

    func pitchRegister(root: Int, note: Int, fine: Int, bend: Int, bendRange: Int) -> Int {
        let b = ((bend - 0x40) * bendRange) >> 2
        let idx: Int, value: Int
        if note < root {
            let d = root - note
            idx = (12 - d % 12) * 16 + b + 0xD0 + fine
            value = (tables.pitch[max(0, min(607, idx))] * 44100) >> (d / 12 + 1)
        } else {
            let d = note - root
            idx = (d % 12) * 16 + b + 0xD0 + fine
            value = (tables.pitch[max(0, min(607, idx))] * 44100) << (d / 12)
        }
        return (value / 48000) & 0xFFFF
    }

    func volumeRegister(seq: Int, channel: Int, program: Int, velocity: Int, tone: Int, pan: Int) -> Int {
        ((((seq * channel * program * velocity) >> 14) * tone * pan) >> 14) & 0x7FFF
    }

    /// SPU ADSR level (0...0x7FFF) per output sample; the key is released after `onSamples`.
    static func envelope(adsr1 a1: UInt16, adsr2 a2: UInt16, onSamples: Int, total: Int) -> [Float] {
        var env = [Float](repeating: 0, count: total)
        let ar = Int(a1 >> 8 & 0x7F), dr = Int(a1 >> 4 & 0xF), sl = Int(a1 & 0xF)
        let aExp = a1 & 0x8000 != 0
        let sExp = a2 & 0x8000 != 0, sDec = a2 & 0x4000 != 0, sr = Int(a2 >> 6 & 0x7F)
        let rExp = a2 & 0x20 != 0, rr = Int(a2 & 0x1F)
        let sustainLevel = (sl + 1) * 0x800
        var level = 0, i = 0
        enum Phase { case attack, decay, sustain, release }
        var phase = Phase.attack
        while i < total {
            if phase != .release, i >= onSamples { phase = .release }
            let shift: Int, step: Int, exp: Bool, dec: Bool
            switch phase {
            case .attack: shift = ar >> 2; step = 7 - (ar & 3); exp = aExp; dec = false
            case .decay: shift = dr; step = -8; exp = true; dec = true
            case .sustain: shift = sr >> 2; exp = sExp; dec = sDec; step = sDec ? -8 + (sr & 3) : 7 - (sr & 3)
            case .release: shift = rr; step = -8; exp = rExp; dec = true
            }
            var cycles = 1 << max(0, shift - 11)
            var st = step << max(0, 11 - shift)
            if exp, !dec, level > 0x6000 { cycles *= 4 }
            if exp, dec { st = (st * level) >> 15 }
            var end = min(total, i + cycles)
            if phase != .release, i < onSamples, onSamples < end { end = onSamples }
            for k in i ..< end { env[k] = Float(level) }
            i = end
            level = max(0, min(0x7FFF, level + st))
            if phase == .attack, level >= 0x7FFF { phase = .decay }
            if phase == .decay, level <= sustainLevel { phase = .sustain }
            if phase == .release, level == 0 { break }
        }
        return env
    }

    /// Mixes `sequence` into `mix` (interleaved stereo) starting `startSeconds` in, growing
    /// the buffer as needed. `sequenceVolume` replaces the file's master volume.
    public func render(_ sequence: SoundSequence, sequenceVolume: Int, startSeconds: Double, tail: Double = 3,
                       maxSeconds: Double = .infinity, into mix: inout [Float]) {
        let rate = Self.sampleRate
        let base = Int(startSeconds * Double(rate))
        var channels = sequence.channels
        var voices: [Voice] = []
        var active: [Int: [Int]] = [:]          // channel << 8 | note -> voice indices
        for e in sequence.events where Double(e.update) / Self.updatesPerSecond <= maxSeconds {
            let t = base + Int(Double(e.update) / Self.updatesPerSecond * Double(rate))
            switch e.kind {
            case .program(let p):
                channels[e.channel].program = p
                channels[e.channel].bend = 0x40
            case .control(7, let v): channels[e.channel].volume = v
            case .control(10, let v): channels[e.channel].pan = v
            case .bend(let v): channels[e.channel].bend = v
            case .noteOn(let note, let velocity):
                let c = channels[e.channel]
                guard let program = bank.programs[c.program] else { continue }
                for tone in program.tones where tone.low <= note && note <= tone.high {
                    let p = max(0, min(0x7F, c.pan + tone.pan - 0x40)) >> 2
                    let (gl, gr) = tables.pan[p]
                    let vel = bank.velocity[min(127, velocity)]
                    voices.append(Voice(
                        start: t, off: nil, tone: tone,
                        pitch: pitchRegister(root: tone.root, note: note, fine: tone.fine, bend: c.bend, bendRange: tone.bendRange),
                        volL: volumeRegister(seq: sequenceVolume, channel: c.volume, program: program.volume, velocity: vel, tone: tone.volume, pan: gl),
                        volR: volumeRegister(seq: sequenceVolume, channel: c.volume, program: program.volume, velocity: vel, tone: tone.volume, pan: gr)))
                    active[e.channel << 8 | note, default: []].append(voices.count - 1)
                    if program.kind == .split { break }
                }
            case .noteOff(let note):
                for v in active.removeValue(forKey: e.channel << 8 | note) ?? [] { voices[v].off = t }
            default:
                break
            }
        }
        let endUpdate = sequence.events.last?.update ?? 0
        let seconds = min(Double(endUpdate) / Self.updatesPerSecond, maxSeconds)
        let total = base + Int((seconds + tail) * Double(rate))
        if mix.count < total * 2 { mix.append(contentsOf: [Float](repeating: 0, count: total * 2 - mix.count)) }

        var samples: [Int: (pcm: [Float], loopStart: Int?)] = [:]
        let master = Float(0x3FFF) / 16384
        for v in voices {
            if samples[v.tone.sampleOffset] == nil { samples[v.tone.sampleOffset] = bank.decodeSample(at: v.tone.sampleOffset) }
            let (pcm, loop) = samples[v.tone.sampleOffset]!
            let n = total - v.start
            guard n > 0 else { continue }
            let env = Self.envelope(adsr1: v.tone.adsr1, adsr2: v.tone.adsr2, onSamples: v.off.map { $0 - v.start } ?? n, total: n)
            let gainL = Float(v.volL) / 16384 * master, gainR = Float(v.volR) / 16384 * master
            let step = Double(v.pitch) / 4096
            let length = pcm.count
            var pos = 0.0
            for k in 0 ..< env.count {
                let e = env[k]
                if e == 0, k > 0, pos > Double(length), loop == nil { break }
                var i0 = Int(pos)
                let frac = Float(pos - Double(i0))
                var i1 = i0 + 1
                if let loop {
                    let span = length - loop
                    if i0 >= length { i0 = loop + (i0 - loop) % span }
                    if i1 >= length { i1 = loop + (i1 - loop) % span }
                } else if i1 >= length {
                    break
                }
                let s = (pcm[i0] * (1 - frac) + pcm[i1] * frac) * e / 32768 / 32768
                mix[(v.start + k) * 2] += s * gainL
                mix[(v.start + k) * 2 + 1] += s * gainR
                pos += step
            }
        }
    }
}

/// The boot chime as OSDSYS schedules it: the bank and sequences come from `rom0:SNDIMAGE`,
/// the driver tables from `rom0:OSDSND`. Both cues are rendered separately so the app can
/// place the transition cue wherever the dive happens to start.
public struct BootSound {
    public let sampleRate = SequenceSynth.sampleRate
    /// `SNDBOOTS`, interleaved stereo, starting at opening frame 0.
    public let chime: [Float]
    /// `SNDTNNLS`, the cue the opening starts when the dive begins (no disc / menu boot).
    public let cue: [Float]
    /// `SNDWARNS`, the ambient loop under the warning scene (first minute of a 5.5-minute piece).
    public let warning: [Float]

    public init(biosURL: URL, driverTableOffsets: (pitch: Int, pan: Int)? = (0x1DBB0 + 0xA0, 0x1E070 + 0xA0)) throws {
        let rom = try ROMDirectory(data: try Data(contentsOf: biosURL))
        let archive = try ROMDirectory(data: try rom.module("SNDIMAGE"))
        func asset(_ name: String) throws -> Data { try OSDCompression.unpack(try archive.module(name)) }
        let bank = try SoundBank(header: try asset("SNDBOOTH"), body: try asset("SNDBOOTB"))
        var tables = DriverTables.computed
        if let offsets = driverTableOffsets, let driver = try? rom.module("OSDSND"),
           let t = DriverTables(driverModule: driver, pitchOffset: offsets.pitch, panOffset: offsets.pan) {
            tables = t
        }
        let synth = SequenceSynth(bank: bank, tables: tables)
        var a: [Float] = [], b: [Float] = []
        synth.render(try SoundSequence(data: try asset("SNDBOOTS")), sequenceVolume: 0x42, startSeconds: 0, into: &a)
        synth.render(try SoundSequence(data: try asset("SNDTNNLS")), sequenceVolume: 0x2A, startSeconds: 0, into: &b)
        var w: [Float] = []
        synth.render(try SoundSequence(data: try asset("SNDWARNS")), sequenceVolume: 0x36, startSeconds: 0, tail: 0, maxSeconds: 60, into: &w)
        chime = a
        cue = b
        warning = w
    }

    /// Both cues mixed, with the transition cue placed at `diveFrame` (of `fps` per second).
    public func mixed(diveFrame: Int, fps: Float = 60) -> [Float] {
        let offset = Int(Double(diveFrame) / Double(fps) * Double(sampleRate)) * 2
        var out = chime
        if out.count < offset + cue.count { out.append(contentsOf: [Float](repeating: 0, count: offset + cue.count - out.count)) }
        for i in 0 ..< cue.count { out[offset + i] += cue[i] }
        return out
    }
}
