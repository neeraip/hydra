//! The urban drainage (`uds`) run path.
//!
//! The engine performs no file I/O: model text is handed to it in memory,
//! and every auxiliary file a model declares — daily climate records,
//! hotstart state, routing interface files — is read or written here.
//! Auxiliary paths resolve relative to the model file's directory, per the
//! predecessor's convention, which is why they need a local model path.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use hydra::uds::io::climate::parse_climate_file;
use hydra::uds::io::objects::parse_network;
use hydra::uds::model::TemperatureSource;
use hydra::uds::simulation::engine::{OpenError, Simulation};

use crate::{emit_error, Cli, ProgressReporter, RunArgs, EXIT_INPUT, EXIT_IO, EXIT_OK};

/// Drive an urban drainage run: open, step to completion, write results.
///
/// The uds session advances hydrology, routing, and water quality together,
/// so progress is a single phase. `--results` writes the engine's
/// predecessor-compatible binary `.out`; `--summary` its text report.
pub(crate) fn run(args: &RunArgs, cli: &Cli, bytes: &[u8]) -> i32 {
    // The wds engine offers a JSON report; this one does not yet. Refuse
    // up front rather than writing a text report under a .json name.
    if args
        .summary
        .as_deref()
        .is_some_and(|p| p.ends_with(".json"))
    {
        emit_error(
            "input/report",
            "JSON summaries are not available for the uds engine yet — use a .rpt path",
            None,
            None,
        );
        return EXIT_INPUT;
    }

    let text = String::from_utf8_lossy(bytes).into_owned();

    // Survey the model's auxiliary-file declarations before opening. Parse
    // problems are deliberately ignored here — the open below re-parses and
    // reports them properly.
    let (net, _) = parse_network(&text);

    let climate_records = match &net.climate.temperature {
        Some(TemperatureSource::File { name, .. }) => {
            let path = match resolve_aux_path(&args.model, name) {
                Ok(p) => p,
                Err(code) => return code,
            };
            let climate_text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(e) => {
                    emit_error(
                        "io/climate",
                        &format!("climate file {}: {e}", path.display()),
                        None,
                        None,
                    );
                    return EXIT_IO;
                }
            };
            match parse_climate_file(&climate_text) {
                Ok(records) => records,
                Err(e) => {
                    emit_error(
                        "input/climate",
                        &format!("climate file {}: {e}", path.display()),
                        None,
                        None,
                    );
                    return EXIT_INPUT;
                }
            }
        }
        _ => Vec::new(),
    };

    // ── Open: parse, validate, build ──────────────────────────────────────────
    let (mut sim, diags, findings) = match Simulation::open_with_climate(&text, climate_records) {
        Ok(session) => session,
        Err(OpenError::Parse(diags)) => {
            for d in diags.iter().filter(|d| d.kind.is_error()) {
                emit_error("input/parse", &d.to_string(), None, None);
            }
            return EXIT_INPUT;
        }
        Err(OpenError::Validation(findings)) => {
            for v in findings.iter().filter(|v| v.kind.is_error()) {
                emit_error("validation/network", &v.to_string(), None, None);
            }
            return EXIT_INPUT;
        }
        Err(OpenError::Routing(r)) => {
            emit_error("input/unsupported", &r.to_string(), None, None);
            return EXIT_INPUT;
        }
        Err(OpenError::Surface(s)) => {
            emit_error("input/unsupported", &s.to_string(), None, None);
            return EXIT_INPUT;
        }
        Err(OpenError::Controls(msg)) | Err(OpenError::Transport(msg)) => {
            emit_error("input/unsupported", &msg, None, None);
            return EXIT_INPUT;
        }
    };

    // Warning-class import and validation findings, before the run so they
    // are visible even if it is long.
    for d in diags.iter().filter(|d| !d.kind.is_error()) {
        emit_warning("input/notice", &d.to_string(), None);
    }
    for v in findings.iter().filter(|v| !v.kind.is_error()) {
        emit_warning("validation/mutation", &v.to_string(), Some(&v.element));
    }

    // ── Auxiliary inputs the engine cannot read itself ────────────────────────
    let iface = &net.interface_files;
    if iface.rainfall.is_some() || iface.runoff.is_some() || iface.rdii.is_some() {
        emit_warning(
            "input/unsupported",
            "rainfall, runoff, and RDII interface files are not supported yet — ignored",
            None,
        );
    }
    if let Some(name) = &iface.hotstart_use {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let hot = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                emit_error(
                    "io/hotstart",
                    &format!("hotstart file {}: {e}", path.display()),
                    None,
                    None,
                );
                return EXIT_IO;
            }
        };
        if let Err(e) = sim.load_hotstart(&hot) {
            emit_error(
                "input/hotstart",
                &format!("hotstart file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_INPUT;
        }
    }
    if let Some(name) = &iface.inflows {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let inflow_text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                emit_error(
                    "io/interface",
                    &format!("routing inflows file {}: {e}", path.display()),
                    None,
                    None,
                );
                return EXIT_IO;
            }
        };
        if let Err(e) = sim.supply_routing_inflows(&inflow_text) {
            emit_error(
                "input/interface",
                &format!("routing inflows file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_INPUT;
        }
    }

    // ── Run ───────────────────────────────────────────────────────────────────
    let mut progress = ProgressReporter::new(std::io::stderr().is_terminal() && !cli.quiet);
    progress.startup_banner();
    let duration = sim.duration();
    loop {
        progress.update("Simulation", sim.time(), duration);
        if !sim.step() {
            break;
        }
    }
    progress.finish_phase(duration);

    for n in &sim.notices {
        emit_warning("runtime/notice", &n.message, None);
    }

    // ── Outputs ───────────────────────────────────────────────────────────────
    if let Some(name) = &iface.hotstart_save {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        if let Err(e) = create_and_write(&path, |w| sim.save_hotstart(w)) {
            emit_error(
                "io/hotstart",
                &format!("hotstart file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_IO;
        }
    }
    if let Some(name) = &iface.outflows {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        if let Err(e) = create_and_write(&path, |w| sim.write_routing_outflows(w)) {
            emit_error(
                "io/interface",
                &format!("routing outflows file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_IO;
        }
    }
    if let Some(out_path) = args.results.as_deref() {
        if let Err(e) = create_and_write(Path::new(out_path), |w| sim.write_out(w)) {
            emit_error("io/output", &format!("{out_path}: {e}"), None, None);
            return EXIT_IO;
        }
    }

    // When the report goes to stdout and progress was printed on stderr,
    // add a blank separator line so the two don't visually run together.
    if args.summary.is_none() && progress.enabled {
        let _ = writeln!(std::io::stderr());
    }
    let report_result = match args.summary.as_deref() {
        None => {
            let mut stdout = std::io::stdout().lock();
            sim.write_report(&mut stdout)
        }
        Some(p) => create_and_write(Path::new(p), |w| sim.write_report(w)),
    };
    if let Err(e) = report_result {
        emit_error("io/report", &e.to_string(), None, None);
        return EXIT_IO;
    }

    EXIT_OK
}

/// Resolve an auxiliary file named by the model, relative to the model
/// file's directory. Needs a local model path: a model fetched over HTTP
/// has no directory to resolve against.
fn resolve_aux_path(model: &str, name: &str) -> Result<PathBuf, i32> {
    if model.starts_with("http://") || model.starts_with("https://") {
        emit_error(
            "input/aux-file",
            &format!(
                "the model declares an auxiliary file ({name:?}), which cannot be \
                 resolved for a model fetched over HTTP — run from a local path"
            ),
            None,
            None,
        );
        return Err(EXIT_INPUT);
    }
    let name_path = Path::new(name);
    if name_path.is_absolute() {
        return Ok(name_path.to_path_buf());
    }
    let base = Path::new(model).parent().unwrap_or(Path::new("."));
    Ok(base.join(name_path))
}

/// Create `path` and stream `f` into it through a buffered writer.
fn create_and_write(
    path: &Path,
    f: impl FnOnce(&mut std::io::BufWriter<std::fs::File>) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    f(&mut w)?;
    w.flush()
}

/// Write a structured JSON-line warning to stderr, mirroring the wds
/// engine's warning stream.
fn emit_warning(code: &str, message: &str, object_id: Option<&str>) {
    let line = serde_json::json!({
        "level": "warning",
        "code": code,
        "message": message,
        "object_id": object_id,
        "time_step": null,
    });
    eprintln!("{line}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aux_paths_resolve_beside_the_model() {
        let p = resolve_aux_path("models/site/net.inp", "climate.dat").unwrap();
        assert_eq!(p, Path::new("models/site/climate.dat"));

        let p = resolve_aux_path("net.inp", "climate.dat").unwrap();
        assert_eq!(p, Path::new("climate.dat"));

        let abs = if cfg!(windows) {
            "C:\\c.dat"
        } else {
            "/tmp/c.dat"
        };
        let p = resolve_aux_path("models/net.inp", abs).unwrap();
        assert_eq!(p, Path::new(abs));
    }

    #[test]
    fn aux_files_refuse_http_models() {
        let err = resolve_aux_path("https://example.com/net.inp", "climate.dat");
        assert_eq!(err.unwrap_err(), EXIT_INPUT);
    }
}
