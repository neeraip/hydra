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

use crate::{
    emit_error, Cli, ProgressReporter, RunArgs, EXIT_INPUT, EXIT_INTERNAL, EXIT_IO, EXIT_OK,
};

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
            "JSON summaries are not available for the uds engine yet. Use a .rpt path",
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

    // External rain records (§14.12): one read per distinct file a gage
    // names, resolved beside the model like every auxiliary file.
    let mut rain_files: Vec<(String, Vec<hydra::uds::io::rain::RainReading>)> = Vec::new();
    for gage in &net.gages {
        let hydra::uds::model::GageSource::File { file, .. } = &gage.source else {
            continue;
        };
        if rain_files.iter().any(|(name, _)| name == file) {
            continue;
        }
        let path = match resolve_aux_path(&args.model, file) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let rain_text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                emit_error(
                    "io/rain",
                    &format!("rain record {}: {e}", path.display()),
                    None,
                    None,
                );
                return EXIT_IO;
            }
        };
        match hydra::uds::io::rain::parse_rain_file(&rain_text) {
            Ok(readings) => rain_files.push((file.clone(), readings)),
            Err(e) => {
                emit_error(
                    "input/rain",
                    &format!("rain record {}: {e}", path.display()),
                    None,
                    None,
                );
                return EXIT_INPUT;
            }
        }
    }

    // ── Open: parse, validate, build ──────────────────────────────────────────
    let (mut sim, diags, findings) =
        match Simulation::open_with_files(&text, climate_records, rain_files) {
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
    // Deferred interface files are the engine's own finding now (§14.8): a
    // USE refuses the open, a SAVE opens with a per-role notice. This used to
    // be restated here, and differently again in the demo, which left every
    // other consumer of the engine with silence.
    let iface = &net.interface_files;
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

    // §14.8.2: a runoff interface file replaces the surface entirely, so a
    // model declaring one and not receiving it would recompute the very
    // hydrology the file exists to skip.
    if let Some((hydra::uds::model::FileMode::Use, name)) = &iface.runoff {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                emit_error(
                    "io/interface",
                    &format!("runoff interface file {}: {e}", path.display()),
                    None,
                    None,
                );
                return EXIT_IO;
            }
        };
        if let Err(e) = sim.supply_runoff(&bytes) {
            emit_error(
                "input/interface",
                &format!("runoff interface file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_INPUT;
        }
    }

    // §14.8.1: an RDII interface file replaces the convolution, so a model
    // declaring one and not receiving it would compute a hydrograph the
    // modeller asked to reuse instead. Either encoding, so bytes.
    if let Some((hydra::uds::model::FileMode::Use, name)) = &iface.rdii {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                emit_error(
                    "io/interface",
                    &format!("RDII interface file {}: {e}", path.display()),
                    None,
                    None,
                );
                return EXIT_IO;
            }
        };
        if let Err(e) = sim.supply_rdii(&bytes) {
            emit_error(
                "input/interface",
                &format!("RDII interface file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_INPUT;
        }
    }

    // ── Run ───────────────────────────────────────────────────────────────────
    // The drive loop, results persistence, warning emission, and summary
    // writing are the shared per-engine dispatch in hydra::engines — only
    // the auxiliary-file handling around them is uds-specific CLI work.
    let mut es = hydra::engines::EngineSession::from_uds(sim);

    if let Some(out_path) = args.results.as_deref() {
        let attach = std::fs::File::create(out_path)
            .and_then(|f| es.begin_results(Box::new(std::io::BufWriter::new(f)), &args.model, ""));
        if let Err(e) = attach {
            emit_error("io/output", &format!("{out_path}: {e}"), None, None);
            return EXIT_IO;
        }
    }

    let mut progress = ProgressReporter::new(std::io::stderr().is_terminal() && !cli.quiet);
    progress.startup_banner();
    if let Err(code) = crate::drive_with_progress(&mut es, &mut progress) {
        return code;
    }

    // ── Outputs ───────────────────────────────────────────────────────────────
    let Some(sim) = es.as_uds() else {
        emit_error("internal", "uds session lost its engine", None, None);
        return EXIT_INTERNAL;
    };
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
    // §14.8.1: SAVE keeps the convolved hydrograph so a later run need not
    // recompute it. SCRATCH asks for the same work and then discards it, so
    // it is not written; the results are identical either way.
    if let Some((hydra::uds::model::FileMode::Save, name)) = &iface.rdii {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        if let Err(e) = create_and_write(&path, |w| sim.write_rdii(w).map(|_| ())) {
            emit_error(
                "io/interface",
                &format!("RDII interface file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_IO;
        }
    }
    // §14.8.2: SAVE keeps the hydrology this run computed so a later run
    // can redo its routing without recomputing the surface.
    if let Some((hydra::uds::model::FileMode::Save, name)) = &iface.runoff {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        if let Err(e) = create_and_write(&path, |w| sim.write_runoff(w).map(|_| ())) {
            emit_error(
                "io/interface",
                &format!("runoff interface file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_IO;
        }
    }
    if let Err(e) = es.finish_results() {
        emit_error("io/output", &e.to_string(), None, None);
        return EXIT_IO;
    }

    // When the report goes to stdout and progress was printed on stderr,
    // add a blank separator line so the two don't visually run together.
    if args.summary.is_none() && progress.enabled {
        let _ = writeln!(std::io::stderr());
    }
    if let Err(e) = crate::write_report(&es, args.summary.as_deref()) {
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
                 resolved for a model fetched over HTTP. Run from a local path"
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
    let as_written = base.join(name_path);
    if as_written.exists() {
        return Ok(as_written);
    }
    // Models carry paths from the machine they were authored on; a file
    // that moved beside the model is found by its trailing name, the same
    // fallback the engine and GUI apply.
    // Split on either separator: a path authored on Windows carries
    // backslashes that `file_name` does not treat as separators here, and
    // an authored-elsewhere path is exactly the case this fallback is
    // for. The engine and the GUI split the same way.
    let by_basename = name
        .rsplit(['/', '\\'])
        .next()
        .map(|tail| base.join(tail))
        .filter(|p| p.exists());
    Ok(by_basename.unwrap_or(as_written))
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
