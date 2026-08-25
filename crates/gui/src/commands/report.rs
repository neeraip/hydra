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
    // Reads <project>/meta.json below, so the id is checked first, as every
    // other project-scoped command does.
    super::projects::validate_id(&project_id)?;
    let app_data = app_data_dir(&app)?;
    match super::projects::project_engine_key(&app_data, &project_id).as_str() {
        "uds" => Ok(hydra::uds::report_blocks::report_catalog()),
        "wds" => Ok(hydra::report_catalog()),
        other => Err(super::projects::unknown_engine(other)),
    }
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
                    let (status, reason) = availability(produce_uds_block_from_file(
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
        "wds" => {
            let network =
                network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            Ok(hydra::report_catalog()
                .iter()
                .map(|block| {
                    let (status, reason) = availability(produce_wds_block_from_file(
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
        other => Err(super::projects::unknown_engine(other)),
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
        "wds" => {
            let network =
                network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            Ok(hydra::report_block_options(&block_id, &network))
        }
        other => Err(super::projects::unknown_engine(other)),
    }
}

/// One criterion of the active engine's assessment standard, enriched
/// with its resolved quantity descriptor so the editor converts display
/// units from engine-published data instead of a hand-kept table.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionDto {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<hydra::common::QuantityDescriptor>,
    pub kind: hydra::common::CriterionKind,
    /// What each region between the cuts means, ascending (spec §7.2).
    /// Empty for a criterion that is judged but never drawn — which is
    /// what tells the canvas it cannot offer a threshold scale for it.
    pub severities: &'static [hydra::common::CategorySeverity],
}

/// The active engine's criteria catalog (hydra-common spec §7.2).
#[tauri::command]
pub fn get_criteria_catalog(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<Vec<CriterionDto>, String> {
    validate_target_ids(&project_id, None)?;
    let app_data = app_data_dir(&app)?;
    let (catalog, quantities): (&[hydra::common::CriterionDescriptor], _) =
        match super::projects::project_engine_key(&app_data, &project_id).as_str() {
            "uds" => (
                hydra::uds::report_blocks::criteria_catalog(),
                hydra::uds::descriptors::QUANTITIES,
            ),
            "wds" => (hydra::criteria_catalog(), hydra::descriptors::QUANTITIES),
            other => return Err(super::projects::unknown_engine(other)),
        };
    Ok(catalog
        .iter()
        .map(|c| CriterionDto {
            key: c.key,
            label: c.label,
            help: c.help,
            quantity: c
                .quantity
                .and_then(|key| quantities.iter().find(|q| q.key == key).copied()),
            kind: c.kind,
            severities: c.severities,
        })
        .collect())
}

/// A wds valuation (hydra-common spec §7.3) from the project's saved
/// criteria shape. The saved shape predates the criteria contract and the
/// canvas still reads it, so the store stays; this is the bridge. The
/// frontend holds the same mapping (`wdsValuation` in `AnalysisPanel`) —
/// a cross-boundary pair, tested on each side.
/// The project's saved criteria valuation for `engine` (hydra-common spec
/// §7.3), or `None` when it has none. The wds store predates the contract,
/// so its saved shape is bridged; uds and every later engine hold a
/// valuation directly.
fn saved_valuation(
    app_data: &std::path::Path,
    project_id: &str,
    engine: &str,
) -> Option<serde_json::Value> {
    let dir = bundle::project_dir(app_data, project_id);
    match engine {
        "uds" => meta::read_criteria_valuation(&dir, "uds"),
        "wds" => meta::read_project_criteria(&dir).map(|c| wds_valuation_of(&c)),
        _ => None,
    }
}

/// One block's options for a *report*: the criteria-derived options with
/// the template's own overlaid, key by key.
///
/// The project's criteria are the standard the whole application judges
/// by, so they are the default for every criteria-shaped block — without
/// this a report judged compliance by the block's built-in defaults while
/// the Analysis page judged it by the user's criteria, and the same block
/// told two stories. A template that names an option still wins, so a
/// report deliberately pinned to a fixed standard stays pinned; the
/// overlay is per key, so pinning the row count does not silently unpin
/// the pressure it is counting.
///
/// The CLI has no project bundle and therefore no criteria: a template
/// rendered there is template-only, which is the same document this
/// produces for a project whose criteria sit at their defaults.
fn report_block_options_for(
    criteria: Option<&serde_json::Value>,
    template: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    match (criteria, template) {
        (None, t) => t.cloned(),
        (Some(c), None) => Some(c.clone()),
        (Some(c), Some(t)) => match (c.as_object(), t.as_object()) {
            (Some(base), Some(over)) => {
                let mut merged = base.clone();
                for (key, value) in over {
                    merged.insert(key.clone(), value.clone());
                }
                Some(serde_json::Value::Object(merged))
            }
            // A non-object on either side is not mergeable; the template's
            // value is the author's explicit instruction, so it wins whole.
            _ => Some(t.clone()),
        },
    }
}

fn wds_valuation_of(c: &meta::ProjectCriteria) -> serde_json::Value {
    serde_json::json!({
        "minPressure": c.min_pressure_m,
        "minResidual": c.min_residual_mg_l,
        "maxAge": c.max_age_h,
        "pressure": [c.pressure.low, c.pressure.required, c.pressure.high],
        "velocity": [c.velocity.low, c.velocity.target, c.velocity.high],
        // No `flow`: the criterion is retired. Flow is diverging, so it can
        // never band the map, and its band drove no block — sending the key
        // would send something nothing reads. The saved shape keeps the
        // field; §7.3 ignores keys the catalog does not declare.
    })
}

/// One analysis panel: a produced fragment, or why it could not be.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisBlockDto {
    pub id: String,
    pub title: String,
    /// Engine-authored grouping heading (common spec §3.2); the frontend
    /// derives its tab set from the categories present, in catalog order.
    pub category: String,
    /// `"ok"`, `"unavailable"`, or `"failed"`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fragment: Option<hydra::common::Fragment>,
}

/// One catalog block's production outcome as the panel DTO — the one place
/// a produced fragment is display-resolved and an error becomes a status.
fn analysis_block_dto(
    block: &hydra::common::BlockDescriptor,
    produced: Result<hydra::common::Fragment, hydra::common::BlockError>,
    settings: &hydra::report::DisplaySettings<'_>,
) -> AnalysisBlockDto {
    let (status, reason, fragment) = match produced {
        Ok(fragment) => (
            "ok",
            None,
            Some(hydra::report::resolve_fragment_display(&fragment, settings)),
        ),
        Err(hydra::common::BlockError::Unavailable { reason }) => {
            ("unavailable", Some(reason), None)
        }
        Err(err) => ("failed", Some(err.to_string()), None),
    };
    AnalysisBlockDto {
        id: block.id.to_string(),
        title: block.title.to_string(),
        category: block.category.to_string(),
        status,
        reason,
        fragment,
    }
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
    criteria: Option<serde_json::Value>,
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
            // The caller's valuation feeds the criteria-shaped blocks
            // (hydra-common §7.4); absent, the saved one applies. The
            // engine owns the mapping and the units.
            let valuation = criteria.or_else(|| saved_valuation(&app_data, &project_id, "uds"));
            let options_by_id = match &valuation {
                Some(v) => hydra::uds::report_blocks::criteria_block_options(v, &network)?,
                None => Default::default(),
            };
            Ok(hydra::uds::report_blocks::report_catalog()
                .iter()
                .map(|block| {
                    let options = options_by_id.get(block.id);
                    analysis_block_dto(
                        block,
                        produce_uds_block_from_file(block.id, &out_path, &network, options),
                        &settings,
                    )
                })
                .collect())
        }
        "wds" => {
            let network =
                network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
            let settings = hydra::report::DisplaySettings {
                family: display_family_for(
                    unit_system.as_deref(),
                    hydra::io::units::is_si(network.options.flow_units),
                ),
                catalog: hydra::descriptors::QUANTITIES,
            };
            // The caller's valuation feeds the criteria-shaped blocks, so
            // the page and the canvas judge by the same numbers. It
            // travels with the request — the frontend applies edits
            // locally and persists fire-and-forget, so a disk read here
            // could race an edit's own save. Absent (another caller), the
            // saved criteria apply, bridged into a valuation. The engine
            // owns the criteria→options mapping and its unit conversion
            // (analysis spec §5).
            let valuation = criteria.or_else(|| saved_valuation(&app_data, &project_id, "wds"));
            let options_by_id = match &valuation {
                Some(v) => hydra::criteria_block_options(v, &network)?,
                None => Default::default(),
            };
            Ok(hydra::report_catalog()
                .iter()
                .map(|block| {
                    let options = options_by_id.get(block.id);
                    analysis_block_dto(
                        block,
                        produce_wds_block_from_file(block.id, &out_path, &network, options),
                        &settings,
                    )
                })
                .collect())
        }
        other => Err(super::projects::unknown_engine(other)),
    }
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
            "No simulation results exist for this target. Run a simulation first".to_string(),
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
            hydra::swmm::out_reader::read_metadata(&out_path)?;
        }
        "wds" => {
            hydra::io::out_reader::read_metadata_checked(&out_path).map_err(|e| e.to_string())?;
        }
        other => return Err(super::projects::unknown_engine(other)),
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
            // The project's criteria are the report's default standard,
            // exactly as they are the Analysis page's — a block must not
            // judge by one standard on screen and another on export.
            let criteria_options = match saved_valuation(&app_data, project_id, "uds") {
                Some(v) => hydra::uds::report_blocks::criteria_block_options(&v, &network)?,
                None => Default::default(),
            };
            let document = assemble(
                template,
                hydra::uds::report_blocks::report_catalog(),
                context,
                |id, options| {
                    let merged = report_block_options_for(criteria_options.get(id), options);
                    produce_uds_block_from_file(id, &out_path, &network, merged.as_ref())
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
        "wds" => {
            let network = network_for_target(&app_data, state, project_id, scenario_id)?;
            let criteria_options = match saved_valuation(&app_data, project_id, "wds") {
                Some(v) => hydra::criteria_block_options(&v, &network)?,
                None => Default::default(),
            };
            let document = assemble(template, hydra::report_catalog(), context, |id, options| {
                let merged = report_block_options_for(criteria_options.get(id), options);
                produce_wds_block_from_file(id, &out_path, &network, merged.as_ref())
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
        other => return Err(super::projects::unknown_engine(other)),
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
/// The wds twin of [`produce_uds_block_from_file`], over the EPANET
/// dialect's results reader.
fn produce_wds_block_from_file(
    id: &str,
    out_path: &std::path::Path,
    network: &hydra::Network,
    options: Option<&serde_json::Value>,
) -> Result<hydra::common::Fragment, hydra::common::BlockError> {
    let src = hydra::io::out_reader::OutFileSource::open(out_path).map_err(|e| {
        hydra::common::BlockError::Failed {
            message: e.to_string(),
        }
    })?;
    hydra::produce_report_block(id, &src, network, options)
}

/// Produce one uds report block from a persisted results file: the
/// engine's blocks read data sources, and the file is this dialect's
/// (format-blind extraction). Opened per call, exactly as the block
/// producer itself used to read the metadata per call.
fn produce_uds_block_from_file(
    id: &str,
    out_path: &std::path::Path,
    network: &hydra::uds::model::Network,
    options: Option<&serde_json::Value>,
) -> Result<hydra::common::Fragment, hydra::common::BlockError> {
    let source = hydra::swmm::session::OutFileSource::open(out_path)
        .map_err(|message| hydra::common::BlockError::Failed { message })?;
    hydra::uds::report_blocks::produce_report_block(id, &source, network, options)
}

#[cfg(test)]
mod criteria_bridge_tests {
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

    /// The bridge produces exactly the valuation keys the engine catalogs
    /// (hydra-common §7.3): the saved wds shape maps field-for-field, and
    /// the frontend's `wdsValuation` mirrors this mapping — a
    /// cross-boundary pair, tested on each side.
    #[test]
    fn the_saved_shape_bridges_to_the_cataloged_valuation() {
        let v = wds_valuation_of(&criteria());
        let keys: std::collections::HashSet<&str> =
            v.as_object().unwrap().keys().map(String::as_str).collect();
        let cataloged: std::collections::HashSet<&str> =
            hydra::criteria_catalog().iter().map(|c| c.key).collect();
        assert_eq!(keys, cataloged, "bridge and catalog disagree on keys");
        assert_eq!(v["minPressure"], 14.0);
        assert_eq!(v["pressure"], serde_json::json!([24.0, 35.0, 45.0]));
        assert_eq!(v["velocity"], serde_json::json!([0.1, 0.5, 1.5]));
        // The saved shape still carries a flow band and the bridge drops
        // it, which the key comparison above already holds — stated here
        // too because it is the point of this change.
        assert!(v.get("flow").is_none());
    }

    /// The bridged valuation consumes through the engine identically to
    /// the pre-contract path: SI options pass through, and every shaped
    /// block sits in a criteria-bearing category (Compliance for the
    /// hydraulic criteria, Quality for the residual/age pair), so a
    /// criteria edit's visible effects are found on known tabs.
    #[test]
    fn bridged_criteria_shape_only_criteria_bearing_categories() {
        let options =
            hydra::criteria_block_options(&wds_valuation_of(&criteria()), &network("LPS"))
                .expect("options");
        assert!(!options.is_empty());
        assert_eq!(
            options["wds.service-compliance"]["minPressure"].as_f64(),
            Some(14.0)
        );
        assert_eq!(
            options["wds.quality-compliance"]["minResidual"].as_f64(),
            Some(0.2)
        );
        for id in options.keys() {
            let block = hydra::report_catalog()
                .iter()
                .find(|b| b.id == *id)
                .unwrap_or_else(|| panic!("{id} not in catalog"));
            assert!(
                block.category == "Compliance" || block.category == "Quality",
                "{id} sits in {:?}",
                block.category
            );
        }
    }

    /// A report with no template options judges by the project's criteria,
    /// not by the block's built-in defaults — the defect this merge exists
    /// for: the Analysis page applied criteria and the exported document
    /// did not, so one block told two stories.
    #[test]
    fn a_template_without_options_takes_the_criteria() {
        let criteria = serde_json::json!({ "minPressure": 20.0 });
        let merged = report_block_options_for(Some(&criteria), None).expect("options");
        assert_eq!(merged["minPressure"].as_f64(), Some(20.0));
    }

    /// A template that names an option keeps its own value — a report
    /// pinned to a fixed standard stays pinned.
    #[test]
    fn a_template_option_overrides_the_criterion() {
        let criteria = serde_json::json!({ "minPressure": 20.0 });
        let template = serde_json::json!({ "minPressure": 14.0 });
        let merged = report_block_options_for(Some(&criteria), Some(&template)).expect("options");
        assert_eq!(merged["minPressure"].as_f64(), Some(14.0));
    }

    /// The overlay is per key: pinning the row count must not silently
    /// unpin the pressure those rows are counted against.
    #[test]
    fn overriding_one_option_leaves_the_others_on_criteria() {
        let criteria = serde_json::json!({ "minPressure": 20.0 });
        let template = serde_json::json!({ "worstCount": 3 });
        let merged = report_block_options_for(Some(&criteria), Some(&template)).expect("options");
        assert_eq!(merged["minPressure"].as_f64(), Some(20.0));
        assert_eq!(merged["worstCount"].as_u64(), Some(3));
    }

    /// A block no criterion shapes passes its template options through,
    /// and a block with neither takes its own documented defaults.
    #[test]
    fn blocks_outside_the_criteria_are_untouched() {
        let template = serde_json::json!({ "topCount": 5 });
        assert_eq!(
            report_block_options_for(None, Some(&template)),
            Some(template.clone())
        );
        assert_eq!(report_block_options_for(None, None), None);
    }

    /// What the merge is ultimately for: for every criteria-shaped block,
    /// the options a report renders with equal the options the Analysis
    /// page produces with, when the template names none. Asserted through
    /// the engine's own mapping so it cannot drift from the engine.
    #[test]
    fn a_report_and_the_analysis_page_judge_by_the_same_numbers() {
        let network = network("LPS");
        let valuation = wds_valuation_of(&criteria());
        let analysis = hydra::criteria_block_options(&valuation, &network).expect("options");
        assert!(!analysis.is_empty());
        for (id, expected) in &analysis {
            let rendered = report_block_options_for(Some(expected), None).expect("options");
            assert_eq!(&rendered, expected, "{id} renders by a different standard");
        }
    }

    /// The uds criteria-shaped blocks likewise sit in one category, for
    /// the same one-tab reason.
    #[test]
    fn uds_criteria_shaped_blocks_share_the_network_category() {
        for id in [
            "uds.surcharge-summary",
            "uds.capacity-summary",
            "uds.velocity-thresholds",
        ] {
            let block = hydra::uds::report_blocks::report_catalog()
                .iter()
                .find(|b| b.id == id)
                .unwrap_or_else(|| panic!("{id} not in catalog"));
            assert_eq!(block.category, "Network", "{id}");
        }
    }
}
