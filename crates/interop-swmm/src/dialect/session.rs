//! Opening, feeding and draining a session through the SWMM dialect
//! (§12.1, §14.8, §14.9): the text-taking surface the engine no longer
//! has, recomposed over its public data API.
//!
//! Everything here parses text into engine data or renders engine data
//! into the predecessor's formats. Callers own reading and writing
//! bytes; the engine owns meaning.

use crate::engine_api::model::validate::ValidationDiagnostic;
use crate::engine_api::model::DailyClimate;
use crate::engine_api::simulation::engine::{OpenError as BuildError, Simulation};
use crate::engine_api::simulation::records::{RainInterface, RainReading, RainRecords};

use crate::dialect::survey::{Diagnostic, DiagnosticKind};

/// Why a model failed to open: refused by this dialect's parsing, or by
/// the engine building the session from the parsed model.
#[derive(Debug)]
pub enum OpenError {
    /// The file was refused by parsing; the diagnostics say where.
    Parse(Vec<Diagnostic>),
    /// The engine refused the parsed model; §14.7 validation, the
    /// router, the surface, controls, transport, or the overland mesh
    /// say why.
    Build(BuildError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Parse(diags) => {
                let shown = diags
                    .iter()
                    .find(|d| d.kind.is_error())
                    .or(diags.first())
                    .map(|d| d.to_string())
                    .unwrap_or_default();
                let errors = diags.iter().filter(|d| d.kind.is_error()).count();
                if errors > 1 {
                    write!(
                        f,
                        "the model was refused by parsing: {shown} (and {} more findings)",
                        errors - 1
                    )
                } else {
                    write!(f, "the model was refused by parsing: {shown}")
                }
            }
            OpenError::Build(e) => write!(f, "{e}"),
        }
    }
}

impl From<BuildError> for OpenError {
    fn from(e: BuildError) -> OpenError {
        OpenError::Build(e)
    }
}

type Opened = (Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>);

/// Load a model from its text alone (§12.1).
pub fn open(input: &str) -> Result<Opened, OpenError> {
    open_with_files(input, Vec::new(), Vec::new())
}

/// Load a model together with daily climate records (§3.1).
pub fn open_with_climate(
    input: &str,
    climate_records: Vec<DailyClimate>,
) -> Result<Opened, OpenError> {
    open_with_files(input, climate_records, Vec::new())
}

/// Load a model together with every auxiliary record the caller read
/// for it (§12.1): daily climate records (§3.1) and external rain
/// records (§14.12) as `(file name, parsed readings)`.
pub fn open_with_files(
    input: &str,
    climate_records: Vec<DailyClimate>,
    rain_files: Vec<(String, Vec<RainReading>)>,
) -> Result<Opened, OpenError> {
    let records = rain_files
        .into_iter()
        .map(|(name, readings)| (name, RainRecords::Station(readings)))
        .collect();
    open_inner(input, climate_records, records, None, None)
}

/// Load a model together with rain files in whichever layout each was
/// written in (§14.12, §14.12.1) — [`crate::dialect::rain::parse_any_rain_file`]
/// recognises them.
pub fn open_with_rain_records(
    input: &str,
    climate_records: Vec<DailyClimate>,
    rain_files: Vec<(String, RainRecords)>,
) -> Result<Opened, OpenError> {
    open_inner(input, climate_records, rain_files, None, None)
}

/// Load a model whose file-sourced gages read a rainfall interface file
/// (§14.8.3) rather than their own records.
pub fn open_with_rain_interface(
    input: &str,
    climate_records: Vec<DailyClimate>,
    rain_interface: &[u8],
) -> Result<Opened, OpenError> {
    let iface = crate::dialect::iface::parse_rain_iface(rain_interface)
        .map_err(|e| BuildError::Transport(format!("rainfall interface file: {e}")))?;
    open_inner(input, climate_records, Vec::new(), Some(iface), None)
}

/// Load a model together with the external mesh file its
/// `[2D_MESH_FILE]` declares (§14.15) — the caller owns reading it.
pub fn open_with_overland_mesh(input: &str, mesh_text: &str) -> Result<Opened, OpenError> {
    open_inner(input, Vec::new(), Vec::new(), None, Some(mesh_text))
}

