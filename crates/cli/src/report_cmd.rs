//! `hydra report` — generate a report *document* from completed simulation
//! results (txt / csv / html, template-driven).
//!
//! Distinct from the legacy report *file* of `hydra <input> <report>
//! <output>`: that is the fixed EPANET-convention run log emitted by every
//! simulation, a frozen compatibility surface. This subcommand produces
//! configurable deliverable documents from a persisted `.out` results file
//! and never runs a simulation itself.

use std::path::Path;

use hydra::report::{assemble, render_csv, render_html, render_txt, ReportContext, ReportTemplate};

use crate::{EXIT_INPUT, EXIT_INTERNAL, EXIT_IO, EXIT_OK};

#[derive(clap::Args, Debug)]
pub struct ReportArgs {
    /// Model INP file the results were produced from.
    #[arg(long, value_name = "PATH")]
    model: String,

    /// Binary results (.out) file from a completed run.
    #[arg(long, value_name = "PATH")]
    results: String,

    /// Report template JSON (which blocks, in what order). When omitted,
    /// the report covers every available block.
    #[arg(long, value_name = "PATH")]
    template: Option<String>,

    /// Output format. Inferred from the --out extension when omitted;
    /// defaults to txt.
    #[arg(long, value_enum)]
    format: Option<Format>,

    /// Output path. When omitted, the report is written to stdout.
    #[arg(long, short = 'o', value_name = "PATH")]
    out: Option<String>,

    /// Warnings recorded by the run, for the warnings block. Defaults to
    /// the file written beside --results when the run finished.
    #[arg(long, value_name = "PATH")]
    warnings: Option<String>,

