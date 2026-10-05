import Foundation

/// One record of the console's play history (22 bytes on the card).
public struct HistoryRecord: Equatable {
    public var name: String      // title ID, empty = unused slot
    public var count: UInt8      // launches, 1...63
    public var mask: UInt8       // which of the title's six towers exist (bits 0-5)
    public var index: UInt8      // bit that is still growing; 7 = record maxed out
    public var date: UInt16      // day | month << 5 | (year - 2000) << 9

    public init(name: String = "", count: UInt8 = 0, mask: UInt8 = 0, index: UInt8 = 0, date: UInt16 = 0) {
        self.name = name; self.count = count; self.mask = mask; self.index = index; self.date = date
    }

    public var isEmpty: Bool { name.isEmpty }
    public var day: Int { Int(date & 31) }
    public var month: Int { Int(date >> 5 & 15) }
    public var year: Int { 2000 + Int(date >> 9) }
}

/// The 21-record table the boot screen is generated from.
public struct PlayHistory: Equatable {
    public static let recordCount = 21
    public static let recordSize = 22
    public static let systemFolders = ["BIDATA-SYSTEM", "BADATA-SYSTEM", "BEDATA-SYSTEM", "BCDATA-SYSTEM"]

    public var records: [HistoryRecord]
    /// Where the table came from (system folder name), if it was read from a card.
    public var source: String?

    public init(records: [HistoryRecord] = [], source: String? = nil) {
        self.records = Array((records + Array(repeating: HistoryRecord(), count: Self.recordCount))
            .prefix(Self.recordCount))
        self.source = source
    }

    /// Parses the raw `history` file; short files leave the remaining records empty.
    public init(fileData: Data, source: String? = nil) {
        var recs: [HistoryRecord] = []
        for i in 0 ..< Self.recordCount {
            let o = i * Self.recordSize
            guard o + Self.recordSize <= fileData.count else { break }
            let r = fileData.bytes(o, Self.recordSize)
            recs.append(HistoryRecord(name: r.cString(0, maxLength: 16), count: r.u8(16),
                                      mask: r.u8(17), index: r.u8(18), date: r.u16(20)))
        }
        self.init(records: recs, source: source)
    }

    /// Looks for `B?DATA-SYSTEM/history` in any region's system folder.
    public init(card: MemoryCard) throws {
        let root = try card.list()
        for folder in Self.systemFolders where root.contains(where: { $0.name == folder && $0.isDirectory }) {
            if let data = try? card.readFile([folder, "history"]) {
                self.init(fileData: data, source: folder)
                return
            }
        }
        throw MemoryCardError.notFound("B?DATA-SYSTEM/history (the BIOS creates it the first time it launches a disc itself)")
    }

    /// A made-up history following the console's update rules, for cards that have none.
    /// `launches[i]` is how often title `i` was started; `titles` optionally names them.
    public static func synthetic(launches: [Int], titles: [String] = [], seed: UInt64 = 1) -> PlayHistory {
        var rng = SplitMix(seed: seed)
        var recs: [HistoryRecord] = []
        for (i, n) in launches.prefix(recordCount).enumerated() where n > 0 {
            let name = i < titles.count ? titles[i] : String(format: "TEST_%03d.%02d", i / 100, i % 100)
            var r = HistoryRecord(name: name, count: 1, mask: 1, index: 0)
            for _ in 1 ..< n {
                if r.mask & 0x3F == 0x3F {
                    if r.count < 0x3F { r.count += 1 } else { r.index = 7 }
                } else {
                    r.count += 1
                    if r.count >= 14, (r.count - 14) % 10 == 0 {
                        var b: UInt8
                        repeat { b = UInt8(rng.next() % 6) } while r.mask & (1 << b) != 0
                        r.index = b
                        r.mask |= 1 << b
                    }
                }
            }
            r.date = UInt16(1 + i % 28) | 6 << 5 | 4 << 9
            recs.append(r)
        }
        return PlayHistory(records: recs, source: "synthetic")
    }

    /// Title IDs of the games that have saves on a card (`BESLES-52541...` -> `SLES_525.41`),
    /// as a stand-in when the card carries no history file.
    public static func titlesWithSaves(on card: MemoryCard) -> [String] {
        var seen: [String] = []
        for e in (try? card.list()) ?? [] where e.isDirectory {
            let n = Array(e.name.utf8)
            guard n.count >= 12, n[0] == UInt8(ascii: "B"), n[6] == UInt8(ascii: "-"),
                  n[7 ..< 12].allSatisfy({ $0 >= 48 && $0 <= 57 }) else { continue }
            let code = String(decoding: n[2 ..< 6], as: UTF8.self)
            let digits = String(decoding: n[7 ..< 12], as: UTF8.self)
            let id = "\(code)_\(digits.prefix(3)).\(digits.suffix(2))"
            if !seen.contains(id) { seen.append(id) }
        }
        return seen
    }
}

struct SplitMix {
    var state: UInt64
    init(seed: UInt64) { state = seed }
    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ z >> 30) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ z >> 27) &* 0x94D0_49BB_1331_11EB
        return z ^ z >> 31
    }
}
