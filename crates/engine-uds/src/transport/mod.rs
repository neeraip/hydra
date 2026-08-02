//! Constituent transport (§8): accumulation, mobilisation, network
//! transport, and treatment. The authoritative specification is `spec.md`
//! in this directory, included in the crate documentation.
//!
//! Currently implemented: §8.4 network transport — vertex and channel
//! reactors under the robust mixing form with exact exponential decay —
//! fed by the §8.1 non-surface mass sources (external inflows, sanitary,
//! subsurface, and sewer-inflow concentrations). Surface accumulation and
//! mobilisation (§8.2–§8.3) and treatment (§8.5) join as they land.

pub mod quality;

pub use quality::NetworkQuality;