    /// Omit the generation timestamp so output is byte-reproducible.
    #[arg(long)]
    no_timestamp: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Format {
    Txt,
    Csv,
    Html,
    Pdf,
}

/// The run's diagnostics for the warnings block, or `None` when they were
/// never recorded.
///
/// The distinction is the point (hydra-common spec §3.4.1). An absent
/// sidecar means this report cannot know what the run complained about, and
/// the block says so; an empty one means the run was watched and raised
/// nothing, which is a different and better thing to be able to print. A
/// sidecar that exists but cannot be read is neither, and fails rather than
/// quietly becoming the first.
fn load_diagnostics(
    explicit: Option<&str>,
    results: &str,
) -> Result<Option<Vec<hydra::common::RunDiagnostic>>, String> {
    let path = match explicit {
        Some(p) => std::path::PathBuf::from(p),
        None => hydra::engines::warnings_path(Path::new(results)),
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        // Only the default location may be missing. An explicit path that is
        // not there is a mistake worth naming, not a run without warnings.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && explicit.is_none() => {
            return Ok(None)
        }
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    hydra::engines::read_warnings(&bytes)
        .map(Some)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Run the subcommand with the arguments following `hydra report`.
/// Returns the process exit code.
pub fn run(cli: &ReportArgs, verbosity: &u8) -> i32 {
    let _ = verbosity;

    // ── Load the model (identifiers and declared units come from it) ──────
    let model_bytes = match std::fs::read(&cli.model) {
        Ok(bytes) => bytes,
        Err(e) => {
            crate::emit_error("io/fetch", &format!("cannot read model: {e}"), None, None);
            return EXIT_INPUT;
        }
    };
    let network = match hydra::io::parse(&model_bytes) {
        Ok(network) => network,
        // Each failure gets the code the simulation path would give it, so a
        // caller can treat both commands' stderr identically. Validation
        // errors are listed individually: ParseError's Display reports only a
        // count, which names nothing the user can act on.
        Err(hydra::io::ParseError::NotSimulable(errors)) => {
            for e in &errors {
                crate::emit_error("validation/network", &e.to_string(), None, None);
            }
            return EXIT_INPUT;
        }
        Err(hydra::io::ParseError::Read(hydra::io::ReadError::ForeignDialect {
            tool,
            section,
        })) => {
            crate::emit_error(
                "input/engine",
                &format!(
                    "this is a {tool} model, not an EPANET one (it declares a [{section}] section)"
                ),
                None,
                None,
            );
            return EXIT_INPUT;
        }
        Err(hydra::io::ParseError::Read(hydra::io::ReadError::UnrecognisedFormat)) => {
            crate::emit_error("input/format", "unrecognised file format", None, None);
            return EXIT_INPUT;
        }
        Err(e) => {
            crate::emit_error("input/parse", &e.to_string(), None, None);
            return EXIT_INPUT;
        }
    };

    // ── Check the results before producing anything from them ─────────────
    //
    // Every block reads this one file, so an unreadable one fails all of them
    // the same way: a document consisting of thirteen copies of a single
    // error, written out with a success exit code. `hydra report && publish`
    // would ship that as a report. Validating here turns it into one stated
    // error and a non-zero exit, which is what the model path above and every
    // GUI entry point already do.
    let results_path = Path::new(&cli.results);
    if let Err(e) = hydra::io::out_reader::read_metadata_checked(results_path) {
        crate::emit_error("input/results", &e.to_string(), None, None);
        return EXIT_INPUT;
    }

    // ── Template: explicit file, or the everything-report default ─────────
    let template = match &cli.template {
        Some(path) => {
            let json = match std::fs::read_to_string(path) {
                Ok(json) => json,
                Err(e) => {
                    crate::emit_error(
                        "io/fetch",
                        &format!("cannot read template {path}: {e}"),
                        None,
                        None,
                    );
                    return EXIT_INPUT;
                }
            };
            match ReportTemplate::from_json(&json) {
                Ok(template) => template,
                Err(e) => {
                    crate::emit_error("input/parse", &e.to_string(), None, None);
                    return EXIT_INPUT;
                }
            }
        }
        None => ReportTemplate::covering("Simulation Report", hydra::report_catalog()),
    };

    let context = ReportContext {
        generated_at: (!cli.no_timestamp)
            .then(|| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        source: vec![
            ("Model".into(), cli.model.clone()),
            ("Results".into(), cli.results.clone()),
        ],
    };

    let diagnostics = match load_diagnostics(cli.warnings.as_deref(), &cli.results) {
        Ok(diagnostics) => diagnostics,
        Err(message) => {
            crate::emit_error("input/warnings", &message, None, None);
            return EXIT_INPUT;
        }
    };

    let document = assemble(
        &template,
        hydra::report_catalog(),
        context,
        |id, options| {
            let src = hydra::io::out_reader::OutFileSource::open(results_path).map_err(|e| {
                hydra::common::BlockError::Failed {
                    message: e.to_string(),
                }
            })?;
            hydra::produce_report_block(id, &src, &network, options, diagnostics.as_deref())
        },
    );
    // Quantity-tagged values arrive in SI display units (hydra-common
    // §3.3); the CLI has no reader preference to honour, so it resolves to
    // the model's own family — which reproduces the file's values exactly.
    let family = if hydra::io::units::is_si(network.options.flow_units) {
        hydra::common::DisplayFamily::Si
    } else {
        hydra::common::DisplayFamily::Us
    };
    let document = hydra::report::resolve_display(
        &document,
        &hydra::report::DisplaySettings {
            family,
            catalog: hydra::descriptors::QUANTITIES,
        },
    );

    let format = cli.format.unwrap_or_else(|| {
        match cli
            .out
            .as_deref()
            .and_then(|p| Path::new(p).extension())
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("csv") => Format::Csv,
            Some("html") | Some("htm") => Format::Html,
            Some("pdf") => Format::Pdf,
            _ => Format::Txt,
        }
    });

    enum Rendered {
        Text(String),
        Binary(Vec<u8>),
    }
    let rendered = match format {
        Format::Txt => Rendered::Text(render_txt(&document)),
        Format::Csv => Rendered::Text(render_csv(&document)),
        Format::Html => Rendered::Text(render_html(&document)),
        Format::Pdf => match hydra::report::render_pdf(&document) {
            Ok(bytes) => Rendered::Binary(bytes),
            Err(e) => {
                eprintln!("error: {e}");
                return EXIT_INTERNAL;
            }
        },
    };

    match (&cli.out, rendered) {
        (Some(path), Rendered::Text(text)) => {
            if let Err(e) = std::fs::write(path, text) {
                eprintln!("error: cannot write {path}: {e}");
                return EXIT_IO;
            }
        }
        (Some(path), Rendered::Binary(bytes)) => {
            if let Err(e) = std::fs::write(path, bytes) {
                eprintln!("error: cannot write {path}: {e}");
                return EXIT_IO;
            }
        }
        (None, Rendered::Text(text)) => print!("{text}"),
        (None, Rendered::Binary(_)) => {
            eprintln!("error: pdf output requires --out <PATH>");
            return EXIT_INPUT;
        }
    }
    EXIT_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three answers `load_diagnostics` has to keep apart. Collapsing
    /// the first two would let a report state that a run raised nothing when
    /// it only failed to find the file (hydra-common spec §3.4.1).
    #[test]
    fn absent_recorded_and_empty_are_three_different_answers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let results = dir.path().join("run.out");
        let results = results.to_str().expect("utf-8 path");

        assert_eq!(None, load_diagnostics(None, results).expect("absent"));

        std::fs::write(hydra::engines::warnings_path(Path::new(results)), b"[]").expect("write");
        assert_eq!(
            Some(vec![]),
            load_diagnostics(None, results).expect("recorded and empty")
        );

        std::fs::write(
            hydra::engines::warnings_path(Path::new(results)),
            br#"[{"code":"warning/pump_xhead","message":"Pump P1 exceeds its maximum head.","elementId":"P1","time":3600.0}]"#,
        )
        .expect("write");
        let read = load_diagnostics(None, results)
            .expect("recorded")
            .expect("some");
        assert_eq!(1, read.len());
        assert_eq!(Some(3600.0), read[0].time);
    }

    /// A default location that is empty means "never recorded"; a path the
    /// caller typed and got wrong is a mistake worth naming, not a run
    /// without warnings.
    #[test]
    fn a_named_warnings_file_that_is_missing_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let named = dir.path().join("nowhere.json");
        let err = load_diagnostics(named.to_str(), "unused.out")
            .expect_err("a named file that is not there must fail");
        assert!(err.contains("nowhere.json"), "{err}");
    }

    #[test]
    fn a_warnings_file_that_cannot_be_parsed_fails_rather_than_reading_as_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("broken.json");
        std::fs::write(&path, b"{not json").expect("write");
        assert!(load_diagnostics(path.to_str(), "unused.out").is_err());
    }
}
