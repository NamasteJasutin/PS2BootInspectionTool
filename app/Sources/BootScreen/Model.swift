import AppKit
import Foundation
import PS2Kit
import simd

/// Where the play history shown on screen comes from.
enum HistorySource: String, CaseIterable, Identifiable {
    case card = "History file on card"
    case saves = "Simulated from saves on card"
    case custom = "Simulated"
    case empty = "Empty (new console)"
    var id: String { rawValue }
}

/// App state: the user's BIOS and memory card, the timeline, and the view settings.
final class AppModel: ObservableObject {
    @Published var biosURL: URL?
    @Published var biosStatus = "No BIOS loaded"
    @Published var cardURL: URL?
    @Published var cardStatus = "No memory card loaded"
    @Published var historySource: HistorySource = .empty { didSet { rebuildHistory() } }
    @Published var customTitles = 12.0 { didSet { if historySource == .custom { rebuildHistory() } } }
    @Published var customLaunches = 30.0 { didSet { if historySource == .custom { rebuildHistory() } } }
    @Published private(set) var history = PlayHistory()

    @Published var sceneKind: SceneKind = .full { didSet { rebuildTimeline(); frame = startFrame } }
    /// Black gap between the opening and the logo while OSDSYS hands over to PS2LOGO.
    @Published var handoffSeconds = 1.2 { didSet { rebuildTimeline() } }
    @Published private(set) var sequence = BootSequence(video: .ntsc, powerOnSeconds: 3, discSettledSeconds: 0, handoffSeconds: 1.2)
    private(set) var disc: DiscImage?
    var handoffSteps: [BootHandoff.Step] { BootHandoff.steps(disc: disc, history: history, video: video) }
    /// The segment of the full sequence at the current frame.
    var currentSegment: BootSequence.Segment? { sceneKind == .full ? sequence.span(at: Int(max(frame, 0))).0.segment : nil }
    @Published var video: VideoMode = .ntsc { didSet { rebuildTimeline() } }
    /// Warning scene: seconds until the drive reports a change and the scene fades out.
    @Published var warningExitSeconds = 10.0 { didSet { rebuildTimeline() } }
    /// Black screen between power-on and the first frame (`notes/boot_sequence.md` §4).
    @Published var powerOnSeconds = 3.0 { didSet { rebuildTimeline() } }
    @Published var frame: Float = 0
    var startFrame: Float { sceneKind == .full ? 0 : -Float(powerOnSeconds) * timeline.framesPerSecond }
    var endFrame: Float { sceneKind == .full ? Float(sequence.totalFrames - 1) : Float(timeline.endFrame) }
    var bootPhase: BootPhase? {
        let fps = timeline.framesPerSecond
        if sceneKind == .full {
            let (span, local) = sequence.span(at: Int(max(frame, 0)))
            switch span.segment {
            case .powerOn: return BootPhase.all[BootPhase.index(elapsed: Float(local) / fps, total: Float(powerOnSeconds))]
            case .handoff: return BootSequence.handoffPhase(elapsed: Float(local) / fps, total: Float(handoffSeconds))
            case .end: return BootPhase(name: "SPU quit; LoadExecPS2(boot ELF) — the game would start here", seconds: 0)
            default: return nil
            }
        }
        guard frame < 0 else { return nil }
        let elapsed = Float(powerOnSeconds) + frame / fps
        return BootPhase.all[BootPhase.index(elapsed: elapsed, total: Float(powerOnSeconds))]
    }
    @Published var playing = true
    @Published var loop = true
    @Published var speed = 1.0
    /// Seconds until the drive has identified the disc; the dive waits for it.
    @Published var discSeconds = 0.0 { didSet { rebuildTimeline() } }
    @Published private(set) var timeline = Timeline()

    @Published var options = RenderOptions()
    /// Console language setting: picks the warning text texture (TEXOPNG + letter).
    @Published var language = "E" { didSet { options.warningTexture = "TEXOPNG" + language } }
    static let languages: [(String, String)] = [("J", "Japanese"), ("E", "English"), ("F", "French"), ("S", "Spanish"),
                                                ("G", "German"), ("I", "Italian"), ("D", "Dutch"), ("P", "Portuguese"),
                                                ("R", "Russian"), ("K", "Korean"), ("H", "Chinese (traditional)"),
                                                ("C", "Chinese (simplified)")]
    @Published var soundEnabled = true
    @Published var soundVolume = 0.8
    @Published var soundStatus = "No sound loaded"
    let audio = AudioPlayer()
    let visualizer = VisualizerState()
    private let analysis = VisualizerAnalysis()
    @Published var freeCameraEnabled = false { didSet { if freeCameraEnabled { resetFreeCamera() } } }
    var freeCamera = FreeCamera()

