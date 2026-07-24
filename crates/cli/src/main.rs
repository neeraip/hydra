// hydra-cli — thin I/O shell around hydra-engine.
//
// Acquires model file bytes (local path or HTTP), drives hydra-engine's session
// API, and writes output bytes. No parsing, unit conversion, or simulation
// logic lives here.
//
// Exit codes:
//   0 — simulation completed (warnings may appear in the report)
//   1 — usage/input error (bad arguments, bad INP, HTTP 4xx, missing input file)
//   2 — solver error (non-convergence or singularity)
//   3 — I/O error (permission denied, HTTP 5xx, network failure)
//   4 — internal error (unexpected engine state; please report a bug)

use std::io::{IsTerminal, Write};
use std::process;
use std::time::Instant;

use clap::{CommandFactory, Parser};
use hydra::io;
use hydra::io::out_writer::OutStreamWriter;
use hydra::io::rpt_writer as rpt;
use hydra::QualityMode;
use hydra::{SessionError, Simulation};

type CliOutWriter = OutStreamWriter<std::io::BufWriter<std::fs::File>>;

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

enum CliRunError {
    Session(SessionError),
    Io(std::io::Error),
}

/// Hydra — water distribution network simulator.
#[derive(Parser, Debug)]
#[command(
    name = "hydra",
    disable_version_flag = true,
    about,
    override_usage = "hydra [OPTIONS] <INPUT> [REPORT] [OUTPUT]\n       hydra [OPTIONS] --input <PATH>"
)]
struct Cli {
    /// Model file path, and optionally report and output file paths.
    /// Follows the EPANET convention: `hydra <input> <report> <output>`.
    /// The input may also be an http:// or https:// URL (redirects are
    /// followed, up to 10; bodies up to 1 GiB are accepted).
    #[arg(value_name = "INPUT [REPORT] [OUTPUT]")]
    positional: Vec<String>,

    /// Path (or http:// / https:// URL) of the model file
    /// (alternative to positional argument).
    #[arg(long = "input", value_name = "PATH")]
    input_named: Option<String>,

    /// Path for the report file (plain text by default; JSON if path ends in .json).
    /// If omitted, the report is written to stdout.
    #[arg(long, value_name = "PATH")]
    report: Option<String>,

    /// Path for the binary output file.
    /// If omitted, no binary output is created.
    #[arg(long, value_name = "PATH")]
    output: Option<String>,

    /// Suppress progress output. Progress is also suppressed automatically
    /// when stderr is not a terminal (e.g. when piping or redirecting).
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,

    /// Print Hydra and CLI version information and exit.
    #[arg(short = 'V', long = "version")]
    version: bool,

    /// Removed flag: `-v` used to mean `--version` but no longer does.
    /// Kept as a hidden flag so we can reject it with a targeted hint
    /// instead of clap's generic "unexpected argument" error.
    #[arg(short = 'v', hide = true)]
    v_removed: bool,
}

impl Cli {
    /// Resolve the input path from positional args or --input flag.
    fn input(&self) -> Option<&str> {
        self.input_named
            .as_deref()
            .or(self.positional.first().map(|s| s.as_str()))
    }

    /// Resolve the report path. Named flag takes precedence over positional.
    fn report(&self) -> Option<&str> {
        self.report
            .as_deref()
            .or(self.positional.get(1).map(|s| s.as_str()))
    }

    /// Resolve the output path. Named flag takes precedence over positional.
    fn output(&self) -> Option<&str> {
        self.output
            .as_deref()
            .or(self.positional.get(2).map(|s| s.as_str()))
    }
}

fn main() {
    // clap's default `parse()` exits with code 2 on usage errors, but this
    // CLI reserves 2 for solver errors. Map usage errors to exit 1 while
    // keeping clap's rendered output verbatim on the stream clap chose
    // (help/version go to stdout and exit 0; errors go to stderr).
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let code = clap_error_exit_code(&e);
            let _ = e.print();
            process::exit(code);
        }
    };
    if cli.v_removed {
        emit_usage_error(V_REMOVED_HINT);
        process::exit(EXIT_INPUT);
    }
    if cli.version {
        print_version_info();
        process::exit(EXIT_OK);
    }
    let exit_code = run(&cli);
    process::exit(exit_code);
}

/// Error message for the removed `-v` short flag. `-v` used to mean
/// `--version`; the version flag is now `-V`, matching GNU/clap convention.
/// `-v` is rejected (rather than silently repurposed) so scripts that relied
/// on the old meaning fail loudly.
const V_REMOVED_HINT: &str =
    "unexpected argument '-v': did you mean '-V' (--version) or '-q' (--quiet)?";

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

