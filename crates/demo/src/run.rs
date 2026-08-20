//! The CLI's run path, without a filesystem.
//!
//! `hydra run model.inp` does four things: decide which engine owns the
//! model, open it, pump the session to completion, and write the summary.
//! Only the first and last of those touch a file, and neither has to —
//! routing reads bytes, and the summary is built as a `String` before the
//! CLI ever writes it. So this module is that same sequence with the file
//! I/O removed, and the resemblance is the point: the diagnostics, the
//! phase names and the report text a browser shows are produced by the same
//! engine calls that produce them at a terminal.
//!
//! # Why this is plain Rust and not `#[wasm_bindgen]`
//!
//! Everything here compiles and runs on the host, so `cargo test` exercises
//! the whole decision surface — engine routing, error classification,
//! auxiliary-file handling, the drive loop — without a browser. The
//! `wasm_bindgen` layer ([`HydraRun`](crate::HydraRun)) is a translation
//! shell over this, thin enough that there is nothing in it left to get
//! wrong.
//!
//! # Why the caller sets the pace
//!
//! [`Run::advance`] takes a step budget instead of running to completion.
//! A wasm module runs on whatever thread called it, so a run driven to the
//! end in one call freezes the page for its whole duration — no progress,
//! no cancel, and a browser eventually offering to kill the tab. Handing
//! back after a bounded number of steps lets the caller paint between
//! them. The budget is a step count rather than a time slice because the
//! wall clock a time slice needs is exactly what `wasm32-unknown-unknown`
//! has no implementation of; the caller has `performance.now()` and can
//! size the next budget from how long the last one took.

use std::io::Write;

use hydra::common::EngineDescriptor;
use hydra::engines::{AdvanceError, EngineSession};
use hydra::{io, SessionError, Simulation};

use crate::aux_files::AuxFiles;
use crate::diagnostic::{
    Diagnostic, Failure, EXIT_INPUT, EXIT_INTERNAL, EXIT_IO, EXIT_OK, EXIT_SOLVER,
};
use crate::sink::SharedSink;

/// What to open, and how.
#[derive(Debug)]
pub struct OpenRequest<'a> {
    /// The model's bytes.
    pub model: &'a [u8],
    /// The model's file name. Recorded in the `.out` prolog, exactly as the
    /// CLI records the path it was given.
    pub model_name: &'a str,
    /// The engine to use, or `None` to detect it from the model. There is
    /// no default engine (common spec §2.5.1).
    pub engine: Option<&'a str>,
    /// Files supplied alongside the model, for the names it declares.
    pub aux: &'a AuxFiles,
    /// Whether to capture the binary `.out` results in memory — the
    /// equivalent of the CLI's `--results`, and off by default because the
    /// whole file has to be held (see [`crate::sink`]).
    pub capture_results: bool,
}

/// Where a run has got to.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// The phase being advanced ("Hydraulics", "Water quality",
    /// "Simulation").
    pub phase: &'static str,
    /// The phase this call *ended*, when it ended one.
    ///
    /// Separate from [`Self::phase`] because at a boundary the two differ,
    /// and the caller needs both: one names the line to close, the other
    /// names the line to open. Reading `phase` for both would label a
    /// finished phase with its successor's name.
    pub completed_phase: Option<&'static str>,
    /// Simulated time reached (s).
    pub t: f64,
    /// Total simulated duration (s).
    pub duration: f64,
    /// Whether the run has completed.
    pub done: bool,
    /// Engine steps taken so far, across every phase. The caller sizes its
    /// next budget from this and its own clock.
    pub steps: u64,
}

/// One model, opened and ready to be pumped.
pub struct Run {
    session: EngineSession,
    engine: &'static EngineDescriptor,
    results: Option<SharedSink>,
    /// Warnings already handed to the caller, so each phase reports only
    /// what it added.
    emitted: usize,
    /// Diagnostics produced but not yet collected.
    pending: Vec<Diagnostic>,
    phase: &'static str,
    /// Simulated time reached, carried across calls so [`Run::progress`]
    /// can answer without advancing anything.
    t: f64,
    steps: u64,
    done: bool,
    /// Set once `finish_results` has run, so a caller that keeps advancing
    /// past the end does not finalise the results file twice.
    finished: bool,
    /// The file name a uds model asked its hotstart state be saved under,
    /// and the bytes once the run has produced them.
    ///
    /// Held as the declared name rather than a flag because the caller
    /// offers it as a download and the model's own name is the one a reader
    /// expects to see — it is what the same run writes at a terminal.
    hotstart: Option<Hotstart>,
}

