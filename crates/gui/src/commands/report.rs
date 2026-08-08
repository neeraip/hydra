//! Report-view commands: block catalog, per-project template persistence,
//! document generation, and export.
//!
//! All heavy lifting lives downstream — the engine produces fragments and
//! `hydra::report` assembles/renders; these commands only resolve bundle
//! paths, load the network (through the shared cache), and shuttle strings.

use hydra::report::{assemble, render_csv, render_html, render_txt, ReportContext, ReportTemplate};

use super::projects::{app_data_dir, results_path_for, validate_target_ids};
use super::results::network_for_target;
use super::NetworkState;
use crate::meta::{self, bundle};

/// The report-block catalog of the project's engine.
#[tauri::command]
pub fn list_report_blocks(
    app: tauri::AppHandle,
    project_id: Option<String>,
) -> Result<&'static [hydra::common::BlockDescriptor], String> {
    // No project (or an unreadable one) serves the wds catalog, matching the
    // pre-dispatch behaviour for existing callers.
    let Some(project_id) = project_id else {
        return Ok(hydra::report_catalog());
    };
    let app_data = app_data_dir(&app)?;
    Ok(
        match super::projects::project_engine_key(&app_data, &project_id).as_str() {
            "uds" => hydra::uds::report_blocks::report_catalog(),
            _ => hydra::report_catalog(),
        },
    )
}

/// Whether one block can be produced for a target, and why not when it cannot.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockAvailabilityDto {
    pub id: String,
    /// `"ok"`, `"unavailable"`, or `"failed"`.
    pub status: &'static str,
    /// Engine-authored explanation; absent when `status` is `"ok"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Which catalog blocks apply to this target's completed run.
///
/// The builder shows this so a section that cannot render is visible in the
/// outline rather than discovered as placeholder prose in the preview. It is
/// one full production pass — the same work the preview already does — so
/// callers should run it per target, not per edit.
///
/// A target with no results yields an empty list: nothing can be produced, and
/// flagging every block as broken would be noise rather than information.
#[tauri::command(async)]
pub fn probe_report_blocks(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Vec<BlockAvailabilityDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    if !out_path.exists() {
        return Ok(Vec::new());
    }
    let availability =
        |result: Result<hydra::common::Fragment, hydra::common::BlockError>| match result {
            Ok(_) => ("ok", None),
            Err(hydra::common::BlockError::Unavailable { reason }) => ("unavailable", Some(reason)),
            Err(err) => ("failed", Some(err.to_string())),
        };
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let network = super::results::uds_network_for_target(
                &app_data,
                &state,
                &project_id,
                scenario_id.as_deref(),
            )?;
            Ok(hydra::uds::report_blocks::report_catalog()
                .iter()
                .map(|block| {
                    let (status, reason) =
                        availability(hydra::uds::report_blocks::produce_report_block(
                            block.id, &out_path, &network, None,
                        ));
                    BlockAvailabilityDto {
                        id: block.id.to_string(),
                        status,
                        reason,
                    }
                })
                .collect())
        }
        _ => {
            let network =
                network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            Ok(hydra::report_catalog()
                .iter()
                .map(|block| {
                    let (status, reason) = availability(hydra::produce_report_block(
                        block.id, &out_path, &network, None,
                    ));
                    BlockAvailabilityDto {
                        id: block.id.to_string(),
                        status,
                        reason,
                    }
                })
                .collect())
        }
    }
}

/// The options `block_id` accepts, resolved against the target's network.
///
/// Resolved per target rather than served from a static table because the
/// defaults and unit labels follow the model's declared unit system — the
/// builder shows `20 psi` on a US model and `14 m` on an SI one without
/// knowing what either means (hydra-common spec §3.2.1).
///
/// An unknown block id yields an empty list rather than an error: descriptions
/// are advisory, and a template may legitimately reference a block this build
/// does not describe.
#[tauri::command(async)]
pub fn get_report_block_options(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    block_id: String,
) -> Result<Vec<hydra::common::OptionDescriptor>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let network = super::results::uds_network_for_target(
                &app_data,
                &state,
                &project_id,
                scenario_id.as_deref(),
            )?;
            Ok(hydra::uds::report_blocks::report_block_options(
                &block_id, &network,
            ))
        }
        _ => {
            let network =
                network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            Ok(hydra::report_block_options(&block_id, &network))
        }
    }
}

/// One analysis panel: a produced fragment, or why it could not be.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisBlockDto {
    pub id: String,
    pub title: String,
    /// `"ok"`, `"unavailable"`, or `"failed"`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment: Option<hydra::common::Fragment>,
}

