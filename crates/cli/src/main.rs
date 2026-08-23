// hydra-cli — thin I/O shell around the SDK.
//
// Acquires model file bytes (local path or HTTP), drives the session API of
// whichever engine the model routes to, and writes output bytes. No parsing,
// unit conversion, or simulation logic lives here.
//
// Exit codes:
//   0 — simulation completed (warnings may appear in the report)
//   1 — usage/input error (bad arguments, bad INP, HTTP 4xx, missing input file)
//   2 — solver error (non-convergence or singularity)
//   3 — I/O error (permission denied, HTTP 5xx, network failure)
//   4 — internal error (unexpected engine state; please report a bug)

mod report_cmd;
mod uds_cmd;

use std::io::{IsTerminal, Write};
use std::process;
use std::time::Instant;

use clap::{CommandFactory, Parser};
use hydra::io;
use hydra::{SessionError, Simulation};

// Exit-code contract (see module doc above and `docs/src/getting-started/cli.md`).
/// Simulation completed (warnings may appear in the report).
const EXIT_OK: i32 = 0;
/// Usage/input error (bad arguments, bad INP, HTTP 4xx, missing input file).
const EXIT_INPUT: i32 = 1;
/// Solver error (non-convergence or singularity).
const EXIT_SOLVER: i32 = 2;
/// I/O error (permission denied, HTTP 5xx, network failure).
const EXIT_IO: i32 = 3;
/// Internal error (unexpected engine state; please report a bug).
const EXIT_INTERNAL: i32 = 4;

// Per-engine run knowledge (phases, streaming, persistence timing) lives in
// hydra::engines::EngineSession — the CLI drives every engine through it.

/// Hydra: water infrastructure simulation.
#[derive(Parser, Debug)]
#[command(
    name = "hydra",
    disable_version_flag = true,
    about,
    long_about = "Hydra: water infrastructure simulation.\n\n\
                  Hydra is a suite of domain engines. The engine that owns a model is \
                  detected from the model itself, never from its file extension: `.inp` \
                  belongs to both EPANET and SWMM. Pass --engine to name one explicitly."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Print Hydra and CLI version information and exit.
    #[arg(short = 'V', long = "version", global = true)]
    version: bool,

    /// Suppress progress output. Progress is also suppressed automatically
    /// when stderr is not a terminal (e.g. when piping or redirecting).
    /// Errors and diagnostics are never suppressed.
    #[arg(short = 'q', long = "quiet", global = true, conflicts_with = "verbose")]
    quiet: bool,

    /// Increase detail. Repeat for more: -v adds per-stage notes, -vv adds
    /// timing and internals.
    #[arg(short = 'v', action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Run a simulation on a model.
    Run(RunArgs),
    /// Build a report document from a completed run's results.
    Report(report_cmd::ReportArgs),
    /// List the simulation engines this build provides.
    Engines,
}

#[derive(clap::Args, Debug)]
struct RunArgs {
    /// Path of the model to run. May be a local path or an http:// or
    /// https:// URL (redirects followed, up to 10; bodies up to 1 GiB).
    #[arg(value_name = "MODEL")]
    model: String,

    /// Engine to run the model with (e.g. `wds`). Omit to detect the engine
    /// from the model's contents. There is no default engine.
    #[arg(long, value_name = "KEY")]
    engine: Option<String>,

    /// Path for the binary time-series results (`.out`). Omitted, no results
    /// file is written.
    #[arg(long, value_name = "PATH")]
    results: Option<String>,

    /// Path for the run summary in the engine's native format (`.rpt`, or
    /// `.json` when the path ends in `.json`). Omitted, it goes to stdout.
    #[arg(long, value_name = "PATH")]
    summary: Option<String>,

    /// Path to write a checkpoint of the finished run to. A run resumed
    /// from one continues exactly as if it had never stopped. Drainage
    /// models only for now.
    #[arg(long, value_name = "PATH")]
    checkpoint: Option<String>,

