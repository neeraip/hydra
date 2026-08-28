//! The uniform run surface (hydra-common spec §2.6): open a model for its
//! engine, advance it, observe progress, persist its results, collect its
//! warnings — implemented once, so no application encodes per-engine run
//! shapes.
//!
//! The shapes genuinely differ, and this enum is where that knowledge
//! lives: a wds run solves hydraulics in one phase and replays quality in
//! a second, streaming each report period to the results sink as soon as
//! its values are final; a uds run steps a single cascade and writes its
//! results when the run completes. Callers see neither — they construct a
//! session for their engine, pump [`EngineSession::advance`] until done,
//! and let the session manage the sink.
//!
//! Construction is deliberately per-engine ([`EngineSession::from_wds`],
//! [`EngineSession::from_uds`]) rather than a uniform `open(bytes)`:
//! opening is where engines legitimately differ in inputs (a uds model may
//! need climate records read from an auxiliary file the application owns)
//! and in error vocabulary (parse diagnostics, validation findings), and
//! flattening those into one signature would erase information callers
//! need. The escape hatches ([`EngineSession::as_wds`],
//! [`EngineSession::as_uds`]) exist for the same reason: engine-specific
//! capabilities — hotstart files, routing interfaces, result queries —
//! remain reachable without giving up the uniform drive loop.

use std::io::{Seek, Write};

use hydra_interop_epanet::WritableSimulation as _;

/// A results sink: the writer a session persists results into. The caller
/// opens the destination (applications own file I/O); the session owns the
/// writer for the duration of the run because the wds engine streams into
/// it between steps.
pub trait WriteSeek: Write + Seek + Send {}
impl<T: Write + Seek + Send> WriteSeek for T {}

/// Whether a run may be asked for a checkpoint.
///
/// Stated by the caller when results are attached, because it decides how
/// much of the run an engine has to keep in memory and no default is
/// safe in both directions. A checkpoint carries the results produced so
/// far, so that a run restored from one writes the whole run's results
/// rather than the tail; an engine that may be asked for one therefore
/// holds every reporting instant, which on a long run is the largest
/// thing it owns. Saying [`MayCheckpoint::No`] gives that up in exchange
/// for the memory, and asking for a checkpoint anyway is an error rather
/// than a checkpoint missing most of its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MayCheckpoint {
    /// The run may be checkpointed; keep what one would carry.
    Yes,
    /// It will not be; keep nothing the results file already holds.
    No,
}

/// Where a run currently is, reported by [`EngineSession::advance`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// Human-facing name of the phase being advanced ("Hydraulics",
    /// "Water quality", "Simulation"). Stable within a phase; changes at
    /// phase boundaries.
    pub phase: &'static str,
    /// Simulated time reached (s).
    pub t: f64,
    /// Total simulated duration (s).
    pub duration: f64,
    /// Whether the run has completed. Once `true`, further `advance` calls
    /// return the same completed progress and do nothing.
    pub done: bool,
}

/// One non-fatal diagnostic from a run, in the neutral shape every engine's
/// warnings map into.
///
/// This is the foundation contract's run-diagnostic record (foundation
/// contract §3.4.1) under the name the run surface uses for it. It is one
/// type, not two that agree: a diagnostic this surface collects is the same
/// value a report block is later produced from, so nothing translates
/// between them and nothing can drift.
pub type SessionWarning = hydra_common::RunDiagnostic;

/// Why [`EngineSession::advance`] failed.
#[derive(Debug)]
pub enum AdvanceError {
    /// The wds session failed — solver or session error, preserved typed so
    /// callers keep their error-classification behaviour.
    Wds(hydra_engine_wds::SessionError),
    /// Writing to the results sink failed.
    Io(std::io::Error),
}

impl std::fmt::Display for AdvanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wds(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "results could not be written: {e}"),
        }
    }
}

impl std::error::Error for AdvanceError {}

struct WdsRun {
    sim: hydra_engine_wds::Simulation,
    done: bool,
    duration: f64,
    output_units: hydra_engine_wds::FlowUnits,
    t: f64,
    stream: Option<hydra_interop_epanet::out_writer::OutStreamWriter<Box<dyn WriteSeek>>>,
}

struct UdsRun {
    sim: hydra_engine_uds::simulation::Simulation,
    done: bool,
}

/// One engine's run, behind the uniform surface. See the module docs.
pub enum EngineSession {
    /// A water-distribution run.
    Wds(Box<WdsRunOpaque>),
    /// An urban-drainage run.
    Uds(Box<UdsRunOpaque>),
}

