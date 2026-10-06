import Foundation

/// What the console learns about a game disc on the way to launching it, read from a plain
/// ISO image the way OSDSYS and the kernel would: ISO 9660 root directory, `SYSTEM.CNF`,
/// the boot ELF's header, the logo sectors.
public struct DiscImage {
    public struct ELFSegment {
        public let vaddr: UInt32, fileSize: UInt32, memSize: UInt32, flags: UInt32
    }
    public struct BootELF {
        public let path: String          // as written in SYSTEM.CNF, e.g. cdrom0:\SLES_530.64;1
        public let fileName: String
        public let lba: Int
        public let size: Int
        public let entry: UInt32
        public let segments: [ELFSegment]
        public let isMIPS: Bool
    }

    public let url: URL
    public let volumeID: String
    public let sectorCount: Int
    public var byteSize: Int { sectorCount * 2048 }
    /// CD if the volume fits a CD (OSDSYS's state 0x6C), otherwise DVD (0x6E).
    public var isDVD: Bool { byteSize > 800 << 20 }
    public let systemCNF: [String: String]
    public let systemCNFText: String
    public let bootELF: BootELF?
    public let logo: DiscLogo?

    /// Title ID as the history file and the browser spell it (`SLES_530.64`).
    public var titleID: String? {
        guard let b = bootELF else { return nil }
        return String(b.fileName.split(separator: ";").first ?? Substring(b.fileName))
    }
    /// The disc-state code OSDSYS's disc thread would settle on.
    public var discStateCode: Int { isDVD ? 0x6E : 0x6C }

    public init(url: URL) throws {
        self.url = url
        let h = try FileHandle(forReadingFrom: url)
        defer { try? h.close() }
        func sector(_ n: Int, count: Int = 1) throws -> Data {
            try h.seek(toOffset: UInt64(n) * 2048)
            return try h.read(upToCount: count * 2048) ?? Data()
        }
        let pvd = try sector(16)
        guard pvd.count >= 2048, pvd.bytes(1, 5) == Data("CD001".utf8) else {
            throw BIOSError.corrupt("not an ISO 9660 image (no primary volume descriptor)")
        }
        volumeID = pvd.cString(40, maxLength: 32).trimmingCharacters(in: .whitespaces)
        sectorCount = Int(pvd.u32(80))
        let rootLBA = Int(pvd.u32(156 + 2)), rootLen = Int(pvd.u32(156 + 10))
        let dir = try sector(rootLBA, count: max(1, (rootLen + 2047) / 2048))
        var entries: [(name: String, lba: Int, size: Int)] = []
        var p = 0
        while p < min(dir.count, rootLen) {
            let len = Int(dir.u8(p))
            if len == 0 { p = (p / 2048 + 1) * 2048; continue }
            let nameLen = Int(dir.u8(p + 32))
            let name = String(decoding: dir.bytes(p + 33, nameLen), as: UTF8.self)
            entries.append((name, Int(dir.u32(p + 2)), Int(dir.u32(p + 10))))
            p += len
        }
        var cnf: [String: String] = [:]
        var text = ""
        if let e = entries.first(where: { $0.name.uppercased().hasPrefix("SYSTEM.CNF") }) {
            let raw = try sector(e.lba, count: (e.size + 2047) / 2048).prefix(min(e.size, 1023))
            text = String(decoding: raw, as: UTF8.self)
            for line in text.split(whereSeparator: { $0.isNewline }) {
                let parts = line.split(separator: "=", maxSplits: 1).map { $0.trimmingCharacters(in: .whitespaces) }
                if parts.count == 2 { cnf[parts[0].uppercased()] = parts[1] }
            }
        }
        systemCNF = cnf
        systemCNFText = text
        var elf: BootELF?
        if let boot = cnf["BOOT2"] {
            let file = String(boot.split(whereSeparator: { $0 == "\\" || $0 == ":" || $0 == "/" }).last ?? "")
            if let e = entries.first(where: { $0.name.uppercased() == file.uppercased() }) {
                let hdr = try sector(e.lba, count: 2)
                var segs: [ELFSegment] = []
                var entry: UInt32 = 0, mips = false
                if hdr.count >= 52, hdr.bytes(0, 4) == Data([0x7F, 0x45, 0x4C, 0x46]) {
                    entry = hdr.u32(0x18)
                    mips = hdr.u16(0x12) == 8
                    let phoff = Int(hdr.u32(0x1C)), phentsize = Int(hdr.u16(0x2A)), phnum = Int(hdr.u16(0x2C))
                    for i in 0 ..< min(phnum, 16) {
                        let o = phoff + i * phentsize
                        guard o + 32 <= hdr.count else { break }
                        if hdr.u32(o) == 1 {
                            segs.append(ELFSegment(vaddr: hdr.u32(o + 8), fileSize: hdr.u32(o + 16), memSize: hdr.u32(o + 20), flags: hdr.u32(o + 24)))
                        }
                    }
                }
                elf = BootELF(path: boot, fileName: e.name, lba: e.lba, size: e.size, entry: entry, segments: segs, isMIPS: mips)
            }
        }
        bootELF = elf
        logo = try? DiscLogo(discImageURL: url)
    }
}

