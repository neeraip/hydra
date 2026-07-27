//! Offline-basemap commands: region listing/deletion, download planning
//! and execution with progress events, viewport coverage checks, and
//! project↔region links.
//!
//! Command handlers stay thin — everything stateful lives in
//! [`crate::basemap`] — so the logic under them is unit-tested without a
//! Tauri runtime. Downloads run on a worker thread (blocking range
//! requests) and report through `basemap:download` events, mirroring the
//! run-queue event pattern.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Serialize;
use tauri::Emitter;

use crate::basemap::{
    plan_extract, run_extract, tiles_in_bbox, Archive, BasemapState, HttpSource, RegionInfo,
    TileCoord,
};

/// Protomaps publishes daily planet builds; this pinned build is the
/// default source and can be overridden per download call.
const DEFAULT_PLANET_URL: &str = "https://build.protomaps.com/20260725.pmtiles";
/// Street-detail zoom window for region downloads (the world overview
/// region covers 0–6 when the user opts into it).
const REGION_MIN_ZOOM: u8 = 7;
const REGION_MAX_ZOOM: u8 = 15;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasemapStorageDto {
    pub regions: Vec<RegionInfo>,
    /// Bytes of downloaded tile data.
    pub data_bytes: u64,
    /// Actual bytes of the store on disk (db + WAL, incl. container overhead).
    pub disk_bytes: u64,
    pub unused_region_ids: Vec<String>,
}

