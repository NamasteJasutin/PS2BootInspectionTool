//! Read-only view of a PS2 memory card as PCSX2 stores it: a raw image (`.ps2`, with or
//! without the 16 spare bytes per page) or a "folder" card.

use crate::bytes::Bytes;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// One directory entry of a card, as [`MemoryCard::list`] returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Entry {
    /// File or folder name (at most 32 bytes on a real card).
    pub name: String,
    /// Whether the entry is a folder.
    pub is_directory: bool,
    /// File size in bytes; for a folder, its number of directory entries.
    pub length: usize,
    cluster: u32,
}

/// An opened card. Reads are served from memory (image) or from the folder on disk.
pub struct MemoryCard {
    backing: Backing,
}

impl std::fmt::Debug for MemoryCard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.backing {
            Backing::Image(img) => f.debug_struct("MemoryCard").field("image_bytes", &img.data.len()).finish(),
            Backing::Folder(root) => f.debug_struct("MemoryCard").field("folder", root).finish(),
        }
    }
}

enum Backing {
    Image(Image),
    Folder(PathBuf),
}

impl MemoryCard {
    /// Opens a `.ps2` image (a file) or a PCSX2 folder card (a directory).
    pub fn open(path: &Path) -> Result<Self> {
        let backing = if path.is_dir() {
            Backing::Folder(path.to_path_buf())
        } else {
            Backing::Image(Image::new(std::fs::read(path)?)?)
        };
        Ok(Self { backing })
    }

    /// Lists a directory given as path components; `&[]` is the root.
    pub fn list(&self, path: &[&str]) -> Result<Vec<Entry>> {
        match &self.backing {
            Backing::Image(img) => img.list(&img.resolve(path)?),
            Backing::Folder(root) => {
                let dir = path.iter().fold(root.clone(), |p, c| p.join(c));
                let mut out = Vec::new();
                for e in std::fs::read_dir(dir)? {
                    let e = e?;
                    let name = e.file_name().to_string_lossy().into_owned();
                    if name.starts_with("_pcsx2_") {
                        continue;
                    }
                    let meta = e.metadata()?;
                    out.push(Entry { name, is_directory: meta.is_dir(), length: meta.len() as usize, cluster: 0 });
                }
                Ok(out)
            }
        }
    }

    /// Reads a whole file given as path components (`&["BEDATA-SYSTEM", "history"]`).
    pub fn read_file(&self, path: &[&str]) -> Result<Vec<u8>> {
        let joined = path.join("/");
        match &self.backing {
            Backing::Image(img) => {
                let (name, dir) = path.split_last().ok_or_else(|| Error::CardNotFound("(empty path)".into()))?;
                let d = img.resolve(dir)?;
                let e = img
                    .list(&d)?
                    .into_iter()
                    .find(|e| e.name == *name && !e.is_directory)
                    .ok_or_else(|| Error::CardNotFound(joined.clone()))?;
                img.read_chain(e.cluster, e.length)
            }
            Backing::Folder(root) => {
                let p = path.iter().fold(root.clone(), |p, c| p.join(c));
                std::fs::read(p).map_err(|_| Error::CardNotFound(joined))
            }
        }
    }
}

/// A directory is a chain of 512-byte entries; its entry count lives in the parent's entry
/// for it, or for the root in the root's own "." entry.
struct Directory {
    cluster: u32,
    count: usize,
}

/// The on-card filesystem: 512-byte pages grouped into clusters, a FAT reached through a
/// list of indirect clusters, and 512-byte directory entries.
struct Image {
    data: Vec<u8>,
    raw_page: usize,
    page: usize,
    pages_per_cluster: usize,
    alloc_offset: u32,
    root_cluster: u32,
    ifc_list: Vec<u32>,
}