    /// Path of a checkpoint to resume from, in place of starting the model
    /// at its beginning. The model and every auxiliary file must be the
    /// ones the checkpoint was taken from.
    #[arg(long, value_name = "PATH")]
    resume: Option<String>,
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            // A bare model path was the pre-3.0 grammar. Say so, rather than
            // letting clap call it an unknown subcommand — every existing
            // script hits this exact error on first run after upgrading.
            if let Some(hint) = legacy_grammar_hint(&e) {
                emit_usage_error(&hint);
                process::exit(EXIT_INPUT);
            }
            let code = clap_error_exit_code(&e);
            let _ = e.print();
            process::exit(code);
        }
    };

    if cli.version {
        print_version_info();
        process::exit(EXIT_OK);
    }

    let exit_code = match &cli.command {
        Some(Command::Run(args)) => run(args, &cli),
        Some(Command::Report(args)) => report_cmd::run(args, &cli.verbose_level()),
        Some(Command::Engines) => list_engines(),
        None => {
            let _ = Cli::command().print_help();
            println!();
            EXIT_INPUT
        }
    };
    process::exit(exit_code);
}

impl Cli {
    /// Verbosity as a level, after `--quiet` has had its say.
    fn verbose_level(&self) -> u8 {
        if self.quiet {
            0
        } else {
            self.verbose
        }
    }
}

/// Recognise the pre-3.0 `hydra <model> [report] [output]` invocation and
/// produce a migration hint naming the replacement.
///
/// Keyed on clap's own parse failure rather than on argument position, so it
/// fires wherever the stray token appears — `hydra -q net.inp` included.
fn legacy_grammar_hint(e: &clap::Error) -> Option<String> {
    use clap::error::{ContextKind, ContextValue, ErrorKind};
    if e.kind() != ErrorKind::InvalidSubcommand {
        return None;
    }
    let ContextValue::String(token) = e.get(ContextKind::InvalidSubcommand)? else {
        return None;
    };
    // Only claim it if the token plausibly names a model; a genuine
    // subcommand typo deserves clap's own "did you mean" output.
    let looks_like_a_path = token.contains('.')
        || token.contains('/')
        || token.contains('\\')
        || token.starts_with("http");
    if !looks_like_a_path {
        return None;
    }
    Some(format!(
        "'hydra {token}' is no longer a command: running a model now needs the \
         'run' subcommand.\n       Use: hydra run {token}\n       \
         The report and output positionals became --summary and --results."
    ))
}

/// `hydra engines` — the registry, with each engine's availability.
fn list_engines() -> i32 {
    let mut out = std::io::stdout().lock();
    for engine in hydra::common::ENGINES {
        let status = if engine.is_available() {
            "available"
        } else {
            "planned"
        };
        let _ = writeln!(out, "{:<5} {:<20} {status}", engine.key, engine.label);
        let _ = writeln!(out, "      {}", engine.summary);
    }
    EXIT_OK
}

/// Exit code for a clap parse error: 0 for help/version display, 1 for
/// genuine usage errors (never clap's default 2, which is reserved for
/// solver errors).
fn clap_error_exit_code(e: &clap::Error) -> i32 {
    use clap::error::ErrorKind;
    match e.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => EXIT_OK,
        _ => EXIT_INPUT,
    }
}

fn print_version_info() {
    if cfg!(debug_assertions) {
        println!("Hydra version: {}", hydra::HYDRA_VERSION);
        println!("  Simulation version: {}", hydra::HYDRA_SIMULATION_VERSION);
        println!(
            "    Hydraulics version: {}",
            hydra::HYDRA_HYDRAULICS_VERSION
        );
        println!("    Quality version: {}", hydra::HYDRA_QUALITY_VERSION);
        println!("  Analysis version: {}", hydra::HYDRA_ANALYSIS_VERSION);
        println!("CLI version: {}", env!("CARGO_PKG_VERSION"));
    } else {
        println!("Hydra version: {}", hydra::HYDRA_VERSION);
        println!("CLI version: {}", env!("CARGO_PKG_VERSION"));
    }
}

