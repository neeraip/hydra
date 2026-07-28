#![doc = include_str!("spec.md")]

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_REPORT_VERSION: &str = env!("CARGO_PKG_VERSION");

mod document;
mod render;
mod template;

pub use document::{assemble, ReportContext, ReportDocument, Section};
pub use render::{render_csv, render_html, render_txt};
#[cfg(feature = "pdf")]
pub use render::{render_pdf, PdfError};
pub use template::{ReportTemplate, TemplateBlock, TemplateError};
