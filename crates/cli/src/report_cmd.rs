//! `hydra report` — generate a report *document* from completed simulation
//! results (txt / csv / html, template-driven).
//!
//! Distinct from the legacy report *file* of `hydra <input> <report>
//! <output>`: that is the fixed EPANET-convention run log emitted by every
//! simulation, a frozen compatibility surface. This subcommand produces
//! configurable deliverable documents from a persisted `.out` results file
//! and never runs a simulation itself.

use std::path::Path;

use clap::Parser;
use hydra::report::{assemble, render_csv, render_html, render_txt, ReportContext, ReportTemplate};

use crate::{EXIT_INPUT, EXIT_IO, EXIT_OK};

#[derive(Parser, Debug)]
#[command(
    name = "hydra report",
    about = "Generate a report document (txt/csv/html) from simulation results",
    disable_version_flag = true
)]
struct ReportCli {
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
}

/// Run the subcommand with the arguments following `hydra report`.
/// Returns the process exit code.
pub fn run<I: IntoIterator<Item = String>>(args: I) -> i32 {
    let cli =
        match ReportCli::try_parse_from(std::iter::once("hydra report".to_string()).chain(args)) {
            Ok(cli) => cli,
            Err(e) => {
                let _ = e.print();
                return match e.kind() {
                    clap::error::ErrorKind::DisplayHelp => EXIT_OK,
                    _ => EXIT_INPUT,
                };
            }
        };

    // ── Load the model (identifiers and declared units come from it) ──────
    let model_bytes = match std::fs::read(&cli.model) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("error: cannot read model {}: {e}", cli.model);
            return EXIT_INPUT;
        }
    };
    let network = match hydra::io::parse(&model_bytes) {
        Ok(network) => network,
        Err(e) => {
            eprintln!("error: cannot parse model {}: {e:?}", cli.model);
            return EXIT_INPUT;
        }
    };

    // ── Template: explicit file, or the everything-report default ─────────
    let template = match &cli.template {
        Some(path) => {
            let json = match std::fs::read_to_string(path) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!("error: cannot read template {path}: {e}");
                    return EXIT_INPUT;
                }
            };
            match ReportTemplate::from_json(&json) {
                Ok(template) => template,
                Err(e) => {
                    eprintln!("error: {e}");
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
    let document = assemble(&template, context, |id| {
        hydra::produce_report_block(id, results_path, &network)
    });

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
            _ => Format::Txt,
        }
    });
    let rendered = match format {
        Format::Txt => render_txt(&document),
        Format::Csv => render_csv(&document),
        Format::Html => render_html(&document),
    };

    match &cli.out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, rendered) {
                eprintln!("error: cannot write {path}: {e}");
                return EXIT_IO;
            }
        }
        None => print!("{rendered}"),
    }
    EXIT_OK
}