/// Decide which engine owns a model: the one the user named, or the one the
/// model itself identifies (common spec §2.5.1).
///
/// Returns the exit code to fail with rather than an error type, because
/// every failure here is terminal and already reported.
fn resolve_engine(
    requested: Option<&str>,
    bytes: &[u8],
) -> Result<&'static hydra::common::EngineDescriptor, i32> {
    if let Some(key) = requested {
        let engine = match hydra::common::engine_by_key(key) {
            Ok(e) => e,
            Err(_) => {
                let known: Vec<_> = hydra::common::ENGINES.iter().map(|e| e.key).collect();
                emit_error(
                    "input/engine",
                    &format!("unknown engine {key:?} (known: {})", known.join(", ")),
                    None,
                    None,
                );
                return Err(EXIT_INPUT);
            }
        };
        // A planned engine resolves but cannot run: distinct from unknown,
        // and worth saying so plainly (common spec §2.3).
        if !engine.is_available() {
            emit_error(
                "input/engine",
                &format!(
                    "the {} engine ({}) is registered but not yet implemented",
                    engine.label, engine.key
                ),
                None,
                None,
            );
            return Err(EXIT_INPUT);
        }
        return Ok(engine);
    }

    match hydra::engines::route(bytes) {
        Ok(engine) => Ok(engine),
        Err(e) => {
            // Both outcomes are terminal; only the ambiguous one is worth
            // suggesting --engine for, since naming an engine is exactly the
            // evidence routing lacked.
            let suggest = matches!(e, hydra::engines::RouteError::Ambiguous { .. });
            let message = if suggest {
                format!("{e}. Name one with --engine")
            } else {
                e.to_string()
            };
            emit_error("input/engine", &message, None, None);
            Err(EXIT_INPUT)
        }
    }
}

/// Drives the full simulation lifecycle.
///
/// Session lifecycle (per-engine run shapes live in `EngineSession`):
/// ```text
/// parse + load                    // exit 1 on parse/validation failure
/// es = EngineSession::from_*(...)
/// es.begin_results(sink)          // attach the --results sink, if any
/// es.advance() until done         // exit 2 on solver error; warnings at phase ends
/// es.finish_results()             // finalize the results file
/// write_report(es)                // plain text or JSON
/// ```
///
/// Returns an exit code (0=ok, 1=input error, 2=solver error, 3=I/O error,
/// 4=internal error).
fn run(args: &RunArgs, cli: &Cli) -> i32 {
    let input_path = args.model.as_str();

    // ── Load network from file (§3.1) ─────────────────────────────────────────
    let bytes = match fetch(input_path) {
        Ok(b) => b,
        Err(FetchError::Input(msg)) => {
            emit_error("io/fetch", &msg, None, None);
            return EXIT_INPUT;
        }
        Err(FetchError::Io(msg)) => {
            emit_error("io/fetch", &msg, None, None);
            return EXIT_IO;
        }
    };

    // ── Decide which engine owns this model ───────────────────────────────────
    // Never by extension: `.inp` belongs to EPANET and SWMM alike. Either the
    // user named an engine, or the model itself has to identify one — there
    // is deliberately no default (common spec §2.5.1).
    let engine = match resolve_engine(args.engine.as_deref(), &bytes) {
        Ok(engine) => {
            if cli.verbose_level() > 0 {
                eprintln!("engine: {} ({})", engine.label, engine.key);
            }
            engine
        }
        Err(code) => return code,
    };
    if engine.key == "uds" {
        return uds_cmd::run(args, cli, bytes);
    }
    // §12.3 is the drainage engine's contract. Saying so beats writing no
    // file and letting a script believe it has one to resume from.
    for (flag, path) in [
        ("--checkpoint", &args.checkpoint),
        ("--resume", &args.resume),
    ] {
        if path.is_some() {
            emit_usage_error(&format!(
                "{flag} is for drainage models; the {} engine does not checkpoint",
                engine.key
            ));
            return EXIT_INPUT;
        }
    }

    let network = match io::parse(&bytes) {
        Ok(n) => n,
        Err(io::ParseError::NotSimulable(errs)) => {
            for e in &errs {
                emit_error("validation/network", &e.to_string(), None, None);
            }
            return EXIT_INPUT;
        }
        // A sound model belonging to another tool is not a damaged file, and
        // gets its own code so a caller can route it rather than report it as
        // bad input (model spec §4.1.2).
        Err(io::ParseError::Read(io::ReadError::ForeignDialect { tool, section })) => {
            emit_error(
                "input/engine",
                &format!(
                    "this is a {tool} model, not an EPANET one \
                     (it declares a [{section}] section)"
                ),
                None,
                None,
            );
            return EXIT_INPUT;
        }
        Err(io::ParseError::Read(io::ReadError::UnrecognisedFormat)) => {
            emit_error("input/format", "unrecognised file format", None, None);
            return EXIT_INPUT;
        }
        Err(e) => {
            emit_error("input/parse", &e.to_string(), None, None);
            return EXIT_INPUT;
        }
    };

    // ── Create session and load network ───────────────────────────────────────
    let mut session = Simulation::create();
    if let Err(e) = session.load(network) {
        emit_session_error(&e);
        return session_error_code(&e);
    }

    let output_units = match session.flow_units() {
        Some(u) => u,
        None => {
            emit_error("internal", "flow units unavailable after load", None, None);
            return EXIT_INTERNAL;
        }
    };

    let mut es = hydra::engines::EngineSession::from_wds(session, output_units);

    if let Some(out_path) = args.results.as_deref() {
        let report_path = args.summary.as_deref().unwrap_or("");
        let attach = std::fs::File::create(out_path).and_then(|f| {
            es.begin_results(
                // The distribution engine holds nothing a checkpoint
                // would want; the answer costs it nothing either way.
                Box::new(std::io::BufWriter::new(f)),
                hydra::engines::MayCheckpoint::No,
                input_path,
                report_path,
            )
        });
        if let Err(e) = attach {
            emit_error("io/output", &e.to_string(), None, None);
            return EXIT_IO;
        }
    }

    let mut progress = ProgressReporter::new(std::io::stderr().is_terminal() && !cli.quiet);
    progress.startup_banner();

    if let Err(code) = drive_with_progress(&mut es, &mut progress) {
        return code;
    }

    if let Err(e) = es.finish_results() {
        emit_error("io/output", &e.to_string(), None, None);
        return EXIT_IO;
    }

    // ── Write report ──────────────────────────────────────────────────────────
    // When the report goes to stdout and progress was printed on stderr,
    // add a blank separator line so the two don't visually run together.
    if args.summary.is_none() && progress.enabled {
        let _ = writeln!(std::io::stderr());
    }
    if let Err(e) = write_report(&es, args.summary.as_deref()) {
        emit_error("io/report", &e.to_string(), None, None);
        return EXIT_IO;
    }

    EXIT_OK
}

