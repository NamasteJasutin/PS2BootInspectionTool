import Foundation
import simd

/// One tower of the opening, derived from a history record.
public struct Tower {
    public let column: Int
    public let row: Int
    public let record: Int
    /// World-space centre (already pushed back so the near cap lines up).
    public let centre: SIMD3<Float>
    public let halfLength: Float
    /// Vertex brightness of the near cap in GS units (128 = texture colour unchanged).
    public let brightness: Float
    /// Still-growing towers sway; finished ones stand still.
    public let growing: Bool
    public let quarterTurns: Int
    public let uvOffset: Float
}

/// Static scene data built once per (assets, history) pair.
public struct OpeningScene {
    public static let towerHalfWidth: Float = 2
    public let towers: [Tower]

    public init(assets: OpeningAssets, history: PlayHistory) {
        var out: [Tower] = []
        for (i, rec) in history.records.enumerated() where !rec.isEmpty && i < assets.slots.count {
            let c = Int(rec.count)
            let step = min(13, c < 14 ? c : 4 + (c - 14) % 10)
            for (k, slot) in assets.slots[i].enumerated() {
                let fill: Float, size: Float
                if k == Int(rec.index) {
                    fill = assets.fill[step]; size = assets.size[step]
                } else if rec.mask >> UInt8(k) & 1 == 1 {
                    fill = 1; size = 1
                } else {
                    continue
                }
                let (col, row) = slot
                let m = assets.towerPositions[col][row]
                let h = max(size * 30, 3)
                let centre = SIMD3<Float>((m.x + 4.8) * 4, (m.y - 6.5) * 4, (m.z + 4) * 12 + 150 + fill * 30 - h)
                let alpha = fill >= 1 ? 0 : Int((1 - fill) * 128)
                let factor = alpha == 0 ? h / 30 : Float(alpha) / 128
                let a = col + 3, b = row + 6
                out.append(Tower(
                    column: col, row: row, record: i, centre: centre, halfLength: h,
                    brightness: Self.lightMap(a, b) * factor,
                    growing: fill != 1,
                    quarterTurns: ((a + b) * a / (row + 7)) % 4,
                    uvOffset: Float((a + b) * (col + 8) / (row + 7) + (a + b) * (col + 7) / (row + 9)) / 256))
            }
        }
        towers = out
    }

    /// Precomputed brightness over the grid: two radial falloffs plus a fixed per-cell jitter.
    static func lightMap(_ inner: Int, _ outer: Int) -> Float {
        let radius = Float(5202).squareRoot()
        func cell(_ k: Int) -> Float { Float(2 * k - 20) * 5.1 * 0.5 + 2.55 }
        let d1 = hypotf(-5.1 - cell(inner), 0 - cell(outer))
        let v1 = min(255, max(32, (radius - 2 * d1) * 255 / radius))
        let d2 = hypotf(10.2 - cell(inner), 5.1 - cell(outer))
        let v2 = min(255, max(32, (radius - 4 * d2) * 255 / radius * 0.5))
        let jitter = Float(((inner + outer) * inner / (outer + 1)) % 11 - 5) * 10
        return min(220, max(32, (v1 + v2) * 0.85 - jitter))
    }
}

/// The scripted camera: a point moving along +Z that rolls about its view axis.
public struct CameraState {
    public var z: Float
    public var roll: Float
    public var stage: Int
    /// Downward tilt of the view direction (the warning scene looks slightly down).
    public var tilt: Float = 0

    public var position: SIMD3<Float> { SIMD3(0, 0, z) }
    public var forward: SIMD3<Float> { SIMD3(0, tilt, 1) }
    public var up: SIMD3<Float> { SIMD3(sin(roll), cos(roll), 0) }
}

/// The console's video mode. The opening integrates its motion with a 1.2 time step at
/// 50 Hz so that both modes take the same wall-clock time, and draws into a field of 224 or
/// 256 lines with a different pixel-aspect factor.
public enum VideoMode: String, CaseIterable, Identifiable {
    case ntsc = "NTSC 60 Hz"
    case pal = "PAL 50 Hz"
    public var id: String { rawValue }
    public var framesPerSecond: Float { self == .pal ? 50 : 60 }
    public var timeStep: Float { self == .pal ? 1.2 : 1 }
    public var fieldHeight: Float { self == .pal ? 256 : 224 }
    public var aspectY: Float { self == .pal ? 0.526271 : 0.457627 }
}

/// What the console is busy with while the screen is still black after power-on. Durations
/// are the mid-points of the estimates in `notes/boot_sequence.md` (nothing was measured).
public struct BootPhase {
    public let name: String
    public let seconds: Float
    public static let all: [BootPhase] = [
        BootPhase(name: "IOP boot #1: IOPBOOT + 29 modules from ROM", seconds: 0.55),
        BootPhase(name: "EELOAD loads rom0:OSDSYS (363 KB)", seconds: 0.20),
        BootPhase(name: "OSDSYS stub decompresses itself to 0x200000", seconds: 0.06),
        BootPhase(name: "IOP boot #2: rom0:UDNL rom0:OSDCNF, 39 modules", seconds: 0.85),
        BootPhase(name: "Memory card mount (sceMcGetInfo, system folder)", seconds: 0.30),
        BootPhase(name: "Asset archives: 1.7 MB read, 1.1 MB LZ-decoded", seconds: 0.70),
        BootPhase(name: "Sound bank upload to SPU2 (410 KB)", seconds: 0.27),
        BootPhase(name: "CDVD S-commands, NVRAM config, history read, threads", seconds: 0.10),
        BootPhase(name: "Video init: GS reset, sync, cleared buffers", seconds: 0.05),
    ]
    public static var totalSeconds: Float { all.reduce(0) { $0 + $1.seconds } }