    private(set) var assets: OpeningAssets?
    private(set) var logoAssets: LogoAssets?
    @Published var discURL: URL?
    @Published var discStatus = "No disc image — the lettering is filled from the BIOS outline"
    private var discLogo: DiscLogo?
    /// Bumped when the logo texture/animation should be re-sent to the renderer.
    private(set) var logoVersion = 0
    var logoAnimation: LogoAnimation? { logoAssets.map { LogoAnimation(assets: $0, video: video) } }
    var logoBitmap: (width: Int, height: Int, grey: [UInt8])? {
        if let d = discLogo { return d.bitmap(for: video) }
        return logoAnimation.map { DiscLogo.synthesised(from: $0) }
    }
    private(set) var scene: OpeningScene?
    private var card: MemoryCard?
    private var cardHistory: PlayHistory?
    /// Bumped whenever the renderer needs to pick up new assets.
    private(set) var assetsVersion = 0

    static let pcsx2 = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Application Support/PCSX2")

    func loadDefaults() {
        let defaults = UserDefaults.standard
        var biosCandidates = [defaults.url(forKey: "bios")].compactMap { $0 }
        biosCandidates += Self.files(in: Self.pcsx2.appendingPathComponent("bios"))
        for url in biosCandidates where assets == nil { loadBIOS(url, quiet: true) }
        var discCandidates = [defaults.url(forKey: "disc")].compactMap { $0 }
        discCandidates += Self.files(in: FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("PS2ISO"))
            .filter { $0.pathExtension.lowercased() == "iso" }
        for url in discCandidates where discLogo == nil { loadDisc(url, quiet: true) }
        var cardCandidates = [defaults.url(forKey: "card")].compactMap { $0 }
        cardCandidates += Self.files(in: Self.pcsx2.appendingPathComponent("memcards"))
        for url in cardCandidates where card == nil { loadCard(url, quiet: true) }
        frame = startFrame
    }

    private static func files(in dir: URL) -> [URL] {
        ((try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? [])
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
    }

    func loadBIOS(_ url: URL, quiet: Bool = false) {
        do {
            let a = try OpeningAssets(biosURL: url)
            assets = a
            assetsVersion += 1
            biosURL = url
            biosStatus = "\(url.lastPathComponent) — ROM \(a.romVersion)"
            // ROMVER's fifth character is the region: E = Europe (50 Hz).
            video = a.romVersion.dropFirst(4).first == "E" ? .pal : .ntsc
            UserDefaults.standard.set(url, forKey: "bios")
            rebuildScene()
            loadSound(url)
            logoAssets = try? LogoAssets(biosURL: url)
            logoVersion += 1
            if let l = logoAssets { audio.loadLogoChime(l.chime()) }
            arrangeAudio()
        } catch {
            if !quiet { biosStatus = "\(url.lastPathComponent): \(error)" }
        }
    }

    func loadCard(_ url: URL, quiet: Bool = false) {
        do {
            let c = try MemoryCard(url: url)
            _ = try c.list()
            card = c
            cardURL = url
            cardHistory = try? PlayHistory(card: c)
            UserDefaults.standard.set(url, forKey: "card")
            if let h = cardHistory {
                cardStatus = "\(url.lastPathComponent) — history in \(h.source ?? "?")"
                historySource = .card
            } else {
                let n = PlayHistory.titlesWithSaves(on: c).count
                cardStatus = "\(url.lastPathComponent) — no history file; \(n) titles have saves"
                historySource = n > 0 ? .saves : .empty
            }
        } catch {
            if !quiet { cardStatus = "\(url.lastPathComponent): \(error)" }
        }
    }

    private func rebuildTimeline() {
        let fps = Double(video.framesPerSecond)
        sequence = BootSequence(video: video, powerOnSeconds: Float(powerOnSeconds), discSettledSeconds: Float(discSeconds),
                                handoffSeconds: Float(handoffSeconds))
        switch sceneKind {
        case .boot, .full: timeline = Timeline(discSettledFrame: Int(discSeconds * fps), video: video)
        case .warning: timeline = .warning(exitFrame: Int(warningExitSeconds * fps), video: video)
        case .logo: timeline = .logo(video: video)
        }
        logoVersion += 1
        arrangeAudio()
    }

    private func arrangeAudio() {
        if sceneKind == .full {
            audio.arrange(scene: .full, fps: timeline.framesPerSecond, diveFrame: sequence.diveFrame,
                          logoStart: sequence.logoStart, openingStart: sequence.openingStart)
        } else {
            audio.arrange(scene: sceneKind, fps: timeline.framesPerSecond, diveFrame: timeline.diveFrame, logoStart: 0, openingStart: 0)
        }
    }

    /// Reads the lettering from sectors 0-11 of a game disc image.
    func loadDisc(_ url: URL, quiet: Bool = false) {
        do {
            let d = try DiscImage(url: url)
            disc = d
            discLogo = d.logo
            discURL = url
            logoVersion += 1
            UserDefaults.standard.set(url, forKey: "disc")
            discStatus = "\(url.lastPathComponent) — \(d.titleID ?? "no BOOT2"), " + (d.logo?.region.map { "logo checksum region \($0)" } ?? "logo checksum: no E/J match")
        } catch {
            if !quiet { discStatus = "\(url.lastPathComponent): \(error)" }
        }
    }

    /// The chime is synthesised from the BIOS on a background thread (a second or so).
    private func loadSound(_ url: URL) {
        soundStatus = "Synthesising the chime from the BIOS…"
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            do {
                let sound = try BootSound(biosURL: url)
                DispatchQueue.main.async {
                    self?.audio.load(sound)
                    self?.arrangeAudio()
                    self?.soundStatus = String(format: "Chime: %.1f s from SNDBOOTH/B/S, cue from SNDTNNLS",
                                               Double(sound.chime.count / 2) / Double(sound.sampleRate))
                }
            } catch {
                DispatchQueue.main.async { self?.soundStatus = "Sound: \(error)" }
            }
        }
    }

