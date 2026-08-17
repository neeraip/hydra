//! The CLI's stderr vocabulary, spoken in a browser.
//!
//! A run that fails says the same thing here as it does at a terminal: one
//! JSON object per diagnostic, with the same `code` strings and the same
//! exit code. That is the point of this crate — someone comparing the demo
//! against `hydra run model.inp` should find the two agree, not merely
//! resemble each other.
//!
//! # Why these strings live here and not in the SDK
//!
//! They are duplicated from `crates/cli/src/main.rs`, which is the only
//! place they have ever existed. Lifting them into `hydra-sdk` would make
//! them a published contract that every integrator's error handling
//! depends on, and that is a decision about the public API rather than
//! about this demo — so they are copied, and [`CLI_ERROR_CODES`] exists to
//! make the copy checkable rather than silent.

use serde::Serialize;

// ── Exit-code contract ────────────────────────────────────────────────────────
//
// Mirrored from `hydra-cli`, whose module doc is the authority. Nothing in a
// browser exits, but the codes classify a failure and the demo prints them,
// so a reader can map what they see to what the CLI would have returned.

/// Simulation completed (warnings may appear in the report).
pub const EXIT_OK: i32 = 0;
/// Usage/input error (bad arguments, bad INP, missing input).
pub const EXIT_INPUT: i32 = 1;
/// Solver error (non-convergence or singularity).
pub const EXIT_SOLVER: i32 = 2;
/// I/O error. Reachable here only through the in-memory results sink.
pub const EXIT_IO: i32 = 3;
/// Internal error (unexpected engine state; please report a bug).
pub const EXIT_INTERNAL: i32 = 4;

/// Every `code` this crate can emit, which is every code the CLI's run path
/// can emit minus the ones about acquiring a file (`io/fetch`) or writing
/// one (`io/report`) — a browser does neither.
///
/// Kept as data so [`crate::run`]'s tests can assert that no diagnostic
/// escapes with a code outside the set. A typo'd code is not a compile
/// error and reads as plausible, which is exactly the kind of drift a
/// hand-mirrored vocabulary produces.
pub const CLI_ERROR_CODES: &[&str] = &[
    "input/engine",
    "input/format",
    "input/parse",
    "input/unsupported",
    "input/notice",
    "validation/network",
    "validation/mutation",
    "solver/hydraulic",
    "solver/quality",
    "session/error",
    "io/output",
    "internal",
];

/// One diagnostic, serialised in the CLI's exact JSON-line shape.
///
/// The fields are declared alphabetically, which reads oddly — `level`
/// belongs first — but is what makes the two byte-identical. The CLI builds
/// its lines with `serde_json::json!`, whose map is a `BTreeMap`, so it
/// emits keys in sorted order; a derived `Serialize` emits them in
/// declaration order. Matching means someone can diff a browser run against
/// a piped `hydra run` directly, rather than reformatting one of them
/// first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnostic {
    /// Stable machine code — one of [`CLI_ERROR_CODES`].
    pub code: String,
    /// `"error"` or `"warning"`.
    pub level: &'static str,
    /// Human-facing message.
    pub message: String,
    /// The affected element's id, when the diagnostic names one.
    pub object_id: Option<String>,
    /// Simulated time (s), when the diagnostic is tied to one.
    pub time_step: Option<f64>,
}

impl Diagnostic {
    /// An error-level diagnostic naming no element and no time.
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            level: "error",
            message: message.into(),
            object_id: None,
            time_step: None,
        }
    }

    /// A warning-level diagnostic naming no element and no time.
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            level: "warning",
            message: message.into(),
            object_id: None,
            time_step: None,
        }
    }

    /// The same warning, attributed to an element.
    pub fn about(mut self, element: Option<String>) -> Self {
        self.object_id = element;
        self
    }

    /// The same diagnostic, tied to a simulated time.
    pub fn at(mut self, time: Option<f64>) -> Self {
        self.time_step = time;
        self
    }

    /// The CLI's stderr line for this diagnostic.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }
}

/// A terminal failure: what went wrong, and what the CLI would have exited
/// with.
///
/// Carries a *list* because the CLI prints every parse or validation error
/// it found rather than the first — a model with nine broken pipes should
/// report nine, not send the reader round nine times.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Failure {
    /// The CLI's exit code for this class of failure.
    pub exit: i32,
    /// Every diagnostic describing it, in the order the CLI would print them.
    pub diagnostics: Vec<Diagnostic>,
}

impl Failure {
    /// A failure described by a single diagnostic.
    pub fn one(exit: i32, diagnostic: Diagnostic) -> Self {
        Self {
            exit,
            diagnostics: vec![diagnostic],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The line is the compatibility surface, so it is asserted literally
    /// rather than by round-tripping through serde — a round trip would
    /// agree with itself after a rename, and key *order* is part of the
    /// claim (see the struct's docs).
    ///
    /// Verified against a real CLI run:
    /// `hydra run junk.inp` prints
    /// `{"code":"input/engine","level":"error",…}`.
    #[test]
    fn a_diagnostic_serialises_as_the_cli_prints_it() {
        let d = Diagnostic::error("input/parse", "unexpected token");
        assert_eq!(
            d.to_line(),
            r#"{"code":"input/parse","level":"error","message":"unexpected token","object_id":null,"time_step":null}"#
        );
    }

    /// `null` rather than an absent key, again because the CLI does that:
    /// `serde_json::json!` writes every key it is given.
    #[test]
    fn an_attributed_warning_keeps_every_key() {
        let d = Diagnostic::warning("warning/unbalanced", "did not converge")
            .about(Some("P1".into()))
            .at(Some(3600.0));
        assert_eq!(
            d.to_line(),
            r#"{"code":"warning/unbalanced","level":"warning","message":"did not converge","object_id":"P1","time_step":3600.0}"#
        );
    }

    /// The keys are in the order `serde_json::json!` emits them, which is
    /// sorted — this is the assertion that fails if someone tidies the
    /// struct's field order into something that reads better.
    #[test]
    fn the_keys_are_in_the_order_the_cli_emits_them() {
        let line = Diagnostic::error("input/parse", "x").to_line();
        let keys: Vec<&str> = line
            .split(',')
            .filter_map(|part| part.split(':').next())
            .map(|k| k.trim_matches(['{', '"']))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "diagnostic keys are no longer sorted");
    }

    #[test]
    fn the_code_list_has_no_duplicates() {
        let mut sorted = CLI_ERROR_CODES.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), before, "CLI_ERROR_CODES lists a code twice");
    }
}
