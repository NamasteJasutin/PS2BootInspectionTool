import AppKit
import MetalKit
import PS2Kit
import SwiftUI

@main
enum Main {
    static func main() {
        let args = CommandLine.arguments
        if args.count >= 2, args[1] == "--render" {
            exit(OfflineRender.run(Array(args.dropFirst(2))))
        }
        if args.count >= 4, args[1] == "--chime" {
            // BootScreen --chime <bios> <out.wav> [dive-frame]: the boot sound as rendered by the app.
            exit(OfflineRender.chime(bios: args[2], out: args[3], diveFrame: args.count > 4 ? Int(args[4]) ?? 121 : 121))
        }
        if args.count >= 2, args[1] == "--path" {
            // BootScreen --path [disc-seconds]: the scripted camera route as CSV on stdout.
            let disc = args.count > 2 ? Float(args[2]) ?? 0 : 0
            print(AppModel.cameraPathCSV(Timeline(discSettledFrame: Int(disc * 60))), terminator: "")
            exit(0)
        }
        BootScreenApp.main()
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Needed when started as a bare executable rather than from an app bundle.
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

struct BootScreenApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup("PS2 Boot Screen") {
            ContentView()
                .environmentObject(model)
                .frame(minWidth: 1000, minHeight: 600)
                .onAppear { model.loadDefaults() }
        }
    }
}

struct ContentView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            ZStack(alignment: .topLeading) {
                MetalView(model: model)
                VStack {
                    Spacer()
                    VisualizerView(state: model.visualizer)
                }
                if model.assets == nil {
                    Text("Open your PS2 BIOS dump to begin.\nNothing from the BIOS is bundled with this app.")
                        .multilineTextAlignment(.center)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
                VStack(alignment: .leading, spacing: 4) {
                    Text(statusLine)
                    if let phase = model.bootPhase {
                        Text("booting: \(phase.name)").foregroundStyle(.yellow.opacity(0.8))
                    }
                }
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(.white.opacity(0.7))
                .padding(8)
            }
            Divider()
            Sidebar().frame(width: 310)
        }
    }

    private var statusLine: String {
        let c = model.camera
        if model.frame < 0 {
            return String(format: "power-on %+5.2f s  (opening starts at 0)", model.frame / model.timeline.framesPerSecond)
        }
        return String(format: "frame %6.1f  %5.2f s  camera z %6.1f  roll %+.2f  stage %d",
                      model.frame, model.frame / model.timeline.framesPerSecond, c.z, c.roll, c.stage)
    }
}

