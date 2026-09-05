//! The engine's own checkpoint format (§12.3).
//!
//! A checkpoint captures the accepted state in full, so a run restored
//! from one continues bit-identically to a run never interrupted. That
//! is the whole of what it promises, and it is why every quantity here
//! is a 64-bit float in the engine's own units: a narrowed value or a
//! converted one restores something subtly other than what it saved.
//!
//! **Completeness is enforced by the compiler, not by a test.** Every
//! state-bearing type is read through an exhaustive destructure, with
//! the fields that are parameters rather than state bound to `_` and
//! said to be so. A field added to any of them fails to compile here
//! until it is either written or declared a parameter. The predecessor
//! copies fields by name into an index-addressed scratch array instead,
//! and its own file carries the consequence: a multi-pollutant hotstart
//! it writes cannot be read back by the reader that wrote it (§14.8).

use std::io::{self, Write};

/// Bytes identifying the format.
pub const STAMP: &[u8] = b"HYDRA-UDS-CHECKPOINT";

/// The format's version. Raised whenever the layout changes; a
/// checkpoint of any other version is refused rather than guessed at.
/// v2: the swale's cross-step rate left the state — the §3.4 advance
/// now uses this step's own start-of-step rate, which is not state.
pub const VERSION: u32 = 4;

/// A 64-bit FNV-1a hash, used to fingerprint a model's identifiers.
///
/// Chosen for having no dependency and no state beyond a `u64`: the
/// fingerprint has to refuse a checkpoint from another model, which
/// needs a hash to be stable and well spread, not cryptographic.
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Fnv {
        Fnv::new()
    }
}

impl Fnv {
    pub fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(0x100_0000_01b3);
        }
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

/// Write a 64-bit float, the only numeric form this format holds.
pub fn put_f(w: &mut impl Write, v: f64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

/// Write a count or index.
pub fn put_u(w: &mut impl Write, v: u64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}

/// Write a flag.
pub fn put_b(w: &mut impl Write, v: bool) -> io::Result<()> {
    w.write_all(&[u8::from(v)])
}

/// Write a length-prefixed slice of floats.
pub fn put_fs(w: &mut impl Write, vs: &[f64]) -> io::Result<()> {
    put_u(w, vs.len() as u64)?;
    for v in vs {
        put_f(w, *v)?;
    }
    Ok(())
}

/// Write a length-prefixed slice of float rows.
pub fn put_rows(w: &mut impl Write, rows: &[Vec<f64>]) -> io::Result<()> {
    put_u(w, rows.len() as u64)?;
    for row in rows {
        put_fs(w, row)?;
    }
    Ok(())
}

/// Reads the format back, one value at a time, refusing a short file
/// rather than reading a default.
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Reader<'a> {
        Reader { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at + n;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| format!("checkpoint ends after {} bytes", self.bytes.len()))?;
        self.at = end;
        Ok(slice)
    }

    pub fn tag(&mut self, expect: &[u8]) -> Result<(), String> {
        if self.take(expect.len())? == expect {
            Ok(())
        } else {
            Err("not a Hydra checkpoint".into())
        }
    }

    pub fn f(&mut self) -> Result<f64, String> {
        let b: [u8; 8] = self.take(8)?.try_into().map_err(|_| "short read")?;
        Ok(f64::from_le_bytes(b))
    }

    pub fn u(&mut self) -> Result<u64, String> {
        let b: [u8; 8] = self.take(8)?.try_into().map_err(|_| "short read")?;
        Ok(u64::from_le_bytes(b))
    }

    pub fn u32(&mut self) -> Result<u32, String> {
        let b: [u8; 4] = self.take(4)?.try_into().map_err(|_| "short read")?;
        Ok(u32::from_le_bytes(b))
    }

    pub fn b(&mut self) -> Result<bool, String> {
        Ok(self.take(1)?[0] != 0)
    }

    pub fn fs(&mut self) -> Result<Vec<f64>, String> {
        let n = self.u()? as usize;
        // A declared length is a claim about a file that may not hold it,
        // so it is checked against what remains before anything is sized
        // from it.
        if self.bytes.len() - self.at < n * 8 {
            return Err(format!(
                "checkpoint declares {n} values and holds {} bytes",
                self.bytes.len() - self.at
            ));
        }
        (0..n).map(|_| self.f()).collect()
    }

    /// Read a signed day index.
    pub fn i64(&mut self) -> Result<i64, String> {
        let b: [u8; 8] = self.take(8)?.try_into().map_err(|_| "short read")?;
        Ok(i64::from_le_bytes(b))
    }

    /// Read a length-prefixed identifier.
    pub fn text(&mut self) -> Result<String, String> {
        let n = self.u()? as usize;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| "checkpoint holds a malformed name".into())
    }

    /// Read length-prefixed float rows.
    pub fn rows(&mut self) -> Result<Vec<Vec<f64>>, String> {
        let n = self.u()? as usize;
        (0..n).map(|_| self.fs()).collect()
    }

    /// Whether every byte has been read. A checkpoint with bytes left
    /// over was written by a layout this one does not share.
    pub fn at_end(&self) -> bool {
        self.at == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_survives_the_round_trip() {
        let mut b = Vec::new();
        put_f(&mut b, core::f64::consts::PI).expect("write");
        put_u(&mut b, 42).expect("write");
        put_b(&mut b, true).expect("write");
        put_fs(&mut b, &[1.0, 2.5]).expect("write");
        let mut r = Reader::new(&b);
        assert_eq!(core::f64::consts::PI, r.f().expect("f"));
        assert_eq!(42, r.u().expect("u"));
        assert!(r.b().expect("b"));
        assert_eq!(vec![1.0, 2.5], r.fs().expect("fs"));
        assert!(r.at_end(), "every byte read");
    }

    /// The reader refuses a short file rather than reading a default,
    /// which is the difference between a refused restore and a run that
    /// silently continues from zero.
    #[test]
    fn a_short_checkpoint_is_refused() {
        let mut b = Vec::new();
        put_f(&mut b, 1.0).expect("write");
        b.truncate(4);
        let err = Reader::new(&b).f().unwrap_err();
        assert!(err.contains("ends after 4 bytes"), "{err}");
    }

    /// A declared length larger than the file is refused before it is
    /// used to size anything.
    #[test]
    fn a_length_larger_than_the_file_is_refused() {
        let mut b = Vec::new();
        put_u(&mut b, 1_000_000).expect("write");
        let err = Reader::new(&b).fs().unwrap_err();
        assert!(err.contains("declares 1000000 values"), "{err}");
    }

    #[test]
    fn the_hash_separates_orderings() {
        let mut a = Fnv::new();
        a.write(b"J1");
        a.write(b"J2");
        let mut b = Fnv::new();
        b.write(b"J2");
        b.write(b"J1");
        assert_ne!(a.finish(), b.finish(), "order must change the hash");
    }
}

#[cfg(test)]
mod format_version_tests {
    /// The checkpoint format version. A checkpoint of any other version is
    /// refused rather than guessed at, so a drift here silently orphans
    /// every checkpoint already saved. It could be changed with the suite
    /// green: the round-trip tests write and read with the same binary.
    #[test]
    fn the_checkpoint_version_is_the_one_every_saved_file_carries() {
        assert_eq!(4, super::VERSION);
    }
}