/// Pump an [`hydra::engines::EngineSession`] to completion, rendering
/// per-phase progress and emitting each phase's warnings as it ends —
/// shared by every engine's run path. On failure the progress line is
/// closed and the mapped exit code returned.
pub(crate) fn drive_with_progress(
    es: &mut hydra::engines::EngineSession,
    progress: &mut ProgressReporter,
) -> Result<(), i32> {
    let duration = es.duration();
    let mut current_phase = es.phase();
    let mut emitted = 0usize;
    progress.update(current_phase, 0.0, duration);
    loop {
        let p = match es.advance() {
            Ok(p) => p,
            Err(e) => {
                progress.finish_line();
                return Err(match e {
                    hydra::engines::AdvanceError::Wds(session_error) => {
                        emit_session_error(&session_error);
                        session_error_code(&session_error)
                    }
                    hydra::engines::AdvanceError::Io(io_error) => {
                        emit_error("io/output", &io_error.to_string(), None, None);
                        EXIT_IO
                    }
                });
            }
        };
        if p.phase != current_phase {
            // Phase boundary: close the finished phase's progress line and
            // flush the warnings it produced before the next phase starts.
            progress.finish_phase(duration);
            emitted = emit_warnings(es, emitted);
            current_phase = p.phase;
        }
        progress.update(p.phase, p.t, duration);
        if p.done {
            progress.finish_phase(duration);
            emit_warnings(es, emitted);
            return Ok(());
        }
    }
}

/// Writes human-readable progress to stderr during a simulation run.
///
/// When stderr is a terminal, each phase renders as a single transient line
/// rewritten in place using carriage-return semantics. The line shows:
/// phase name, simulated time / total duration, percentage, and a progress bar.
///
/// When stderr is not a terminal (pipe, redirect, `--quiet`), no output is
/// produced. Structured JSON diagnostics on stderr are unaffected.
struct ProgressReporter {
    enabled: bool,
    line_active: bool,
    phase_start: Option<Instant>,
    last_phase: String,
}

