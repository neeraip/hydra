//! Model input and output: the predecessor file formats (specification §14).
//!
//! This module is the Tier 1 boundary: everything here binds syntax and
//! interpretation, and nothing here constrains how results are computed.
//! Model bytes are supplied in memory by callers; this crate performs no
//! filesystem or network I/O.

pub mod admin;
pub mod climate;
pub mod hydrology;
pub mod keywords;
pub mod lex;
pub mod lid;
pub mod objects;
pub mod options;
pub mod quality;
pub mod snow_rdii;
pub mod streets;
pub mod survey;
pub mod tables;
pub mod transects;