/// Opaque holder so the enum variants leak no internal state. Constructed
/// only through [`EngineSession::from_wds`].
pub struct WdsRunOpaque(WdsRun);
/// Opaque holder so the enum variants leak no internal state. Constructed
/// only through [`EngineSession::from_uds`].
pub struct UdsRunOpaque(UdsRun);

impl EngineSession {
    /// Wrap a **loaded** wds session (a network must have been loaded —
    /// this is the state `Simulation::from_network` and `load` leave the
    /// session in). `output_units` is the flow-unit system results are
    /// written in (applications with a fixed convention pass it; pass the
    /// model's own units to match the CLI's behaviour).
    pub fn from_wds(
        sim: hydra_engine_wds::Simulation,
        output_units: hydra_engine_wds::FlowUnits,
    ) -> Self {
        let duration = sim.net().options.duration;
        Self::Wds(Box::new(WdsRunOpaque(WdsRun {
            sim,
            done: false,
            duration,
            output_units,
            t: 0.0,
            stream: None,
        })))
    }

    /// Wrap an opened uds session.
    pub fn from_uds(sim: hydra_engine_uds::simulation::Simulation) -> Self {
        Self::Uds(Box::new(UdsRunOpaque(UdsRun { sim, done: false })))
    }

    /// Total simulated duration (s).
    pub fn duration(&self) -> f64 {
        match self {
            Self::Wds(r) => r.0.duration,
            Self::Uds(r) => r.0.sim.duration(),
        }
    }

