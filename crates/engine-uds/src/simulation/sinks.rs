//! Result destinations (§14.9, §14.16): the engine streams instants to
//! whatever the caller attached, as pure data callbacks.
//!
//! The engine defines the traits and never a format: the dialect
//! tooling implements them over its file layouts, and a test can
//! implement them over a vector. Attachment is the caller's statement
//! of intent (§12.3); the engine feeds each instant as it is recorded
//! and seals the destination at finish.

use crate::overland::marcher::Marcher;
use crate::simulation::engine::Snapshot;

/// §14.9: a destination for reporting instants.
pub trait SnapshotSink: Send {
    /// One reporting instant, in recording order.
    fn append(&mut self, snap: &Snapshot) -> std::io::Result<()>;
    /// Seal the destination. Called exactly once, at finish.
    fn finish(self: Box<Self>) -> std::io::Result<()>;
}

/// §14.16: a destination for overland records, coupled runs only.
pub trait OverlandSink: Send {
    /// One overland record: the instant's run time, the live marcher,
    /// and the per-point exchange rates over the reporting period.
    fn append(&mut self, t: f64, marcher: &Marcher, exchange_rate: &[f64]) -> std::io::Result<()>;
    /// Seal the destination. Called exactly once, at finish.
    fn finish(self: Box<Self>) -> std::io::Result<()>;
}