/// A hotstart file the model asked to save, waiting to be collected.
struct Hotstart {
    name: String,
    bytes: Option<Vec<u8>>,
}

impl Run {
    /// Decide the engine, open the model, and attach the results sink.
    ///
    /// Mirrors the CLI's `run`/`uds_cmd::run` up to the point the drive
    /// loop starts, including which failures are fatal and what each one
    /// exits with.
    pub fn open(req: OpenRequest<'_>) -> Result<Self, Failure> {
        let engine = resolve_engine(req.engine, req.model)?;
        let mut pending = Vec::new();
        let mut hotstart = None;
        let mut session = match engine.key {
            "uds" => open_uds(req.model, req.aux, &mut pending, &mut hotstart)?,
            _ => open_wds(req.model)?,
        };

        let results = if req.capture_results {
            let sink = SharedSink::new();
            session
                .begin_results(Box::new(sink.clone()), req.model_name, "")
                .map_err(|e| {
                    Failure::one(EXIT_IO, Diagnostic::error("io/output", e.to_string()))
                })?;
            Some(sink)
        } else {
            None
        };

        let phase = session.phase();
        Ok(Self {
            session,
            engine,
            results,
            emitted: 0,
            pending,
            phase,
            t: 0.0,
            steps: 0,
            done: false,
            finished: false,
            hotstart,
        })
    }