/// Drives the full simulation lifecycle.
///
/// Session lifecycle:
/// ```text
/// sim = create()
/// load(sim, model_bytes)        // exit 1 on validation failure
/// begin_out_stream(sim, ...)    // write prolog + energy placeholder (if --output)
/// step_hydraulics() until done  // exit 2 on solver error
/// append_out_periods()          // after each successful hydraulic step
/// step_quality() until done     // exit 2 on solver error; no-op if quality=None
/// append_out_periods()          // after each successful quality step
/// finish_out_stream(sim)        // patch n_periods + epilog (if --output)
/// write_report(sim)             // plain text or JSON
/// ```
///
/// Returns an exit code (0=ok, 1=input error, 2=solver error, 3=I/O error,
/// 4=internal error).
fn run(cli: &Cli) -> i32 {
    // ── Validate positional arg count ──────────────────────────────────────────
    if cli.positional.len() > 3 {
        emit_usage_error(&format!(
            "expected at most 3 positional arguments, got {}",
            cli.positional.len()
        ));
        return EXIT_INPUT;
    }

    // ── Resolve input path ────────────────────────────────────────────────────
    let input_path = match cli.input() {
        Some(p) => p,
        None => {
            emit_usage_error("no input file specified");
            return EXIT_INPUT;
        }
    };

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

    let network = match io::parse(&bytes) {
        Ok(n) => n,
        Err(io::ParseError::ValidationFailed(errs)) => {
            for e in &errs {
                emit_error("validation/network", &e.to_string(), None, None);
            }
            return EXIT_INPUT;
        }
        Err(io::ParseError::UnrecognisedFormat) => {
            emit_error("input/format", "unrecognised file format", None, None);
            return EXIT_INPUT;
        }
        Err(e) => {
            emit_error("input/parse", &e.to_string(), None, None);
            return EXIT_INPUT;
        }
    };

    let duration = network.options.duration;
    let quality_enabled = network.options.quality_mode != QualityMode::None;

    // ── Create session and load network ───────────────────────────────────────
    let mut session = Simulation::create();
    if let Err(e) = session.load(network) {
        emit_session_error(&e);
        return session_error_code(&e);
    }

    let mut progress = ProgressReporter::new(std::io::stderr().is_terminal() && !cli.quiet);
    progress.startup_banner();

    let output_units = match session.flow_units() {
        Some(u) => u,
        None => {
            emit_error("internal", "flow units unavailable after load", None, None);
            return EXIT_INTERNAL;
        }
    };

    let mut out_stream = if let Some(out_path) = cli.output() {
        let report_path = cli.report().unwrap_or("");
        let stream_result = (|| -> anyhow::Result<CliOutWriter> {
            let f = std::io::BufWriter::new(std::fs::File::create(out_path)?);
            let mut stream =
                OutStreamWriter::begin(f, &session, input_path, report_path, output_units)?;
            stream.append_available(&session)?;
            Ok(stream)
        })();

        match stream_result {
            Ok(stream) => Some(stream),
            Err(e) => {
                emit_error("io/output", &e.to_string(), None, None);
                return EXIT_IO;
            }
        }
    } else {
        None
    };

    // ── Run hydraulics ────────────────────────────────────────────────────────
    if let Err(e) =
        run_hydraulics_with_progress(&mut session, &mut progress, duration, &mut out_stream)
    {
        progress.finish_line();
        match e {
            CliRunError::Session(session_error) => {
                emit_session_error(&session_error);
                return session_error_code(&session_error);
            }
            CliRunError::Io(io_error) => {
                emit_error("io/output", &io_error.to_string(), None, None);
                return EXIT_IO;
            }
        }
    }
    progress.finish_phase(duration);

    // Emit hydraulic warnings to stderr.
    emit_warnings(&session, 0);

    // ── Run quality ───────────────────────────────────────────────────────────
    let n_warnings_before_quality = session.warnings().len();
    if let Err(e) = run_quality_with_progress(
        &mut session,
        &mut progress,
        duration,
        quality_enabled,
        &mut out_stream,
    ) {
        progress.finish_line();
        match e {
            CliRunError::Session(session_error) => {
                emit_session_error(&session_error);
                return session_error_code(&session_error);
            }
            CliRunError::Io(io_error) => {
                emit_error("io/output", &io_error.to_string(), None, None);
                return EXIT_IO;
            }
        }
    }
    progress.finish_phase(duration);

    // Emit any new warnings generated during the quality phase.
    emit_warnings(&session, n_warnings_before_quality);

    // ── Finalize binary output stream (§4.1) ─────────────────────────────────
    if let Some(out_writer) = out_stream.take() {
        if let Err(e) = out_writer.finish(&session) {
            emit_error("io/output", &e.to_string(), None, None);
            return EXIT_IO;
        }
    }

    // ── Write report ──────────────────────────────────────────────────────────
    // When the report goes to stdout and progress was printed on stderr,
    // add a blank separator line so the two don't visually run together.
    if cli.report().is_none() && progress.enabled {
        let _ = writeln!(std::io::stderr());
    }
    if let Err(e) = write_report(&session, cli.report()) {
        emit_error("io/report", &e.to_string(), None, None);
        return EXIT_IO;
    }

    EXIT_OK
}

