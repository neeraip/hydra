//! The urban drainage (`uds`) run path.
//!
//! The engine performs no file I/O: model text is handed to it in memory,
//! and every auxiliary file a model declares — daily climate records,
//! hotstart state, routing interface files — is read or written here.
//! Auxiliary paths resolve relative to the model file's directory, per the
//! predecessor's convention, which is why they need a local model path.

use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use hydra::swmm::climate::parse_any_climate_file;
use hydra::swmm::objects::parse_network;
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
pub(crate) fn run(args: &RunArgs, cli: &Cli, bytes: Vec<u8>) -> i32 {
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

    // Borrowed where the bytes are already valid UTF-8, which is every
    // model anyone actually has: `into_owned` copied the whole file for
    // nothing, and a model file can be tens of megabytes.
    let text = String::from_utf8_lossy(&bytes);

    // Survey the model's auxiliary-file declarations before opening. Parse
    // problems are deliberately ignored here — the open below re-parses and
    // reports them properly.
    //
    // Only the four sections the survey reads are handed to it. Parsing
    // the whole model to reach a handful of file names costs what parsing
    // the whole model costs, and it is paid twice: on a 306 MB model whose
    // time series run to five million records, the survey's tokens alone
    // were half a gigabyte, held beside the ones the real open was about
    // to build.
    let survey_text = survey_sections(&text);
    let (net, _) = parse_network(&survey_text);

    let climate_records = match &net.climate.temperature {
        Some(TemperatureSource::File { name, units, .. }) => {
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
            match parse_any_climate_file(&climate_text, net.options.flow_units.is_us(), *units) {
                Ok((records, notices)) => {
                    // §14.14: a units word that governs nothing is said
                    // out loud rather than silently dropped.
                    for notice in notices {
                        emit_warning(
                            "input/climate",
                            &format!("climate file {}: {notice}", path.display()),
                            None,
                        );
                    }
                    records
                }
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

    // §14.8.3: a rainfall interface file caches records already parsed, so
    // when the model reads one the gages' own records are not read at all.
    let rain_iface = match &net.interface_files.rainfall {
        Some((hydra::uds::model::FileMode::Use, name)) => {
            let path = match resolve_aux_path(&args.model, name) {
                Ok(p) => p,
                Err(code) => return code,
            };
            match std::fs::read(&path) {
                Ok(bytes) => Some(bytes),
                Err(e) => {
                    emit_error(
                        "io/interface",
                        &format!("rainfall interface file {}: {e}", path.display()),
                        None,
                        None,
                    );
                    return EXIT_IO;
                }
            }
        }
        _ => None,
    };

    // External rain records (§14.12): one read per distinct file a gage
    // names, resolved beside the model like every auxiliary file.
    let mut rain_files: Vec<(String, hydra::swmm::rain::RainRecords)> = Vec::new();
    for gage in &net.gages {
        if rain_iface.is_some() {
            break;
        }
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
        match hydra::swmm::rain::parse_any_rain_file(&rain_text) {
            Ok((records, notices)) => {
                // §14.12.1: an accumulation this engine spread evenly is
                // said out loud, because four identical hours read as a
                // measurement to anyone not told otherwise.
                for notice in notices {
                    emit_warning(
                        "input/rain",
                        &format!("rain record {}: {notice}", path.display()),
                        None,
                    );
                }
                rain_files.push((file.clone(), records));
            }
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

    // Everything still wanted from the survey above is a handful of file
    // names, so the model it was read from goes now rather than sitting
    // beside the one the open is about to build. Two parsed networks of a
    // large model is tens of megabytes for no reason.
    let iface = net.interface_files.clone();
    // §14.15: a declared external mesh file is the caller's to read,
    // like every auxiliary.
    let mesh_file = net.overland.as_ref().and_then(|m| m.mesh_file.clone());
    drop(net);
    let mesh_text = match &mesh_file {
        Some(name) => {
            let path = match resolve_aux_path(&args.model, name) {
                Ok(p) => p,
                Err(code) => return code,
            };
            match std::fs::read_to_string(&path) {
                Ok(t) => Some(t),
                Err(e) => {
                    emit_error(
                        "io/mesh",
                        &format!("mesh file {}: {e}", path.display()),
                        None,
                        None,
                    );
                    return EXIT_IO;
                }
            }
        }
        None => None,
    };

    // ── Open: parse, validate, build ──────────────────────────────────────────
    // A model with an external mesh cannot run yet (§1.8), so the mesh
    // path needs no combination with the climate and rain paths: it
    // exists so the refusal and IGNORE_2D behaviours see the real mesh.
    let opened = match (&mesh_text, &rain_iface) {
        (Some(mesh), _) => hydra::swmm::session::open_with_overland_mesh(&text, mesh),
        (None, Some(bytes)) => {
            hydra::swmm::session::open_with_rain_interface(&text, climate_records, bytes)
        }
        (None, None) => {
            hydra::swmm::session::open_with_rain_records(&text, climate_records, rain_files)
        }
    };
    let (mut sim, diags, findings) = match opened {
        Ok(session) => session,
        Err(hydra::swmm::session::OpenError::Parse(diags)) => {
            for d in diags.iter().filter(|d| d.kind.is_error()) {
                emit_error("input/parse", &d.to_string(), None, None);
            }
            return EXIT_INPUT;
        }
        Err(hydra::swmm::session::OpenError::Build(OpenError::Validation(findings))) => {
            for v in findings.iter().filter(|v| v.kind.is_error()) {
                emit_error("validation/network", &v.to_string(), None, None);
            }
            return EXIT_INPUT;
        }
        Err(hydra::swmm::session::OpenError::Build(OpenError::Routing(r))) => {
            emit_error("input/unsupported", &r.to_string(), None, None);
            return EXIT_INPUT;
        }
        Err(hydra::swmm::session::OpenError::Build(OpenError::Surface(s))) => {
            emit_error("input/unsupported", &s.to_string(), None, None);
            return EXIT_INPUT;
        }
        Err(hydra::swmm::session::OpenError::Build(
            OpenError::Controls(msg) | OpenError::Transport(msg) | OpenError::Overland(msg),
        )) => {
            emit_error("input/unsupported", &msg, None, None);
            return EXIT_INPUT;
        }
    };
    // The session owns everything it needs, so the model text goes now
    // instead of riding the whole run: on the largest real model it is
    // 320 MB the next four minutes have no use for.
    drop(text);
    drop(bytes);

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
        if let Err(e) = hydra::swmm::session::supply_routing_inflows(&mut sim, &inflow_text) {
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
        if let Err(e) = hydra::swmm::session::supply_runoff(&mut sim, &bytes) {
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
        if let Err(e) = hydra::swmm::session::supply_rdii(&mut sim, &bytes) {
            emit_error(
                "input/interface",
                &format!("RDII interface file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_INPUT;
        }
    }

    // §12.3: resume before the run starts and after every auxiliary file
    // is in place, since the checkpoint is checked against the files this
    // run was given.
    if let Some(name) = args.resume.as_deref() {
        let path = std::path::Path::new(name);
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                emit_error(
                    "io/checkpoint",
                    &format!("checkpoint {}: {e}", path.display()),
                    None,
                    None,
                );
                return EXIT_IO;
            }
        };
        if let Err(e) = sim.load_checkpoint(&bytes) {
            emit_error(
                "input/checkpoint",
                &format!("checkpoint {}: {e}", path.display()),
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

    // §12.3: a run keeps every reporting instant only for a checkpoint
    // that may be asked for. With neither a results file (whose attach
    // states the intent) nor `--checkpoint`, say so up front — on the
    // largest real model the instants are hundreds of megabytes held
    // for a guarantee nothing will use.
    if args.results.is_none() && args.checkpoint.is_none() {
        es.forgo_checkpoint();
    }

    if let Some(out_path) = args.results.as_deref() {
        let attach = std::fs::File::create(out_path).and_then(|f| {
            // The run keeps what a checkpoint carries only when one
            // was asked for: `--checkpoint` is the whole question.
            let may = if args.checkpoint.is_some() {
                hydra::engines::MayCheckpoint::Yes
            } else {
                hydra::engines::MayCheckpoint::No
            };
            es.begin_results(Box::new(std::io::BufWriter::new(f)), may, &args.model, "")
        });
        if let Err(e) = attach {
            emit_error("io/output", &format!("{out_path}: {e}"), None, None);
            return EXIT_IO;
        }
    }

    // §14.16: a mesh model's overland results stream to a sidecar
    // alongside the §14.9 file.
    if let Some(out_path) = args.results.as_deref() {
        if es.as_uds().is_some_and(Simulation::has_overland) {
            let p2 = two_d_results_path(out_path);
            let attach = std::fs::File::create(&p2).and_then(|f| {
                hydra::swmm::session::begin_overland_results(
                    es.as_uds_mut().expect("checked above"),
                    Box::new(std::io::BufWriter::new(f)),
                )
            });
            if let Err(e) = attach {
                emit_error("io/output", &format!("{p2}: {e}"), None, None);
                return EXIT_IO;
            }
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
        if let Err(e) = create_and_write(&path, |w| {
            hydra::swmm::session::write_routing_outflows(sim, w)
        }) {
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
        if let Err(e) = create_and_write(&path, |w| {
            hydra::swmm::session::write_rdii(sim, w).map(|_| ())
        }) {
            emit_error(
                "io/interface",
                &format!("RDII interface file {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_IO;
        }
    }
    // §14.8.4: each usage line naming a report file gets one, written
    // beside the model like every auxiliary file.
    for (i, (name, parcel, control)) in sim.lid_report_files().iter().enumerate() {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        if let Err(e) = create_and_write(&path, |w| sim.write_lid_report(i, w)) {
            emit_error(
                "io/interface",
                &format!(
                    "control-measure report file {} ({control} in {parcel}): {e}",
                    path.display()
                ),
                None,
                None,
            );
            return EXIT_IO;
        }
    }
    // §12.3: the checkpoint is written from the finished run, so a script
    // that resumes from it continues where this one stopped.
    if let Some(name) = args.checkpoint.as_deref() {
        let path = std::path::Path::new(name);
        let written = std::fs::File::create(path)
            .map_err(|e| e.to_string())
            .and_then(|f| {
                let mut w = std::io::BufWriter::new(f);
                sim.save_checkpoint(&mut w)?;
                std::io::Write::flush(&mut w).map_err(|e| e.to_string())
            });
        if let Err(e) = written {
            emit_error(
                "io/checkpoint",
                &format!("checkpoint {}: {e}", path.display()),
                None,
                None,
            );
            return EXIT_IO;
        }
    }
    // §14.8.3: SAVE keeps the parsed rain records so a later run need not
    // parse them again.
    if let Some((hydra::uds::model::FileMode::Save, name)) = &iface.rainfall {
        let path = match resolve_aux_path(&args.model, name) {
            Ok(p) => p,
            Err(code) => return code,
        };
        if let Err(e) = create_and_write(&path, |w| {
            hydra::swmm::session::write_rain(sim, w).map(|_| ())
        }) {
            emit_error(
                "io/interface",
                &format!("rainfall interface file {}: {e}", path.display()),
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
        if let Err(e) = create_and_write(&path, |w| {
            hydra::swmm::session::write_runoff(sim, w).map(|_| ())
        }) {
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
/// The sections the auxiliary-file survey reads, verbatim, and no others.
///
/// Section headers are `[NAME]` at the start of a line, case-insensitively,
/// which is the predecessor's own rule. Anything before the first header
/// is kept: a model may open with a title or comments, and dropping them
/// would change line numbers in a diagnostic the survey never emits but a
/// reader might still compare against.
fn survey_sections(text: &str) -> String {
    const WANTED: [&str; 5] = [
        "[OPTIONS]",
        "[TEMPERATURE]",
        "[FILES]",
        "[RAINGAGES]",
        "[2D_MESH_FILE",
    ];
    let mut out = String::new();
    let mut keep = true;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            let head = t.to_ascii_uppercase();
            keep = WANTED.iter().any(|w| head.starts_with(w));
        }
        if keep {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

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

    /// The survey reads four sections, and is given four sections.
    ///
    /// It used to be handed the whole model to reach a handful of file
    /// names, and pay for parsing it — twice, since the real open parses
    /// again. On a 306 MB model whose time series run to five million
    /// records that was 233 MB of peak memory for nothing.
    #[test]
    fn the_survey_gets_the_sections_it_reads_and_no_others() {
        let inp = "\
[TITLE]
a model

[OPTIONS]
FLOW_UNITS  CMS

[RAINGAGES]
G1  INTENSITY  0:05  1.0  FILE  \"rain.dat\"  STA1  IN

[TIMESERIES]
TS1  0:00  1
TS1  0:05  2

[FILES]
USE RAINFALL  cache.rff

[COORDINATES]
J1  0  0
";
        let out = survey_sections(inp);
        for kept in [
            "[OPTIONS]",
            "[RAINGAGES]",
            "[FILES]",
            "rain.dat",
            "cache.rff",
        ] {
            assert!(out.contains(kept), "dropped {kept}:\n{out}");
        }
        // The bulk sections are what this exists to leave behind.
        for dropped in ["[TIMESERIES]", "TS1", "[COORDINATES]"] {
            assert!(!out.contains(dropped), "kept {dropped}:\n{out}");
        }
        // Headers are matched case-insensitively, as the predecessor does.
        assert!(survey_sections("[options]\nFLOW_UNITS  CMS\n").contains("FLOW_UNITS"));
        // And a section the survey does not read cannot smuggle lines in
        // by sharing a prefix.
        assert!(!survey_sections("[FILESYSTEM]\nx  y\n").contains("x  y"));
    }

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

/// The §14.16 sidecar's path beside the §14.9 results file: `run.out`
/// becomes `run.2d.out`, and a path without the extension gains it.
fn two_d_results_path(out_path: &str) -> String {
    match out_path.strip_suffix(".out") {
        Some(stem) => format!("{stem}.2d.out"),
        None => format!("{out_path}.2d.out"),
    }
}

#[cfg(test)]
mod overland_path_tests {
    use super::two_d_results_path;

    /// The sidecar lands beside the results file, named after it.
    #[test]
    fn the_sidecar_is_named_after_the_results_file() {
        assert_eq!(two_d_results_path("run.out"), "run.2d.out");
        assert_eq!(two_d_results_path("a/b/city.out"), "a/b/city.2d.out");
        assert_eq!(two_d_results_path("results"), "results.2d.out");
    }
}