impl ProgressReporter {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            line_active: false,
            phase_start: None,
            last_phase: String::new(),
        }
    }

    fn startup_banner(&mut self) {
        if !self.enabled {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "Hydra v{}", env!("CARGO_PKG_VERSION"));
        let _ = stderr.flush();
    }

    fn update(&mut self, phase: &str, simulated_s: f64, total_s: f64) {
        if !self.enabled {
            return;
        }
        if self.phase_start.is_none() || self.last_phase != phase {
            self.phase_start = Some(Instant::now());
            self.last_phase = phase.to_owned();
        }
        let wall_s = self.phase_start.unwrap().elapsed().as_secs_f64();
        let mut stderr = std::io::stderr().lock();
        let _ = write!(
            stderr,
            "\r{}",
            render_progress_line(phase, simulated_s, total_s, wall_s)
        );
        let _ = stderr.flush();
        self.line_active = true;
    }

    /// Overwrite the progress line with a clean completion summary.
    /// No-op if no progress line is currently displayed.
    fn finish_phase(&mut self, sim_s: f64) {
        if !self.enabled || !self.line_active {
            return;
        }
        let phase = self.last_phase.clone();
        let wall_s = self
            .phase_start
            .map(|s| s.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let done = render_done_line(&phase, sim_s, wall_s);
        let mut stderr = std::io::stderr().lock();
        // Pad to clear any leftover characters from the wider progress line.
        let _ = writeln!(stderr, "\r{done:<72}");
        let _ = stderr.flush();
        self.line_active = false;
        self.phase_start = None;
    }

    /// Move off the progress line without printing a completion summary.
    /// Use on error paths so the error message starts on a clean line.
    fn finish_line(&mut self) {
        if !self.enabled || !self.line_active {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr);
        let _ = stderr.flush();
        self.line_active = false;
    }
}

fn format_sim_clock(time_s: f64) -> String {
    let total_seconds = time_s.round().max(0.0) as u64;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    format!("{hours}:{minutes:02}:{seconds:02}")
}

fn render_progress_line(phase: &str, simulated_s: f64, total_s: f64, wall_s: f64) -> String {
    let pct = if total_s > 0.0 {
        ((100.0 * simulated_s / total_s).clamp(0.0, 100.0)) as u32
    } else {
        100
    };
    let bar = render_bar(pct, 20);
    let sim_str = format!(
        "{} / {}",
        format_sim_clock(simulated_s),
        format_sim_clock(total_s.max(0.0))
    );
    format!(
        "  {phase:<14} {bar} {pct:>3}%   {sim_str:<21}   {}",
        format_wall(wall_s)
    )
}

fn render_bar(pct: u32, width: usize) -> String {
    let filled = ((pct as usize) * width / 100).min(width);
    let empty = width - filled;
    format!(
        "[{}{}]",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty)
    )
}

fn render_done_line(phase: &str, sim_s: f64, wall_s: f64) -> String {
    format!(
        "  \u{2713} {phase:<14} {}   {}",
        format_sim_clock(sim_s),
        format_wall(wall_s)
    )
}

fn format_wall(s: f64) -> String {
    if s < 60.0 {
        format!("{:.1}s", s)
    } else {
        let secs = s as u64;
        let m = secs / 60;
        let sec = secs % 60;
        format!("{m}m {sec:02}s")
    }
}

// ── Source resolution ────────────────────────────────────────────────────────

/// Error from fetching an input source, with exit code classification.
enum FetchError {
    /// Input error ([`EXIT_INPUT`]): file not found, HTTP 4xx.
    Input(String),
    /// I/O error ([`EXIT_IO`]): network failure, HTTP 5xx, local I/O.
    Io(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Input(msg) | FetchError::Io(msg) => f.write_str(msg),
        }
    }
}

/// Fetch the raw bytes of a model file from a local path or HTTP URL.
fn fetch(uri: &str) -> Result<Vec<u8>, FetchError> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        fetch_http(uri)
    } else {
        std::fs::read(uri).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FetchError::Input(format!("{uri}: {e}"))
            } else {
                FetchError::Io(format!("{uri}: {e}"))
            }
        })
    }
}

/// Connect timeout for HTTP model fetches.
const HTTP_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Global timeout for an entire HTTP model fetch (connect + response + body).
const HTTP_GLOBAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);
/// Maximum accepted response body size for an HTTP model fetch (1 GiB).
/// ureq's default is 10 MB, which is too small for large network models.
const HTTP_BODY_LIMIT: u64 = 1024 * 1024 * 1024;

