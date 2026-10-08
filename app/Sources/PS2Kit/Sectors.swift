import Foundation

/// Reads 2048-byte data sectors from a disc image, whether it is a plain ISO or a raw
/// 2352-byte-sector BIN (MODE1 or MODE2/XA form 1, as CD rips usually are). A `.cue` is
/// resolved to its BIN.
public struct SectorReader {
    private let handle: FileHandle
    public let sectorSize: UInt64
    public let dataOffset: UInt64
    public var isRaw: Bool { sectorSize == 2352 }

    public static let imageExtensions = ["iso", "bin", "cue", "img"]

    public init(url: URL) throws {
        let resolved = try Self.resolveCue(url)
        handle = try FileHandle(forReadingFrom: resolved)
        let head = try handle.read(upToCount: 16) ?? Data()
        let sync = Data([0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0])
        if head.count == 16, head.prefix(12) == sync {
            sectorSize = 2352
            dataOffset = head[15] == 2 ? 24 : 16
        } else {
            sectorSize = 2048
            dataOffset = 0
        }
    }

    /// `FILE "name" BINARY` in a cue sheet names the image next to it.
    private static func resolveCue(_ url: URL) throws -> URL {
        guard url.pathExtension.lowercased() == "cue" else { return url }
        for line in try String(contentsOf: url, encoding: .utf8).split(whereSeparator: { $0.isNewline }) {
            let t = line.trimmingCharacters(in: .whitespaces)
            guard t.hasPrefix("FILE ") else { continue }
            var name = t.dropFirst(5).trimmingCharacters(in: .whitespaces)
            if name.hasSuffix("BINARY") { name = String(name.dropLast(6)).trimmingCharacters(in: .whitespaces) }
            name = name.trimmingCharacters(in: CharacterSet(charactersIn: "\""))
            return url.deletingLastPathComponent().appendingPathComponent(name)
        }
        throw BIOSError.corrupt("cue sheet has no FILE line")
    }

    /// Reads `count` sectors starting at `lba`; short reads at the end are returned as is.
    public func read(lba: Int, count: Int) throws -> Data {
        var out = Data(capacity: count * 2048)
        for i in 0 ..< count {
            try handle.seek(toOffset: UInt64(lba + i) * sectorSize + dataOffset)
            guard let chunk = try handle.read(upToCount: 2048), !chunk.isEmpty else { break }
            out.append(chunk)
        }
        return out
    }
}
