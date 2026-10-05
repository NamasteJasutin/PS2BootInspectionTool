import Foundation

/// Little-endian accessors over a byte buffer.
extension Data {
    func u8(_ o: Int) -> UInt8 { self[startIndex + o] }
    func u16(_ o: Int) -> UInt16 { UInt16(u8(o)) | UInt16(u8(o + 1)) << 8 }
    func u32(_ o: Int) -> UInt32 { UInt32(u16(o)) | UInt32(u16(o + 2)) << 16 }
    func i32(_ o: Int) -> Int32 { Int32(bitPattern: u32(o)) }
    func f32(_ o: Int) -> Float { Float(bitPattern: u32(o)) }
    func bytes(_ o: Int, _ n: Int) -> Data { subdata(in: startIndex + o ..< startIndex + o + n) }

    /// NUL-terminated ASCII string of at most `maxLength` bytes.
    func cString(_ o: Int, maxLength: Int) -> String {
        let raw = bytes(o, Swift.min(maxLength, count - o))
        let end = raw.firstIndex(of: 0) ?? raw.endIndex
        return String(decoding: raw[raw.startIndex ..< end], as: UTF8.self)
    }
}