/// Produce every catalog block for a target as analysis panels — the
/// engine's report blocks doubling as the Results view's content (the
/// analysis-as-blocks convergence). An absent results file yields an empty
/// list: nothing ran, nothing to analyse.
#[tauri::command(async)]
pub fn get_analysis_blocks(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    unit_system: Option<String>,
    criteria: Option<meta::ProjectCriteria>,
) -> Result<Vec<AnalysisBlockDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;
    let app_data = app_data_dir(&app)?;
    let out_path = results_path_for(&app_data, &project_id, scenario_id.as_deref());
    if !out_path.exists() {
        return Ok(Vec::new());
    }
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "uds" => {
            let network = super::results::uds_network_for_target(
                &app_data,
                &state,
                &project_id,
                scenario_id.as_deref(),
            )?;
            // Tagged values resolve here, not in the frontend: the analysis
            // surface and a rendered report must never disagree about what
            // a value reads as (report spec §4.0).
            let settings = hydra::report::DisplaySettings {
                family: display_family_for(
                    unit_system.as_deref(),
                    !network.options.flow_units.is_us(),
                ),
                catalog: hydra::uds::descriptors::QUANTITIES,
            };
            Ok(hydra::uds::report_blocks::report_catalog()
                .iter()
                .map(|block| {
                    match hydra::uds::report_blocks::produce_report_block(
                        block.id, &out_path, &network, None,
                    ) {
                        Ok(fragment) => AnalysisBlockDto {
                            id: block.id.to_string(),
                            title: block.title.to_string(),
                            status: "ok",
                            reason: None,
                            fragment: Some(hydra::report::resolve_fragment_display(
                                &fragment, &settings,
                            )),
                        },
                        Err(hydra::common::BlockError::Unavailable { reason }) => {
                            AnalysisBlockDto {
                                id: block.id.to_string(),
                                title: block.title.to_string(),
                                status: "unavailable",
                                reason: Some(reason),
                                fragment: None,
                            }
                        }
                        Err(err) => AnalysisBlockDto {
                            id: block.id.to_string(),
                            title: block.title.to_string(),
                            status: "failed",
                            reason: Some(err.to_string()),
                            fragment: None,
                        },
                    }
                })
                .collect())
        }
        _ => {
            let network =
                network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            let settings = hydra::report::DisplaySettings {
                family: display_family_for(
                    unit_system.as_deref(),
                    hydra::io::units::is_si(network.options.flow_units),
                ),
                catalog: hydra::descriptors::QUANTITIES,
            };
            // The caller's criteria feed the criteria-shaped blocks, so
            // the page and the canvas judge by the same numbers. They
            // travel with the request — the frontend applies edits locally
            // and persists fire-and-forget, so a disk read here could race
            // an edit's own save. Absent (another caller), the saved ones
            // apply. Criteria are stored in SI; block options are file
            // display units by the engine's own spec, so the conversion
            // happens here with the engine's factors — composition-root
            // work.
            let criteria = criteria.or_else(|| {
                meta::read_project_criteria(&bundle::project_dir(&app_data, &project_id))
            });
            let options_by_id = wds_analysis_options(criteria.as_ref(), &network);
            Ok(hydra::report_catalog()
                .iter()
                .map(|block| {
                    let options = options_by_id.get(block.id);
                    match hydra::produce_report_block(block.id, &out_path, &network, options) {
                        Ok(fragment) => AnalysisBlockDto {
                            id: block.id.to_string(),
                            title: block.title.to_string(),
                            status: "ok",
                            reason: None,
                            fragment: Some(hydra::report::resolve_fragment_display(
                                &fragment, &settings,
                            )),
                        },
                        Err(hydra::common::BlockError::Unavailable { reason }) => {
                            AnalysisBlockDto {
                                id: block.id.to_string(),
                                title: block.title.to_string(),
                                status: "unavailable",
                                reason: Some(reason),
                                fragment: None,
                            }
                        }
                        Err(err) => AnalysisBlockDto {
                            id: block.id.to_string(),
                            title: block.title.to_string(),
                            status: "failed",
                            reason: Some(err.to_string()),
                            fragment: None,
                        },
                    }
                })
                .collect())
        }
    }
}