impl Image {
    fn new(data: Vec<u8>) -> Result<Self> {
        let magic = b"Sony PS2 Memory Card Format ";
        if data.len() < 0x154 || &data[..magic.len()] != magic {
            return Err(Error::NotACard("superblock signature missing (unformatted card?)".into()));
        }
        let page = data.u16(0x28) as usize;
        let pages_per_cluster = data.u16(0x2A) as usize;
        let clusters = data.u32(0x30) as usize;
        if page != 512 || pages_per_cluster == 0 || clusters == 0 {
            return Err(Error::CardCorrupt("unsupported geometry".into()));
        }
        let pages = clusters * pages_per_cluster;
        let raw_page = if data.len() >= pages * (page + 16) {
            page + 16
        } else if data.len() >= pages * page {
            page
        } else {
            return Err(Error::CardCorrupt("image is shorter than the card it describes".into()));
        };
        Ok(Self {
            alloc_offset: data.u32(0x34),
            root_cluster: data.u32(0x3C),
            ifc_list: (0..32).map(|i| data.u32(0x50 + i * 4)).collect(),
            data,
            raw_page,
            page,
            pages_per_cluster,
        })
    }

    fn cluster_size(&self) -> usize { self.page * self.pages_per_cluster }

    fn cluster(&self, n: u32) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(self.cluster_size());
        for p in 0..self.pages_per_cluster {
            let off = (n as usize * self.pages_per_cluster + p) * self.raw_page;
            if off + self.page > self.data.len() {
                return Err(Error::CardCorrupt(format!("cluster {n} out of range")));
            }
            out.extend_from_slice(&self.data[off..off + self.page]);
        }
        Ok(out)
    }

    /// FAT entry for a cluster number relative to the allocatable area.
    fn fat(&self, n: u32) -> Result<u32> {
        let per = (self.cluster_size() / 4) as u32;
        let indirect = n / per;
        let ifc = (indirect / per) as usize;
        let ifc_cluster = *self.ifc_list.get(ifc).ok_or_else(|| Error::CardCorrupt("FAT index out of range".into()))?;
        let fat_cluster = self.cluster(ifc_cluster)?.u32(((indirect % per) * 4) as usize);
        Ok(self.cluster(fat_cluster)?.u32(((n % per) * 4) as usize))
    }

    fn read_chain(&self, first: u32, length: usize) -> Result<Vec<u8>> {
        // `length` comes from a directory entry: never trust it for the allocation.
        let mut out = Vec::with_capacity(length.min(self.data.len()));
        let mut c = first;
        let mut guard = 0;
        while out.len() < length {
            let n = c.checked_add(self.alloc_offset).ok_or_else(|| Error::CardCorrupt("cluster number overflows".into()))?;
            out.extend(self.cluster(n)?);
            let next = self.fat(c)?;
            if next == 0xFFFF_FFFF || next & 0x8000_0000 == 0 {
                break;
            }
            c = next & 0x7FFF_FFFF;
            guard += 1;
            if guard > 0x10000 {
                return Err(Error::CardCorrupt("FAT chain loops".into()));
            }
        }
        if out.len() < length {
            return Err(Error::CardCorrupt("file chain ends early".into()));
        }
        out.truncate(length);
        Ok(out)
    }

    fn list(&self, dir: &Directory) -> Result<Vec<Entry>> {
        if dir.count == 0 || dir.count >= 0x10000 {
            return Err(Error::CardCorrupt("bad directory size".into()));
        }
        let raw = self.read_chain(dir.cluster, dir.count * 512)?;
        let mut out = Vec::new();
        for i in 0..dir.count {
            let e = &raw[i * 512..(i + 1) * 512];
            let mode = e.u16(0);
            if mode & 0x8000 == 0 {
                continue;
            }
            let name = e.cstr(0x40, 32);
            if name == "." || name == ".." {
                continue;
            }
            out.push(Entry { name, is_directory: mode & 0x20 != 0, length: e.u32(4) as usize, cluster: e.u32(0x10) });
        }
        Ok(out)
    }

    fn resolve(&self, path: &[&str]) -> Result<Directory> {
        let root = self.root_cluster.checked_add(self.alloc_offset).ok_or_else(|| Error::CardCorrupt("root cluster out of range".into()))?;
        let mut dir = Directory { cluster: self.root_cluster, count: self.cluster(root)?.u32(4) as usize };
        for part in path {
            let e = self
                .list(&dir)?
                .into_iter()
                .find(|e| e.name == *part && e.is_directory)
                .ok_or_else(|| Error::CardNotFound(path.join("/")))?;
            dir = Directory { cluster: e.cluster, count: e.length };
        }
        Ok(dir)
    }
}
