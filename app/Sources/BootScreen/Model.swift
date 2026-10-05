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

    @Published var frame: Float = 0
    @Published var playing = true
    @Published var loop = true
    @Published var speed = 1.0
    /// Seconds until the drive has identified the disc; the dive waits for it.
    @Published var discSeconds = 0.0 { didSet { timeline = Timeline(discSettledFrame: Int(discSeconds * 60)) } }
    @Published private(set) var timeline = Timeline()

    @Published var options = RenderOptions()
    @Published var freeCameraEnabled = false { didSet { if freeCameraEnabled { resetFreeCamera() } } }
    var freeCamera = FreeCamera()

    private(set) var assets: OpeningAssets?
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
        var cardCandidates = [defaults.url(forKey: "card")].compactMap { $0 }
        cardCandidates += Self.files(in: Self.pcsx2.appendingPathComponent("memcards"))
        for url in cardCandidates where card == nil { loadCard(url, quiet: true) }
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
            UserDefaults.standard.set(url, forKey: "bios")
            rebuildScene()
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
        var f = frame + Float(dt * speed) * Timeline.framesPerSecond
        let end = Float(timeline.endFrame)
        if f >= end {
            if loop { f = 0 } else { f = end; playing = false }
        }
        frame = f
    }

    var camera: CameraState { timeline.camera(at: frame) }

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
            out += String(format: "%d,%.4f,0,0,%.5f,%.6f,%.6f,%.6f,0,%d\n", f, Float(f) / Timeline.framesPerSecond,
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