/// Options for the criteria-shaped wds blocks, from the project's saved
/// criteria — absent criteria means absent options, which is each block's
/// documented default.
///
/// Criteria are stored in SI (metres, m/s); wds block options are in the
/// results file's display units (analysis spec §4.1.1), so US models
/// convert with the engine's own §3.1 factors. Threshold edges must ascend
/// strictly, so a degenerate band (two equal boundaries) sends no edges
/// and the block falls back to its documented defaults rather than
/// failing production.
fn wds_analysis_options(
    criteria: Option<&meta::ProjectCriteria>,
    network: &hydra::Network,
) -> std::collections::HashMap<&'static str, serde_json::Value> {
    let mut options = std::collections::HashMap::new();
    let Some(criteria) = criteria else {
        return options;
    };
    let ucf =
        hydra::io::units::make_ucf(network.options.flow_units, network.options.specific_gravity);
    let si = hydra::io::units::is_si(network.options.flow_units);
    let pressure = |m: f64| if si { m } else { m * ucf.pressure };
    let velocity = |ms: f64| if si { ms } else { ms * ucf.elev };

    options.insert(
        "wds.service-compliance",
        serde_json::json!({ "minPressure": pressure(criteria.min_pressure_m) }),
    );
    let ascending = |edges: &[f64]| edges.windows(2).all(|w| w[1] > w[0]);
    let pressure_edges: Vec<f64> = [
        criteria.pressure.low,
        criteria.pressure.required,
        criteria.pressure.high,
    ]
    .iter()
    .map(|&m| pressure(m))
    .collect();
    if ascending(&pressure_edges) {
        options.insert(
            "wds.pressure-thresholds",
            serde_json::json!({ "edges": pressure_edges }),
        );
    }
    let velocity_edges: Vec<f64> = [
        criteria.velocity.low,
        criteria.velocity.target,
        criteria.velocity.high,
    ]
    .iter()
    .map(|&ms| velocity(ms))
    .collect();
    if ascending(&velocity_edges) {
        options.insert(
            "wds.velocity-thresholds",
            serde_json::json!({ "edges": velocity_edges }),
        );
    }
    options
}

fn template_path(app_data: &std::path::Path, project_id: &str) -> std::path::PathBuf {
    bundle::project_dir(app_data, project_id).join("report-template.json")
}

