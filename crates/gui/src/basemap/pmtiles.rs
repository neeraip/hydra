//! Minimal PMTiles v3 reader.
//!
//! Implements exactly the subset the download pipeline needs: header
//! parsing, gzip-compressed varint directories (root + one leaf level),
//! Hilbert tile IDs, and entry lookup returning each tile's exact byte
//! range in the archive. Per-entry byte lengths are the basis of the
//! exact "N MB to download" preflight numbers, which is why this exists
//! instead of a higher-level client that hides the directory.
//!
//! Format reference: <https://github.com/protomaps/PMTiles/blob/main/spec/v3/spec.md>

use std::collections::HashMap;
use std::io::Read;

pub const HEADER_SIZE: usize = 127;
const MAGIC: &[u8; 7] = b"PMTiles";

/// PMTiles compression codes (spec §compression).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gzip,
    Brotli,
    Zstd,
}

impl Compression {
    fn from_u8(v: u8) -> Result<Self, String> {
        match v {
            1 => Ok(Self::None),
            2 => Ok(Self::Gzip),
            3 => Ok(Self::Brotli),
            4 => Ok(Self::Zstd),
            other => Err(format!("unsupported PMTiles compression code {other}")),
        }
    }

    /// Value for the `Content-Encoding` header / store meta.
    pub fn encoding_name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gzip => "gzip",
            Self::Brotli => "br",
            Self::Zstd => "zstd",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Header {
    pub root_offset: u64,
    pub root_length: u64,
    pub leaf_offset: u64,
    pub data_offset: u64,
    pub internal_compression: Compression,
    pub tile_compression: Compression,
}

pub fn parse_header(bytes: &[u8]) -> Result<Header, String> {
    if bytes.len() < HEADER_SIZE {
        return Err("PMTiles header truncated".into());
    }
    if &bytes[0..7] != MAGIC {
        return Err("not a PMTiles archive (bad magic)".into());
    }
    if bytes[7] != 3 {
        return Err(format!("unsupported PMTiles version {}", bytes[7]));
    }
    let u64_at = |off: usize| u64::from_le_bytes(bytes[off..off + 8].try_into().expect("8 bytes"));
    Ok(Header {
        root_offset: u64_at(8),
        root_length: u64_at(16),
        leaf_offset: u64_at(40),
        data_offset: u64_at(56),
        internal_compression: Compression::from_u8(bytes[97])?,
        tile_compression: Compression::from_u8(bytes[98])?,
    })
}

/// One directory entry. `run_length == 0` marks a pointer to a leaf
/// directory; otherwise the entry covers tile IDs
/// `[tile_id, tile_id + run_length)`, all sharing one data range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub tile_id: u64,
    /// Relative to the data section (tile entries) or the leaf section
    /// (leaf pointers).
    pub offset: u64,
    pub length: u32,
    pub run_length: u32,
}

/// Decode a (already decompressed) serialized directory.
pub fn parse_directory(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let mut cur = bytes;
    let n = read_varint(&mut cur)? as usize;
    // Guard against a hostile/corrupt count before allocating.
    if n > bytes.len() {
        return Err("directory entry count exceeds payload".into());
    }
    let mut entries = vec![
        Entry {
            tile_id: 0,
            offset: 0,
            length: 0,
            run_length: 0
        };
        n
    ];
    let mut tile_id = 0u64;
    for e in &mut entries {
        tile_id = tile_id
            .checked_add(read_varint(&mut cur)?)
            .ok_or("tile id overflow")?;
        e.tile_id = tile_id;
    }
    for e in &mut entries {
        e.run_length = read_varint(&mut cur)? as u32;
    }
    for e in &mut entries {
        e.length = read_varint(&mut cur)? as u32;
    }
    let mut prev: Option<(u64, u32)> = None;
    for e in &mut entries {
        let v = read_varint(&mut cur)?;
        e.offset = if v == 0 {
            let (off, len) = prev.ok_or("offset back-reference without predecessor")?;
            off + u64::from(len)
        } else {
            v - 1
        };
        prev = Some((e.offset, e.length));
    }
    Ok(entries)
}

/// Decompress a directory or metadata payload per `internal_compression`.
pub fn decompress(compression: Compression, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match compression {
        Compression::None => Ok(bytes.to_vec()),
        Compression::Gzip => {
            let mut out = Vec::new();
            flate2::read::GzDecoder::new(bytes)
                .read_to_end(&mut out)
                .map_err(|e| format!("gzip decompress failed: {e}"))?;
            Ok(out)
        }
        other => Err(format!(
            "unsupported internal compression {other:?} (planet builds use gzip)"
        )),
    }
}