/// Download a model file over HTTP/HTTPS.
///
/// Performs a single GET and buffers the full response before returning
/// (HTTP bodies cannot be seeked, so the two-pass INP parser runs against
/// the buffer). Redirects (up to 10) are followed automatically by ureq,
/// and plain `http://` is accepted — callers wrapping the CLI should be
/// aware of both. The fetch uses a 10 s connect timeout and a 300 s global
/// timeout so a stalled server cannot hang the CLI forever, and accepts
/// response bodies up to 1 GiB.
/// Error mapping: HTTP 4xx → Input (exit 1), 5xx / network → Io (exit 3).
fn fetch_http(url: &str) -> Result<Vec<u8>, FetchError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(HTTP_CONNECT_TIMEOUT))
        .timeout_global(Some(HTTP_GLOBAL_TIMEOUT))
        .build()
        .new_agent();
    let response = agent.get(url).call().map_err(|e| match &e {
        ureq::Error::StatusCode(code) => {
            let code = *code;
            if (400..500).contains(&code) {
                FetchError::Input(format!("HTTP {code} fetching {url}"))
            } else {
                FetchError::Io(format!("HTTP {code} fetching {url}"))
            }
        }
        _ => FetchError::Io(format!("network error fetching {url}: {e}")),
    })?;
    response
        .into_body()
        .with_config()
        .limit(HTTP_BODY_LIMIT)
        .read_to_vec()
        .map_err(|e| FetchError::Io(format!("error reading response body from {url}: {e}")))
}

// ── Report writing ───────────────────────────────────────────────────────────

/// Write the simulation report to `path` (None → stdout).
pub(crate) fn write_report(
    es: &hydra::engines::EngineSession,
    path: Option<&str>,
) -> anyhow::Result<()> {
    match path {
        None => {
            let mut stdout = std::io::stdout().lock();
            es.write_summary_text(&mut stdout)?;
            Ok(())
        }
        Some(p) if p.ends_with(".json") => match es.summary_json() {
            Some(json) => {
                std::fs::write(p, json?)?;
                Ok(())
            }
            None => {
                anyhow::bail!("JSON summaries are not available for this engine. Use a .rpt path")
            }
        },
        Some(p) => {
            let mut w = std::io::BufWriter::new(std::fs::File::create(p)?);
            es.write_summary_text(&mut w)?;
            w.flush()?;
            Ok(())
        }
    }
}

fn emit_usage_error(message: &str) {
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "error: {message}");
    let _ = writeln!(stderr);
    let _ = write!(stderr, "{}", usage_text());
    let _ = writeln!(stderr);
    let _ = writeln!(stderr, "For more information, try '--help'.");
}