    /// Human-facing name of the phase the next [`Self::advance`] call will
    /// work in — for progress display before the first step.
    pub fn phase(&self) -> &'static str {
        match self {
            // Quality advances alongside the hydraulics rather than in a
            // pass of its own, so a water-distribution run has one phase, and
            // it is named as the drainage engine names its own.
            Self::Wds(_) | Self::Uds(_) => "Simulation",
        }
    }

    /// Attach the results sink before the first [`Self::advance`] call.
    ///
    /// `input_name` and `report_name` are results-file metadata (the wds
    /// binary prolog records them; the uds format carries its own header).
    /// A wds session writes its prolog immediately and streams periods as
    /// they become final; a uds session holds the sink and writes on
    /// [`Self::finish_results`].
    pub fn begin_results(
        &mut self,
        sink: Box<dyn WriteSeek>,
        may_checkpoint: MayCheckpoint,
        input_name: &str,
        report_name: &str,
    ) -> std::io::Result<()> {
        match self {
            Self::Wds(r) => {
                let run = &mut r.0;
                let mut stream = hydra_interop_epanet::out_writer::OutStreamWriter::begin(
                    sink,
                    &run.sim,
                    input_name,
                    report_name,
                    run.output_units,
                )?;
                stream.append_available(&run.sim)?;
                run.stream = Some(stream);
                Ok(())
            }
            Self::Uds(r) => hydra_interop_swmm::session::begin_results(
                &mut r.0.sim,
                Box::new(sink),
                may_checkpoint == MayCheckpoint::Yes,
            ),
        }
    }

    /// State that this run will never be asked for a checkpoint, so it
    /// need not keep the reporting instants a checkpoint carries. For a
    /// run with a results sink, [`Self::begin_results`] makes the same
    /// statement; call this for a run without one. A no-op for engines
    /// without checkpointing.
    pub fn forgo_checkpoint(&mut self) {
        match self {
            Self::Wds(_) => {}
            Self::Uds(r) => r.0.sim.forgo_checkpoint(),
        }
    }

    /// Advance the run by one step and report where it is.
    pub fn advance(&mut self) -> Result<Progress, AdvanceError> {
        match self {
            Self::Wds(r) => r.0.advance(),
            Self::Uds(r) => {
                let run = &mut r.0;
                if !run.done {
                    run.done = !run.sim.step();
                }
                Ok(Progress {
                    phase: "Simulation",
                    t: run.sim.time(),
                    duration: run.sim.duration(),
                    done: run.done,
                })
            }
        }
    }

    /// Finalize the results sink after the run completes. A no-op when no
    /// sink was attached.
    pub fn finish_results(&mut self) -> std::io::Result<()> {
        match self {
            Self::Wds(r) => {
                let run = &mut r.0;
                if let Some(stream) = run.stream.take() {
                    let mut w = stream.finish(&run.sim)?;
                    w.flush()?;
                }
                Ok(())
            }
            Self::Uds(r) => r.0.sim.finish_results(),
        }
    }

    /// Every non-fatal diagnostic the run has produced so far, in the
    /// neutral warning shape.
    pub fn warnings(&self) -> Vec<SessionWarning> {
        match self {
            Self::Wds(r) => {
                let sim = &r.0.sim;
                sim.warnings()
                    .iter()
                    .map(|w| {
                        let (code, message, element) =
                            hydra_interop_epanet::rpt_writer::describe_warning(w, sim);
                        SessionWarning {
                            code,
                            message,
                            element_id: element,
                            time: Some(w.t),
                        }
                    })
                    .collect()
            }
            Self::Uds(r) => {
                r.0.sim
                    .notices
                    .iter()
                    .map(|n| SessionWarning {
                        code: "runtime/notice".to_string(),
                        message: n.message.clone(),
                        element_id: None,
                        time: Some(n.t),
                    })
                    .collect()
            }
        }
    }

    /// Serialise this run's diagnostics into a caller-opened sink.
    ///
    /// The caller owns the destination, as it does for results
    /// ([`Self::begin_results`]): this writes bytes and never opens a file.
    /// Call it once the run is finished, since [`Self::warnings`] reports
    /// what the run has produced *so far*.
    ///
    /// The bytes are a JSON array of run-diagnostic records
    /// ([`SessionWarning`]), which is the format both applications store
    /// beside a results file under the name [`warnings_path`] gives. An
    /// empty array is meaningful and must still be written: it says the run
    /// was observed and raised nothing, which is not what an absent file
    /// says (foundation contract §3.4.1).
    pub fn write_warnings(&self, sink: &mut dyn Write) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&self.warnings()).map_err(std::io::Error::other)?;
        sink.write_all(&bytes)
    }

    /// Write the engine's text summary report.
    pub fn write_summary_text(&self, mut w: &mut dyn Write) -> std::io::Result<()> {
        match self {
            Self::Wds(r) => {
                let text = hydra_interop_epanet::rpt_writer::build_text_report(&r.0.sim)
                    .map_err(std::io::Error::other)?;
                w.write_all(text.as_bytes())
            }
            Self::Uds(r) => hydra_interop_swmm::session::write_report(&r.0.sim, &mut w),
        }
    }

    /// The engine's JSON summary, for engines that offer one.
    pub fn summary_json(&self) -> Option<std::io::Result<String>> {
        match self {
            Self::Wds(r) => Some(
                hydra_interop_epanet::rpt_writer::build_json_report(&r.0.sim)
                    .map_err(std::io::Error::other),
            ),
            Self::Uds(_) => None,
        }
    }

    /// The wds session, when this is a wds run — for engine-specific
    /// capabilities the uniform surface deliberately omits.
    pub fn as_wds(&self) -> Option<&hydra_engine_wds::Simulation> {
        match self {
            Self::Wds(r) => Some(&r.0.sim),
            Self::Uds(_) => None,
        }
    }

    /// The uds session, when this is a uds run — hotstart saving, routing
    /// interfaces, and other engine-specific capabilities.
    pub fn as_uds(&self) -> Option<&hydra_engine_uds::simulation::Simulation> {
        match self {
            Self::Wds(_) => None,
            Self::Uds(r) => Some(&r.0.sim),
        }
    }

    /// The uds session, mutably — attaching the §14.16 overland
    /// results stream is the capability that needs it.
    pub fn as_uds_mut(&mut self) -> Option<&mut hydra_engine_uds::simulation::Simulation> {
        match self {
            Self::Wds(_) => None,
            Self::Uds(r) => Some(&mut r.0.sim),
        }
    }

    /// Whether this engine can checkpoint a run at all.
    ///
    /// Asked before offering the operation rather than discovering it from
    /// a failure, so an application can leave it out of its interface
    /// instead of showing something that always refuses.
    pub fn checkpoints(&self) -> bool {
        matches!(self, Self::Uds(_))
    }

    /// Write a checkpoint of this run, from which another run continues
    /// exactly as if it had never stopped.
    ///
    /// `Err` when the engine does not checkpoint. Every engine that does
    /// writes its own format; nothing here interprets the bytes.
    pub fn save_checkpoint(&self, w: &mut dyn Write) -> Result<(), CheckpointError> {
        match self {
            Self::Uds(r) => {
                r.0.sim
                    .save_checkpoint(&mut { w })
                    .map_err(CheckpointError::Refused)
            }
            Self::Wds(_) => Err(CheckpointError::Unsupported("wds")),
        }
    }

    /// Restore a checkpoint over this run, which must have been opened
    /// from the same model with the same auxiliary files.
    pub fn load_checkpoint(&mut self, bytes: &[u8]) -> Result<(), CheckpointError> {
        match self {
            Self::Uds(r) => {
                r.0.sim
                    .load_checkpoint(bytes)
                    .map_err(CheckpointError::Refused)
            }
            Self::Wds(_) => Err(CheckpointError::Unsupported("wds")),
        }
    }
}