/// Cumulative pyramid size below zoom `z`: `sum_{i<z} 4^i`.
fn pyramid_base(z: u8) -> u64 {
    // (4^z - 1) / 3, safe for z <= 31.
    ((1u64 << (2 * u32::from(z))) - 1) / 3
}

/// PMTiles tile ID: pyramid base + Hilbert index within the zoom level.
pub fn tile_id(z: u8, x: u32, y: u32) -> u64 {
    if z == 0 {
        return 0;
    }
    pyramid_base(z) + xy2h(z, x, y)
}

/// Hilbert curve distance for an order-`z` curve (standard iterative
/// algorithm with quadrant rotation).
fn xy2h(z: u8, mut x: u32, mut y: u32) -> u64 {
    let mut d: u64 = 0;
    let mut s: u32 = 1 << (z - 1);
    while s > 0 {
        let rx = u32::from(x & s > 0);
        let ry = u32::from(y & s > 0);
        d += u64::from(s) * u64::from(s) * u64::from((3 * rx) ^ ry);
        // Rotate quadrant.
        if ry == 0 {
            if rx == 1 {
                x = s.wrapping_sub(1).wrapping_sub(x);
                y = s.wrapping_sub(1).wrapping_sub(y);
            }
            std::mem::swap(&mut x, &mut y);
        }
        s /= 2;
    }
    d
}

/// Binary-search a directory for the entry covering `id`. Returns the
/// covering tile entry, or the leaf pointer to descend into.
pub fn find_entry(entries: &[Entry], id: u64) -> Option<&Entry> {
    let idx = match entries.binary_search_by_key(&id, |e| e.tile_id) {
        Ok(i) => i,
        Err(0) => return None,
        Err(i) => i - 1,
    };
    let e = &entries[idx];
    if e.run_length == 0 {
        // Leaf pointer: candidate for any id at or after its first tile.
        Some(e)
    } else if id < e.tile_id + u64::from(e.run_length) {
        Some(e)
    } else {
        None
    }
}

/// Byte-range reads from an archive, local or remote.
pub trait RangeSource {
    fn read_range(&self, offset: u64, length: u64) -> Result<Vec<u8>, String>;
}

/// Remote archive over HTTP range requests, with simple retry.
pub struct HttpSource {
    client: reqwest::blocking::Client,
    url: String,
}

impl HttpSource {
    pub fn new(url: &str) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("hydra-gui/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            url: url.to_string(),
        })
    }
}

impl RangeSource for HttpSource {
    fn read_range(&self, offset: u64, length: u64) -> Result<Vec<u8>, String> {
        let range = format!("bytes={}-{}", offset, offset + length - 1);
        let mut last_err = String::new();
        for attempt in 0..3 {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(500 * (1 << attempt)));
            }
            match self
                .client
                .get(&self.url)
                .header(reqwest::header::RANGE, &range)
                .send()
            {
                Ok(resp) if resp.status() == reqwest::StatusCode::PARTIAL_CONTENT => {
                    match resp.bytes() {
                        Ok(b) if b.len() as u64 == length => return Ok(b.to_vec()),
                        Ok(b) => last_err = format!("short range read: {} of {length}", b.len()),
                        Err(e) => last_err = e.to_string(),
                    }
                }
                Ok(resp) => last_err = format!("range request returned {}", resp.status()),
                Err(e) => last_err = e.to_string(),
            }
        }
        Err(format!("range read failed after retries: {last_err}"))
    }
}

/// An opened archive: header + root directory + cached leaf directories.
pub struct Archive<S: RangeSource> {
    source: S,
    pub header: Header,
    root: Vec<Entry>,
    leaf_cache: HashMap<u64, Vec<Entry>>,
}

impl<S: RangeSource> Archive<S> {
    pub fn open(source: S) -> Result<Self, String> {
        let head = source.read_range(0, HEADER_SIZE as u64)?;
        let header = parse_header(&head)?;
        let root_raw = source.read_range(header.root_offset, header.root_length)?;
        let root = parse_directory(&decompress(header.internal_compression, &root_raw)?)?;
        Ok(Self {
            source,
            header,
            root,
            leaf_cache: HashMap::new(),
        })
    }