    /// Index of the phase active `elapsed` seconds into a black period of `total` seconds.
    public static func index(elapsed: Float, total: Float) -> Int {
        let scaled = elapsed / max(total, 0.01) * totalSeconds
        var t: Float = 0
        for (i, p) in all.enumerated() {
            t += p.seconds
            if scaled < t { return i }
        }
        return all.count - 1
    }
}

/// Which of the opening's two scenes is being shown.
public enum SceneKind: String, CaseIterable, Identifiable {
    case boot = "Boot (towers)"
    case warning = "Warning (insert disc)"
    case logo = "PlayStation 2 logo (disc boot)"
    public var id: String { rawValue }
}

/// The opening's stage machine and camera kinematics, precomputed for every frame.
///
/// Frames are 60 Hz (the console scales its time step by 1.2 at 50 Hz, so PAL plays the
/// same motion with fewer frames).
public struct Timeline {
    public let video: VideoMode
    public var framesPerSecond: Float { video.framesPerSecond }

    public let states: [CameraState]
    public let kind: SceneKind
    /// Frame at which the dive starts (boot scene) or the exit fade starts (warning scene).
    public let diveFrame: Int
    /// First frame after the scene has ended.
    public var endFrame: Int { states.count - 1 }

    /// The warning scene: dolly from z = 672 to 800, then hold with a slow roll until the
    /// drive reports a change (`exitFrame`), after which the scene fades for 128 frames.
    public static func warning(exitFrame: Int, video: VideoMode = .ntsc) -> Timeline {
        let dt = video.timeStep
        var z: Float = 672, vz: Float = 2.16, az: Float = -0.0178
        var roll: Float = 0
        let vr: Float = 0.00462
        var stage = 4
        var out = [CameraState(z: z, roll: roll, stage: stage, tilt: -0.03)]
        let thresholds: [Float] = [16, 56, 104, 320, 672, 800, 1160]
        for _ in 0 ..< exitFrame + 129 {
            if stage < 6, thresholds[stage] < z { stage += 1 }
            if stage == 6 { vz = 0; az = 0 }
            vz += az * dt
            z += (2 * vz + az) * 0.5 * dt
            roll += vr * dt
            if roll > .pi { roll -= 2 * .pi }
            out.append(CameraState(z: z, roll: roll, stage: stage, tilt: -0.03))
        }
        return Timeline(states: out, kind: .warning, diveFrame: exitFrame, video: video)
    }

    /// The logo program: its animated fields followed by the 120-field hold.
    public static func logo(video: VideoMode) -> Timeline {
        let animated = (video == .pal ? 35 - 14 : 42 - 17) + 1
        let states = [CameraState](repeating: CameraState(z: 0, roll: 0, stage: 0), count: animated + 120 + 1)
        return Timeline(states: states, kind: .logo, diveFrame: 0, video: video)
    }

    private init(states: [CameraState], kind: SceneKind, diveFrame: Int, video: VideoMode) {
        self.states = states; self.kind = kind; self.diveFrame = diveFrame; self.video = video
    }

    /// - Parameter discSettledFrame: when the drive has finished identifying the disc
    ///   (0 = already known). The dive needs this and at least two seconds of drift.
    public init(discSettledFrame: Int = 0, video: VideoMode = .ntsc) {
        self.video = video
        let dt = video.timeStep
        let fps = Int(video.framesPerSecond)
        let thresholds: [Float] = [16, 56, 104]
        var z: Float = 16, vz: Float = 0.04, az: Float = 0, jz: Float = 0
        var roll: Float = -0.12, vr: Float = 0.001, ar: Float = 0
        var stage = 0, diving = false, dive = -1
        var out = [CameraState(z: z, roll: roll, stage: 0)]
        var frame = 0
        while stage < 3, frame < 4000 {
            if thresholds[stage] < z { stage += 1 }
            switch stage {
            case 1:
                jz = 4e-7
                if frame >= discSettledFrame, frame > 2 * fps { diving = true; stage = 2 }
            case 2:
                if frame > 10 * fps { diving = true }
                if diving {
                    if dive < 0 { dive = frame }
                    az = 0.0099
                    ar = 0.000195
                }
            default:
                break
            }
            vr += ar * dt
            vz += (2 * az + jz) * 0.5 * dt
            az += jz * dt
            roll += (2 * vr + ar) * 0.5 * dt
            z += (2 * vz + az) * 0.5 * dt
            if roll > .pi { roll -= 2 * .pi }
            if roll < -.pi { roll += 2 * .pi }
            frame += 1
            out.append(CameraState(z: z, roll: roll, stage: stage))
        }
        states = out
        kind = .boot
        diveFrame = max(dive, 0)
    }