/// Why a checkpoint could not be written or read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointError {
    /// The engine keyed here does not checkpoint.
    Unsupported(&'static str),
    /// The engine refused, with its own reason.
    Refused(String),
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointError::Unsupported(key) => {
                write!(f, "the {key} engine does not checkpoint a run")
            }
            CheckpointError::Refused(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for CheckpointError {}

impl WdsRun {
    fn advance(&mut self) -> Result<Progress, AdvanceError> {
        let duration = self.duration;
        if self.done {
            return Ok(Progress {
                phase: "Simulation",
                t: duration,
                duration,
                done: true,
            });
        }
        // One loop: the step carries its own quality sub-steps, and the
        // instant it records must be taken before the next step overwrites it.
        let dt = self.sim.step_hydraulics().map_err(AdvanceError::Wds)?;
        self.append()?;
        if dt == 0.0 {
            self.done = true;
        } else {
            self.t += dt;
        }
        Ok(Progress {
            phase: "Simulation",
            t: self.t,
            duration,
            done: self.done,
        })
    }

    fn append(&mut self) -> Result<(), AdvanceError> {
        if let Some(stream) = self.stream.as_mut() {
            stream
                .append_available(&self.sim)
                .map_err(AdvanceError::Io)?;
        }
        Ok(())
    }
}

/// Where a run's diagnostics are stored, given the path its results were
/// written to: the results path with `.warnings.json` appended.
///
/// The convention lives here rather than in either application because both
/// must agree on it — the graphical app writes the file after a run and the
/// command-line app reads it when building a report, and a report that
/// silently found no diagnostics is indistinguishable from one whose run
/// raised none. Appending rather than replacing the extension keeps one
/// sidecar per results file, so two runs sharing a directory cannot
/// overwrite each other's diagnostics.
pub fn warnings_path(results: &std::path::Path) -> std::path::PathBuf {
    let mut name = results.file_name().unwrap_or_default().to_os_string();
    name.push(".warnings.json");
    results.with_file_name(name)
}

/// Parse a diagnostics sidecar written by [`EngineSession::write_warnings`].
///
/// Takes bytes rather than a path because acquiring them is the
/// application's job. A caller that finds no file must pass no list at all
/// rather than an empty one: the two say different things (foundation
/// contract §3.4.1).
pub fn read_warnings(bytes: &[u8]) -> Result<Vec<SessionWarning>, serde_json::Error> {
    serde_json::from_slice(bytes)
}

#[cfg(test)]
mod sidecar_tests {
    use super::*;

    /// One sidecar per results file, so two runs sharing a directory cannot
    /// overwrite each other's diagnostics. Appending rather than replacing
    /// the extension is what guarantees it.
    #[test]
    fn each_results_file_gets_its_own_sidecar() {
        let a = warnings_path(std::path::Path::new("/runs/morning.out"));
        let b = warnings_path(std::path::Path::new("/runs/evening.out"));
        assert_eq!(std::path::Path::new("/runs/morning.out.warnings.json"), a);
        assert_ne!(a, b);
        assert_eq!(a.parent(), b.parent(), "both sit beside their results");
    }

    #[test]
    fn a_sidecar_round_trips_through_its_own_format() {
        let written = vec![
            SessionWarning {
                code: "negative-pressure".into(),
                message: "Negative pressure at junction J1 at 1:00:00".into(),
                element_id: Some("J1".into()),
                time: Some(3600.0),
            },
            SessionWarning {
                code: "runtime/notice".into(),
                message: "A rain record was divided evenly.".into(),
                element_id: None,
                time: None,
            },
        ];
        let bytes = serde_json::to_vec(&written).expect("serialise");
        assert_eq!(written, read_warnings(&bytes).expect("read back"));
    }

    /// An empty list is a real answer and must survive the round trip: it
    /// says the run was observed and raised nothing, which an absent file
    /// does not say (foundation contract §3.4.1).
    #[test]
    fn an_empty_sidecar_is_not_an_absent_one() {
        let bytes = serde_json::to_vec(&Vec::<SessionWarning>::new()).expect("serialise");
        assert_eq!(
            Vec::<SessionWarning>::new(),
            read_warnings(&bytes).expect("read back")
        );
        assert!(read_warnings(b"").is_err(), "no bytes is not an empty list");
    }
}