fn open_inner(
    input: &str,
    climate_records: Vec<DailyClimate>,
    rain_files: Vec<(String, RainRecords)>,
    rain_interface: Option<RainInterface>,
    overland_mesh: Option<&str>,
) -> Result<Opened, OpenError> {
    let (mut net, mut diags) = crate::dialect::objects::parse_network(input);
    if diags.iter().any(|d| d.kind.is_error()) {
        return Err(OpenError::Parse(diags));
    }
    // §14.15: a declared external mesh file carries the mesh's own
    // sections; supplied, they continue the inline ones — indices,
    // units header and all — through one combined parse. Declared and
    // not supplied, the mesh cannot be read: under IGNORE_2D the run
    // warns and proceeds without it, and otherwise the engine refuses
    // with the file named.
    let mesh_file = net.overland.as_ref().and_then(|m| m.mesh_file.clone());
    match (&mesh_file, overland_mesh) {
        (Some(_), Some(external)) => {
            net.overland = crate::dialect::overland::reparse_with_external(
                input,
                external,
                &net.options,
                &mut diags,
            );
            // Combined: the declaration is consumed, and the engine's
            // "never combined" refusal must not see it.
            if let Some(m) = net.overland.as_mut() {
                m.mesh_file = None;
            }
        }
        (Some(name), None) if net.options.ignore_overland => {
            // The 1D half runs, but the author should hear that the
            // unreadable mesh was dropped, file named.
            diags.push(Diagnostic {
                line: 0,
                kind: DiagnosticKind::UnknownOverlandOption {
                    key: format!("mesh file {name:?} not supplied; mesh ignored"),
                },
            });
            net.overland = None;
        }
        // Declared, not supplied, not ignored: the engine's own
        // refusal names the file (from_network sees mesh_file set).
        (Some(_), None) | (None, _) => {}
    }
    let (sim, findings) =
        Simulation::from_network(net, climate_records, rain_files, rain_interface)?;
    Ok((sim, diags, findings))
}

// ── Feeding a running session (§14.8) ─────────────────────────────────

/// Supply the routing interface inflow file's text (§14.8).
pub fn supply_routing_inflows(sim: &mut Simulation, text: &str) -> Result<(), String> {
    let data = crate::dialect::iface::parse_routing_file(text, sim.network())?;
    sim.supply_routing_data(data, text.as_bytes());
    Ok(())
}

/// Supply the runoff interface file's bytes (§14.8.2).
pub fn supply_runoff(sim: &mut Simulation, bytes: &[u8]) -> Result<(), String> {
    let data = crate::dialect::iface::parse_runoff_file(bytes, sim.network())?;
    sim.supply_runoff_data(data, bytes)
}

/// Supply the RDII interface file's bytes (§14.8.1).
pub fn supply_rdii(sim: &mut Simulation, bytes: &[u8]) -> Result<(), String> {
    let cv = crate::engine_api::simulation::records::flow_cv_of(sim.network().options.flow_units);
    let data = crate::dialect::iface::parse_rdii_file(bytes, sim.network(), cv)?;
    sim.supply_rdii_data(data, bytes);
    Ok(())
}

// ── Results (§14.9, §14.16) ───────────────────────────────────────────

/// Stream the §14.9 results to `sink` as the run produces them.
pub fn begin_results(
    sim: &mut Simulation,
    sink: Box<dyn std::io::Write + Send>,
    may_checkpoint: bool,
) -> std::io::Result<()> {
    let out = crate::dialect::out_writer::OutStream::begin(
        sink,
        sim.network(),
        sim.start_epoch(),
        sim.report_step(),
        sim.first_report_instant(),
    )?;
    sim.attach_results(Box::new(out), may_checkpoint)
}

/// Stream the §14.16 overland sidecar to `sink` alongside the §14.9
/// stream. Refused for a model with no mesh.
pub fn begin_overland_results(
    sim: &mut Simulation,
    sink: Box<dyn std::io::Write + Send>,
) -> std::io::Result<()> {
    let (Some(mesh), Some(marcher)) = (sim.network().overland.as_ref(), sim.overland_marcher())
    else {
        return Err(std::io::Error::other(
            "this run has no overland mesh to record",
        ));
    };
    let out = crate::dialect::overland_out::OverlandStream::begin(
        sink,
        mesh,
        marcher,
        sim.start_epoch(),
        sim.report_step(),
        sim.first_report_instant(),
    )?;
    sim.attach_overland_results(Box::new(out))
}

/// Write the §14.9 binary results to `w` from the gathered instants.
pub fn write_out(sim: &Simulation, w: &mut impl std::io::Write) -> std::io::Result<()> {
    // A run that disclaimed checkpointing kept no instants (§12.3), so
    // there is nothing here to write. An empty file would be a wrong
    // answer rather than a failure, which is worse.
    if !sim.retains_snapshots() {
        return Err(std::io::Error::other(
            "this run was opened without checkpointing, so the reporting instants \
             this file is built from were not kept",
        ));
    }
    crate::dialect::out_writer::write_out(
        sim.network(),
        &sim.snapshots,
        sim.start_epoch(),
        sim.report_step(),
        w,
    )
}

/// Write the §14.9 text report to `w`.
pub fn write_report(sim: &Simulation, w: &mut impl std::io::Write) -> std::io::Result<()> {
    crate::dialect::rpt_writer::write_rpt(&sim.report_inputs(), w)
}

