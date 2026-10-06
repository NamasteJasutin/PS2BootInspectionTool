import Foundation

public enum BIOSError: Error, CustomStringConvertible {
    case notABIOS(String)
    case unsupportedVersion(String)
    case missing(String)
    case corrupt(String)

    public var description: String {
        switch self {
        case .notABIOS(let m): return "not a PS2 BIOS image: \(m)"
        case .unsupportedVersion(let v): return "BIOS version \(v) is not supported yet (data tables are located per version)"
        case .missing(let m): return "BIOS module missing: \(m)"
        case .corrupt(let m): return "BIOS data is damaged: \(m)"
        }
    }
}

/// A ROMDIR archive: the BIOS image itself, and the asset bundles nested inside it.
public struct ROMDirectory {
    public let data: Data
    public private(set) var entries: [String: Range<Int>] = [:]

    public init(data: Data) throws {
        self.data = data
        guard let base = data.range(of: Data("RESET\0\0\0\0\0".utf8))?.lowerBound else {
            throw BIOSError.notABIOS("no ROMDIR table")
        }
        var p = base - data.startIndex
        var offset = 0
        while p + 16 <= data.count, data.u8(p) != 0 {
            let name = data.cString(p, maxLength: 10)
            let size = Int(data.u32(p + 12))
            if name != "-", entries[name] == nil, offset + size <= data.count {
                entries[name] = offset ..< offset + size
            }
            offset += (size + 15) & ~15
            p += 16
        }
    }

    public func module(_ name: String) throws -> Data {
        guard let r = entries[name] else { throw BIOSError.missing(name) }
        return data.bytes(r.lowerBound, r.count)
    }
}

/// The LZ scheme used for the OSD program and its assets.
///
/// `u32` output size, then groups of 30 tokens, each group preceded by a big-endian
/// word whose top 30 bits flag literal (0) / match (1) and whose low 2 bits `n` set the
/// offset/length split of a 16-bit big-endian match word.
public enum OSDCompression {
    public static func unpack(_ src: Data, at start: Int = 0) throws -> Data {
        guard start + 4 <= src.count else { throw BIOSError.corrupt("truncated stream") }
        let size = Int(src.u32(start))
        guard size < 64 << 20 else { throw BIOSError.corrupt("implausible stream size") }
        var out = [UInt8]()
        out.reserveCapacity(size)
        var pos = start + 4
        var desc: UInt32 = 0, left = 0, shift = 14, mask = 0x3FFF
        let bytes = [UInt8](src)
        while out.count < size {
            if left == 0 {
                guard pos + 4 <= bytes.count else { throw BIOSError.corrupt("truncated stream") }
                desc = UInt32(bytes[pos]) << 24 | UInt32(bytes[pos + 1]) << 16 | UInt32(bytes[pos + 2]) << 8 | UInt32(bytes[pos + 3])
                pos += 4
                let n = Int(desc & 3)
                shift = 14 - n
                mask = 0x3FFF >> n
                left = 30
            }
            if desc & 0x8000_0000 != 0 {
                guard pos + 2 <= bytes.count else { throw BIOSError.corrupt("truncated stream") }
                let h = Int(bytes[pos]) << 8 | Int(bytes[pos + 1])
                pos += 2
                let off = (h & mask) + 1
                guard off <= out.count else { throw BIOSError.corrupt("match before start of output") }
                for _ in 0 ..< (h >> shift) + 3 { out.append(out[out.count - off]) }
            } else {
                guard pos < bytes.count else { throw BIOSError.corrupt("truncated stream") }
                out.append(bytes[pos])
                pos += 1
            }
            desc <<= 1
            left -= 1
        }
        return Data(out.prefix(size))
    }
}

/// A decoded texture, RGBA8 with PS2 alpha rescaled so that 0x80 becomes 255.
public struct TextureImage {
    public let name: String
    public let width: Int
    public let height: Int
    public var rgba: [UInt8]
    /// Extra mip levels the console generates for this texture.
    public let mipLevels: Int
}