fn usage_text() -> String {
    Cli::command().render_usage().to_string()
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

/// Write a structured JSON-line diagnostic to stderr.
///
/// Format: `{"level":"error","code":"<code>","message":"...","object_id":...,"time_step":...}`
pub(crate) fn emit_error(
    code: &str,
    message: &str,
    object_id: Option<&str>,
    time_step: Option<f64>,
) {
    let line = serde_json::json!({
        "level": "error",
        "code": code,
        "message": message,
        "object_id": object_id,
        "time_step": time_step,
    });
    eprintln!("{line}");
}

/// Emit session warnings `[from..]` as JSON-line diagnostics on stderr,
/// returning the new emitted count.
///
/// `from` lets each phase emit only the warnings it added, without
/// repeating those already printed.
fn emit_warnings(es: &hydra::engines::EngineSession, from: usize) -> usize {
    let warnings = es.warnings();
    let stderr = std::io::stderr();
    let mut buf = std::io::BufWriter::new(stderr.lock());
    for w in &warnings[from..] {
        let line = serde_json::json!({
            "level": "warning",
            "code": w.code,
            "message": w.message,
            "object_id": w.element,
            "time_step": w.time,
        });
        let _ = writeln!(buf, "{line}");
    }
    warnings.len()
}

fn emit_session_error(e: &SessionError) {
    let (code, msg) = match e {
        SessionError::ValidationFailed(_) => ("validation/network", e.to_string()),
        SessionError::HydraulicSolve(_) => ("solver/hydraulic", e.to_string()),
        SessionError::QualityEngine(_) => ("solver/quality", e.to_string()),
        _ => ("session/error", e.to_string()),
    };
    emit_error(code, &msg, None, None);
}

fn session_error_code(e: &SessionError) -> i32 {
    match e {
        SessionError::ValidationFailed(_) => EXIT_INPUT,
        SessionError::HydraulicSolve(_) | SessionError::QualityEngine(_) => EXIT_SOLVER,
        _ => EXIT_INPUT,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Parse a command line into a Cli struct.
    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("parse failed")
    }

    fn run_args(cli: &Cli) -> &RunArgs {
        match cli.command.as_ref().expect("no subcommand") {
            Command::Run(a) => a,
            other => panic!("expected run, got {other:?}"),
        }
    }

    #[test]
    fn run_takes_the_model_positionally() {
        let cli = parse(&["hydra", "run", "net1.inp"]);
        let a = run_args(&cli);
        assert_eq!(a.model, "net1.inp");
        assert_eq!(a.results, None);
        assert_eq!(a.summary, None);
        assert_eq!(a.engine, None);
    }

    #[test]
    fn run_names_its_artifacts() {
        let cli = parse(&[
            "hydra",
            "run",
            "net1.inp",
            "--summary",
            "r.rpt",
            "--results",
            "o.out",
        ]);
        let a = run_args(&cli);
        assert_eq!(a.summary.as_deref(), Some("r.rpt"));
        assert_eq!(a.results.as_deref(), Some("o.out"));
    }

    /// The pre-3.0 grammar was `hydra <input> <report> <output>`, where the
    /// second and third positionals were the report and binary output. There
    /// is now exactly one positional, so the old form cannot be misread as a
    /// valid new one.
    #[test]
    fn the_legacy_positional_triple_is_gone() {
        assert!(Cli::try_parse_from(["hydra", "run", "net1.inp", "r.rpt", "o.out"]).is_err());
    }

    #[test]
    fn engine_is_never_defaulted_at_the_parse_layer() {
        // Absent means "detect", not "wds". If this ever gains a default the
        // no-fallback rule (common spec §2.5.1) is silently broken.
        assert_eq!(run_args(&parse(&["hydra", "run", "n.inp"])).engine, None);
        assert_eq!(
            run_args(&parse(&["hydra", "run", "n.inp", "--engine", "wds"]))
                .engine
                .as_deref(),
            Some("wds")
        );
    }

    #[test]
    fn a_bare_model_path_gets_a_migration_hint_not_a_subcommand_error() {
        let err = Cli::try_parse_from(["hydra", "net1.inp"]).unwrap_err();
        let hint = legacy_grammar_hint(&err).expect("no hint for a bare model path");
        assert!(hint.contains("hydra run net1.inp"), "{hint}");
        assert!(hint.contains("--summary"), "{hint}");
    }

    #[test]
    fn the_migration_hint_fires_regardless_of_flag_position() {
        // Keyed on clap's parse failure, not on argv position — the old
        // dispatch missed `hydra --quiet report ...` for exactly this reason.
        let err = Cli::try_parse_from(["hydra", "-q", "net1.inp"]).unwrap_err();
        assert!(legacy_grammar_hint(&err).is_some());
    }

    #[test]
    fn a_genuine_subcommand_typo_keeps_claps_own_error() {
        // "reprot" names no file, so clap's "did you mean report?" is more
        // useful than a migration hint.
        let err = Cli::try_parse_from(["hydra", "reprot"]).unwrap_err();
        assert!(legacy_grammar_hint(&err).is_none());
    }

    #[test]
    fn a_global_flag_before_the_subcommand_still_dispatches() {
        // The pre-3.0 argv sniff only looked at position 1, so
        // `hydra --quiet report ...` failed with "unexpected argument".
        let cli = parse(&[
            "hydra",
            "--quiet",
            "report",
            "--model",
            "m.inp",
            "--results",
            "r.out",
        ]);
        assert!(cli.quiet);
        assert!(matches!(cli.command, Some(Command::Report(_))));
    }

    #[test]
    fn verbosity_counts_and_quiet_conflicts_with_it() {
        assert_eq!(parse(&["hydra", "run", "n.inp"]).verbose_level(), 0);
        assert_eq!(parse(&["hydra", "run", "n.inp", "-v"]).verbose_level(), 1);
        assert_eq!(parse(&["hydra", "run", "n.inp", "-vv"]).verbose_level(), 2);
        assert!(Cli::try_parse_from(["hydra", "run", "n.inp", "-v", "-q"]).is_err());
    }

    #[test]
    fn lower_v_is_verbosity_now_not_a_rejected_flag() {
        // Reclaimed at the 3.0 major boundary; -V remains version.
        let cli = parse(&["hydra", "run", "n.inp", "-v"]);
        assert!(!cli.version);
        assert_eq!(cli.verbose, 1);
        assert!(parse(&["hydra", "-V"]).version);
        assert!(parse(&["hydra", "--version"]).version);
    }

    #[test]
    fn engines_subcommand_parses() {
        assert!(matches!(
            parse(&["hydra", "engines"]).command,
            Some(Command::Engines)
        ));
    }

    #[test]
    fn unknown_flag_maps_to_exit_1_not_clap_default_2() {
        let err = Cli::try_parse_from(["hydra", "run", "n.inp", "--nope"]).unwrap_err();
        assert_eq!(clap_error_exit_code(&err), EXIT_INPUT);
    }

    #[test]
    fn help_display_maps_to_exit_0() {
        let err = Cli::try_parse_from(["hydra", "--help"]).unwrap_err();
        assert_eq!(clap_error_exit_code(&err), EXIT_OK);
    }

    /// The documented exit-code contract (module doc, cli.md, README):
    /// 0=ok, 1=usage/input, 2=solver, 3=I/O, 4=internal. Internal errors
    /// cannot be triggered cheaply end-to-end, so the mapping is pinned here.
    #[test]
    fn exit_code_contract_is_stable() {
        assert_eq!(EXIT_OK, 0);
        assert_eq!(EXIT_INPUT, 1);
        assert_eq!(EXIT_SOLVER, 2);
        assert_eq!(EXIT_IO, 3);
        assert_eq!(EXIT_INTERNAL, 4);
    }

    #[test]
    fn session_error_codes_never_use_internal_code() {
        assert_eq!(
            session_error_code(&SessionError::ValidationFailed(Vec::new())),
            EXIT_INPUT
        );
        assert_eq!(
            session_error_code(&SessionError::UnknownId("X".into())),
            EXIT_INPUT
        );
    }

    #[test]
    fn sim_clock_format_zero() {
        assert_eq!(format_sim_clock(0.0), "0:00:00");
    }

    #[test]
    fn sim_clock_format_whole_hours() {
        assert_eq!(format_sim_clock(2540.0 * 3600.0), "2540:00:00");
    }

    #[test]
    fn sim_clock_format_mixed_time() {
        assert_eq!(format_sim_clock(3661.0), "1:01:01");
    }

    #[test]
    fn render_progress_line_includes_percent_and_time_range() {
        let line = render_progress_line("Hydraulics", 1800.0, 7200.0, 0.0);
        assert!(line.contains("25%"), "missing percent: {line}");
        assert!(
            line.contains("0:30:00 / 2:00:00"),
            "missing sim clock: {line}"
        );
    }

    #[test]
    fn render_progress_line_zero_duration_reports_complete() {
        let line = render_progress_line("Hydraulics", 0.0, 0.0, 0.0);
        assert!(line.contains("100%"), "missing 100%%: {line}");
        assert!(
            line.contains("0:00:00 / 0:00:00"),
            "missing sim clock: {line}"
        );
    }

    #[test]
    fn usage_text_contains_usage() {
        let usage = usage_text();
        assert!(usage.contains("Usage:"), "{usage}");
        // Usage is now clap-derived from the subcommands rather than a hand
        // written override, so it cannot drift from the real grammar.
        assert!(usage.contains("hydra"), "{usage}");
    }

    // ── End-to-end simulation ────────────────────────────────────────────────

    /// Loads a real fixture INP file, runs the full hydraulic simulation,
    /// and verifies that every node produces a finite head value.
    ///
    /// This exercises the full path: INP parse → session load → run_hydraulics
    /// → get_node_result — without any output files.
    #[test]
    fn e2e_four_node_loop_runs_without_error() {
        use hydra::NodeQuantity;

        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let inp_path = workspace.join("tests/fixtures/wds/four_node_loop.inp");
        let bytes = match std::fs::read(&inp_path) {
            Ok(b) => b,
            Err(_) => return, // fixture absent in this environment — skip
        };
        let network = hydra::io::parse(&bytes).expect("parse four_node_loop.inp");
        let mut session = Simulation::from_network(network).expect("load network");
        session.run_hydraulics().expect("run_hydraulics");

        let times = session.snapshot_times();
        assert!(!times.is_empty(), "expected at least one snapshot");

        let t0 = times[0];
        for id in session.node_ids() {
            let head = session
                .get_node_result(id, NodeQuantity::Head, t0)
                .expect("get_node_result");
            assert!(
                head.is_finite(),
                "head for node {id} at t={t0} is not finite: {head}"
            );
        }
    }
}
