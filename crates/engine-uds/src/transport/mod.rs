//! Constituent transport (§8): accumulation, mobilisation, network
//! transport, and treatment. The authoritative specification is `spec.md`
//! in this directory, included in the crate documentation.
//!
//! Currently implemented: §8.2–§8.3 surface quality — mass-state
//! accumulation, street cleaning, the three mobilisation relations, and
//! the ponded store — and §8.4 network transport: vertex and channel
//! reactors under the robust mixing form with exact exponential decay,
//! fed by the §8.1 mass sources. Treatment (§8.5) joins as it lands.

pub mod quality;
pub mod surface;

pub use quality::NetworkQuality;
pub use surface::SurfaceQuality;
