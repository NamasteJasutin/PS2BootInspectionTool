import Foundation

public enum MemoryCardError: Error, CustomStringConvertible {
    case notACard(String)
    case corrupt(String)
    case notFound(String)

    public var description: String {
        switch self {
        case .notACard(let m): return "not a PS2 memory card: \(m)"
        case .corrupt(let m): return "memory card is damaged: \(m)"
        case .notFound(let m): return "not found on card: \(m)"
        }
    }
}

/// Read-only view of a PS2 memory card as PCSX2 stores it: either a raw image
/// (`.ps2`, with or without the 16 spare bytes per page) or a "folder" card.
public struct MemoryCard {
    public struct Entry {
        public let name: String
        public let isDirectory: Bool
        public let length: Int
        fileprivate let cluster: UInt32
    }

    private enum Backing {
        case image(Image)
        case folder(URL)
    }

    private let backing: Backing

    public init(url: URL) throws {
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) else {
            throw MemoryCardError.notACard("no such file")
        }
        backing = isDir.boolValue ? .folder(url) : .image(try Image(data: try Data(contentsOf: url)))
    }

    /// Lists a directory given as path components, e.g. `["BEDATA-SYSTEM"]`; `[]` is the root.
    public func list(_ path: [String] = []) throws -> [Entry] {
        switch backing {
        case .image(let img):
            return try img.list(try img.resolveDirectory(path))
        case .folder(let root):
            let dir = path.reduce(root) { $0.appendingPathComponent($1) }
            let keys: [URLResourceKey] = [.isDirectoryKey, .fileSizeKey]
            return try FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: keys)
                .filter { !$0.lastPathComponent.hasPrefix("_pcsx2_") }
                .map {
                    let v = try $0.resourceValues(forKeys: Set(keys))
                    return Entry(name: $0.lastPathComponent, isDirectory: v.isDirectory ?? false,
                                 length: v.fileSize ?? 0, cluster: 0)
                }
        }
    }

    public func readFile(_ path: [String]) throws -> Data {
        switch backing {
        case .image(let img):
            guard let name = path.last else { throw MemoryCardError.notFound("(empty path)") }
            let dir = try img.resolveDirectory(Array(path.dropLast()))
            guard let e = try img.list(dir).first(where: { $0.name == name && !$0.isDirectory }) else {
                throw MemoryCardError.notFound(path.joined(separator: "/"))
            }
            return try img.readChain(first: e.cluster, length: e.length)
        case .folder(let root):
            let url = path.reduce(root) { $0.appendingPathComponent($1) }
            guard let d = try? Data(contentsOf: url) else {
                throw MemoryCardError.notFound(path.joined(separator: "/"))
            }
            return d
        }
    }

    // MARK: - Raw image

    /// The on-card filesystem: 512-byte pages grouped into clusters, a FAT reached through
    /// a list of indirect clusters, and 512-byte directory entries.
    private struct Image {
        let data: Data
        let rawPageSize: Int        // 512, or 528 when the spare (ECC) area is stored
        let pageSize: Int
        let pagesPerCluster: Int
        let allocOffset: UInt32     // first cluster of the allocatable area
        let rootCluster: UInt32     // relative to allocOffset
        let ifcList: [UInt32]

        var clusterSize: Int { pageSize * pagesPerCluster }

        init(data: Data) throws {
            let magic = "Sony PS2 Memory Card Format "
            guard data.count >= 0x154, data.cString(0, maxLength: 28) == magic else {
                throw MemoryCardError.notACard("superblock signature missing (unformatted card?)")
            }
            self.data = data
            pageSize = Int(data.u16(0x28))
            pagesPerCluster = Int(data.u16(0x2A))
            let clusters = Int(data.u32(0x30))
            allocOffset = data.u32(0x34)
            rootCluster = data.u32(0x3C)
            ifcList = (0 ..< 32).map { data.u32(0x50 + $0 * 4) }
            guard pageSize == 512, pagesPerCluster > 0, clusters > 0 else {
                throw MemoryCardError.corrupt("unsupported geometry")
            }
            let pages = clusters * pagesPerCluster
            if data.count >= pages * (pageSize + 16) {
                rawPageSize = pageSize + 16
            } else if data.count >= pages * pageSize {
                rawPageSize = pageSize
            } else {
                throw MemoryCardError.corrupt("image is shorter than the card it describes")
            }
        }

        func cluster(_ n: UInt32) throws -> Data {
            var out = Data(capacity: clusterSize)
            for p in 0 ..< pagesPerCluster {
                let off = (Int(n) * pagesPerCluster + p) * rawPageSize
                guard off + pageSize <= data.count else { throw MemoryCardError.corrupt("cluster \(n) out of range") }
                out.append(data.bytes(off, pageSize))
            }
            return out
        }

        /// FAT entry for a cluster number relative to the allocatable area.
        func fat(_ n: UInt32) throws -> UInt32 {
            let perCluster = UInt32(clusterSize / 4)
            let indirectIndex = n / perCluster
            let ifc = Int(indirectIndex / perCluster)
            guard ifc < ifcList.count else { throw MemoryCardError.corrupt("FAT index out of range") }
            let fatCluster = try cluster(ifcList[ifc]).u32(Int(indirectIndex % perCluster) * 4)
            return try cluster(fatCluster).u32(Int(n % perCluster) * 4)
        }

        func readChain(first: UInt32, length: Int) throws -> Data {
            var out = Data(capacity: length)
            var c = first
            var guardCount = 0
            while out.count < length {
                out.append(try cluster(c + allocOffset))
                let next = try fat(c)
                if next == 0xFFFF_FFFF || next & 0x8000_0000 == 0 { break }
                c = next & 0x7FFF_FFFF
                guardCount += 1
                if guardCount > 0x10000 { throw MemoryCardError.corrupt("FAT chain loops") }
            }
            guard out.count >= length else { throw MemoryCardError.corrupt("file chain ends early") }
            return out.prefix(length)
        }

        /// A directory is a chain of 512-byte entries; its entry count lives in the parent's
        /// entry for it, or for the root in the root's own "." entry.
        struct Directory {
            let cluster: UInt32
            let count: Int
        }

        func list(_ dir: Directory) throws -> [Entry] {
            let entrySize = 512
            guard dir.count > 0, dir.count < 0x10000 else { throw MemoryCardError.corrupt("bad directory size") }
            let raw = try readChain(first: dir.cluster, length: dir.count * entrySize)
            var out: [Entry] = []
            for i in 0 ..< dir.count {
                let e = raw.bytes(i * entrySize, entrySize)
                let mode = e.u16(0)
                guard mode & 0x8000 != 0 else { continue }      // deleted / unused
                let name = e.cString(0x40, maxLength: 32)
                if name == "." || name == ".." { continue }
                out.append(Entry(name: name, isDirectory: mode & 0x0020 != 0,
                                 length: Int(e.u32(4)), cluster: e.u32(0x10)))
            }
            return out
        }

        func resolveDirectory(_ path: [String]) throws -> Directory {
            var dir = Directory(cluster: rootCluster,
                                count: Int(try cluster(rootCluster + allocOffset).u32(4)))
            for part in path {
                guard let e = try list(dir).first(where: { $0.name == part && $0.isDirectory }) else {
                    throw MemoryCardError.notFound(path.joined(separator: "/"))
                }
                dir = Directory(cluster: e.cluster, count: e.length)
            }
            return dir
        }
    }
}