#[tauri::command(async)]
pub fn list_basemap_regions(
    state: tauri::State<'_, BasemapState>,
) -> Result<BasemapStorageDto, String> {
    let store = state.store()?;
    Ok(BasemapStorageDto {
        regions: store.list_regions()?,
        data_bytes: store.data_bytes()?,
        disk_bytes: store.disk_bytes(),
        unused_region_ids: store.unused_regions()?,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreedDto {
    pub freed_bytes: u64,
}

#[tauri::command(async)]
pub fn delete_basemap_region(
    state: tauri::State<'_, BasemapState>,
    region_id: String,
) -> Result<FreedDto, String> {
    Ok(FreedDto {
        freed_bytes: state.store()?.delete_region(&region_id)?,
    })
}

#[tauri::command(async)]
pub fn link_project_basemap_region(
    state: tauri::State<'_, BasemapState>,
    project_id: String,
    region_id: String,
) -> Result<(), String> {
    state.store()?.link_project(&project_id, &region_id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPlanDto {
    /// Exact bytes a download would fetch.
    pub new_bytes: u64,
    /// Bytes already on disk the region would share.
    pub shared_bytes: u64,
    pub missing_tiles: u64,
    pub present_tiles: u64,
    /// In-bbox tiles the planet archive has no data for (open water).
    pub absent_tiles: u64,
}

/// Resolve the exact download cost of a bbox without fetching tile data.
#[tauri::command(async)]
pub fn plan_basemap_download(
    state: tauri::State<'_, BasemapState>,
    bbox: (f64, f64, f64, f64),
    archive_url: Option<String>,
) -> Result<DownloadPlanDto, String> {
    let store = state.store()?;
    let url = archive_url.as_deref().unwrap_or(DEFAULT_PLANET_URL);
    let mut archive = Archive::open(HttpSource::new(url)?)?;
    let plan = plan_extract(store, &mut archive, bbox, REGION_MIN_ZOOM, REGION_MAX_ZOOM)?;
    Ok(DownloadPlanDto {
        new_bytes: plan.new_bytes,
        shared_bytes: plan.shared_bytes,
        missing_tiles: plan.missing.len() as u64,
        present_tiles: plan.present.len() as u64,
        absent_tiles: plan.absent,
    })
}

/// Progress payload for `basemap:download` events.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadEvent {
    region_name: String,
    phase: &'static str,
    done_bytes: u64,
    total_bytes: u64,
    region_id: Option<String>,
    error: Option<String>,
}

/// Start a region download on a worker thread. Progress, completion, and
/// failure all arrive as `basemap:download` events; only one download
/// runs at a time.
#[tauri::command]
pub fn download_basemap_region(
    app: tauri::AppHandle,
    state: tauri::State<'_, BasemapState>,
    name: String,
    bbox: (f64, f64, f64, f64),
    project_id: Option<String>,
    archive_url: Option<String>,
) -> Result<(), String> {
    let cancel = Arc::new(AtomicBool::new(false));
    state.begin_download(cancel.clone())?;
    let url = archive_url.unwrap_or_else(|| DEFAULT_PLANET_URL.to_string());
    let planet_build = url
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".pmtiles"))
        .map(str::to_string);

    std::thread::spawn(move || {
        use tauri::Manager;
        let state = app.state::<BasemapState>();
        let emit = |payload: DownloadEvent| {
            if let Err(e) = app.emit("basemap:download", payload) {
                tracing::warn!("basemap download event emit failed: {e}");
            }
        };
        let event = |phase: &'static str, done: u64, total: u64| DownloadEvent {
            region_name: name.clone(),
            phase,
            done_bytes: done,
            total_bytes: total,
            region_id: None,
            error: None,
        };

        let result = (|| -> Result<(String, u64), String> {
            let store = state.store()?;
            let mut archive = Archive::open(HttpSource::new(&url)?)?;
            emit(event("planning", 0, 0));
            let plan = plan_extract(store, &mut archive, bbox, REGION_MIN_ZOOM, REGION_MAX_ZOOM)?;
            let total = plan.new_bytes;
            let region_id = run_extract(
                store,
                &mut archive,
                &plan,
                &name,
                planet_build.as_deref(),
                &mut |done, total| emit(event("downloading", done, total)),
                &cancel,
            )?;
            if let Some(project) = &project_id {
                store.link_project(project, &region_id)?;
            }
            Ok((region_id, total))
        })();

        state.end_download();
        match result {
            Ok((region_id, total)) => emit(DownloadEvent {
                region_name: name.clone(),
                phase: "complete",
                done_bytes: total,
                total_bytes: total,
                region_id: Some(region_id),
                error: None,
            }),
            Err(e) => emit(DownloadEvent {
                region_name: name.clone(),
                phase: if e == "cancelled" {
                    "cancelled"
                } else {
                    "failed"
                },
                done_bytes: 0,
                total_bytes: 0,
                region_id: None,
                error: (e != "cancelled").then_some(e),
            }),
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_basemap_download(state: tauri::State<'_, BasemapState>) {
    state.cancel_download();
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageDto {
    /// Present / total tiles for the viewport at the checked zoom.
    pub present_tiles: u64,
    pub total_tiles: u64,
    pub covered: bool,
}

/// Is the viewport covered by stored tiles at (clamped) `zoom`?
#[tauri::command(async)]
pub fn basemap_coverage(
    state: tauri::State<'_, BasemapState>,
    bbox: (f64, f64, f64, f64),
    zoom: f64,
) -> Result<CoverageDto, String> {
    let z = (zoom.floor().clamp(0.0, f64::from(REGION_MAX_ZOOM))) as u8;
    let coords: Vec<TileCoord> = tiles_in_bbox(bbox, z);
    let total = coords.len() as u64;
    let present = state.store()?.count_present(&coords)?;
    Ok(CoverageDto {
        present_tiles: present,
        total_tiles: total,
        covered: total > 0 && present == total,
    })
}

/// Project-deletion hook: release the project's region references so
/// affected regions surface as unused rather than lingering silently.
pub(crate) fn release_project_regions(app: &tauri::AppHandle, project_id: &str) {
    use tauri::Manager;
    let state = app.state::<BasemapState>();
    match state.store() {
        Ok(store) => {
            if let Err(e) = store.unlink_project(project_id) {
                tracing::warn!("failed to release basemap regions of {project_id}: {e}");
            }
        }
        Err(e) => tracing::warn!("basemap store unavailable during project delete: {e}"),
    }
}