    /// The engine that owns this model.
    pub fn engine(&self) -> &'static EngineDescriptor {
        self.engine
    }

    /// Total simulated duration (s).
    pub fn duration(&self) -> f64 {
        self.session.duration()
    }

    /// Where the run is, without advancing it. Reports no completed phase —
    /// only [`Run::advance`] can end one.
    pub fn progress(&self) -> Progress {
        self.progress_completing(None)
    }

    fn progress_completing(&self, completed_phase: Option<&'static str>) -> Progress {
        Progress {
            phase: self.phase,
            completed_phase,
            t: self.t,
            duration: self.duration(),
            done: self.done,
            steps: self.steps,
        }
    }

    /// Advance by at most `max_steps` engine steps, stopping early when the
    /// run completes or the phase changes.
    ///
    /// Stopping at a phase boundary is not an optimisation: it is where the
    /// finished phase's warnings become available, and where the CLI closes
    /// one progress line and opens the next. Returning there lets the
    /// caller do both at the same moment the CLI does.
    pub fn advance(&mut self, max_steps: u32) -> Result<Progress, Failure> {
        if self.done {
            return Ok(self.progress());
        }
        let mut completed = None;
        for _ in 0..max_steps.max(1) {
            let p = self.session.advance().map_err(|e| {
                // The failed step's warnings explain the failure, so they
                // are collected before the error is returned rather than
                // lost with the run.
                self.collect_warnings();
                advance_failure(e)
            })?;
            self.steps += 1;
            self.t = p.t;
            // The boundary is handled before completion, and deliberately
            // does not consume `done` even when the same step carries it.
            // The last step of a wds run both ends hydraulics and finishes
            // quality; collapsing those into one result would skip the
            // quality phase's line entirely, and the CLI draws it. Ending
            // the call here gives the caller a frame to close one phase and
            // open the next; advancing a finished session is a no-op, so
            // the next call reports completion.
            if p.phase != self.phase {
                completed = Some(self.phase);
                self.phase = p.phase;
                self.collect_warnings();
                break;
            }
            if p.done {
                completed = Some(self.phase);
                self.t = p.duration;
                self.done = true;
                self.collect_warnings();
                self.finish()?;
                break;
            }
        }
        Ok(self.progress_completing(completed))
    }

    /// Every diagnostic produced since the last call, and clear them.
    ///
    /// Draining rather than accumulating: the caller prints each line as it
    /// arrives, exactly as the CLI writes them to stderr as a phase ends,
    /// and a growing list would reprint the earlier ones every time.
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.pending)
    }

    /// The engine's text summary — byte-for-byte what `hydra run` prints to
    /// stdout.
    pub fn report_text(&self) -> Result<String, Failure> {
        let mut buf: Vec<u8> = Vec::new();
        self.session
            .write_summary_text(&mut buf)
            .map_err(|e| Failure::one(EXIT_IO, Diagnostic::error("io/output", e.to_string())))?;
        String::from_utf8(buf).map_err(|e| {
            Failure::one(
                EXIT_INTERNAL,
                Diagnostic::error("internal", format!("report was not valid UTF-8: {e}")),
            )
        })
    }

    /// The engine's JSON summary, for engines that offer one. `None` for
    /// engines that do not, which is what the CLI refuses a `.json` path
    /// for.
    pub fn report_json(&self) -> Option<Result<String, Failure>> {
        self.session.summary_json().map(|r| {
            r.map_err(|e| Failure::one(EXIT_IO, Diagnostic::error("io/output", e.to_string())))
        })
    }

    /// The binary `.out` results, when they were captured and the run has
    /// finished writing them.
    pub fn results_bytes(&self) -> Option<Vec<u8>> {
        self.results.as_ref()?.bytes()
    }

    /// The exit code the CLI would return for a run that reached here.
    pub fn exit_code(&self) -> i32 {
        EXIT_OK
    }

    /// The hotstart file the model asked to save: its declared name and its
    /// bytes. `None` unless the model declared one and the run finished.
    ///
    /// The CLI writes this beside the model. There is nowhere to write here,
    /// so the caller is handed the bytes and offers them as a download —
    /// the same artifact, arriving by the only route a browser has.
    pub fn hotstart(&self) -> Option<(&str, &[u8])> {
        let h = self.hotstart.as_ref()?;
        Some((h.name.as_str(), h.bytes.as_deref()?))
    }

    fn finish(&mut self) -> Result<(), Failure> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        // Before `finish_results`, matching the CLI's order: it saves the
        // hotstart state from the finished session and only then finalises
        // the results file.
        self.save_hotstart()?;
        self.session
            .finish_results()
            .map_err(|e| Failure::one(EXIT_IO, Diagnostic::error("io/output", e.to_string())))
    }

    fn save_hotstart(&mut self) -> Result<(), Failure> {
        let Some(pending) = self.hotstart.as_mut() else {
            return Ok(());
        };
        let Some(sim) = self.session.as_uds() else {
            // Only a uds model can have asked, so reaching here would mean
            // the session changed engine underneath us.
            return Err(Failure::one(
                EXIT_INTERNAL,
                Diagnostic::error("internal", "uds session lost its engine"),
            ));
        };
        let mut bytes: Vec<u8> = Vec::new();
        sim.save_hotstart(&mut bytes).map_err(|e| {
            Failure::one(
                EXIT_IO,
                Diagnostic::error(
                    "io/output",
                    format!("hotstart file {:?}: {e}", pending.name),
                ),
            )
        })?;
        pending.bytes = Some(bytes);
        Ok(())
    }

    /// Move the session's new warnings into `pending`, in the CLI's shape.
    fn collect_warnings(&mut self) {
        let warnings = self.session.warnings();
        for w in &warnings[self.emitted.min(warnings.len())..] {
            self.pending.push(
                Diagnostic::warning(&w.code, w.message.clone())
                    .about(w.element.clone())
                    .at(w.time),
            );
        }
        self.emitted = warnings.len();
    }
}

// ── Engine resolution ─────────────────────────────────────────────────────────

/// Mirrors the CLI's `resolve_engine`: an explicit key is checked against
/// the registry and for availability; otherwise the model is asked. There
/// is deliberately no default.
fn resolve_engine(
    requested: Option<&str>,
    bytes: &[u8],
) -> Result<&'static EngineDescriptor, Failure> {
    if let Some(key) = requested.filter(|k| !k.is_empty()) {
        let engine = hydra::common::engine_by_key(key).map_err(|_| {
            let known: Vec<_> = hydra::common::ENGINES.iter().map(|e| e.key).collect();
            Failure::one(
                EXIT_INPUT,
                Diagnostic::error(
                    "input/engine",
                    format!("unknown engine {key:?} (known: {})", known.join(", ")),
                ),
            )
        })?;
        if !engine.is_available() {
            return Err(Failure::one(
                EXIT_INPUT,
                Diagnostic::error(
                    "input/engine",
                    format!(
                        "the {} engine ({}) is registered but not yet implemented",
                        engine.label, engine.key
                    ),
                ),
            ));
        }
        return Ok(engine);
    }

    hydra::engines::route(bytes).map_err(|e| {
        // Only ambiguity is worth suggesting an engine for — naming one is
        // exactly the evidence routing lacked.
        let message = if matches!(e, hydra::engines::RouteError::Ambiguous { .. }) {
            format!("{e}. Choose one explicitly")
        } else {
            e.to_string()
        };
        Failure::one(EXIT_INPUT, Diagnostic::error("input/engine", message))
    })
}