    private func rebuildHistory() {
        switch historySource {
        case .card:
            history = cardHistory ?? PlayHistory()
        case .saves:
            // Launch counts are invented: a fixed pseudo-random spread per title.
            let titles = card.map(PlayHistory.titlesWithSaves) ?? []
            let launches = titles.map { 3 + Int($0.utf8.reduce(UInt32(7)) { ($0 &* 31) &+ UInt32($1) } % 58) }
            history = .synthetic(launches: launches, titles: titles)
        case .custom:
            history = .synthetic(launches: Array(repeating: Int(customLaunches), count: Int(customTitles)))
        case .empty:
            history = PlayHistory()
        }
        rebuildScene()
    }

    private func rebuildScene() {
        scene = assets.map { OpeningScene(assets: $0, history: history) }
    }

    /// Advances the clock by `dt` seconds of wall time.
    func tick(_ dt: Double) {
        guard playing else { return }
        var f = frame + Float(dt * speed) * timeline.framesPerSecond
        let end = endFrame
        if f >= end {
            if loop { f = startFrame } else { f = end; playing = false }
        }
        frame = f
    }

    /// Keeps audio and the visualiser in step with the clock; called once per display frame.
    func syncAudio() {
        audio.sync(frame: frame, speed: speed, playing: playing, fps: timeline.framesPerSecond,
                   enabled: soundEnabled && audio.ready, volume: Float(soundVolume))
        if visualizer.mode != .off {
            let at = Int(Double(frame) / Double(timeline.framesPerSecond) * audio.sampleRate)
            visualizer.snapshot = analysis.measure(player: audio, frame: at, mode: visualizer.mode)
        }
    }

    var camera: CameraState {
        if sceneKind == .full {
            let (span, local) = sequence.span(at: Int(max(frame, 0)))
            return span.segment == .opening ? sequence.opening.camera(at: Float(local)) : CameraState(z: 0, roll: 0, stage: 0)
        }
        return timeline.camera(at: frame)
    }

    /// Turning the path on from the scripted view would show it end-on, so step outside.
    func setCameraPath(_ on: Bool) {
        options.cameraPath = on
        if on, !freeCameraEnabled {
            freeCameraEnabled = true
            viewPathFromSide()
        }
    }

    /// Puts the free camera beside the route, looking at its middle.
    func viewPathFromSide() {
        let position = SIMD3<Float>(130, -60, 25), target = SIMD3<Float>(0, 0, 60)
        let f = simd_normalize(target - position)
        freeCamera = FreeCamera(position: position, yaw: atan2(f.x, f.z), pitch: asin(f.y))
    }

    /// The route as CSV: one row per 60 Hz frame.
    func cameraPathCSV() -> String {
        Self.cameraPathCSV(timeline)
    }

    static func cameraPathCSV(_ timeline: Timeline) -> String {
        var out = "frame,seconds,x,y,z,roll_rad,up_x,up_y,up_z,stage\n"
        for (f, s) in timeline.states.enumerated() {
            // "up" here is the direction that points up on screen.
            out += String(format: "%d,%.4f,0,0,%.5f,%.6f,%.6f,%.6f,0,%d\n", f, Float(f) / timeline.framesPerSecond,
                          s.z, s.roll, -s.up.x, -s.up.y, s.stage)
        }
        return out
    }

    func resetFreeCamera() {
        let c = camera
        freeCamera = FreeCamera(position: c.position, yaw: 0, pitch: 0)
    }
}

/// A fly-through camera: position plus yaw/pitch, level horizon.
struct FreeCamera {
    var position = SIMD3<Float>(0, 0, 16)
    var yaw: Float = 0
    var pitch: Float = 0

    var forward: SIMD3<Float> { SIMD3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch)) }
    var view: ViewCamera { ViewCamera(position: position, forward: forward, up: SIMD3(0, 1, 0)) }

    mutating func look(dx: Float, dy: Float) {
        yaw += dx * 0.005
        pitch = min(1.5, max(-1.5, pitch + dy * 0.005))
    }

    mutating func move(right: Float, down: Float, forward amount: Float) {
        let b = view.basis
        position += b.x * right + b.y * down + b.z * amount
    }
}
