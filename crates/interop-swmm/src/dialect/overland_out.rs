//! §14.16: the overland results stream — this engine's own framed
//! sidecar layout, written at the §14.9 reporting instants.
//!
//! Everything is little-endian and SI. The clock, geometry and ledger
//! are eight-byte floats; per-cell record values are four-byte floats,
//! the resolution every §14.9 reported result already has.

use std::io::{self, Write};

use crate::engine_api::overland::marcher::Marcher;
pub use crate::engine_api::overland::LedgerRow;
use crate::engine_api::overland::OverlandMesh;

/// §14.16 leading and closing magic.
pub const MAGIC: u32 = 1_214_727_218;
/// §14.16 format version.
pub const VERSION: u32 = 1;

/// The size of one record (bytes) for `nc` cells and `np` points.
pub(crate) fn record_len(nc: usize, np: usize) -> u64 {
    8 + 4 * 4 * nc as u64 + 4 * np as u64 + 8 * 11
}

/// The header's size (bytes) for `nv` vertices, `nc` cells, `np`
/// points.
pub(crate) fn header_len(nv: usize, nc: usize, np: usize) -> u64 {
    4 * 5 + 8 * 3 + 24 * nv as u64 + 12 * nc as u64 + 4 * np as u64
}

/// §14.16: the stream, generic over its sink exactly as the §14.9
/// stream is.
pub struct OverlandStream<W: Write> {
    w: W,
    nc: usize,
    np: usize,
    periods: i32,
}

impl<W: Write> OverlandStream<W> {
    /// Write the header: identity, counts, the reporting clock, and
    /// the mesh geometry a viewer renders without the model.
    pub fn begin(
        mut sink: W,
        mesh: &OverlandMesh,
        marcher: &Marcher,
        start_epoch: f64,
        report_step: f64,
        first_report_t: f64,
    ) -> io::Result<Self> {
        let (nv, nc) = (mesh.verts.len(), mesh.cells.len());
        let np = marcher.coupling_points().len();
        sink.write_all(&MAGIC.to_le_bytes())?;
        sink.write_all(&VERSION.to_le_bytes())?;
        for n in [nv as u32, nc as u32, np as u32] {
            sink.write_all(&n.to_le_bytes())?;
        }
        for f in [start_epoch, report_step, first_report_t] {
            sink.write_all(&f.to_le_bytes())?;
        }
        for v in &mesh.verts {
            for f in [v.x, v.y, v.z] {
                sink.write_all(&f.to_le_bytes())?;
            }
        }
        for c in &mesh.cells {
            for i in c.v {
                sink.write_all(&i.to_le_bytes())?;
            }
        }
        for cp in marcher.coupling_points() {
            sink.write_all(&cp.cell.to_le_bytes())?;
        }
        Ok(OverlandStream {
            w: sink,
            nc,
            np,
            periods: 0,
        })
    }

    /// Append one reporting instant: run time, per-cell depth, surface
    /// elevation and velocity, per-point exchange rate, and the §15.8
    /// ledger.
    pub fn append(&mut self, t: f64, marcher: &Marcher, exchange_rate: &[f64]) -> io::Result<()> {
        debug_assert_eq!(exchange_rate.len(), self.np);
        self.w.write_all(&t.to_le_bytes())?;
        let dry = marcher.dry_depth();
        for ci in 0..self.nc {
            let h = marcher.depth[ci];
            let (u, v) = if h > dry {
                let (qx, qy) = marcher.cell_velocity_proxy(ci);
                ((qx / h) as f32, (qy / h) as f32)
            } else {
                (0.0, 0.0)
            };
            for f in [h as f32, marcher.eta[ci] as f32, u, v] {
                self.w.write_all(&f.to_le_bytes())?;
            }
        }
        for q in exchange_rate {
            self.w.write_all(&(*q as f32).to_le_bytes())?;
        }
        for f in LedgerRow::of(marcher).to_array() {
            self.w.write_all(&f.to_le_bytes())?;
        }
        self.periods += 1;
        Ok(())
    }

    /// Write the epilog. A file without it is a run that did not
    /// finish, and the reader says so.
    pub fn finish(mut self) -> io::Result<W> {
        self.w.write_all(&self.periods.to_le_bytes())?;
        self.w.write_all(&MAGIC.to_le_bytes())?;
        self.w.flush()?;
        Ok(self.w)
    }
}

/// §14.16 as a destination: the sidecar stream is the sink.
impl crate::engine_api::simulation::sinks::OverlandSink for OverlandStream<Box<dyn Write + Send>> {
    fn append(&mut self, t: f64, marcher: &Marcher, exchange_rate: &[f64]) -> io::Result<()> {
        OverlandStream::append(self, t, marcher, exchange_rate)
    }

    fn finish(self: Box<Self>) -> io::Result<()> {
        OverlandStream::finish(*self).map(|_| ())
    }
}
