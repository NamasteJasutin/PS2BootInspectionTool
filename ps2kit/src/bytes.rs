//! Little-endian accessors over byte slices (bounds-checked, returning 0 past the end).
//!
//! Crate-private on purpose: "0 past the end" is the right default for the readers, which
//! validate what they find, but it would silently hide truncation from an outside caller.

pub(crate) trait Bytes {
    fn u8(&self, o: usize) -> u8;
    fn u16(&self, o: usize) -> u16;
    fn u32(&self, o: usize) -> u32;
    fn i32(&self, o: usize) -> i32 { self.u32(o) as i32 }
    fn i16(&self, o: usize) -> i16 { self.u16(o) as i16 }
    fn f32(&self, o: usize) -> f32 { f32::from_bits(self.u32(o)) }
    /// NUL-terminated string of at most `max` bytes.
    fn cstr(&self, o: usize, max: usize) -> String;
}

impl Bytes for [u8] {
    fn u8(&self, o: usize) -> u8 { self.get(o).copied().unwrap_or(0) }
    fn u16(&self, o: usize) -> u16 { self.u8(o) as u16 | (self.u8(o + 1) as u16) << 8 }
    fn u32(&self, o: usize) -> u32 { self.u16(o) as u32 | (self.u16(o + 2) as u32) << 16 }
    fn cstr(&self, o: usize, max: usize) -> String {
        let end = o.saturating_add(max).min(self.len());
        let s = &self[o.min(end)..end];
        let s = &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())];
        String::from_utf8_lossy(s).into_owned()
    }
}