struct Sidebar: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                section("Your files") {
                    fileRow("BIOS", status: model.biosStatus) { pick { model.loadBIOS($0) } }
                    fileRow("Memory card", status: model.cardStatus) { pick(directories: true) { model.loadCard($0) } }
                }
                section("Play history") {
                    Picker("Source", selection: $model.historySource) {
                        ForEach(HistorySource.allCases) { Text($0.rawValue).tag($0) }
                    }
                    .labelsHidden()
                    if model.historySource == .custom {
                        slider("Titles", $model.customTitles, 0 ... 21, step: 1, format: "%.0f")
                        slider("Launches each", $model.customLaunches, 1 ... 64, step: 1, format: "%.0f")
                    }
                    if model.historySource == .saves {
                        Text("Launch counts are invented; only the titles come from the card.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    HistoryTable(history: model.history)
                }
                section("Scene") {
                    Picker("Scene", selection: $model.sceneKind) {
                        ForEach(SceneKind.allCases) { Text($0.rawValue).tag($0) }
                    }
                    .labelsHidden()
                    if model.sceneKind == .boot {
                        Picker("Video", selection: $model.video) {
                            ForEach(VideoMode.allCases) { Text($0.rawValue).tag($0) }
                        }
                        .pickerStyle(.segmented)
                    }
                    if model.sceneKind == .warning {
                        Picker("Video", selection: $model.video) {
                            ForEach(VideoMode.allCases) { Text($0.rawValue).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        Picker("Language", selection: $model.language) {
                            ForEach(AppModel.languages, id: \.0) { Text($0.1).tag($0.0) }
                        }
                        slider("Drive reports a change after", $model.warningExitSeconds, 3 ... 60, format: "%.0f s")
                        Text("The console holds this screen until a disc is inserted or removed; then it fades out over two seconds.")
                            .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    }
                }
                section("Time") {
                    HStack {
                        Button(model.playing ? "Pause" : "Play") { model.playing.toggle() }
                            .keyboardShortcut(.space, modifiers: [])
                        Button("Restart") { model.frame = model.startFrame; model.playing = true }
                        Toggle("Loop", isOn: $model.loop)
                    }
                    Slider(value: Binding(get: { Double(model.frame) }, set: { model.frame = Float($0) }),
                           in: Double(model.startFrame) ... Double(max(model.timeline.endFrame, 1)))
                    slider("Power-on black screen", $model.powerOnSeconds, 0 ... 6, format: "%.1f s")
                    slider("Speed", $model.speed, 0.05 ... 2, format: "%.2fx")
                    if model.sceneKind == .boot {
                        slider("Disc identified after", $model.discSeconds, 0 ... 10.5, format: "%.1f s")
                    }
                }
                section("Camera") {
                    Toggle("Free camera", isOn: $model.freeCameraEnabled)
                    Text("Drag to look, scroll to fly, right-drag to slide.")
                        .font(.caption).foregroundStyle(.secondary)
                    Button("Back to the scripted position") { model.resetFreeCamera() }
                        .disabled(!model.freeCameraEnabled)
                    Toggle("Show the scripted camera path", isOn: Binding(get: { model.options.cameraPath },
                                                                         set: { model.setCameraPath($0) }))
                    if model.options.cameraPath {
                        PathLegend()
                        HStack {
                            Button("View from the side") { model.freeCameraEnabled = true; model.viewPathFromSide() }
                            Button("Export CSV…") { exportPath() }
                        }
                    }
                }
                section("Sound") {
                    Toggle("Boot chime", isOn: $model.soundEnabled)
                    slider("Volume", $model.soundVolume, 0 ... 1, format: "%.0f%%", scale: 100)
                    VisualizerPicker(state: model.visualizer)
                    Text(model.soundStatus).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
                section("Layers") {
                    Toggle("Towers", isOn: $model.options.towers)
                    Toggle("Frame-to-frame smear", isOn: $model.options.trails)
                    Toggle("Fog", isOn: $model.options.fog)
                    Toggle("Light orbs", isOn: $model.options.orbs)
                    Toggle("Glass cubes (approximate)", isOn: $model.options.glass)
                    Toggle("Defocus during dive", isOn: $model.options.defocus)
                    Toggle("Fade to black", isOn: $model.options.fade)
                    Toggle("Lettering", isOn: $model.options.lettering)
                    Toggle("Letterbox bars", isOn: $model.options.letterbox)
                    Toggle("8-bit overflow on tower caps", isOn: $model.options.colourWrap)
                }
            }
            .padding(12)
        }
    }

    private func section<C: View>(_ title: String, @ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.headline)
            content()
        }
    }

    private func fileRow(_ label: String, status: String, action: @escaping () -> Void) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text(label)
                Spacer()
                Button("Open…", action: action)
            }
            Text(status).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
    }

    private func slider(_ label: String, _ value: Binding<Double>, _ range: ClosedRange<Double>,
                        step: Double? = nil, format: String, scale: Double = 1) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(label)
                Spacer()
                Text(String(format: format, value.wrappedValue * scale)).monospacedDigit().foregroundStyle(.secondary)
            }
            if let step {
                Slider(value: value, in: range, step: step)
            } else {
                Slider(value: value, in: range)
            }
        }
    }

    private func exportPath() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "ps2-opening-camera-path.csv"
        if panel.runModal() == .OK, let url = panel.url {
            try? model.cameraPathCSV().write(to: url, atomically: true, encoding: .utf8)
        }
    }

    private func pick(directories: Bool = false, _ handler: @escaping (URL) -> Void) {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = directories      // PCSX2 "folder" memory cards
        panel.allowsMultipleSelection = false
        panel.directoryURL = AppModel.pcsx2
        if panel.runModal() == .OK, let url = panel.url { handler(url) }
    }
}