    /// Resolve a tile's absolute byte range in the archive, if present.
    pub fn locate(&mut self, z: u8, x: u32, y: u32) -> Result<Option<(u64, u32)>, String> {
        let id = tile_id(z, x, y);
        let Some(entry) = find_entry(&self.root, id) else {
            return Ok(None);
        };
        let entry = if entry.run_length == 0 {
            let leaf_off = entry.offset;
            let leaf_len = entry.length;
            if !self.leaf_cache.contains_key(&leaf_off) {
                let raw = self
                    .source
                    .read_range(self.header.leaf_offset + leaf_off, u64::from(leaf_len))?;
                let dir = parse_directory(&decompress(self.header.internal_compression, &raw)?)?;
                self.leaf_cache.insert(leaf_off, dir);
            }
            let leaf = &self.leaf_cache[&leaf_off];
            match find_entry(leaf, id) {
                // A leaf pointer inside a leaf is out of spec for v3.
                Some(e) if e.run_length > 0 => e.clone(),
                _ => return Ok(None),
            }
        } else {
            entry.clone()
        };
        Ok(Some((self.header.data_offset + entry.offset, entry.length)))
    }

    /// Read `length` bytes at an absolute archive offset.
    pub fn read_at(&self, offset: u64, length: u64) -> Result<Vec<u8>, String> {
        self.source.read_range(offset, length)
    }
}

