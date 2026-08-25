// io — I/O layer for hydra-engine: format parsing and output writing.
//
// This module owns all format-specific reading and writing. Writers are
// generic over `WritableSimulation` so the trait object can be provided by
// the simulation module without creating a circular module dependency.

/// Network topology digest (model spec §4.4.7).
pub mod digest;
/// INP (EPANET input file) reader — public entry point is [`parse`].
pub mod inp_reader;
/// INP (EPANET input file) writer — public entry point is [`write_inp`].
pub mod inp_writer;
/// Binary `.out` result file reader.
pub mod out_reader;
/// Binary `.out` result file writer (used during simulation).
pub mod out_writer;
/// `.rpt` plain-text report writer.
pub mod rpt_writer;
/// EPANET unit conversion factors: model semantics, hosted with the
/// model (format-blind extraction); this alias keeps the historical
/// path alive until the lift re-points consumers.
pub use crate::engine_api::model::units;

pub use crate::engine_api::simulation::contract::{
    FlowBalance, FlowBalanceSummary, HydSnapshot, LinkQuantity, MassBalance, NodeQuantity,
    PumpEnergy, SimWarning, WarningKind, WritableSimulation,
};
pub use digest::compute_network_digest;
pub use inp_writer::{control_statements, rule_statements, write_inp};

use std::fmt;

use hydra_common::Recognition;

use crate::{Network, ValidationError};

// ── Parse entry point (§4 of crates/engine-wds/src/model/spec.md) ───────────

/// Why a model file could not be read at all (model spec §4.1.2).
///
/// Never a §2.9 constraint violation: those describe a network that *was*
/// constructed, and are reported separately — see [`ParseError`]. Holding the
/// two in one type let callers report "invalid network" for a file no network
/// was ever built from.
///
/// [`ForeignDialect`](Self::ForeignDialect) is the outcome to match on
/// separately. It says the file belongs to a different engine, not that it is a
/// bad file — the same bytes may be a flawless model in the tool that owns
/// them, and an application offering several engines can route it there.
#[derive(Debug)]
pub enum ReadError {
    /// The file format was not recognised (not an INP file).
    UnrecognisedFormat,
    /// The file is an INP file, but another modelling tool's (spec §4.1.1).
    ForeignDialect {
        /// The tool the file appears to belong to, e.g. `"SWMM"`.
        tool: &'static str,
        /// The foreign section whose presence gave it away, without brackets.
        section: &'static str,
    },
    /// A specific field value was syntactically valid but semantically out of range.
    InvalidField {
        /// The name of the offending INP field.
        field: String,
        /// Human-readable explanation of why the value is invalid.
        reason: String,
    },
    /// A node or link ID was defined more than once (EPANET error 215).
    DuplicateId {
        /// Object class: `"node"` or `"link"`.
        object: &'static str,
        /// The duplicated ID.
        id: String,
    },
    /// A parse error annotated with the INP section and 1-based source line
    /// number where it occurred.
    AtLine {
        /// INP section name (upper-case, without brackets).
        section: String,
        /// 1-based line number in the input file.
        line: usize,
        /// The underlying parse error.
        source: Box<ReadError>,
    },
}