/// Where the opening's data sits inside one specific OSDSYS build.
struct OpeningLayout {
    let loadAddress = 0x200000
    let stubStreamOffset = 0x100D80 - 0x100000 + 0x80   // LZ stream inside the ROM's OSDSYS ELF
    let assetNames: Int
    let textureTable: Int
    let textureCount: Int
    let slotTable: Int
    let fillTable: Int
    let sizeTable: Int
    let towerPositions: Int
    let orbColours: Int
    let cubePositions: Int
    let prismPositions: Int

    static let byROMVersion: [String: OpeningLayout] = [
        // SCPH-70004, v2.00 Europe, 2004-06-14
        "0200EC20040614": OpeningLayout(
            assetNames: 0x27B4F8, textureTable: 0x287700, textureCount: 25,
            slotTable: 0x2891E0, fillTable: 0x289E60, sizeTable: 0x289E98,
            towerPositions: 0x2895F0, orbColours: 0x289080, cubePositions: 0x289190, prismPositions: 0x289ED0),
    ]
}

/// Everything the opening needs from the user's BIOS dump.
public struct OpeningAssets {
    public static let columns = 14
    public static let rows = 9

    public let romVersion: String
    /// `slots[record][k]` = (column, row) of the k-th tower owned by a history record.
    public let slots: [[(column: Int, row: Int)]]
    /// Indexed by launch-count step: how solid / how long the growing tower is.
    public let fill: [Float]
    public let size: [Float]
    /// Model-space base position per slot, `[column][row]`.
    public let towerPositions: [[SIMD3<Float>]]
    public let orbColours: [SIMD3<Float>]
    public let cubePositions: [SIMD3<Float>]
    /// Glass prisms of the warning scene (model units; z maps as (z - 2.5) * 128 + 788).
    public let prismPositions: [SIMD3<Float>]
    public let textures: [String: TextureImage]

    public init(biosURL: URL) throws {
        let rom = try ROMDirectory(data: try Data(contentsOf: biosURL))
        let version = try rom.module("ROMVER").cString(0, maxLength: 14)
        guard let layout = OpeningLayout.byROMVersion[version] else {
            throw BIOSError.unsupportedVersion(version)
        }
        romVersion = version

        let osd = try OSDCompression.unpack(try rom.module("OSDSYS"), at: layout.stubStreamOffset)
        func at(_ vaddr: Int) -> Int { vaddr - layout.loadAddress }

        slots = (0 ..< PlayHistory.recordCount).map { i in
            (0 ..< 6).map { k in
                let o = at(layout.slotTable) + i * 0x30 + k * 8
                return (Int(osd.i32(o)), Int(osd.i32(o + 4)))
            }
        }
        guard slots.joined().allSatisfy({ (0 ..< Self.columns).contains($0.column) && (0 ..< Self.rows).contains($0.row) }) else {
            throw BIOSError.corrupt("tower slot table")
        }
        fill = (0 ..< 14).map { osd.f32(at(layout.fillTable) + $0 * 4) }
        size = (0 ..< 14).map { osd.f32(at(layout.sizeTable) + $0 * 4) }
        towerPositions = (0 ..< Self.columns).map { c in
            (0 ..< Self.rows).map { r in
                let o = at(layout.towerPositions) + c * 0x90 + r * 16
                return SIMD3(osd.f32(o), osd.f32(o + 4), osd.f32(o + 8))
            }
        }
        orbColours = (0 ..< 4).map { let o = at(layout.orbColours) + $0 * 16
            return SIMD3(osd.f32(o), osd.f32(o + 4), osd.f32(o + 8)) }
        cubePositions = (0 ..< 5).map { let o = at(layout.cubePositions) + $0 * 16
            return SIMD3(osd.f32(o), osd.f32(o + 4), osd.f32(o + 8)) }
        prismPositions = (0 ..< 5).map { let o = at(layout.prismPositions) + $0 * 16
            return SIMD3(osd.f32(o), osd.f32(o + 4), osd.f32(o + 8)) }

        // Asset names, in table order, to resolve the texture descriptors' asset indices.
        var names: [String?] = []
        var p = at(layout.assetNames)
        while osd.u32(p) != 0xFFFF_FFFF {
            let ptr = Int(osd.u32(p))
            names.append(ptr == 0 ? nil : osd.cString(at(ptr), maxLength: 16))
            p += 16
        }

        let archive = try ROMDirectory(data: try rom.module("TEXIMAGE"))
        var decoded: [String: TextureImage] = [:]
        for i in 0 ..< layout.textureCount {
            let d = at(layout.textureTable) + i * 0xF0
            let assetIndex = Int(osd.i32(d + 4))
            guard assetIndex < names.count, let name = names[assetIndex] else { continue }
            let clut = Int(osd.u32(d + 8))
            let w = Int(osd.i32(d + 24)), h = Int(osd.i32(d + 28))
            let mips = Int(osd.i32(d + 32)), skip = Int(osd.i32(d + 36)), format = Int(osd.i32(d + 40))
            guard let packed = try? archive.module(name) else { continue }
            let raw = try OSDCompression.unpack(packed)
            let palette = clut == 0 ? nil : osd.bytes(at(clut), 64)
            decoded[name] = TextureImage(
                name: name, width: w, height: h,
                rgba: try Self.decode(raw.dropFirst(skip), width: w, height: h, format: format, palette: palette),
                mipLevels: mips)
        }
        textures = decoded
    }

