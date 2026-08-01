//! Model input and output: the predecessor file formats (specification §14).
//!
//! This module is the Tier 1 boundary: everything here binds syntax and
//! interpretation, and nothing here constrains how results are computed.
//! Model bytes are supplied in memory by callers; this crate performs no
//! filesystem or network I/O.

pub mod keywords;
pub mod lex;
pub mod survey;