// ── Per-engine opening ────────────────────────────────────────────────────────

fn open_wds(bytes: &[u8]) -> Result<EngineSession, Failure> {
    let network = match io::parse(bytes) {
        Ok(n) => n,
        // Every validation error, not just the first: a model with nine
        // broken elements should report nine.
        Err(io::ParseError::NotSimulable(errs)) => {
            return Err(Failure {
                exit: EXIT_INPUT,
                diagnostics: errs
                    .iter()
                    .map(|e| Diagnostic::error("validation/network", e.to_string()))
                    .collect(),
            })
        }
        // A sound model belonging to another tool is not a damaged file.
        Err(io::ParseError::Read(io::ReadError::ForeignDialect { tool, section })) => {
            return Err(Failure::one(
                EXIT_INPUT,
                Diagnostic::error(
                    "input/engine",
                    format!(
                        "this is a {tool} model, not an EPANET one \
                         (it declares a [{section}] section)"
                    ),
                ),
            ))
        }
        Err(io::ParseError::Read(io::ReadError::UnrecognisedFormat)) => {
            return Err(Failure::one(
                EXIT_INPUT,
                Diagnostic::error("input/format", "unrecognised file format"),
            ))
        }
        Err(e) => {
            return Err(Failure::one(
                EXIT_INPUT,
                Diagnostic::error("input/parse", e.to_string()),
            ))
        }
    };

    let mut session = Simulation::create();
    session.load(network).map_err(session_failure)?;
    let units = session.flow_units().ok_or_else(|| {
        Failure::one(
            EXIT_INTERNAL,
            Diagnostic::error("internal", "flow units unavailable after load"),
        )
    })?;
    Ok(EngineSession::from_wds(session, units))
}

