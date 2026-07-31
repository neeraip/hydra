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

    let results_path = Path::new(&cli.results);
    let document = assemble(
        &template,
        hydra::report_catalog(),
        context,
        |id, options| hydra::produce_report_block(id, results_path, &network, options),
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