/// Read a LEB128 varint, advancing the slice.
fn read_varint(cur: &mut &[u8]) -> Result<u64, String> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let (&byte, rest) = cur.split_first().ok_or("varint past end of buffer")?;
        *cur = rest;
        if shift >= 64 {
            return Err("varint overflow".into());
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Serialize a tiny synthetic archive so reader tests run offline.
    use super::*;
    use std::io::Write;

    pub fn write_varint(out: &mut Vec<u8>, mut v: u64) {
        loop {
            let mut byte = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if v == 0 {
                return;
            }
        }
    }

    pub fn serialize_directory(entries: &[Entry]) -> Vec<u8> {
        let mut out = Vec::new();
        write_varint(&mut out, entries.len() as u64);
        let mut last = 0u64;
        for e in entries {
            write_varint(&mut out, e.tile_id - last);
            last = e.tile_id;
        }
        for e in entries {
            write_varint(&mut out, u64::from(e.run_length));
        }
        for e in entries {
            write_varint(&mut out, u64::from(e.length));
        }
        for e in entries {
            // Always the explicit (offset + 1) form; 0 is the
            // back-reference sentinel.
            write_varint(&mut out, e.offset + 1);
        }
        out
    }

    pub fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        enc.write_all(bytes).unwrap();
        enc.finish().unwrap()
    }

    pub struct MemSource(pub Vec<u8>);

    impl RangeSource for MemSource {
        fn read_range(&self, offset: u64, length: u64) -> Result<Vec<u8>, String> {
            let start = offset as usize;
            let end = start + length as usize;
            self.0
                .get(start..end)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| "range out of bounds".into())
        }
    }

    /// Build an archive holding `tiles` as `(z, x, y, payload)`, with the
    /// root directory pointing at every tile directly (no leaves) unless
    /// `via_leaf` is set, in which case a single leaf directory holds all
    /// entries and the root holds one leaf pointer.
    pub fn build_archive(tiles: &[(u8, u32, u32, Vec<u8>)], via_leaf: bool) -> Vec<u8> {
        let mut ids: Vec<(u64, &Vec<u8>)> = tiles
            .iter()
            .map(|(z, x, y, b)| (tile_id(*z, *x, *y), b))
            .collect();
        ids.sort_by_key(|(id, _)| *id);

        let mut data = Vec::new();
        let mut entries = Vec::new();
        for (id, payload) in &ids {
            entries.push(Entry {
                tile_id: *id,
                offset: data.len() as u64,
                length: payload.len() as u32,
                run_length: 1,
            });
            data.extend_from_slice(payload);
        }

        let (root_dir, leaf_section) = if via_leaf {
            let leaf = gzip(&serialize_directory(&entries));
            let root = vec![Entry {
                tile_id: entries.first().map_or(0, |e| e.tile_id),
                offset: 0,
                length: leaf.len() as u32,
                run_length: 0,
            }];
            (gzip(&serialize_directory(&root)), leaf)
        } else {
            (gzip(&serialize_directory(&entries)), Vec::new())
        };

        let root_offset = HEADER_SIZE as u64;
        let leaf_offset = root_offset + root_dir.len() as u64;
        let data_offset = leaf_offset + leaf_section.len() as u64;

        let mut out = vec![0u8; HEADER_SIZE];
        out[0..7].copy_from_slice(MAGIC);
        out[7] = 3;
        out[8..16].copy_from_slice(&root_offset.to_le_bytes());
        out[16..24].copy_from_slice(&(root_dir.len() as u64).to_le_bytes());
        out[40..48].copy_from_slice(&leaf_offset.to_le_bytes());
        out[48..56].copy_from_slice(&(leaf_section.len() as u64).to_le_bytes());
        out[56..64].copy_from_slice(&data_offset.to_le_bytes());
        out[64..72].copy_from_slice(&(data.len() as u64).to_le_bytes());
        out[97] = 2; // internal compression: gzip
        out[98] = 2; // tile compression: gzip (as planet builds ship)
        out[100] = 0;
        out[101] = 15;
        out.extend_from_slice(&root_dir);
        out.extend_from_slice(&leaf_section);
        out.extend_from_slice(&data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    /// Vectors from the reference implementation's test suite.
    #[test]
    fn tile_ids_match_reference_vectors() {
        assert_eq!(tile_id(0, 0, 0), 0);
        assert_eq!(tile_id(1, 1, 0), 4);
        assert_eq!(tile_id(2, 1, 3), 11);
        assert_eq!(tile_id(3, 3, 0), 26);
        assert_eq!(tile_id(20, 0, 0), 366_503_875_925);
        assert_eq!(tile_id(21, 0, 0), 1_466_015_503_701);
    }

    #[test]
    fn directory_roundtrip_including_offset_backref() {
        let entries = vec![
            Entry {
                tile_id: 5,
                offset: 0,
                length: 10,
                run_length: 1,
            },
            Entry {
                tile_id: 9,
                offset: 10,
                length: 20,
                run_length: 3,
            },
            Entry {
                tile_id: 40,
                offset: 30,
                length: 7,
                run_length: 1,
            },
        ];
        let ser = serialize_directory(&entries);
        assert_eq!(parse_directory(&ser).unwrap(), entries);

        // The 0-sentinel back-reference form decodes to prev.offset+len.
        let mut with_backref = Vec::new();
        write_varint(&mut with_backref, 2);
        write_varint(&mut with_backref, 5); // id 5
        write_varint(&mut with_backref, 4); // id 9
        write_varint(&mut with_backref, 1);
        write_varint(&mut with_backref, 1); // run lengths
        write_varint(&mut with_backref, 10);
        write_varint(&mut with_backref, 20); // lengths
        write_varint(&mut with_backref, 1); // offset 0 (explicit form: 0+1)
        write_varint(&mut with_backref, 0); // sentinel → 0+10
        let parsed = parse_directory(&with_backref).unwrap();
        assert_eq!(parsed[1].offset, 10);
    }

    #[test]
    fn find_entry_handles_runs_and_gaps() {
        let entries = vec![
            Entry {
                tile_id: 10,
                offset: 0,
                length: 4,
                run_length: 2,
            },
            Entry {
                tile_id: 20,
                offset: 4,
                length: 4,
                run_length: 1,
            },
        ];
        assert!(find_entry(&entries, 9).is_none());
        assert_eq!(find_entry(&entries, 10).unwrap().tile_id, 10);
        assert_eq!(find_entry(&entries, 11).unwrap().tile_id, 10); // run
        assert!(find_entry(&entries, 12).is_none()); // gap
        assert_eq!(find_entry(&entries, 20).unwrap().tile_id, 20);
        assert!(find_entry(&entries, 21).is_none());
    }

    #[test]
    fn archive_locates_tiles_direct_and_via_leaf() {
        for via_leaf in [false, true] {
            let tiles = vec![
                (7u8, 20u32, 44u32, b"tile-a".to_vec()),
                (7, 21, 44, b"tile-b-longer".to_vec()),
            ];
            let bytes = build_archive(&tiles, via_leaf);
            let mut archive = Archive::open(MemSource(bytes)).unwrap();

            let (off_a, len_a) = archive.locate(7, 20, 44).unwrap().unwrap();
            assert_eq!(archive.read_at(off_a, u64::from(len_a)).unwrap(), b"tile-a");
            let (off_b, len_b) = archive.locate(7, 21, 44).unwrap().unwrap();
            assert_eq!(
                archive.read_at(off_b, u64::from(len_b)).unwrap(),
                b"tile-b-longer"
            );
            assert!(
                archive.locate(7, 0, 0).unwrap().is_none(),
                "leaf={via_leaf}"
            );
            assert_eq!(archive.header.tile_compression, Compression::Gzip);
        }
    }

    #[test]
    fn header_rejects_bad_magic_and_version() {
        let mut bytes = build_archive(&[], false);
        bytes[0] = b'X';
        assert!(parse_header(&bytes).is_err());
        let mut bytes = build_archive(&[], false);
        bytes[7] = 2;
        assert!(parse_header(&bytes).is_err());
    }

    #[test]
    fn varint_rejects_truncation_and_overflow() {
        let mut cur: &[u8] = &[0x80];
        assert!(read_varint(&mut cur).is_err());
        let mut cur: &[u8] = &[
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01,
        ];
        assert!(read_varint(&mut cur).is_err());
    }
}