fn open_uds(
    bytes: &[u8],
    aux: &AuxFiles,
    pending: &mut Vec<Diagnostic>,
    hotstart: &mut Option<Hotstart>,
) -> Result<EngineSession, Failure> {
    use hydra::uds::io::climate::parse_any_climate_file;
    use hydra::uds::io::objects::parse_network;
    use hydra::uds::model::TemperatureSource;
    use hydra::uds::simulation::engine::{OpenError, Simulation as UdsSimulation};

    let text = String::from_utf8_lossy(bytes).into_owned();

    // Survey the model's auxiliary-file declarations before opening, as the
    // CLI does. Parse problems here are ignored on purpose — the open below
    // re-parses and reports them properly.
    let (net, _) = parse_network(&text);

    let climate = match &net.climate.temperature {
        Some(TemperatureSource::File { name, units, .. }) => match aux.get_text(name) {
            Some(text) => {
                parse_any_climate_file(&text, net.options.flow_units.is_us(), *units)
                    .map_err(|e| {
                        Failure::one(
                            EXIT_INPUT,
                            Diagnostic::error("input/parse", format!("climate file {name:?}: {e}")),
                        )
                    })?
                    .0
            }
            None => {
                // Not fatal, but it changes the answer, so it is said
                // loudly rather than run past.
                pending.push(Diagnostic::warning(
                    "input/notice",
                    format!(
                        "the model declares a climate file ({name:?}) that was not supplied, \
                         so temperature-driven processes run without it"
                    ),
                ));
                Vec::new()
            }
        },
        _ => Vec::new(),
    };

    // §14.8.3 and §14.12: the rainfall interface cache, then the gages'
    // own records in whichever layout each is written. Read here for the
    // same reason the CLI reads them — a model that runs differently in
    // the browser than at a terminal is a defect in whichever is behind —
    // and a file that was not dropped in is a warning rather than a
    // refusal, which is this surface's own policy for auxiliary files.
    let rain_iface = match &net.interface_files.rainfall {
        Some((hydra::uds::model::FileMode::Use, name)) => match aux.get(name) {
            Some(bytes) => Some(bytes.to_vec()),
            None => {
                pending.push(Diagnostic::warning(
                    "input/notice",
                    format!(
                        "the model declares a rainfall interface file ({name:?}) that was \
                         not supplied, so the gages' own records are read instead"
                    ),
                ));
                None
            }
        },
        _ => None,
    };
    let mut rain_files: Vec<(String, hydra::uds::io::rain::RainRecords)> = Vec::new();
    if rain_iface.is_none() {
        for gage in &net.gages {
            let hydra::uds::model::GageSource::File { file, .. } = &gage.source else {
                continue;
            };
            if rain_files.iter().any(|(name, _)| name == file) {
                continue;
            }
            match aux.get_text(file) {
                Some(rain_text) => {
                    let (records, notices) = hydra::uds::io::rain::parse_any_rain_file(&rain_text)
                        .map_err(|e| {
                            Failure::one(
                                EXIT_INPUT,
                                Diagnostic::error(
                                    "input/parse",
                                    format!("rain record {file:?}: {e}"),
                                ),
                            )
                        })?;
                    for notice in notices {
                        pending.push(Diagnostic::warning(
                            "input/rain",
                            format!("rain record {file:?}: {notice}"),
                        ));
                    }
                    rain_files.push((file.clone(), records));
                }
                None => pending.push(Diagnostic::warning(
                    "input/notice",
                    format!(
                        "the model reads rainfall from {file:?}, which was not supplied, \
                         so its gage receives no rain"
                    ),
                )),
            }
        }
    }

    let opened = match &rain_iface {
        Some(bytes) => UdsSimulation::open_with_rain_interface(&text, climate, bytes),
        None => UdsSimulation::open_with_rain_records(&text, climate, rain_files),
    };
    let (mut sim, diags, findings) = opened.map_err(|e| match e {
        OpenError::Parse(diags) => Failure {
            exit: EXIT_INPUT,
            diagnostics: diags
                .iter()
                .filter(|d| d.kind.is_error())
                .map(|d| Diagnostic::error("input/parse", d.to_string()))
                .collect(),
        },
        OpenError::Validation(findings) => Failure {
            exit: EXIT_INPUT,
            diagnostics: findings
                .iter()
                .filter(|v| v.kind.is_error())
                .map(|v| Diagnostic::error("validation/network", v.to_string()))
                .collect(),
        },
        OpenError::Routing(r) => Failure::one(
            EXIT_INPUT,
            Diagnostic::error("input/unsupported", r.to_string()),
        ),
        OpenError::Surface(s) => Failure::one(
            EXIT_INPUT,
            Diagnostic::error("input/unsupported", s.to_string()),
        ),
        OpenError::Controls(msg) | OpenError::Transport(msg) => {
            Failure::one(EXIT_INPUT, Diagnostic::error("input/unsupported", msg))
        }
    })?;

    for d in diags.iter().filter(|d| !d.kind.is_error()) {
        pending.push(Diagnostic::warning("input/notice", d.to_string()));
    }
    for v in findings.iter().filter(|v| !v.kind.is_error()) {
        pending.push(
            Diagnostic::warning("validation/mutation", v.to_string())
                .about(Some(v.element.clone())),
        );
    }

    // Deferred interface files are the engine's own finding now (§14.8), and
    // are reported through the validation findings above.
    let iface = &net.interface_files;
    // A declared hotstart save is honoured: the engine writes it to memory
    // when the run finishes and the caller offers it as a download. Recorded
    // under the name the model chose, which is the name the same run writes
    // at a terminal.
    if let Some(name) = &iface.hotstart_save {
        *hotstart = Some(Hotstart {
            name: name.clone(),
            bytes: None,
        });
    }
    // Routing outflows are the one write with nowhere to go. Said out loud
    // rather than skipped in silence, because a run that quietly produces
    // one fewer artifact than the same command at a terminal is worse than
    // one that admits it.
    if iface.outflows.is_some() {
        pending.push(Diagnostic::warning(
            "input/unsupported",
            "routing-outflow files cannot be saved in a browser, so this one is ignored",
        ));
    }

    if let Some(name) = &iface.hotstart_use {
        match aux.get(name) {
            Some(bytes) => sim.load_hotstart(bytes).map_err(|e| {
                Failure::one(
                    EXIT_INPUT,
                    Diagnostic::error("input/parse", format!("hotstart file {name:?}: {e}")),
                )
            })?,
            None => pending.push(Diagnostic::warning(
                "input/notice",
                format!(
                    "the model declares a hotstart file ({name:?}) that was not supplied, \
                     so the run starts from the model's initial state"
                ),
            )),
        }
    }

    if let Some((hydra::uds::model::FileMode::Use, name)) = &iface.runoff {
        match aux.get(name) {
            Some(bytes) => sim.supply_runoff(bytes).map_err(|e| {
                Failure::one(
                    EXIT_INPUT,
                    Diagnostic::error(
                        "input/parse",
                        format!("runoff interface file {name:?}: {e}"),
                    ),
                )
            })?,
            None => pending.push(Diagnostic::warning(
                "input/notice",
                format!(
                    "the model replays runoff from {name:?}, which was not supplied, \
                     so the surface is computed instead"
                ),
            )),
        }
    }

    if let Some((hydra::uds::model::FileMode::Use, name)) = &iface.rdii {
        match aux.get(name) {
            Some(bytes) => sim.supply_rdii(bytes).map_err(|e| {
                Failure::one(
                    EXIT_INPUT,
                    Diagnostic::error("input/parse", format!("RDII interface file {name:?}: {e}")),
                )
            })?,
            None => pending.push(Diagnostic::warning(
                "input/notice",
                format!(
                    "the model reads sewer inflow from {name:?}, which was not supplied, \
                     so it is convolved from the unit hydrographs instead"
                ),
            )),
        }
    }

    if let Some(name) = &iface.inflows {
        match aux.get_text(name) {
            Some(text) => sim.supply_routing_inflows(&text).map_err(|e| {
                Failure::one(
                    EXIT_INPUT,
                    Diagnostic::error("input/parse", format!("routing inflows file {name:?}: {e}")),
                )
            })?,
            None => pending.push(Diagnostic::warning(
                "input/notice",
                format!(
                    "the model declares a routing inflows file ({name:?}) that was not \
                     supplied, so those boundary inflows are zero"
                ),
            )),
        }
    }

    Ok(EngineSession::from_uds(sim))
}

