// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "BootScreen",
    platforms: [.macOS(.v13)],
    products: [.library(name: "PS2Kit", targets: ["PS2Kit"])],
    targets: [
        // Format readers and the scene simulation; no UI, no Sony data.
        .target(name: "PS2Kit"),
        .executableTarget(name: "ps2history", dependencies: ["PS2Kit"]),
        .executableTarget(name: "BootScreen", dependencies: ["PS2Kit"]),
    ]
)