/// The project's saved report template JSON, or `None` before one exists.
#[tauri::command]
pub fn get_report_template(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Option<String>, String> {
    validate_target_ids(&project_id, None)?;
    let path = template_path(&app_data_dir(&app)?, &project_id);
    match std::fs::read_to_string(&path) {
        Ok(json) => Ok(Some(json)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Persist the project's report template. Validated before writing so a
/// frontend bug can never wedge the stored template.
#[tauri::command]
pub fn save_report_template(
    app: tauri::AppHandle,
    project_id: String,
    template_json: String,
) -> Result<(), String> {
    validate_target_ids(&project_id, None)?;
    ReportTemplate::from_json(&template_json).map_err(|e| e.to_string())?;
    let path = template_path(&app_data_dir(&app)?, &project_id);
    bundle::atomic_write(&path, template_json.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Render a report document for a target's persisted results. `format` is
/// `"txt"`, `"csv"`, `"html"`, or `"pdf"` — pdf returns base64-encoded
/// bytes (IPC carries strings); the rest return the text verbatim.
/// Preview calls pass `with_timestamp: false` so re-renders are stable;
/// exports stamp the generation time.
// Eight arguments because each is a named IPC field the frontend sends;
// bundling them would rename the wire protocol, not simplify it.
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn generate_report(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    template_json: String,
    format: String,
    with_timestamp: bool,
    unit_system: Option<String>,
) -> Result<String, String> {
    let template = ReportTemplate::from_json(&template_json).map_err(|e| e.to_string())?;
    render_for_target(
        &app,
        &state,
        &project_id,
        scenario_id.as_deref(),
        &RenderRequest {
            template: &template,
            format: &format,
            with_timestamp,
            unit_system: unit_system.as_deref(),
        },
    )
}

/// Generate (with timestamp) and save a report document via the OS save
/// dialog. Resolves to the chosen path, or `None` when cancelled.
#[tauri::command(async)]
pub async fn export_report(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    template_json: String,
    format: String,
    unit_system: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let template = ReportTemplate::from_json(&template_json).map_err(|e| e.to_string())?;
    let rendered = render_for_target(
        &app,
        &state,
        &project_id,
        scenario_id.as_deref(),
        &RenderRequest {
            template: &template,
            format: &format,
            with_timestamp: true,
            unit_system: unit_system.as_deref(),
        },
    )?;

    let (filter_name, ext) = match format.as_str() {
        "csv" => ("CSV", "csv"),
        "html" => ("HTML", "html"),
        "pdf" => ("PDF", "pdf"),
        _ => ("Text", "txt"),
    };
    let app_data = app_data_dir(&app)?;
    let default_name = meta::read_project_meta(&bundle::project_dir(&app_data, &project_id))
        .map(|m| format!("{}-report.{ext}", m.name))
        .unwrap_or_else(|_| format!("report.{ext}"));

    // The dialog call blocks until the user answers — run it on the
    // blocking pool so it does not tie up an async runtime worker.
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog_app
            .dialog()
            .file()
            .add_filter(filter_name, &[ext])
            .set_file_name(default_name)
            .blocking_save_file()
    })
    .await
    .map_err(|e| format!("file dialog task panicked: {e}"))?;

    let file_path = match picked {
        Some(p) => p,
        None => return Ok(None), // user cancelled
    };
    let path = file_path.into_path().map_err(|e| e.to_string())?;
    // Pdf travels base64-encoded through render_for_target; decode back
    // to raw bytes for the file.
    if format == "pdf" {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(rendered.as_bytes())
            .map_err(|e| format!("internal pdf encoding error: {e}"))?;
        std::fs::write(&path, bytes)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    } else {
        std::fs::write(&path, rendered)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// The display family for tagged fragment values: the caller's choice
/// where one arrived, else the model's own family — which reproduces the
/// results file's values exactly, making "no preference" byte-compatible
/// with the pre-tagging output.
fn display_family_for(
    unit_system: Option<&str>,
    model_is_si: bool,
) -> hydra::common::DisplayFamily {
    match unit_system {
        Some("si") => hydra::common::DisplayFamily::Si,
        Some("us") => hydra::common::DisplayFamily::Us,
        _ => {
            if model_is_si {
                hydra::common::DisplayFamily::Si
            } else {
                hydra::common::DisplayFamily::Us
            }
        }
    }
}

/// How one render should read: the template, the output format, and the
/// presentation choices that vary per call.
struct RenderRequest<'a> {
    template: &'a ReportTemplate,
    format: &'a str,
    with_timestamp: bool,
    /// The reader's display system ("si"/"us"), or `None` for the model's
    /// own family.
    unit_system: Option<&'a str>,
}

fn render_for_target(
    app: &tauri::AppHandle,
    state: &NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
    request: &RenderRequest<'_>,
) -> Result<String, String> {
    let RenderRequest {
        template,
        format,
        with_timestamp,
        unit_system,
    } = *request;
    validate_target_ids(project_id, scenario_id)?;
    let app_data = app_data_dir(app)?;
    let out_path = results_path_for(&app_data, project_id, scenario_id);
    if !out_path.exists() {
        return Err(
            "No simulation results exist for this target — run a simulation first".to_string(),
        );
    }
    // Existing is not the same as readable. Every block reads this one file,
    // so results this build cannot parse fail all of them identically — and
    // an export of thirteen copies of one error would be delivered as a
    // finished document. Surface it once, as a failed export.
    //
    // `probe_report_blocks` deliberately does not do this: reporting per-block
    // status is its purpose, so there the failures are the answer.
    let engine = super::projects::project_engine_key(&app_data, project_id);
    match engine.as_str() {
        "uds" => {
            hydra::uds::io::out_reader::read_metadata(&out_path)?;
        }
        _ => {
            hydra::io::out_reader::read_metadata_checked(&out_path).map_err(|e| e.to_string())?;
        }
    }

    let project_name = meta::read_project_meta(&bundle::project_dir(&app_data, project_id))
        .map(|m| m.name)
        .unwrap_or_else(|_| project_id.to_string());
    let scenario_name = match scenario_id {
        Some(id) => meta::read_scenario_meta(&bundle::scenario_dir(&app_data, project_id, id))
            .map(|m| m.name)
            .unwrap_or_else(|_| id.to_string()),
        None => "Base".to_string(),
    };
    let context = ReportContext {
        generated_at: with_timestamp
            .then(|| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        source: vec![
            ("Project".into(), project_name),
            ("Scenario".into(), scenario_name),
        ],
    };

    let document = match engine.as_str() {
        "uds" => {
            let network =
                super::results::uds_network_for_target(&app_data, state, project_id, scenario_id)?;
            let document = assemble(
                template,
                hydra::uds::report_blocks::report_catalog(),
                context,
                |id, options| {
                    hydra::uds::report_blocks::produce_report_block(
                        id, &out_path, &network, options,
                    )
                },
            );
            let family = display_family_for(unit_system, !network.options.flow_units.is_us());
            hydra::report::resolve_display(
                &document,
                &hydra::report::DisplaySettings {
                    family,
                    catalog: hydra::uds::descriptors::QUANTITIES,
                },
            )
        }
        _ => {
            let network = network_for_target(&app_data, state, project_id, scenario_id)?;
            let document = assemble(template, hydra::report_catalog(), context, |id, options| {
                hydra::produce_report_block(id, &out_path, &network, options)
            });
            let family = display_family_for(
                unit_system,
                hydra::io::units::is_si(network.options.flow_units),
            );
            hydra::report::resolve_display(
                &document,
                &hydra::report::DisplaySettings {
                    family,
                    catalog: hydra::descriptors::QUANTITIES,
                },
            )
        }
    };
    Ok(match format {
        "txt" => render_txt(&document),
        "csv" => render_csv(&document),
        "html" => render_html(&document),
        "pdf" => {
            use base64::Engine as _;
            let bytes = hydra::report::render_pdf(&document).map_err(|e| e.to_string())?;
            base64::engine::general_purpose::STANDARD.encode(bytes)
        }
        other => return Err(format!("unknown report format: {other:?}")),
    })
}

#[cfg(test)]
mod wds_options_tests {
    use super::*;

    fn network(units_line: &str) -> hydra::Network {
        let inp = format!(
            "[JUNCTIONS]\nJ1  0  10\n\n[RESERVOIRS]\nR1  100\n\n\
             [PIPES]\nP1  R1  J1  1000  300  100  0  Open\n\n\
             [OPTIONS]\nUnits  {units_line}\nHeadloss  H-W\n\n[END]\n"
        );
        hydra::io::parse(inp.as_bytes()).expect("parse")
    }

    fn criteria() -> meta::ProjectCriteria {
        serde_json::from_value(serde_json::json!({
            "version": 1,
            "minPressureM": 14.0,
            "pressure": { "low": 24.0, "required": 35.0, "high": 45.0 },
            "velocity": { "low": 0.1, "target": 0.5, "high": 1.5 },
            "flow": { "low": 0.1, "target": 1.0, "high": 10.0 },
        }))
        .expect("criteria json")
    }

    /// Criteria are SI; an SI model's options pass through untouched.
    #[test]
    fn si_criteria_feed_si_options_unchanged() {
        let options = wds_analysis_options(Some(&criteria()), &network("LPS"));
        assert_eq!(
            options["wds.service-compliance"]["minPressure"]
                .as_f64()
                .expect("number"),
            14.0
        );
        let edges: Vec<f64> = options["wds.pressure-thresholds"]["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .map(|v| v.as_f64().expect("number"))
            .collect();
        assert_eq!(edges, vec![24.0, 35.0, 45.0]);
    }

    /// Block options are file display units by the engine's spec, so a US
    /// model's criteria convert with the engine's own factors — 14 m of
    /// service pressure becomes ~20 psi, which is also the engine's own
    /// US default for exactly that criterion.
    #[test]
    fn us_criteria_convert_to_file_units() {
        let options = wds_analysis_options(Some(&criteria()), &network("GPM"));
        let psi = options["wds.service-compliance"]["minPressure"]
            .as_f64()
            .expect("number");
        assert!(
            (psi - 19.9).abs() < 0.2,
            "14 m should be ~20 psi, got {psi}"
        );
        let edges: Vec<f64> = options["wds.velocity-thresholds"]["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .map(|v| v.as_f64().expect("number"))
            .collect();
        // m/s → ft/s.
        assert!((edges[2] - 1.5 * 3.2808).abs() < 1e-6, "{edges:?}");
    }

    /// A degenerate band cannot make strictly-ascending edges; the block
    /// falls back to its documented defaults rather than failing.
    #[test]
    fn a_degenerate_band_sends_no_edges() {
        let mut c = criteria();
        c.pressure.required = c.pressure.low;
        let options = wds_analysis_options(Some(&c), &network("LPS"));
        assert!(!options.contains_key("wds.pressure-thresholds"));
        assert!(options.contains_key("wds.velocity-thresholds"));
    }

    /// No saved criteria means no options at all — every block runs on its
    /// own documented defaults.
    #[test]
    fn absent_criteria_send_nothing() {
        assert!(wds_analysis_options(None, &network("LPS")).is_empty());
    }
}