/// Colour key for the camera-path overlay.
struct PathLegend: View {
    private let items: [(String, Color)] = [
        ("route (bright = travelled); rungs every 10 frames point screen-up, long ones mark seconds", Color(red: 1, green: 0.82, blue: 0.25)),
        ("lettering appears", Color(red: 0.3, green: 0.9, blue: 1)),
        ("dive starts", Color(red: 0.3, green: 1, blue: 0.4)),
        ("defocus starts", Color(red: 1, green: 0.4, blue: 1)),
        ("fade to black starts", Color(red: 1, green: 0.55, blue: 0.15)),
        ("scene ends", Color(red: 1, green: 0.25, blue: 0.25)),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(items, id: \.0) { item in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    RoundedRectangle(cornerRadius: 2).fill(item.1).frame(width: 10, height: 10)
                    Text(item.0).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}

struct VisualizerPicker: View {
    @ObservedObject var state: VisualizerState

    var body: some View {
        Picker("Visualiser", selection: $state.mode) {
            ForEach(VisualizerMode.allCases) { Text($0.rawValue).tag($0) }
        }
        .pickerStyle(.segmented)
    }
}

struct HistoryTable: View {
    let history: PlayHistory

    var body: some View {
        let used = history.records.filter { !$0.isEmpty }
        VStack(alignment: .leading, spacing: 1) {
            if used.isEmpty {
                Text("No titles: the screen shows no towers.").font(.caption).foregroundStyle(.secondary)
            }
            ForEach(Array(used.enumerated()), id: \.offset) { _, r in
                HStack {
                    Text(r.name)
                    Spacer()
                    Text("\(r.count)×")
                    Text((0 ..< 6).map { r.mask >> UInt8($0) & 1 == 1 ? ($0 == Int(r.index) ? "▫︎" : "▪︎") : "·" }.joined())
                }
                .font(.system(.caption, design: .monospaced))
            }
        }
    }
}

/// The Metal view; also turns mouse input into free-camera motion.
final class SceneView: MTKView {
    weak var model: AppModel?

    override var acceptsFirstResponder: Bool { true }

    override func mouseDragged(with e: NSEvent) {
        guard let m = model, m.freeCameraEnabled else { return }
        m.freeCamera.look(dx: Float(e.deltaX), dy: Float(e.deltaY))
    }

    override func rightMouseDragged(with e: NSEvent) {
        guard let m = model, m.freeCameraEnabled else { return }
        m.freeCamera.move(right: -Float(e.deltaX) * 0.1, down: -Float(e.deltaY) * 0.1, forward: 0)
    }

    override func scrollWheel(with e: NSEvent) {
        guard let m = model, m.freeCameraEnabled else { return }
        m.freeCamera.move(right: 0, down: 0, forward: Float(e.scrollingDeltaY) * (e.hasPreciseScrollingDeltas ? 0.05 : 1))
    }
}

struct MetalView: NSViewRepresentable {
    let model: AppModel

    func makeCoordinator() -> Coordinator { Coordinator(model: model) }

    func makeNSView(context: Context) -> SceneView {
        let view = SceneView(frame: .zero, device: MTLCreateSystemDefaultDevice())
        view.model = model
        view.colorPixelFormat = .bgra8Unorm
        view.clearColor = MTLClearColor(red: 0, green: 0, blue: 0, alpha: 1)
        view.preferredFramesPerSecond = 60
        view.delegate = context.coordinator
        context.coordinator.attach(view)
        return view
    }

    func updateNSView(_ view: SceneView, context: Context) {}

    final class Coordinator: NSObject, MTKViewDelegate {
        private let model: AppModel
        private var renderer: Renderer?
        private var loadedAssets = -1
        private var lastTime = CACurrentMediaTime()

        init(model: AppModel) { self.model = model }

        func attach(_ view: MTKView) {
            if let device = view.device { renderer = try? Renderer(device: device) }
        }

        func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

        func draw(in view: MTKView) {
            let now = CACurrentMediaTime()
            model.tick(min(now - lastTime, 0.1))
            lastTime = now
            model.syncAudio()
            guard let renderer, let drawable = view.currentDrawable, let cb = renderer.queue.makeCommandBuffer() else { return }
            if loadedAssets != model.assetsVersion, let a = model.assets {
                renderer.setAssets(a)
                loadedAssets = model.assetsVersion
            }
            if let scene = model.scene {
                renderer.render(frame: model.frame, scene: scene, timeline: model.timeline,
                                freeCamera: model.freeCameraEnabled ? model.freeCamera.view : nil,
                                options: model.options, commandBuffer: cb)
                renderer.present(cb, to: drawable.texture)
            }
            cb.present(drawable)
            cb.commit()
        }
    }
}