fn run_hydraulics_with_progress(
    session: &mut Simulation,
    progress: &mut ProgressReporter,
    duration: f64,
    out_stream: &mut Option<CliOutWriter>,
) -> Result<(), CliRunError> {
    let mut simulated_t = 0.0;
    loop {
        progress.update("Hydraulics", simulated_t, duration);
        let dt = session.step_hydraulics().map_err(CliRunError::Session)?;
        if let Some(writer) = out_stream.as_mut() {
            writer.append_available(session).map_err(CliRunError::Io)?;
        }
        if dt == 0.0 {
            break;
        }
        simulated_t += dt;
    }
    Ok(())
}

fn run_quality_with_progress(
    session: &mut Simulation,
    progress: &mut ProgressReporter,
    duration: f64,
    quality_enabled: bool,
    out_stream: &mut Option<CliOutWriter>,
) -> Result<(), CliRunError> {
    if !quality_enabled {
        session.run_quality().map_err(CliRunError::Session)?;
        if let Some(writer) = out_stream.as_mut() {
            writer.append_available(session).map_err(CliRunError::Io)?;
        }
        return Ok(());
    }

    let mut simulated_t = 0.0;
    loop {
        progress.update("Water quality", simulated_t, duration);
        let dt = session.step_quality().map_err(CliRunError::Session)?;
        if let Some(writer) = out_stream.as_mut() {
            writer.append_available(session).map_err(CliRunError::Io)?;
        }
        if dt == 0.0 {
            break;
        }
        simulated_t += dt;
    }
    Ok(())
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
fn write_report(session: &Simulation, path: Option<&str>) -> anyhow::Result<()> {
    match path {
        None => {
            let text = rpt::build_text_report(session)?;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(text.as_bytes())?;
            Ok(())
        }
        Some(p) if p.ends_with(".json") => {
            let json = rpt::build_json_report(session)?;
            std::fs::write(p, json)?;
            Ok(())
        }
        Some(p) => {
            let text = rpt::build_text_report(session)?;
            std::fs::write(p, text)?;
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
fn emit_error(code: &str, message: &str, object_id: Option<&str>, time_step: Option<f64>) {
    let line = serde_json::json!({
        "level": "error",
        "code": code,
        "message": message,
        "object_id": object_id,
        "time_step": time_step,
    });
    eprintln!("{line}");
}

/// Emit session warnings `[from..]` as JSON-line diagnostics on stderr.
///
/// `from` lets the quality phase emit only the warnings it added, without
/// repeating the hydraulic-phase warnings already printed.
fn emit_warnings(session: &Simulation, from: usize) {
    let stderr = std::io::stderr();
    let mut buf = std::io::BufWriter::new(stderr.lock());
    for w in &session.warnings()[from..] {
        let (code, msg, oid) = rpt::describe_warning(w, session);
        let line = serde_json::json!({
            "level": "warning",
            "code": code,
            "message": msg,
            "object_id": oid,
            "time_step": w.t,
        });
        let _ = writeln!(buf, "{line}");
    }
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

    // ── Positional arguments ──────────────────────────────────────────────

    #[test]
    fn positional_input_only() {
        let cli = parse(&["hydra", "net1.inp"]);
        assert_eq!(cli.input(), Some("net1.inp"));
        assert_eq!(cli.report(), None);
        assert_eq!(cli.output(), None);
    }

    #[test]
    fn positional_input_and_report() {
        let cli = parse(&["hydra", "net1.inp", "net1.rpt"]);
        assert_eq!(cli.input(), Some("net1.inp"));
        assert_eq!(cli.report(), Some("net1.rpt"));
        assert_eq!(cli.output(), None);
    }

    #[test]
    fn positional_input_report_output() {
        let cli = parse(&["hydra", "net1.inp", "net1.rpt", "net1.out"]);
        assert_eq!(cli.input(), Some("net1.inp"));
        assert_eq!(cli.report(), Some("net1.rpt"));
        assert_eq!(cli.output(), Some("net1.out"));
    }

    // ── Named flags ──────────────────────────────────────────────────────

    #[test]
    fn named_input_only() {
        let cli = parse(&["hydra", "--input", "net1.inp"]);
        assert_eq!(cli.input(), Some("net1.inp"));
        assert_eq!(cli.report(), None);
        assert_eq!(cli.output(), None);
    }

    #[test]
    fn named_all_flags() {
        let cli = parse(&[
            "hydra", "--input", "net1.inp", "--report", "r.json", "--output", "o.bin",
        ]);
        assert_eq!(cli.input(), Some("net1.inp"));
        assert_eq!(cli.report(), Some("r.json"));
        assert_eq!(cli.output(), Some("o.bin"));
    }

    // ── Named flags override positionals ─────────────────────────────────

    #[test]
    fn named_input_overrides_positional() {
        let cli = parse(&["hydra", "pos.inp", "--input", "named.inp"]);
        assert_eq!(cli.input(), Some("named.inp"));
    }

    #[test]
    fn named_report_overrides_positional() {
        let cli = parse(&["hydra", "net1.inp", "pos.rpt", "--report", "named.rpt"]);
        assert_eq!(cli.report(), Some("named.rpt"));
    }

    #[test]
    fn named_output_overrides_positional() {
        let cli = parse(&[
            "hydra",
            "net1.inp",
            "net1.rpt",
            "pos.out",
            "--output",
            "named.out",
        ]);
        assert_eq!(cli.output(), Some("named.out"));
    }

    // ── Missing input ────────────────────────────────────────────────────

    #[test]
    fn no_args_yields_no_input() {
        let cli = parse(&["hydra"]);
        assert_eq!(cli.input(), None);
    }

    // ── Too many positional args ─────────────────────────────────────────

    #[test]
    fn four_positional_args_rejected() {
        // clap will still parse them; run() rejects at runtime
        let cli = parse(&["hydra", "a", "b", "c", "d"]);
        assert_eq!(cli.positional.len(), 4);
        // run() would return exit code 1 for this case
    }

    // ── Usage-error exit codes ───────────────────────────────────────────

    #[test]
    fn unknown_flag_maps_to_exit_1_not_clap_default_2() {
        let err = Cli::try_parse_from(["hydra", "--no-such-flag"])
            .expect_err("unknown flag should fail to parse");
        assert_eq!(clap_error_exit_code(&err), 1);
    }

    #[test]
    fn help_display_maps_to_exit_0() {
        let err = Cli::try_parse_from(["hydra", "--help"]).expect_err("--help renders as Err");
        assert_eq!(clap_error_exit_code(&err), 0);
    }

    // ── Version flag (-V) and removed -v ─────────────────────────────────

    #[test]
    fn short_upper_v_sets_version() {
        let cli = parse(&["hydra", "-V"]);
        assert!(cli.version);
        assert!(!cli.v_removed);
    }

    #[test]
    fn long_version_sets_version() {
        let cli = parse(&["hydra", "--version"]);
        assert!(cli.version);
    }

    #[test]
    fn short_lower_v_parses_as_removed_flag() {
        // -v must still parse (so main() can reject it with a targeted hint
        // instead of clap's generic unknown-argument error), but it must not
        // mean --version any more.
        let cli = parse(&["hydra", "-v"]);
        assert!(cli.v_removed);
        assert!(!cli.version);
    }

    #[test]
    fn v_removed_hint_mentions_both_replacements() {
        assert!(V_REMOVED_HINT.contains("-V"), "hint: {V_REMOVED_HINT}");
        assert!(V_REMOVED_HINT.contains("--quiet"), "hint: {V_REMOVED_HINT}");
    }

    // ── Exit-code contract ───────────────────────────────────────────────

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
    fn usage_text_contains_usage_and_input_forms() {
        let usage = usage_text();
        assert!(usage.contains("Usage:"));
        assert!(usage.contains("hydra [OPTIONS] <INPUT> [REPORT] [OUTPUT]"));
        assert!(usage.contains("hydra [OPTIONS] --input <PATH>"));
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
        let inp_path = workspace.join("tests/fixtures/four_node_loop.inp");
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
