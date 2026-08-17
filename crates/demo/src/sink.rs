//! An in-memory results sink the caller can still read after the run.
//!
//! `EngineSession::begin_results` takes ownership of its sink and
//! `finish_results` drops it, which is right for the CLI — the sink is a
//! file, and the bytes are on disk once it closes. A browser has nowhere to
//! put a file, so the `.out` has to stay reachable after the session has
//! finished with it, and that means the sink and the caller must share one
//! buffer rather than pass it along.
//!
//! Hence a handle: [`SharedSink`] hands a clone to the session, keeps a
//! clone itself, and both address the same bytes.
//!
//! # The size this costs
//!
//! The whole results file lives in memory here, which is exactly what
//! `io::out_reader`'s path-based streaming exists to avoid on native. A
//! browser cannot stream from a path, so capturing results is opt-in: the
//! demo asks before it does this, and a model whose results will not fit
//! can run without it.

use std::io::{Cursor, Result as IoResult, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};

/// A `Write + Seek + Send` sink over a shared `Vec<u8>`.
///
/// Cloning yields another handle to the *same* buffer, which is the whole
/// purpose — a clone is not a copy of the bytes.
#[derive(Clone, Default)]
pub struct SharedSink(Arc<Mutex<Cursor<Vec<u8>>>>);

impl SharedSink {
    /// An empty sink.
    pub fn new() -> Self {
        Self::default()
    }

    /// A copy of everything written so far.
    ///
    /// Returns `None` only if a previous writer panicked while holding the
    /// lock, which would mean the buffer's contents are not trustworthy
    /// anyway.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        let guard = self.0.lock().ok()?;
        Some(guard.get_ref().clone())
    }

    /// How many bytes have been written, without copying them.
    pub fn len(&self) -> usize {
        self.0.lock().map(|g| g.get_ref().len()).unwrap_or(0)
    }

    /// Whether nothing has been written yet.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn locked<T>(&self, f: impl FnOnce(&mut Cursor<Vec<u8>>) -> IoResult<T>) -> IoResult<T> {
        let mut guard = self.0.lock().map_err(|_| {
            std::io::Error::other("results buffer was poisoned by a panicking write")
        })?;
        f(&mut guard)
    }
}

impl Write for SharedSink {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        self.locked(|c| c.write(buf))
    }

    fn flush(&mut self) -> IoResult<()> {
        self.locked(|c| c.flush())
    }
}

impl Seek for SharedSink {
    fn seek(&mut self, pos: SeekFrom) -> IoResult<u64> {
        self.locked(|c| c.seek(pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reason the type exists: the session gets a handle, finishes with
    /// it, and the caller can still read what it wrote.
    #[test]
    fn a_clone_writes_into_the_same_buffer() {
        let sink = SharedSink::new();
        let mut handed_over = sink.clone();
        handed_over.write_all(b"periods").expect("write");
        drop(handed_over);
        assert_eq!(sink.bytes().expect("bytes"), b"periods");
    }

    /// The `.out` writer backfills its prolog after the run, so a sink that
    /// only appends would produce a file with a placeholder header.
    #[test]
    fn seeking_back_overwrites_rather_than_appends() {
        let sink = SharedSink::new();
        let mut w = sink.clone();
        w.write_all(b"XXXXtail").expect("write");
        w.seek(SeekFrom::Start(0)).expect("seek");
        w.write_all(b"head").expect("rewrite");
        assert_eq!(sink.bytes().expect("bytes"), b"headtail");
    }

    #[test]
    fn an_untouched_sink_is_empty() {
        let sink = SharedSink::new();
        assert!(sink.is_empty());
        assert_eq!(sink.len(), 0);
    }

    #[test]
    fn length_tracks_the_shared_buffer_not_the_handle() {
        let sink = SharedSink::new();
        let mut w = sink.clone();
        w.write_all(&[0u8; 32]).expect("write");
        assert_eq!(sink.len(), 32);
        assert!(!sink.is_empty());
    }
}