    /// Source formats as the console's loader interprets them.
    static func decode(_ raw: Data, width w: Int, height h: Int, format: Int, palette: Data?) throws -> [UInt8] {
        let src = [UInt8](raw)
        var out = [UInt8](repeating: 0, count: w * h * 4)
        func need(_ n: Int) throws { if src.count < n { throw BIOSError.corrupt("texture data too short") } }
        func scaleAlpha(_ a: UInt8) -> UInt8 { UInt8(min(Int(a) * 2, 255)) }
        switch format {
        case 0:     // RGBA32
            try need(w * h * 4)
            for i in 0 ..< w * h {
                out[i * 4] = src[i * 4]; out[i * 4 + 1] = src[i * 4 + 1]; out[i * 4 + 2] = src[i * 4 + 2]
                out[i * 4 + 3] = scaleAlpha(src[i * 4 + 3])
            }
        case 2:     // 16-bit, 5 bits per channel
            try need(w * h * 2)
            for i in 0 ..< w * h {
                let v = Int(src[i * 2]) | Int(src[i * 2 + 1]) << 8
                out[i * 4] = UInt8((v & 31) << 3); out[i * 4 + 1] = UInt8((v >> 5 & 31) << 3)
                out[i * 4 + 2] = UInt8((v >> 10 & 31) << 3); out[i * 4 + 3] = 255
            }
        case 3, 4:  // 8-bit alpha; RGB white (3) or black (4)
            try need(w * h)
            let rgb: UInt8 = format == 3 ? 255 : 0
            for i in 0 ..< w * h {
                out[i * 4] = rgb; out[i * 4 + 1] = rgb; out[i * 4 + 2] = rgb
                out[i * 4 + 3] = src[i]     // used as a 0...255 mask, not rescaled
            }
        case 5:     // (intensity, alpha) pairs
            try need(w * h * 2)
            for i in 0 ..< w * h {
                let v = src[i * 2]
                out[i * 4] = v; out[i * 4 + 1] = v; out[i * 4 + 2] = v; out[i * 4 + 3] = scaleAlpha(src[i * 2 + 1])
            }
        case 0x14:  // 4-bit indexed, low nibble first
            try need(w * h / 2)
            guard let pal = palette.map({ [UInt8]($0) }) else { throw BIOSError.corrupt("palette missing") }
            for i in 0 ..< w * h {
                let idx = Int(i & 1 == 0 ? src[i / 2] & 15 : src[i / 2] >> 4)
                out[i * 4] = pal[idx * 4]; out[i * 4 + 1] = pal[idx * 4 + 1]; out[i * 4 + 2] = pal[idx * 4 + 2]
                out[i * 4 + 3] = scaleAlpha(pal[idx * 4 + 3])
            }
        default:
            throw BIOSError.corrupt("unknown texture format \(format)")
        }
        return out
    }
}
