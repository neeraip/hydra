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

/// The report-block catalog of the project's engine. Single-engine today:
/// serves the wds catalog; with engine #2 this takes the project id and
/// dispatches on its engine key.
#[tauri::command]
pub fn list_report_blocks() -> &'static [hydra::common::BlockDescriptor] {
    hydra::report_catalog()
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
    let network = network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    Ok(hydra::report_catalog()
        .iter()
        .map(|block| {
            let (status, reason) =
                match hydra::produce_report_block(block.id, &out_path, &network, None) {
                    Ok(_) => ("ok", None),
                    Err(hydra::common::BlockError::Unavailable { reason }) => {
                        ("unavailable", Some(reason))
                    }
                    Err(err) => ("failed", Some(err.to_string())),
                };
            BlockAvailabilityDto {
                id: block.id.to_string(),
                status,
                reason,
            }
        })
        .collect())
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
    let network = network_for_target(&app_data, &state, &project_id, scenario_id.as_deref())?;
    Ok(hydra::report_block_options(&block_id, &network))
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
#[tauri::command(async)]
pub fn generate_report(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
    template_json: String,
    format: String,
    with_timestamp: bool,
) -> Result<String, String> {
    let template = ReportTemplate::from_json(&template_json).map_err(|e| e.to_string())?;
    render_for_target(
        &app,
        &state,
        &project_id,
        scenario_id.as_deref(),
        &template,
        &format,
        with_timestamp,
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
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let template = ReportTemplate::from_json(&template_json).map_err(|e| e.to_string())?;
    let rendered = render_for_target(
        &app,
        &state,
        &project_id,
        scenario_id.as_deref(),
        &template,
        &format,
        true,
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

fn render_for_target(
    app: &tauri::AppHandle,
    state: &NetworkState,
    project_id: &str,
    scenario_id: Option<&str>,
    template: &ReportTemplate,
    format: &str,
    with_timestamp: bool,
) -> Result<String, String> {
    validate_target_ids(project_id, scenario_id)?;
    let app_data = app_data_dir(app)?;
    let out_path = results_path_for(&app_data, project_id, scenario_id);
    if !out_path.exists() {
        return Err(
            "No simulation results exist for this target — run a simulation first".to_string(),
        );
    }
    let network = network_for_target(&app_data, state, project_id, scenario_id)?;

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

    let document = assemble(template, hydra::report_catalog(), context, |id, options| {
        hydra::produce_report_block(id, &out_path, &network, options)
    });
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