    /// Camera at a (possibly fractional) frame; clamps outside the scene.
    public func camera(at frame: Float) -> CameraState {
        let f = min(max(frame, 0), Float(endFrame))
        let i = min(Int(f), endFrame - 1 < 0 ? 0 : endFrame - 1)
        let a = states[i], b = states[min(i + 1, endFrame)]
        let t = f - Float(i)
        return CameraState(z: a.z + (b.z - a.z) * t, roll: a.roll + (b.roll - a.roll) * t, stage: a.stage, tilt: a.tilt)
    }

    /// First frame whose camera has passed depth `z`.
    public func frame(passing z: Float) -> Int {
        states.firstIndex { $0.z > z } ?? endFrame
    }
}

/// Closed-form animation of everything that is not the camera.
public enum OpeningMotion {
    /// Extra rotation (radians) of a still-growing tower: ±10°, 360-frame period.
    public static func sway(frame: Float) -> Float {
        let phase = frame.truncatingRemainder(dividingBy: 360) - 180
        return sin(phase * .pi / 180) * 10 * .pi / 180
    }

    /// World position of light orb `i` (0...3).
    public static func orbPosition(_ i: Int, frame: Float, seed: Float) -> SIMD3<Float> {
        let k = Float(i + 10)
        let a = (frame + seed + Float(17 * i)) * 0.01 * k * 0.1
        let b = (frame + seed + Float(15 * i)) * 0.005 * k * 0.1
        return SIMD3(Float(10 - i) * cos(a), Float(i + 3) * sin(b), cos(a) * 12 + 88)
    }

    /// Texture scroll of fog layer `i` (0...5), in texture widths.
    public static func fogScroll(_ i: Int, frame: Float) -> Float {
        (Float(14 - i) * 0.0001 * Float(i + 1) * 0.5 * frame).truncatingRemainder(dividingBy: 1)
    }

    /// Euler angles of glass cube `i` (0...4).
    public static func cubeRotation(_ i: Int, frame: Float) -> SIMD3<Float> {
        let s: Float = i == 2 ? 0.9 : Float(i - 2) * 0.8
        let start = s * Float(i % 3) * 3.7 + 0.285599
        let speed = SIMD3<Float>(0.0031 / s, s * 0.0022, s / 1000 + 0.0013)
        return SIMD3(repeating: start) + speed * frame
    }

    /// Opacity (0...1) of the "Sony Computer Entertainment" lettering, `t` frames after
    /// the camera passed z = 18.
    public static func letteringAlpha(framesSinceTrigger t: Float) -> Float {
        guard t > 0, t < 120 else { return 0 }
        let counter = t <= 60 ? 4 * t : 480 - 4 * t
        return min(counter, 112) / 128
    }

    /// Opacity of the black fade for camera depth `z`.
    public static func fadeAlpha(z: Float) -> Float {
        z > 72 ? min(128, (z - 72) * 4) / 128 : 0
    }

    /// Number of defocus passes for camera depth `z`.
    public static func defocusPasses(z: Float) -> Int {
        z > 56 ? min(3, max(0, Int((z - 56) / 12))) : 0
    }

    // MARK: Warning scene

    /// Scene brightness that scales the light source: 26 at z = 672, 39 once parked at 800.
    public static func warningBrightness(z: Float) -> Float {
        (740 - (1160 - z)) * 128 / 740 * 0.6
    }

    /// Light-source disc `i` (0...6): orbit position and the (wildly spinning) rotation vector.
    public static func warningDisc(_ i: Int, frame: Float) -> (position: SIMD3<Float>, rotation: SIMD3<Float>) {
        let n = Float(i + 1), k = Float((7 - i) * (7 - i)) * 2 * .pi / 64
        let r = k / 2
        let phi = frame.truncatingRemainder(dividingBy: 201) / 32 - .pi + k
        let base = SIMD3<Float>(0, 0, Float(i) * 0.925 * 2 * .pi / 7)
        return (SIMD3(r * cos(phi), r * sin(phi), 1160), base + SIMD3(0.2, 0.27, 0.35) * n * frame)
    }

    /// Black overlay of the warning scene before the camera has parked.
    public static func warningFadeIn(z: Float) -> Float {
        z < 800 ? max(0, min(1, (128 - (z - 672)) / 128)) : 0
    }

    /// Glass prism `i` (0...4) of the warning scene: Euler angles at `frame`.
    public static func prismRotation(_ i: Int, frame: Float) -> SIMD3<Float> {
        let s: Float = i == 2 ? 0.9 : Float(i - 2) * 0.8
        let start = SIMD3<Float>(s * Float((2 * i) % 9) / 4, s * Float((2 * i) % 8) / 5, s * Float((2 * i) % 7) / 6)
        return start + SIMD3(0.004 / s, 0.003 * s, s / 800 + 0.002) * frame
    }
}