impl ReadError {
    /// Attach section and line context to an error that does not already have it.
    pub(crate) fn at_line(self, section: &str, line: usize) -> ReadError {
        match self {
            Self::AtLine { .. } => self,
            other => Self::AtLine {
                section: section.to_string(),
                line,
                source: Box::new(other),
            },
        }
    }
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnrecognisedFormat => write!(f, "unrecognised model file format"),
            Self::ForeignDialect { tool, section } => write!(
                f,
                "this looks like a {tool} model, not an EPANET one \
                 (it declares a [{section}] section, which EPANET has no concept of)"
            ),
            Self::InvalidField { field, reason } => {
                write!(f, "invalid field '{field}': {reason}")
            }
            Self::DuplicateId { object, id } => {
                write!(f, "duplicate {object} ID '{id}'")
            }
            Self::AtLine {
                section,
                line,
                source,
            } => {
                write!(f, "[{section}] line {line}: {source}")
            }
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AtLine { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

/// Every way [`parse`] can fail: the file could not be read at all, or a
/// network was recovered and is not simulable (model spec §4.1.2).
///
/// The two are separate variants rather than one flat list because they call
/// for different responses. A [`Read`](Self::Read) failure leaves nothing to
/// act on; [`NotSimulable`](Self::NotSimulable) describes a network that
/// exists and can be opened and repaired — which is what [`parse_tolerant`]
/// returns instead of failing, and why its error type is [`ReadError`] alone.
#[derive(Debug)]
pub enum ParseError {
    /// No network could be built from the bytes, or they belong to another
    /// tool. Match on the inner [`ReadError`] to tell those apart.
    Read(ReadError),
    /// A network was recovered but violates one or more §2.9 constraints.
    NotSimulable(Vec<ValidationError>),
}

impl From<ReadError> for ParseError {
    fn from(err: ReadError) -> Self {
        Self::Read(err)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(err) => write!(f, "{err}"),
            Self::NotSimulable(errs) => {
                write!(f, "validation failed: {} error(s)", errs.len())
            }
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read(err) => Some(err),
            Self::NotSimulable(_) => None,
        }
    }
}

/// Parse a model file from raw bytes, returning a fully validated `Network`.
///
/// Format detection is by content: if the first non-whitespace byte is `[` or
/// `;` the input is treated as an EPANET INP file. Anything else is an error.
///
/// Any EPANET 2.x dialect is accepted (model spec §4.3) — 2.3 is the newest
/// understood, not a requirement — because unrecognised sections and option
/// keywords are skipped rather than rejected.
pub fn parse(bytes: &[u8]) -> Result<Network, ParseError> {
    match detect_format(bytes) {
        Some(()) => inp_reader::parse_inp(bytes),
        None => Err(ReadError::UnrecognisedFormat.into()),
    }
}

/// Parse a model file tolerantly (model spec §4.1.2): return the recovered
/// network together with its §2.9 validation errors, instead of failing on
/// them.
///
/// For callers that must read a model which is not yet simulable — an editor
/// loading a network under construction, where a junction exists for some
/// interval before anything connects it to a source.
///
/// The [`ReadError`] return type is the contract: tolerance extends to a
/// network that needs work, never to bytes no network could be built from
/// (an unknown format, a malformed line, an ambiguous id) nor to another
/// tool's model, which is sound but belongs elsewhere.
///
/// A non-empty error list means the network **must not be simulated**. Use
/// [`parse`] anywhere a model is read in order to run it, so an unsimulable
/// network cannot reach the solver.
pub fn parse_tolerant(bytes: &[u8]) -> Result<(Network, Vec<ValidationError>), ReadError> {
    match detect_format(bytes) {
        Some(()) => inp_reader::parse_inp_tolerant(bytes),
        None => Err(ReadError::UnrecognisedFormat),
    }
}

/// Judge whether these bytes are this engine's model (model spec §4.1.3).
///
/// This is the water-distribution engine's answer to the foundation layer's
/// recognition question (hydra-common spec §2.5), letting an application
/// route a model of unknown provenance without guessing from its extension.
///
/// Deliberately **stricter than parsing**: a file this returns
/// [`Recognition::Plausible`] for is still parsed normally by [`parse`] when
/// this engine is asked to. Automatic routing must not guess; an explicit
/// instruction from the user supplies evidence routing does not have.
///
/// Cheap by construction — section names only, no field parsing — so it can
/// be run against every registered engine before any model is read.
pub fn recognize(bytes: &[u8]) -> Recognition {
    match detect_format(bytes) {
        Some(()) => inp_reader::recognize_dialect(bytes),
        None => Recognition::no(),
    }
}

/// Content-based format detection (§4.1): `Some(())` when the bytes look like
/// an INP file. Shared so both parse modes sniff identically.
fn detect_format(bytes: &[u8]) -> Option<()> {
    let first = bytes
        .iter()
        .find(|&&b| !b.is_ascii_whitespace())
        .copied()
        .unwrap_or(0);
    matches!(first, b'[' | b';').then_some(())
}

// ── Result types (moved from hydra-simulation) ────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A sound model belonging to SWMM rather than EPANET.
    const SWMM: &[u8] = b"[TITLE]\n\n[SUBCATCHMENTS]\nS1 RG1 J1 10 50 500 0.5 0\n\n[END]\n";
    /// Structurally fine, but every reference to `J1` is ambiguous.
    const AMBIGUOUS: &[u8] = b"[JUNCTIONS]\nJ1 10\nJ1 20\n\n[OPTIONS]\nUnits LPS\n";
    /// A real network that simply is not finished: nothing fixes its grade.
    const UNFINISHED: &[u8] = b"[JUNCTIONS]\nJ1 10\n\n[OPTIONS]\nUnits LPS\n";