/// The hand-off the console would perform after the logo, as a list of steps the app can
/// show instead of doing. Every line names the function in the notes that does it.
public enum BootHandoff {
    public struct Step {
        public let who: String
        public let what: String
    }

    public static func steps(disc: DiscImage?, history: PlayHistory, video: VideoMode) -> [Step] {
        guard let d = disc else {
            return [Step(who: "OSDSYS", what: "No disc: OpeningDecideNext → ctx[0x5E8] = 2, the clock/main-menu module is woken (not re-created here).")]
        }
        let id = d.titleID ?? "?"
        var s: [Step] = []
        s.append(Step(who: "CDVD (disc thread 0x20F478)", what: String(format: "disc type register → state 0x%02X (%@); Ps2DiscVerifyAndGetId reads the disc key twice → title ID %@", d.discStateCode, d.isDVD ? "PlayStation 2 DVD" : "PlayStation 2 CD", id)))
        s.append(Step(who: "OSDSYS OpeningDecideNext (0x2165A0)", what: "latched state → ctx[0x14] = \(d.isDVD ? 0 : 1) (launch request: PS2 \(d.isDVD ? "DVD" : "CD"))"))
        s.append(Step(who: "OSDSYS Launch (0x203970 → 0x202AB0)", what: "DiscThreadEnable(0); read cdrom0:\\SYSTEM.CNF;1 → BOOT2 = \(d.systemCNF["BOOT2"] ?? "?"); file name must match the first 10 characters of the disc ID"))
        let rec = history.records.first { $0.name == id }
        let count = Int(rec?.count ?? 0) + 1
        s.append(Step(who: "OSDSYS HistoryUpdate (0x201E98) + save (0x204AC0)", what: rec == nil
            ? "new record for \(id): count 1, mask 0x01 → one tower stub appears on the next boot; written to mc0:/B?DATA-SYSTEM/history"
            : "\(id): count \(rec!.count) → \(count)\(count >= 14 && (count - 14) % 10 == 0 ? ", a new random tower bit is added" : ""); written to mc0:/B?DATA-SYSTEM/history"))
        s.append(Step(who: "OSDSYS", what: "shutdown of subsystems (0x2021E8), then LoadExecPS2(\"rom0:PS2LOGO\", argc 1, argv {\"\(d.systemCNF["BOOT2"] ?? "")\"})"))
        s.append(Step(who: "KERNEL KLoadExec", what: "HardwareRestart, EELOAD re-copied to 0x82000, loads rom0:PS2LOGO (stub + LZ stream → 0x100000, main 0x102040)"))
        s.append(Step(who: "PS2LOGO", what: "rom0:ROMVER → region; load rom0:OSDSND and the embedded one-sample bank; GS 640×512 interlaced FIELD mode"))
        s.append(Step(who: "PS2LOGO LoadImage (0x101540)", what: "sceCdDecSet(1,1,5), sceCdRead(lba 0, 12 sectors) → 24,576 bytes; checksum \(d.logo?.region.map { "matches region \($0)" } ?? "does not match E/J (A/C consoles skip it)")"))
        s.append(Step(who: "PS2LOGO", what: "chime (cmd 0x5200 ×5), animation fields \(video == .pal ? "14–35" : "17–42"), then the last frame is held for 120 fields"))
        if let b = d.bootELF {
            let total = b.segments.reduce(0) { $0 + Int($1.memSize) }
            s.append(Step(who: "PS2LOGO → KERNEL", what: "SPU quit; LoadExecPS2(\"\(b.path)\", …) → KLoadExec → EELOAD LoadElfAll: \(b.fileName) at LBA \(b.lba), \(b.size) bytes"))
            s.append(Step(who: "EELOAD / kernel", what: String(format: "ELF %@: %d PT_LOAD segment(s), %d bytes in memory, entry 0x%08X — ExecPS2 jumps there with argv[0] = the boot path", b.isMIPS ? "(MIPS R5900)" : "(unexpected machine)", b.segments.count, total, b.entry)))
            for (i, seg) in b.segments.enumerated() {
                s.append(Step(who: "  segment \(i)", what: String(format: "vaddr 0x%08X, %d bytes from file, %d bytes in memory%@", seg.vaddr, seg.fileSize, seg.memSize, seg.memSize > seg.fileSize ? " (bss zeroed)" : "")))
            }
        }
        s.append(Step(who: "— stop —", what: "This is where the game takes over the console. The app ends the sequence here."))
        return s
    }
}
