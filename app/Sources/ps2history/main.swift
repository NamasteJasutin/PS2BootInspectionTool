import Foundation
import PS2Kit

// usage: ps2history <card.ps2 | folder-card> [ls [dir] | cat <file>]
//        ps2history --bios <bios image>          (check that the opening's data can be read)
let args = CommandLine.arguments
func fail(_ message: String, code: Int32 = 1) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(code)
}
guard args.count >= 2 else {
    fail("usage: ps2history <memory card image or folder> [ls [dir]]\n       ps2history --bios <bios image>", code: 2)
}

do {
    if args[1] == "--bios" {
        guard args.count >= 3 else { fail("missing BIOS path", code: 2) }
        let assets = try OpeningAssets(biosURL: URL(fileURLWithPath: args[2]))
        print("ROM \(assets.romVersion): \(assets.textures.count) textures, " +
              "\(assets.slots.count) history records x \(assets.slots[0].count) slots")
        for t in assets.textures.values.sorted(by: { $0.name < $1.name }) {
            print("  \(t.name.padding(toLength: 9, withPad: " ", startingAt: 0)) \(t.width)x\(t.height)")
        }
        exit(0)
    }
    let card = try MemoryCard(url: URL(fileURLWithPath: args[1]))
    if args.count >= 3, args[2] == "ls" {
        let path = args.count > 3 ? args[3].split(separator: "/").map(String.init) : []
        for e in try card.list(path) {
            print("\(e.isDirectory ? "d" : "-") \(String(e.length).padding(toLength: 8, withPad: " ", startingAt: 0)) \(e.name)")
        }
        exit(0)
    }
    if args.count >= 4, args[2] == "cat" {
        FileHandle.standardOutput.write(try card.readFile(args[3].split(separator: "/").map(String.init)))
        exit(0)
    }
    let history = try PlayHistory(card: card)
    print("history from \(history.source ?? "?")")
    print(" #  title             count  towers  growing  last played")
    for (i, r) in history.records.enumerated() where !r.isEmpty {
        let towers = (0 ..< 6).map { r.mask >> UInt8($0) & 1 == 1 ? "#" : "." }.joined()
        let growing = r.index == 7 ? "-" : String(r.index)
        let name = r.name.padding(toLength: 16, withPad: " ", startingAt: 0)
        print(String(format: "%2d  %@  %5d  %@  %7@  %04d-%02d-%02d", i, name, Int(r.count), towers, growing,
                     r.year, r.month, r.day))
    }
    if history.records.allSatisfy(\.isEmpty) { print("(all 21 records are empty)") }
} catch {
    fail("error: \(error)")
}