    #[test]
    fn the_three_reading_outcomes_are_distinguishable_by_type() {
        // Spec §4.1.2 requires these to be separable without reading prose,
        // because an interface that merges any two of them misreports both:
        // it calls another tool's model invalid, or offers to repair a network
        // that was never built.

        // 1. Another tool's model — sound, just not ours.
        assert!(matches!(
            parse(SWMM),
            Err(ParseError::Read(ReadError::ForeignDialect {
                tool: "SWMM",
                ..
            }))
        ));

        // 2. Unreadable — no network exists to hand back.
        let err = parse(AMBIGUOUS).expect_err("an ambiguous id cannot yield a network");
        assert!(matches!(err, ParseError::Read(_)));
        assert!(
            !matches!(
                err,
                ParseError::Read(ReadError::ForeignDialect { .. }) | ParseError::NotSimulable(_)
            ),
            "a duplicate id is neither a foreign dialect nor a recoverable network"
        );

        // 3. Not simulable — a network exists, and can be opened and repaired.
        assert!(matches!(
            parse(UNFINISHED),
            Err(ParseError::NotSimulable(_))
        ));
    }

    #[test]
    fn tolerance_applies_only_to_the_recoverable_outcome() {
        // The narrower error type is the point of the split: `parse_tolerant`
        // cannot fail for a network that merely needs work, and its signature
        // says so — there is no `NotSimulable` to return.
        let (network, errors) = parse_tolerant(UNFINISHED).expect("a network exists here");
        assert_eq!(network.nodes.len(), 1);
        assert!(
            !errors.is_empty(),
            "the reason must travel with the network"
        );

        assert!(matches!(
            parse_tolerant(SWMM),
            Err(ReadError::ForeignDialect { .. })
        ));
        assert!(parse_tolerant(AMBIGUOUS).is_err());
    }

    #[test]
    fn parse_rejects_unrecognised_format() {
        let bytes = b"{\"not\":\"inp\"}";
        let err = parse(bytes).expect_err("should reject non-INP content");
        assert!(matches!(
            err,
            ParseError::Read(ReadError::UnrecognisedFormat)
        ));
    }

    #[test]
    fn parse_accepts_whitespace_then_inp_section() {
        let inp_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/wds/single_pipe_hw.inp");
        let bytes = std::fs::read(inp_path).expect("read fixture inp");
        let mut with_prefix = b"\n\t  ".to_vec();
        with_prefix.extend_from_slice(&bytes);

        let network = parse(&with_prefix).expect("parse fixture as INP");
        assert!(!network.nodes.is_empty());
        assert!(!network.links.is_empty());
    }

    #[test]
    fn pump_energy_avg_efficiency_zero_when_offline() {
        let pe = PumpEnergy::default();
        assert_eq!(pe.avg_efficiency(), 0.0);
    }

    #[test]
    fn pump_energy_avg_efficiency_time_weighted() {
        let pe = PumpEnergy {
            efficiency_sum: 1800.0,
            time_online: 3600.0,
            ..PumpEnergy::default()
        };
        assert!((pe.avg_efficiency() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn flow_balance_ratio_accounts_for_storage_change_direction() {
        let fb = FlowBalance {
            total_inflow: 100.0,
            total_outflow: 90.0,
            demand_deficit: 0.0,
            initial_tank_volume: 50.0,
        };
        // Tank fills by 10: numerator adds +10.
        assert!((fb.balance_ratio(60.0) - 1.0).abs() < 1e-12);
        // Tank drains by 10: denominator adds +10.
        assert!((fb.balance_ratio(40.0) - (90.0 / 110.0)).abs() < 1e-12);
    }

    #[test]
    fn mass_balance_ratio_defaults_to_one_when_no_input_mass() {
        let mb = MassBalance::default();
        assert_eq!(mb.ratio(), 1.0);
    }
}