// ── Error classification ──────────────────────────────────────────────────────

/// Mirrors the CLI's `emit_session_error` + `session_error_code`, which
/// split one error into a code string and an exit code by the same match.
fn session_failure(e: SessionError) -> Failure {
    let (code, exit) = match e {
        SessionError::ValidationFailed(_) => ("validation/network", EXIT_INPUT),
        SessionError::HydraulicSolve(_) => ("solver/hydraulic", EXIT_SOLVER),
        SessionError::QualityEngine(_) => ("solver/quality", EXIT_SOLVER),
        _ => ("session/error", EXIT_INPUT),
    };
    Failure::one(exit, Diagnostic::error(code, e.to_string()))
}

fn advance_failure(e: AdvanceError) -> Failure {
    match e {
        AdvanceError::Wds(session_error) => session_failure(session_error),
        AdvanceError::Io(io_error) => Failure::one(
            EXIT_IO,
            Diagnostic::error("io/output", io_error.to_string()),
        ),
    }
}

/// Run a model to completion in one call, for callers that do not need to
/// paint between steps — tests, mostly.
///
/// Deliberately not what the browser uses: see the module docs on why the
/// caller sets the pace.
pub fn run_to_completion(req: OpenRequest<'_>) -> Result<(Run, Vec<Diagnostic>), Failure> {
    let mut run = Run::open(req)?;
    let mut diagnostics = run.take_diagnostics();
    while !run.progress().done {
        run.advance(u32::MAX)?;
        diagnostics.extend(run.take_diagnostics());
    }
    Ok((run, diagnostics))
}

/// Write `bytes` as the CLI's stderr stream would, one JSON line each.
pub fn write_diagnostics(w: &mut impl Write, diagnostics: &[Diagnostic]) -> std::io::Result<()> {
    for d in diagnostics {
        writeln!(w, "{}", d.to_line())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