// ── Interface files a run records (§14.8) ─────────────────────────────

/// Write the rainfall interface file for this model's gages, if it
/// asked for one (§14.8.3). `false` when it did not.
pub fn write_rain(sim: &Simulation, w: &mut impl std::io::Write) -> std::io::Result<bool> {
    let Some(gages) = sim.rain_interface_records() else {
        return Ok(false);
    };
    crate::dialect::iface::write_rain_iface(gages, w)?;
    Ok(true)
}

/// Write the runoff interface file this run recorded, if it recorded
/// one (§14.8.2). `false` when the model asked for no such file.
pub fn write_runoff(sim: &Simulation, w: &mut impl std::io::Write) -> std::io::Result<bool> {
    let Some(rows) = sim.runoff_records() else {
        return Ok(false);
    };
    crate::dialect::iface::write_runoff_file(sim.network(), rows, w)?;
    Ok(true)
}

/// Write the RDII interface file (§14.8.1), if the model asked for one.
/// `false` when it did not.
pub fn write_rdii(sim: &Simulation, w: &mut impl std::io::Write) -> std::io::Result<bool> {
    let Some((vertices, step, rows)) = sim.rdii_records() else {
        return Ok(false);
    };
    crate::dialect::iface::write_rdii_file(sim.network(), &vertices, step, rows, w)?;
    Ok(true)
}

/// Write the routing interface outflow file (§14.8): outlet vertices'
/// inflows and concentrations per reporting period.
pub fn write_routing_outflows(
    sim: &Simulation,
    w: &mut impl std::io::Write,
) -> std::io::Result<()> {
    if !sim.retains_snapshots() {
        return Err(std::io::Error::other(
            "this run was opened without checkpointing, so the reporting instants \
             this file is built from were not kept",
        ));
    }
    crate::dialect::iface::write_routing_file(
        sim.network(),
        &sim.snapshots,
        sim.start_epoch(),
        sim.report_step(),
        w,
    )
}

// ── Report blocks over a persisted results file (§13, §14.9) ──────────

/// A §14.9 results file as a
/// [`PeriodSource`](crate::engine_api::report_blocks::source::PeriodSource):
/// metadata read at open, periods scanned from the path on demand — the
/// filesystem carve-out, for the same reason as ever.
pub struct OutFileSource {
    path: std::path::PathBuf,
    file_meta: crate::dialect::out_reader::OutMetadata,
    meta: crate::engine_api::report_blocks::source::ResultsMeta,
}

impl OutFileSource {
    /// Open and validate the results file at `path`.
    pub fn open(path: &std::path::Path) -> Result<OutFileSource, String> {
        let file_meta = crate::dialect::out_reader::read_metadata(path)?;
        let meta = crate::engine_api::report_blocks::source::ResultsMeta {
            subcatchment_ids: file_meta.subcatchment_ids.clone(),
            node_ids: file_meta.node_ids.clone(),
            link_ids: file_meta.link_ids.clone(),
            pollutant_ids: file_meta.pollutant_ids.clone(),
            n_subcatch_vars: file_meta.n_subcatch_vars,
            n_node_vars: file_meta.n_node_vars,
            n_link_vars: file_meta.n_link_vars,
            n_periods: file_meta.n_periods,
            report_step_s: file_meta.report_step_s,
            flow_units: file_meta.flow_units,
        };
        Ok(OutFileSource {
            path: path.to_path_buf(),
            file_meta,
            meta,
        })
    }
}

impl crate::engine_api::report_blocks::source::PeriodSource for OutFileSource {
    fn meta(&self) -> &crate::engine_api::report_blocks::source::ResultsMeta {
        &self.meta
    }

    fn scan(
        &self,
        f: &mut dyn FnMut(usize, &crate::engine_api::report_blocks::source::PeriodValues),
    ) -> Result<(), String> {
        crate::dialect::out_reader::scan_periods(&self.path, &self.file_meta, |i, rec| {
            let values = crate::engine_api::report_blocks::source::PeriodValues {
                epoch_s: rec.epoch_s,
                subcatchments: rec.subcatchments.clone(),
                nodes: rec.nodes.clone(),
                links: rec.links.clone(),
                system: rec.system,
            };
            f(i, &values);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A parse refusal reads as prose, never as a debug dump.
    #[test]
    fn a_parse_refusal_displays_as_prose() {
        let (_, diags) = crate::dialect::objects::parse_network("not a model at all");
        let e = OpenError::Parse(diags);
        let line = e.to_string();
        assert!(
            line.starts_with("the model was refused by parsing"),
            "{line}"
        );
        assert!(
            !line.contains("Diagnostic") && !line.contains('{'),
            "{line}"
        );
    }
}
