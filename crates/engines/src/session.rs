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

use hydra_engine_wds::io::WritableSimulation as _;

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
#[derive(Debug, Clone, PartialEq)]
pub struct SessionWarning {
    /// Stable machine code, engine-authored (e.g. `warning/unbalanced`,
    /// `runtime/notice`).
    pub code: String,
    /// Human-facing message.
    pub message: String,
    /// The affected element's id, when the warning names one.
    pub element: Option<String>,
    /// Simulated time of the warning (s), when it is tied to one.
    pub time: Option<f64>,
}

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

enum WdsPhase {
    Hydraulics,
    Quality,
    Done,
}

struct WdsRun {
    sim: hydra_engine_wds::Simulation,
    phase: WdsPhase,
    quality_enabled: bool,
    duration: f64,
    output_units: hydra_engine_wds::FlowUnits,
    t: f64,
    stream: Option<hydra_engine_wds::io::out_writer::OutStreamWriter<Box<dyn WriteSeek>>>,
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
        let options = &sim.net().options;
        let quality_enabled = options.quality_mode != hydra_engine_wds::QualityMode::None;
        let duration = options.duration;
        Self::Wds(Box::new(WdsRunOpaque(WdsRun {
            sim,
            phase: WdsPhase::Hydraulics,
            quality_enabled,
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
            Self::Wds(r) => match r.0.phase {
                WdsPhase::Hydraulics => "Hydraulics",
                WdsPhase::Quality | WdsPhase::Done => "Water quality",
            },
            Self::Uds(_) => "Simulation",
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
                let mut stream = hydra_engine_wds::io::out_writer::OutStreamWriter::begin(
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
            Self::Uds(r) => {
                r.0.sim
                    .begin_results(Box::new(sink), may_checkpoint == MayCheckpoint::Yes)
            }
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
                            hydra_engine_wds::io::rpt_writer::describe_warning(w, sim);
                        SessionWarning {
                            code,
                            message,
                            element,
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
                        element: None,
                        time: Some(n.t),
                    })
                    .collect()
            }
        }
    }

    /// Write the engine's text summary report.
    pub fn write_summary_text(&self, mut w: &mut dyn Write) -> std::io::Result<()> {
        match self {
            Self::Wds(r) => {
                let text = hydra_engine_wds::io::rpt_writer::build_text_report(&r.0.sim)
                    .map_err(std::io::Error::other)?;
                w.write_all(text.as_bytes())
            }
            Self::Uds(r) => r.0.sim.write_report(&mut w),
        }
    }

    /// The engine's JSON summary, for engines that offer one.
    pub fn summary_json(&self) -> Option<std::io::Result<String>> {
        match self {
            Self::Wds(r) => Some(
                hydra_engine_wds::io::rpt_writer::build_json_report(&r.0.sim)
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
        match self.phase {
            WdsPhase::Hydraulics => {
                let dt = self.sim.step_hydraulics().map_err(AdvanceError::Wds)?;
                self.append()?;
                if dt == 0.0 {
                    self.phase = WdsPhase::Quality;
                    self.t = 0.0;
                } else {
                    self.t += dt;
                }
                Ok(Progress {
                    phase: "Hydraulics",
                    t: self.t,
                    duration,
                    done: false,
                })
            }
            WdsPhase::Quality => {
                if !self.quality_enabled {
                    self.sim.run_quality().map_err(AdvanceError::Wds)?;
                    self.append()?;
                    self.phase = WdsPhase::Done;
                    return Ok(Progress {
                        phase: "Water quality",
                        t: duration,
                        duration,
                        done: true,
                    });
                }
                let dt = self.sim.step_quality().map_err(AdvanceError::Wds)?;
                self.append()?;
                if dt == 0.0 {
                    self.phase = WdsPhase::Done;
                } else {
                    self.t += dt;
                }
                Ok(Progress {
                    phase: "Water quality",
                    t: self.t,
                    duration,
                    done: matches!(self.phase, WdsPhase::Done),
                })
            }
            WdsPhase::Done => Ok(Progress {
                phase: "Water quality",
                t: duration,
                duration,
                done: true,
            }),
        }
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
