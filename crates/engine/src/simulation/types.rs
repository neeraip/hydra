use crate::ValidationError;
use crate::hydraulics::HydraulicError;
use crate::quality::QualityError;

// SimWarning, WarningKind, NodeQuantity, and LinkQuantity are defined in
// crate::io so that the output writers in that crate can reference them
// without a circular dependency.
pub use crate::io::{LinkQuantity, NodeQuantity, SimWarning, WarningKind};

// ── Error types ───────────────────────────────────────────────────────────────

/// Errors returned by the session API (§8.4).
#[derive(Debug, Clone)]
pub enum SessionError {
    // ── Fatal pre-simulation ─────────────────────────────────────────────────
    /// The data model failed one or more validation checks (§2.9).
    ValidationFailed(Vec<ValidationError>),
    /// The requested object ID does not exist in the loaded network.
    UnknownId(String),
    /// A result was requested for a time that has no recorded snapshot.
    NoSnapshotAtTime {
        /// The requested simulation time (seconds).
        requested_t: f64,
    },
    /// The operation is not valid in the current session phase.
    InvalidPhase {
        /// Phase name expected for this operation.
        expected: String,
        /// Actual phase at the time of the call.
        actual: String,
    },
    // ── Fatal mid-simulation ─────────────────────────────────────────────────
    /// The hydraulic solver encountered an error.
    HydraulicSolve(HydraulicError),
    /// The quality engine encountered an error.
    QualityEngine(QualityError),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ValidationFailed(errs) => {
                write!(f, "validation failed: {} error(s)", errs.len())
            }
            Self::UnknownId(id) => write!(f, "unknown object ID: '{id}'"),
            Self::NoSnapshotAtTime { requested_t } => {
                write!(f, "no result snapshot at t={requested_t}")
            }
            Self::InvalidPhase { expected, actual } => {
                write!(f, "invalid phase: expected {expected}, actual {actual}")
            }
            Self::HydraulicSolve(e) => write!(f, "hydraulic solver error: {e}"),
            Self::QualityEngine(e) => write!(f, "quality engine error: {e:?}"),
        }
    }
}

impl std::error::Error for SessionError {}

// ── Settable property enums ───────────────────────────────────────────────────

/// Settable node properties via `set_node_property`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeProperty {
    /// Override elevation (internal length unit).
    Elevation,
    /// Initial water quality.
    InitialQuality,
}

/// Settable link properties via `set_link_property`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkProperty {
    /// Override pipe roughness.
    Roughness,
    /// Override initial status (0 = Closed, 1 = Open).
    InitialStatus,
    /// Override initial setting (pump speed or valve setpoint).
    InitialSetting,
}

// ── Session phases ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Phase {
    /// Session allocated; no network loaded.
    Created,
    /// Network loaded and validated; hydraulic simulation not yet started.
    Loaded,
    /// Hydraulic EPS complete.
    HydraulicsDone,
    /// Hydraulic + quality EPS both complete.
    QualityDone,
}

impl Phase {
    pub(super) fn name(self) -> &'static str {
        match self {
            Phase::Created => "Created",
            Phase::Loaded => "Loaded",
            Phase::HydraulicsDone => "HydraulicsDone",
            Phase::QualityDone => "QualityDone",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hydraulics::HydraulicError;
    use crate::quality::QualityError;

    #[test]
    fn phase_name_all_variants() {
        assert_eq!(Phase::Created.name(), "Created");
        assert_eq!(Phase::Loaded.name(), "Loaded");
        assert_eq!(Phase::HydraulicsDone.name(), "HydraulicsDone");
        assert_eq!(Phase::QualityDone.name(), "QualityDone");
    }

    #[test]
    fn session_error_display_validation_failed() {
        let msg = SessionError::ValidationFailed(vec![]).to_string();
        assert_eq!(msg, "validation failed: 0 error(s)");
    }

    #[test]
    fn session_error_display_unknown_id() {
        let msg = SessionError::UnknownId("ABC".into()).to_string();
        assert_eq!(msg, "unknown object ID: 'ABC'");
    }

    #[test]
    fn session_error_display_no_snapshot_at_time() {
        let msg = SessionError::NoSnapshotAtTime { requested_t: 42.0 }.to_string();
        assert!(msg.contains("42"), "got: {msg}");
    }

    #[test]
    fn session_error_display_invalid_phase() {
        let msg = SessionError::InvalidPhase {
            expected: "Loaded".into(),
            actual: "Created".into(),
        }
        .to_string();
        assert!(
            msg.contains("Loaded") && msg.contains("Created"),
            "got: {msg}"
        );
    }

    #[test]
    fn session_error_display_hydraulic_solve() {
        let msg = SessionError::HydraulicSolve(HydraulicError::NotConverged).to_string();
        assert!(msg.contains("hydraulic solver"), "got: {msg}");
    }

    #[test]
    fn session_error_display_quality_engine() {
        let msg = SessionError::QualityEngine(QualityError::ModeNone).to_string();
        assert!(msg.contains("quality engine"), "got: {msg}");
    }
}
