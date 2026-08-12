//! Network mutation commands: single-field patches, structural create/delete
//! with index remapping, pattern/curve/control/rule editing, patch previews,
//! and network validation. Mutating commands emit `network-changed` while
//! still holding the `NetworkState` lock (see `NETWORK_CHANGED_EVENT`).

use serde::{Deserialize, Serialize};

use super::network_dto::{
    control_from_dto, curve_axes, format_read_error, link_to_dto, network_to_dto, node_to_dto,
    rule_from_dto, ControlDto, LinkDto, NetworkDto, NetworkState, NetworkStateInner, NodeDto,
    RuleDto, M3S_TO_LPS, M_TO_MM,
};
use super::projects::{app_data_dir, model_path_for, read_model_bytes, validate_target_ids};
use super::simulation::emit_or_warn;

/// Mutating commands emit this event *while still holding* the `NetworkState`
/// lock, so emission order always matches mutation commit order and a window
/// can never end up applying a stale delta that was emitted after a newer one.
/// This is safe: `tauri::Emitter::emit` only serialises the payload and posts
/// it to the webview — it never re-enters managed state, so no deadlock.
const NETWORK_CHANGED_EVENT: &str = "network-changed";

/// Apply a single field mutation to a `Network` in place. Shared between
/// `patch_elements` (which commits to state) and
/// `preview_patches` (dry-run, never touches state).
///
/// `kind`  — `"junction"` | `"reservoir"` | `"tank"` | `"pipe"` | `"pump"` | `"valve"`
/// `id`    — element ID as it appears in the INP
/// `field` — camelCase field name matching the frontend's display label
/// `value` — new value **in the same display units the frontend uses**:
///   • distances / elevations : metres  (m)
///   • flows / demands        : litres per second  (L/s)
///   • pipe/valve diameters   : millimetres  (mm)
///   • roughness / speed      : dimensionless number
///   • status                 : string `"Open"` | `"Closed"` | `"CV"` (pipes;
///     case-insensitive — CV marks the pipe as a check valve)
///   • curve / headPattern    : string ID
/// Set one axis of a node's `[COORDINATES]` entry, inserting a `(0, 0)`
/// entry first when the node has none yet. Shared by the junction /
/// reservoir / tank `"x"` / `"y"` arms of [`apply_patch_to_network`].
fn set_node_coordinate(network: &mut hydra::Network, id: &str, set_x: bool, value: f64) {
    let entry = network
        .coordinates
        .entry(id.to_string())
        .or_insert((0.0, 0.0));
    if set_x {
        entry.0 = value;
    } else {
        entry.1 = value;
    }
}

pub(crate) fn apply_patch_to_network(
    network: &mut hydra::Network,
    kind: &str,
    id: &str,
    field: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    // Display → internal. The engine stores SI, so lengths need no factor
    // at all; only the two quantities whose display unit differs from the
    // SI base unit do. These must stay the exact inverses of
    // `network_dto`'s outbound scales.
    const LPS_TO_M3S: f64 = 1.0 / M3S_TO_LPS;
    const MM_TO_M: f64 = 1.0 / M_TO_MM;

    let as_f64 = |v: &serde_json::Value| -> Result<f64, String> {
        v.as_f64()
            .ok_or_else(|| format!("expected number, got {v}"))
    };

    match kind {
        "junction" => {
            let node = network
                .nodes
                .iter_mut()
                .find(|n| n.base.id == id && matches!(n.kind, hydra::NodeKind::Junction(_)))
                .ok_or_else(|| format!("junction '{id}' not found"))?;
            match field {
                "elevation" => {
                    node.base.elevation = as_f64(&value)?;
                }
                "baseDemand" => {
                    if let hydra::NodeKind::Junction(ref mut j) = node.kind {
                        let demand_m3s = as_f64(&value)? * LPS_TO_M3S;
                        if let Some(first) = j.demands.first_mut() {
                            first.base_demand = demand_m3s;
                        } else {
                            j.demands.push(hydra::DemandCategory {
                                base_demand: demand_m3s,
                                pattern: None,
                                name: None,
                            });
                        }
                    }
                }
                "x" => set_node_coordinate(network, id, true, as_f64(&value)?),
                "y" => set_node_coordinate(network, id, false, as_f64(&value)?),
                other => return Err(format!("unknown junction field '{other}'")),
            }
        }
        "reservoir" => {
            let node = network
                .nodes
                .iter_mut()
                .find(|n| n.base.id == id && matches!(n.kind, hydra::NodeKind::Reservoir(_)))
                .ok_or_else(|| format!("reservoir '{id}' not found"))?;
            match field {
                "head" => {
                    node.base.elevation = as_f64(&value)?;
                }
                "headPattern" => {
                    if let hydra::NodeKind::Reservoir(ref mut r) = node.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        r.head_pattern = if s.is_empty() { None } else { Some(s) };
                    }
                }
                "x" => set_node_coordinate(network, id, true, as_f64(&value)?),
                "y" => set_node_coordinate(network, id, false, as_f64(&value)?),
                other => return Err(format!("unknown reservoir field '{other}'")),
            }
        }
        "tank" => {
            let node = network
                .nodes
                .iter_mut()
                .find(|n| n.base.id == id && matches!(n.kind, hydra::NodeKind::Tank(_)))
                .ok_or_else(|| format!("tank '{id}' not found"))?;
            match field {
                "elevation" => {
                    let new_bottom_m = as_f64(&value)?;
                    if let hydra::NodeKind::Tank(ref t) = node.kind {
                        node.base.elevation = new_bottom_m + t.min_level;
                    }
                }
                "minLevel" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        let old_min = t.min_level;
                        let new_min = as_f64(&value)?;
                        node.base.elevation = node.base.elevation - old_min + new_min;
                        t.min_level = new_min;
                    }
                }
                "maxLevel" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        t.max_level = as_f64(&value)?;
                    }
                }
                "initialLevel" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        t.initial_level = as_f64(&value)?;
                    }
                }
                "diameter" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        t.diameter = as_f64(&value)?;
                    }
                }
                "volumeCurve" => {
                    if let hydra::NodeKind::Tank(ref mut t) = node.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        t.volume_curve = if s.is_empty() { None } else { Some(s) };
                    }
                }
                "x" => set_node_coordinate(network, id, true, as_f64(&value)?),
                "y" => set_node_coordinate(network, id, false, as_f64(&value)?),
                other => return Err(format!("unknown tank field '{other}'")),
            }
        }
        "pipe" => {
            let link = network
                .links
                .iter_mut()
                .find(|l| l.base.id == id && matches!(l.kind, hydra::LinkKind::Pipe(_)))
                .ok_or_else(|| format!("pipe '{id}' not found"))?;
            if let hydra::LinkKind::Pipe(ref mut p) = link.kind {
                match field {
                    "length" => {
                        p.length = as_f64(&value)?;
                    }
                    "diameter" => {
                        let new_diam_m = as_f64(&value)? * MM_TO_M;
                        if p.minor_loss > 0.0 {
                            let old_d4 = p.diameter.powi(4);
                            let kv = p.minor_loss * old_d4 / 0.02517;
                            let new_d4 = new_diam_m.powi(4);
                            p.minor_loss = 0.02517 * kv / new_d4;
                        }
                        p.diameter = new_diam_m;
                    }
                    "roughness" => {
                        p.roughness = as_f64(&value)?;
                    }
                    "status" => {
                        let s = value
                            .as_str()
                            .ok_or_else(|| format!("expected string status, got {value}"))?;
                        // CV is modelled as `Pipe::check_valve` with an Open
                        // initial status (mirroring the INP reader); plain
                        // open/closed clears the CV flag so the INP writer —
                        // which emits "CV" for any check-valve pipe — round-
                        // trips whichever status was last patched.
                        let (status, check_valve) = match s.to_ascii_lowercase().as_str() {
                            "open" => (hydra::LinkStatus::Open, false),
                            "closed" => (hydra::LinkStatus::Closed, false),
                            "cv" => (hydra::LinkStatus::Open, true),
                            _ => return Err(format!("unknown pipe status '{s}'")),
                        };
                        link.base.initial_status = status;
                        p.check_valve = check_valve;
                    }
                    other => return Err(format!("unknown pipe field '{other}'")),
                }
            }
        }
        "pump" => {
            let link = network
                .links
                .iter_mut()
                .find(|l| l.base.id == id && matches!(l.kind, hydra::LinkKind::Pump(_)))
                .ok_or_else(|| format!("pump '{id}' not found"))?;
            match field {
                "speed" => {
                    link.base.initial_setting = Some(as_f64(&value)?);
                }
                "curve" => {
                    if let hydra::LinkKind::Pump(ref mut p) = link.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        p.head_curve = if s.is_empty() { None } else { Some(s) };
                        // Curve and constant power are mutually exclusive.
                        if p.head_curve.is_some() {
                            p.power = None;
                        }
                    }
                }
                "powerKw" => {
                    if let hydra::LinkKind::Pump(ref mut p) = link.kind {
                        // power is stored in Watts; input is kW
                        p.power = Some(as_f64(&value)? * 1000.0);
                        // Constant power replaces the head curve.
                        p.head_curve = None;
                    }
                }
                other => return Err(format!("unknown pump field '{other}'")),
            }
        }
        "valve" => {
            let link = network
                .links
                .iter_mut()
                .find(|l| l.base.id == id && matches!(l.kind, hydra::LinkKind::Valve(_)))
                .ok_or_else(|| format!("valve '{id}' not found"))?;
            match field {
                "diameter" => {
                    if let hydra::LinkKind::Valve(ref mut v) = link.kind {
                        v.diameter = as_f64(&value)? * MM_TO_M;
                    }
                }
                "valveType" => {
                    let s = value.as_str().unwrap_or("").to_ascii_uppercase();
                    if let hydra::LinkKind::Valve(ref mut v) = link.kind {
                        v.valve_type = match s.as_str() {
                            "PRV" => hydra::ValveType::Prv,
                            "PSV" => hydra::ValveType::Psv,
                            "FCV" => hydra::ValveType::Fcv,
                            "TCV" => hydra::ValveType::Tcv,
                            "GPV" => hydra::ValveType::Gpv,
                            "PCV" => hydra::ValveType::Pcv,
                            "PBV" => hydra::ValveType::Pbv,
                            other => return Err(format!("unknown valve type '{other}'")),
                        };
                    }
                }
                "valveSetting" => {
                    let raw = as_f64(&value)?;
                    // Read the valve type before taking a mutable borrow on link.kind.
                    let vt = if let hydra::LinkKind::Valve(ref v) = link.kind {
                        v.valve_type
                    } else {
                        unreachable!()
                    };
                    link.base.initial_setting = Some(match vt {
                        hydra::ValveType::Prv | hydra::ValveType::Psv | hydra::ValveType::Pbv => {
                            raw
                        }
                        hydra::ValveType::Fcv => raw * LPS_TO_M3S,
                        _ => raw,
                    });
                }
                "valveCurve" => {
                    if let hydra::LinkKind::Valve(ref mut v) = link.kind {
                        let s = value.as_str().unwrap_or("").trim().to_string();
                        v.curve = if s.is_empty() { None } else { Some(s) };
                    }
                }
                other => return Err(format!("unknown valve field '{other}'")),
            }
        }
        other => return Err(format!("unknown element kind '{other}'")),
    }
    Ok(())
}

/// One updated element — exactly one of `node` / `link` is set. Also used as the entry type of the `network-changed` event's
/// delta payload so every window can update the element in place instead of
/// refetching the full snapshot.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchedElementDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<LinkDto>,
}

/// Payload for the `network-changed` event.
///
/// `elements` lists the updated element DTOs when the mutation was limited to
/// known elements (`patch_elements` / `patch_node_position`); the frontend patches its local arrays in place.
/// Structural mutations (create/delete/pattern/curve/control commands) emit a
/// `null` payload, which the frontend treats as "refetch the full snapshot".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkChangedPayload {
    pub elements: Vec<PatchedElementDto>,
}

/// Rebuild the DTO of the single element identified by `kind`/`id` in place
/// inside the cached `NetworkDto`, and return a copy of the updated DTO.
///
/// O(nodes + links) for the lookup only — no full 2×46k DTO rebuild.
fn refresh_element_dto(
    network: &hydra::Network,
    dto: &mut NetworkDto,
    kind: &str,
    id: &str,
) -> Result<PatchedElementDto, String> {
    match kind {
        "junction" | "reservoir" | "tank" => {
            let node = network
                .nodes
                .iter()
                .find(|n| n.base.id == id)
                .ok_or_else(|| format!("node '{id}' not found"))?;
            let updated = node_to_dto(network, node);
            match dto.nodes.iter_mut().find(|n| n.id == id) {
                Some(slot) => *slot = updated.clone(),
                None => dto.nodes.push(updated.clone()),
            }
            Ok(PatchedElementDto {
                node: Some(updated),
                link: None,
            })
        }
        "pipe" | "pump" | "valve" => {
            let link = network
                .links
                .iter()
                .find(|l| l.base.id == id)
                .ok_or_else(|| format!("link '{id}' not found"))?;
            // `nodes[i].base.index == i + 1` is a model invariant (see
            // `Network`), which the solver already indexes on and every
            // mutation here restores after a delete. Scanning for the index
            // instead cost a walk of all 46k nodes twice per patched link,
            // which a bulk edit pays for once per link it touches.
            let node_id_of = |idx: usize| {
                network
                    .nodes
                    .get(idx.wrapping_sub(1))
                    .map(|n| n.base.id.clone())
                    .unwrap_or_default()
            };
            let mut updated = link_to_dto(
                link,
                node_id_of(link.base.from_node),
                node_id_of(link.base.to_node),
            );
            let status_code = super::network_dto::link_initial_status_code(link);
            // The frontend replaces its link object wholesale with this DTO,
            // so the delta must be shape-complete: attach the fields the full
            // snapshot ships through its dedicated binary columns (vertices,
            // pipe initial status), mirroring `decodeNetworkSnapshot`'s
            // object shape (vertices omitted when empty; status pipes-only).
            updated.vertices = network.vertices.get(id).filter(|v| !v.is_empty()).cloned();
            if matches!(link.kind, hydra::LinkKind::Pipe(_)) {
                updated.initial_status =
                    Some(super::network_dto::link_initial_status_str(status_code).to_string());
            }
            match dto.links.iter().position(|l| l.id == id) {
                Some(pos) => {
                    dto.links[pos] = updated.clone();
                    // Keep the snapshot's parallel initial-status column in
                    // sync — a pipe "status" patch changes it without a full
                    // DTO rebuild. Missing entries are tolerated (the encoder
                    // defaults them to 0), matching `link_vertices`.
                    if let Some(slot) = dto.link_initial_status.get_mut(pos) {
                        *slot = status_code;
                    }
                }
                None => {
                    dto.links.push(updated.clone());
                    dto.link_initial_status.push(status_code);
                }
            }
            Ok(PatchedElementDto {
                node: None,
                link: Some(updated),
            })
        }
        other => Err(format!("unknown element kind '{other}'")),
    }
}

/// Apply a structural mutation to the loaded network: run `f` on it, then
/// mark the state dirty and rebuild the full cached `NetworkDto`.
///
/// Returns `Err("no network loaded")` when the state is empty, and `f`'s
/// error — with nothing marked dirty and the DTO untouched — when the
/// mutation fails. Kept free of Tauri types so it is unit-testable; commands
/// go through [`mutate_structural`], which adds the lock + event emission.
fn apply_structural_mutation<F>(inner: &mut NetworkStateInner, f: F) -> Result<(), String>
where
    F: FnOnce(&mut hydra::Network) -> Result<(), String>,
{
    match inner {
        NetworkStateInner::Loaded {
            dirty,
            network,
            dto,
            ..
        } => {
            // `make_mut` copies only while another reader still holds the
            // previous version; the common case is an in-place mutation.
            f(std::sync::Arc::make_mut(network))?;
            *dirty = true;
            *dto = network_to_dto(network);
            Ok(())
        }
        NetworkStateInner::LoadedUds { .. } => Err(
            "This project's engine is read-only in the GUI — editing is not available yet.".into(),
        ),
        NetworkStateInner::Empty => Err("no network loaded".into()),
    }
}

/// Apply a mutation to a loaded drainage network, marking it dirty.
///
/// Separate from [`apply_structural_mutation`] rather than folded into
/// it: the two engines' networks are different types, and the drainage
/// canvas reads a snapshot instead of the wds DTO, so there is no DTO to
/// rebuild here.
fn apply_uds_mutation<F>(inner: &mut NetworkStateInner, f: F) -> Result<(), String>
where
    F: FnOnce(&mut hydra::uds::model::Network) -> Result<(), String>,
{
    match inner {
        NetworkStateInner::LoadedUds { dirty, network, .. } => {
            f(std::sync::Arc::make_mut(network))?;
            *dirty = true;
            Ok(())
        }
        NetworkStateInner::Loaded { .. } => Err("this command is for drainage models".into()),
        NetworkStateInner::Empty => Err("no network loaded".into()),
    }
}

/// Rename a drainage element, following the name everywhere it is used.
///
/// A drainage model refers to an element by name in two places the model
/// itself does not resolve: the preserved display sections, and the
/// retained control-rule text. Its `[REPORT]` selections need nothing —
/// they hold indices, not names — which is worth stating because the
/// water-distribution engine's hold names and do.
fn rename_uds_element(
    net: &mut hydra::uds::model::Network,
    old_id: &str,
    new_id: &str,
) -> Result<(), String> {
    if new_id == old_id {
        return Ok(());
    }
    // Vertices, links and parcels share one namespace here: the reader
    // registers them in one table, so a duplicate across kinds is a
    // duplicate.
    let taken = net
        .vertices
        .iter()
        .any(|v| v.id.eq_ignore_ascii_case(new_id))
        || net.links.iter().any(|l| l.id.eq_ignore_ascii_case(new_id))
        || net
            .parcels
            .iter()
            .any(|p| p.id.eq_ignore_ascii_case(new_id));
    if taken {
        return Err(format!("ID '{new_id}' is already in use"));
    }
    let found = if let Some(v) = net
        .vertices
        .iter_mut()
        .find(|v| v.id.eq_ignore_ascii_case(old_id))
    {
        v.id = new_id.to_string();
        true
    } else if let Some(l) = net
        .links
        .iter_mut()
        .find(|l| l.id.eq_ignore_ascii_case(old_id))
    {
        l.id = new_id.to_string();
        true
    } else if let Some(p) = net
        .parcels
        .iter_mut()
        .find(|p| p.id.eq_ignore_ascii_case(old_id))
    {
        p.id = new_id.to_string();
        true
    } else {
        false
    };
    if !found {
        return Err(format!("element '{old_id}' not found"));
    }
    super::uds_view::rename_in_display(net, old_id, new_id);
    super::uds_view::rename_in_controls(net, old_id, new_id);
    Ok(())
}

/// Command wrapper for structural mutations (create/delete/pattern/curve/
/// control/rule commands): applies [`apply_structural_mutation`] and, on
/// success, emits the structural `network-changed` event (payload-less →
/// `null` on the frontend, triggering a full snapshot refetch). The state
/// lock is held across the emit (see `NETWORK_CHANGED_EVENT`) so event order
/// always matches mutation commit order.
fn mutate_structural<F>(app: &tauri::AppHandle, state: &NetworkState, f: F) -> Result<(), String>
where
    F: FnOnce(&mut hydra::Network) -> Result<(), String>,
{
    let mut guard = state.0.lock();
    let result = apply_structural_mutation(&mut guard, f);
    if result.is_ok() {
        emit_or_warn(app, NETWORK_CHANGED_EVENT, ());
    }
    drop(guard);
    result
}

/// Result of a bulk `patch_elements` call: per-item failures are collected
/// instead of aborting the batch, mirroring the frontend's previous
/// one-command-per-field error accounting.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchElementsResult {
    /// Number of patches applied successfully.
    pub applied: u32,
    /// Human-readable error strings for the patches that failed.
    pub errors: Vec<String>,
}

#[tauri::command(async)]
/// Apply a batch of property edits in one IPC call: one lock acquisition, one
/// dirty-flag set, one `network-changed` event — instead of one full
/// command round-trip (and formerly one INP re-serialisation) per field.
pub fn patch_elements(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    patches: Vec<PatchItem>,
) -> Result<PatchElementsResult, String> {
    // Lock held across the emit below (see `NETWORK_CHANGED_EVENT`).
    let mut guard = state.0.lock();
    let (result, elements) =
        {
            match &mut *guard {
                NetworkStateInner::Loaded {
                    dirty,
                    network,
                    dto,
                    ..
                } => {
                    let mut applied = 0u32;
                    let mut errors = Vec::new();
                    // Unique (kind, id) pairs of successfully patched elements,
                    // in first-touched order.
                    let mut touched: Vec<(String, String)> = Vec::new();
                    for patch in patches {
                        match apply_patch_to_network(
                            std::sync::Arc::make_mut(network),
                            &patch.kind,
                            &patch.id,
                            &patch.field,
                            patch.value,
                        ) {
                            Ok(()) => {
                                applied += 1;
                                *dirty = true;
                                if !touched
                                    .iter()
                                    .any(|(k, i)| *k == patch.kind && *i == patch.id)
                                {
                                    touched.push((patch.kind, patch.id));
                                }
                            }
                            Err(e) => errors.push(e),
                        }
                    }
                    let mut elements = Vec::with_capacity(touched.len());
                    for (kind, id) in &touched {
                        if let Ok(el) = refresh_element_dto(network, dto, kind, id) {
                            elements.push(el);
                        }
                    }
                    (PatchElementsResult { applied, errors }, elements)
                }
                NetworkStateInner::LoadedUds { .. } => return Err(
                    "This project's engine is read-only in the GUI — editing is not available yet."
                        .into(),
                ),
                NetworkStateInner::Empty => return Err("no network loaded".into()),
            }
        };
    if !elements.is_empty() {
        emit_or_warn(
            &app,
            NETWORK_CHANGED_EVENT,
            NetworkChangedPayload { elements },
        );
    }
    drop(guard);
    Ok(result)
}

/// Move a node to a new coordinate position in a single write (avoids two
/// serial coordinate patches and two INP re-serialisations). Fails when
/// `id` names no existing node.
#[tauri::command(async)]
pub fn patch_node_position(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    // Lock held across the emit below (see `NETWORK_CHANGED_EVENT`).
    let mut guard = state.0.lock();
    let result = {
        match &mut *guard {
            NetworkStateInner::Loaded {
                dirty,
                network,
                dto,
                ..
            } => {
                // Reject unknown ids instead of silently inserting an orphan
                // `[COORDINATES]` entry (and dirtying the model) for a node
                // that does not exist.
                if !network.nodes.iter().any(|n| n.base.id == id) {
                    return Err(format!("node '{id}' not found"));
                }
                let entry = std::sync::Arc::make_mut(network)
                    .coordinates
                    .entry(id.clone())
                    .or_insert((0.0, 0.0));
                entry.0 = x;
                entry.1 = y;
                let mut moved: Option<NodeDto> = None;
                if let Some(node) = dto.nodes.iter_mut().find(|n| n.id == id) {
                    node.x = x;
                    node.y = y;
                    moved = Some(node.clone());
                }
                *dirty = true;
                Ok(moved)
            }
            NetworkStateInner::LoadedUds { dirty, network, .. } => {
                // The same refusal the wds arm makes, for the same reason:
                // an unknown id would append a display line naming nothing
                // and dirty the model to do it.
                if !network.vertices.iter().any(|v| v.id == id) {
                    return Err(format!("node '{id}' not found"));
                }
                // A drainage node's position is not a field on the node —
                // it is a line in a preserved display section (§14.5), so
                // moving one is a text edit rather than an assignment.
                super::uds_view::set_display_point(
                    std::sync::Arc::make_mut(network),
                    "[COORDINATES]",
                    &id,
                    x,
                    y,
                );
                *dirty = true;
                // No delta: this engine's canvas reads a snapshot rather
                // than the wds DTO, so the frontend refetches.
                Ok(None)
            }
            NetworkStateInner::Empty => Err("no network loaded".into()),
        }
    };
    match result {
        Ok(moved) => {
            // Node present in the cached DTO: emit a delta so the frontend
            // patches in place. Node in the network but missing from the DTO
            // (cache out of sync — should not happen): emit a payload-less
            // event so the frontend falls back to a full refetch.
            match moved {
                Some(node) => {
                    emit_or_warn(
                        &app,
                        NETWORK_CHANGED_EVENT,
                        NetworkChangedPayload {
                            elements: vec![PatchedElementDto {
                                node: Some(node),
                                link: None,
                            }],
                        },
                    );
                }
                None => {
                    emit_or_warn(&app, NETWORK_CHANGED_EVENT, ());
                }
            }
            drop(guard);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// Rewrite a `[REPORT] NODES`/`LINKS` selection through `keep`, which
/// returns the id an entry becomes or `None` to drop it.
///
/// These are the model's other id-keyed references to elements, and unlike
/// coordinates or tags they are a plain list rather than a map, which is
/// why they were easy to miss: nothing fails when they go stale. A
/// selection left naming a renamed or deleted element writes an id into the
/// saved file that resolves to nothing, and the element the user renamed
/// quietly stops appearing in its own report.
///
/// A selection emptied by deletions becomes `None` rather than an empty
/// list — `None` is what "report no items" means, and an empty list would
/// write a bare `NODES` line.
fn retarget_report_selection(
    selection: &mut hydra::ReportSelection,
    keep: impl Fn(&str) -> Option<String>,
) {
    let hydra::ReportSelection::Some(ids) = selection else {
        // `All` and `None` name no element, so nothing can go stale.
        return;
    };
    let kept: Vec<String> = ids.iter().filter_map(|id| keep(id)).collect();
    *selection = if kept.is_empty() {
        hydra::ReportSelection::None
    } else {
        hydra::ReportSelection::Some(kept)
    };
}

/// Names of controls/rules that reference any of the given (old, 1-based)
/// node or link indices — used to block deletion of a still-referenced
/// element, mirroring `delete_curve`/`delete_pattern`'s safety check.
fn control_rule_refs(
    network: &hydra::Network,
    node_idx: &[usize],
    link_idx: &[usize],
) -> Vec<String> {
    let mut refs = Vec::new();
    for (i, ctrl) in network.controls.iter().enumerate() {
        let hits_link = link_idx.contains(&ctrl.link);
        let hits_node = ctrl.trigger_node.is_some_and(|n| node_idx.contains(&n));
        if hits_link || hits_node {
            refs.push(format!("Control #{}", i + 1));
        }
    }
    for (i, rule) in network.rules.iter().enumerate() {
        let mut hit = false;
        for p in &rule.premises {
            match p.object {
                hydra::PremiseObject::Node(idx) => {
                    if node_idx.contains(&idx) {
                        hit = true;
                    }
                }
                hydra::PremiseObject::Link(idx) => {
                    if link_idx.contains(&idx) {
                        hit = true;
                    }
                }
                hydra::PremiseObject::Clock => {}
            }
        }
        for a in rule.then_actions.iter().chain(rule.else_actions.iter()) {
            if link_idx.contains(&a.link) {
                hit = true;
            }
        }
        if hit {
            refs.push(format!("Rule R{}", i + 1));
        }
    }
    refs
}

/// Remap a 1-based index after the elements at `removed` (old 1-based
/// indices) have been removed from the vec it addresses.
fn remap_index(old: usize, removed: &[usize]) -> usize {
    let shift = removed.iter().filter(|&&r| r < old).count();
    old - shift
}

/// Fix up every control/rule's node/link index references after node(s)
/// and/or link(s) at the given old 1-based indices have been removed.
fn remap_controls_rules(
    network: &mut hydra::Network,
    removed_nodes: &[usize],
    removed_links: &[usize],
) {
    for ctrl in network.controls.iter_mut() {
        ctrl.link = remap_index(ctrl.link, removed_links);
        if let Some(n) = ctrl.trigger_node {
            ctrl.trigger_node = Some(remap_index(n, removed_nodes));
        }
    }
    for rule in network.rules.iter_mut() {
        for p in rule.premises.iter_mut() {
            match &mut p.object {
                hydra::PremiseObject::Node(idx) => *idx = remap_index(*idx, removed_nodes),
                hydra::PremiseObject::Link(idx) => *idx = remap_index(*idx, removed_links),
                hydra::PremiseObject::Clock => {}
            }
        }
        for a in rule
            .then_actions
            .iter_mut()
            .chain(rule.else_actions.iter_mut())
        {
            a.link = remap_index(a.link, removed_links);
        }
    }
}

/// Remove a node or link from `network` (see [`delete_element`] for the full
/// contract). Extracted from the command so the deletion/index-remap logic is
/// unit-testable without an `AppHandle`.
fn delete_element_from_network(
    network: &mut hydra::Network,
    kind: &str,
    id: &str,
) -> Result<(), String> {
    match kind {
        "junction" | "reservoir" | "tank" => {
            let pos = network
                .nodes
                .iter()
                .position(|n| n.base.id == id)
                .ok_or_else(|| format!("node '{}' not found", id))?;
            let node_1based = pos + 1;
            // Collect + remove dangling links that reference this node.
            let dangling: Vec<(String, usize)> = network
                .links
                .iter()
                .filter(|l| l.base.from_node == node_1based || l.base.to_node == node_1based)
                .map(|l| (l.base.id.clone(), l.base.index))
                .collect();
            let dangling_idx: Vec<usize> = dangling.iter().map(|(_, idx)| *idx).collect();

            let refs = control_rule_refs(network, &[node_1based], &dangling_idx);
            if !refs.is_empty() {
                return Err(format!(
                    "node '{}' is still attached to {}; detach it first",
                    id,
                    refs.join(", ")
                ));
            }

            for (lid, _) in &dangling {
                network.vertices.remove(lid);
                network.link_tags.remove(lid);
            }
            network
                .links
                .retain(|l| l.base.from_node != node_1based && l.base.to_node != node_1based);
            // Remove the node itself.
            network.nodes.remove(pos);
            network.coordinates.remove(id);
            network.node_tags.remove(id);
            // Rebuild node indices and fix up link from/to references.
            for (i, n) in network.nodes.iter_mut().enumerate() {
                n.base.index = i + 1;
            }
            for l in network.links.iter_mut() {
                // from_node and to_node are 1-based; shift down if they
                // referred to a node that was after the deleted one.
                if l.base.from_node > node_1based {
                    l.base.from_node -= 1;
                }
                if l.base.to_node > node_1based {
                    l.base.to_node -= 1;
                }
            }
            // Rebuild link indices too: the cascade `retain` above leaves
            // gaps, and a stale `base.index` on a surviving link would
            // corrupt the next delete's control/rule guard + remap and let
            // `create_link` (which uses `links.len() + 1`) mint duplicates.
            for (i, l) in network.links.iter_mut().enumerate() {
                l.base.index = i + 1;
            }
            remap_controls_rules(network, &[node_1based], &dangling_idx);
            retarget_report_selection(&mut network.report.nodes, |r| {
                (r != id).then(|| r.to_string())
            });
            let gone: Vec<&str> = dangling.iter().map(|(lid, _)| lid.as_str()).collect();
            retarget_report_selection(&mut network.report.links, |r| {
                (!gone.contains(&r)).then(|| r.to_string())
            });
        }
        "pipe" | "pump" | "valve" => {
            let pos = network
                .links
                .iter()
                .position(|l| l.base.id == id)
                .ok_or_else(|| format!("link '{}' not found", id))?;
            let link_1based = pos + 1;

            let refs = control_rule_refs(network, &[], &[link_1based]);
            if !refs.is_empty() {
                return Err(format!(
                    "link '{}' is still attached to {}; detach it first",
                    id,
                    refs.join(", ")
                ));
            }

            network.links.remove(pos);
            network.vertices.remove(id);
            network.link_tags.remove(id);
            // Rebuild link indices.
            for (i, l) in network.links.iter_mut().enumerate() {
                l.base.index = i + 1;
            }
            remap_controls_rules(network, &[], &[link_1based]);
            retarget_report_selection(&mut network.report.links, |r| {
                (r != id).then(|| r.to_string())
            });
        }
        other => return Err(format!("unknown element kind '{}'", other)),
    }
    Ok(())
}

/// Remove a node or link from the in-memory network.
///
/// `kind` must be one of `"junction"`, `"reservoir"`, `"tank"`, `"pipe"`,
/// `"pump"`, or `"valve"`.  The element is removed from the relevant vec,
/// from all ancillary maps (`coordinates`, `vertices`, `node_tags`,
/// `link_tags`), and from the `[REPORT]` selection that named it.
/// Any links that reference a deleted node are also removed (dangling links),
/// and they leave the `[REPORT] LINKS` selection with them.
/// All node *and* link `base.index` values are rebuilt after deletion so the
/// INP writer produces a valid file and later index-based operations
/// (control/rule guards, `create_link`) see contiguous indices.
///
/// Fails without mutating anything if the node/link (or, for nodes, any link
/// that would be cascade-removed with it) is still referenced by a control
/// or rule — the reference must be cleared first. Every surviving control's
/// and rule's node/link index references are remapped afterward so they
/// keep pointing at the correct element once indices shift.
#[tauri::command(async)]
pub fn delete_element(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    id: String,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        delete_element_from_network(network, &kind, &id)
    })
}

/// Validate a user-supplied element/curve ID and return it trimmed.
///
/// Rejects empty IDs and any that contain whitespace, `;` (the INP comment
/// character), or quotes — all of which would break INP tokenisation on the
/// next round-trip. `what` names the thing being renamed for the error text.
fn validate_inp_id(raw: &str, what: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{what} ID must not be empty"));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(format!("{what} ID must not contain spaces"));
    }
    if trimmed.contains([';', '"', '\'']) {
        return Err(format!("{what} ID must not contain ; or quotes"));
    }
    Ok(trimmed.to_string())
}

/// Rename a node or link in place, cascading the new ID everywhere the model
/// keys on the old string ID. See [`rename_element`] for the full contract.
/// Extracted from the command so the cascade is unit-testable without an
/// `AppHandle`. `new_id` is assumed already validated by [`validate_inp_id`].
fn rename_element_in_network(
    network: &mut hydra::Network,
    kind: &str,
    old_id: &str,
    new_id: &str,
) -> Result<(), String> {
    // EPANET keeps node IDs and link IDs in SEPARATE namespaces — a node and a
    // link may legitimately share an ID (common with numeric IDs). So a node
    // rename only conflicts with other nodes, a link rename only with other
    // links; checking across both would wrongly reject reusing an ID that the
    // other namespace happens to hold.
    match kind {
        "junction" | "reservoir" | "tank" => {
            if !network.nodes.iter().any(|n| n.base.id == old_id) {
                return Err(format!("node '{old_id}' not found"));
            }
            if new_id == old_id {
                return Ok(());
            }
            if network.nodes.iter().any(|n| n.base.id == new_id) {
                return Err(format!("ID '{new_id}' is already in use by another node"));
            }
            for n in network.nodes.iter_mut() {
                if n.base.id == old_id {
                    n.base.id = new_id.to_string();
                }
            }
            // Re-key the id-keyed side tables. Links/controls/rules reference
            // nodes by *index*, so they need no rewrite; only these maps and
            // the quality trace node key on the string ID.
            if let Some(v) = network.coordinates.remove(old_id) {
                network.coordinates.insert(new_id.to_string(), v);
            }
            if let Some(v) = network.node_tags.remove(old_id) {
                network.node_tags.insert(new_id.to_string(), v);
            }
            if network.options.trace_node.as_deref() == Some(old_id) {
                network.options.trace_node = Some(new_id.to_string());
            }
            retarget_report_selection(&mut network.report.nodes, |id| {
                Some(if id == old_id { new_id } else { id }.to_string())
            });
        }
        "pipe" | "pump" | "valve" => {
            if !network.links.iter().any(|l| l.base.id == old_id) {
                return Err(format!("link '{old_id}' not found"));
            }
            if new_id == old_id {
                return Ok(());
            }
            if network.links.iter().any(|l| l.base.id == new_id) {
                return Err(format!("ID '{new_id}' is already in use by another link"));
            }
            for l in network.links.iter_mut() {
                if l.base.id == old_id {
                    l.base.id = new_id.to_string();
                }
            }
            // Endpoints (from/to), controls, and rules reference links by
            // index; only the vertices and tags maps key on the link ID.
            if let Some(v) = network.vertices.remove(old_id) {
                network.vertices.insert(new_id.to_string(), v);
            }
            if let Some(v) = network.link_tags.remove(old_id) {
                network.link_tags.insert(new_id.to_string(), v);
            }
            retarget_report_selection(&mut network.report.links, |id| {
                Some(if id == old_id { new_id } else { id }.to_string())
            });
        }
        other => return Err(format!("unknown element kind '{other}'")),
    }
    Ok(())
}

/// Rename a node or link, cascading the new ID to every place the model keys
/// on the old string ID.
///
/// `kind` is one of `"junction"`/`"reservoir"`/`"tank"` (nodes) or
/// `"pipe"`/`"pump"`/`"valve"` (links). Because the engine references links'
/// endpoints, controls, and rules by *index* (not string ID), those need no
/// rewrite; the cascade only re-keys the id-keyed side tables:
/// - Node: `base.id`, `[COORDINATES]`, `[TAGS]`, and `options.trace_node`.
/// - Link: `base.id`, `[VERTICES]`, `[TAGS]`.
///
/// `new_id` must be non-empty, contain no whitespace/`;`/quotes, and be unique
/// across all nodes and links. Fails without mutating anything on any
/// violation. Renaming to the current ID is a no-op success.
#[tauri::command(async)]
pub fn rename_element(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    old_id: String,
    new_id: String,
) -> Result<(), String> {
    let new_id = validate_inp_id(&new_id, "element")?;
    // Drainage renames route to their own path: a different network type,
    // and different places the old name is written down.
    {
        let mut guard = state.0.lock();
        if matches!(&*guard, NetworkStateInner::LoadedUds { .. }) {
            apply_uds_mutation(&mut guard, |network| {
                rename_uds_element(network, &old_id, &new_id)
            })?;
            emit_or_warn(&app, NETWORK_CHANGED_EVENT, ());
            return Ok(());
        }
    }
    mutate_structural(&app, &state, |network| {
        rename_element_in_network(network, &kind, &old_id, &new_id)
    })
}

/// Add a new node (junction, tank, or reservoir) to the in-memory network.
///
/// `id` must be unique across all nodes and links.  `x` / `y` are geographic
/// coordinates (longitude / latitude in WGS-84) stored directly in
/// `[COORDINATES]`.  Sensible hydraulic defaults are used for all
/// type-specific fields so the resulting network is immediately parseable.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn create_node(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    id: String,
    x: f64,
    y: f64,
    elevation: Option<f64>,
    min_level: Option<f64>,
    max_level: Option<f64>,
    initial_level: Option<f64>,
) -> Result<(), String> {
    let elev_m = elevation.unwrap_or(0.0);
    mutate_structural(&app, &state, |network| {
        let id = validate_inp_id(&id, "element")?;
        // Node ids are unique among nodes only — a link may share the id
        // (EPANET keeps node and link namespaces separate; the parser accepts
        // it), so do not reject an id merely because a link holds it.
        if network.nodes.iter().any(|n| n.base.id == id) {
            return Err(format!("ID '{}' is already in use by another node", id));
        }
        let index = network.nodes.len() + 1;
        // Tank level defaults: ~3 m min gap, ~1.5 m initial (matching original 10 ft / 5 ft).
        let min_m = min_level.unwrap_or(0.0);
        let max_m = max_level.unwrap_or(10.0);
        let init_m = initial_level.unwrap_or(5.0);
        let node_kind = match kind.as_str() {
            "junction" => hydra::NodeKind::Junction(hydra::Junction {
                demands: vec![hydra::DemandCategory {
                    base_demand: 0.0,
                    pattern: None,
                    name: None,
                }],
                emitter_coeff: 0.0,
                emitter_exp: 0.5,
            }),
            "reservoir" => hydra::NodeKind::Reservoir(hydra::Reservoir { head_pattern: None }),
            "tank" => hydra::NodeKind::Tank(hydra::Tank {
                min_level: min_m,
                max_level: max_m,
                initial_level: init_m,
                diameter: 10.0,
                min_volume: 0.0,
                volume_curve: None,
                mix_model: hydra::MixModel::Cstr,
                mix_fraction: 1.0,
                bulk_coeff: 0.0,
                overflow: false,
            }),
            other => return Err(format!("unknown node kind '{}'", other)),
        };
        // For tanks: EPANET stores base.elevation = bottom + min_level (the minimum
        // piezometric head).  For junctions / reservoirs: base.elevation = elevation.
        let base_elev = if matches!(node_kind, hydra::NodeKind::Tank(_)) {
            elev_m + min_m
        } else {
            elev_m
        };
        network.nodes.push(hydra::Node {
            base: hydra::NodeBase {
                id: id.clone(),
                index,
                elevation: base_elev,
                initial_quality: 0.0,
            },
            kind: node_kind,
            source: None,
        });
        network.coordinates.insert(id.clone(), (x, y));
        Ok(())
    })
}

/// Default attributes for a link created by `create_link`, in the engine's
/// **internal SI units** (metres / m³/s / Watts). Pipe: length 100 m,
/// diameter 300 mm, roughness 100 (Hazen-Williams C). Pump: constant-power
/// 10 kW. Valve: PRV, diameter 300 mm.
fn default_link_kind(kind: &str) -> Result<hydra::LinkKind, String> {
    match kind {
        "pipe" => Ok(hydra::LinkKind::Pipe(hydra::Pipe {
            length: 100.0,
            diameter: 0.3,
            roughness: 100.0,
            minor_loss: 0.0,
            check_valve: false,
            bulk_coeff: None,
            wall_coeff: None,
            leak_coeff_1: 0.0,
            leak_coeff_2: 0.0,
        })),
        "pump" => Ok(hydra::LinkKind::Pump(hydra::Pump {
            curve_type: hydra::PumpCurveType::ConstHp,
            head_curve: None,
            power: Some(10_000.0), // 10 kW in Watts
            efficiency_curve: None,
            default_efficiency: 0.75,
            speed_pattern: None,
            energy_price: None,
            price_pattern: None,
        })),
        "valve" => Ok(hydra::LinkKind::Valve(hydra::Valve {
            valve_type: hydra::ValveType::Prv,
            diameter: 0.3,
            minor_loss: 0.0,
            curve: None,
        })),
        other => Err(format!("unknown link kind '{}'", other)),
    }
}

/// Add a new link (pipe or pump) between two existing nodes.
///
/// `id` must be unique across all nodes and links.  `from_id` / `to_id` must
/// identify existing nodes.  Pipe defaults: length 100 m, diameter 300 mm,
/// roughness 100 (Hazen-Williams C).  Pump defaults: constant-power 10 kW.
#[tauri::command(async)]
pub fn create_link(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    kind: String,
    id: String,
    from_id: String,
    to_id: String,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        let id = validate_inp_id(&id, "element")?;
        // Link ids are unique among links only — a node may share the id
        // (EPANET keeps node and link namespaces separate; the parser accepts
        // it), so do not reject an id merely because a node holds it.
        if network.links.iter().any(|l| l.base.id == id) {
            return Err(format!("ID '{}' is already in use by another link", id));
        }
        let from_node = network
            .nodes
            .iter()
            .find(|n| n.base.id == from_id)
            .map(|n| n.base.index)
            .ok_or_else(|| format!("node '{}' not found", from_id))?;
        let to_node = network
            .nodes
            .iter()
            .find(|n| n.base.id == to_id)
            .map(|n| n.base.index)
            .ok_or_else(|| format!("node '{}' not found", to_id))?;
        if from_node == to_node {
            return Err("from and to nodes must be different".into());
        }
        let index = network.links.len() + 1;
        let link_kind = default_link_kind(&kind)?;
        let initial_setting = match &link_kind {
            hydra::LinkKind::Valve(_) => Some(0.0),
            _ => None,
        };
        network.links.push(hydra::Link {
            base: hydra::LinkBase {
                id,
                index,
                from_node,
                to_node,
                initial_status: hydra::LinkStatus::Open,
                initial_setting,
            },
            kind: link_kind,
        });
        Ok(())
    })
}

/// Create a new pump-head curve with default two-point data.
///
/// `id` must be unique within the network. The default points span
/// (0 L/s, 50 m) → (5 L/s, 0 m) in display units, converted to the
/// engine's internal SI (m³/s, m) for storage.
#[tauri::command(async)]
pub fn create_curve(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        create_curve_in_network(network, &id)
    })
}

/// The kind and default points a newly created curve gets — factored out of
/// the command so a test can assert the kind without a Tauri handle. The
/// editor stages new curves under this kind's id to look their axes up.
pub(crate) fn create_curve_in_network(
    network: &mut hydra::Network,
    id: &str,
) -> Result<(), String> {
    let id = validate_inp_id(id, "curve")?;
    if network.curves.iter().any(|c| c.id == id) {
        return Err(format!("curve '{}' already exists", id));
    }
    // Default: two-point pump-head curve spanning ~(0 L/s, 50 m) to (5 L/s, 0 m)
    network.curves.push(hydra::Curve {
        id: id.clone(),
        kind: hydra::CurveKind::PumpHead,
        points: vec![
            hydra::CurvePoint { x: 0.0, y: 50.0 },
            hydra::CurvePoint {
                x: 5.0 / M3S_TO_LPS,
                y: 0.0,
            },
        ],
    });
    Ok(())
}

/// Ids of the elements that reference `curve_id`, in node-then-link order.
///
/// Every place a curve can be named, in one list, because the two callers
/// that care must agree: deletion refuses while this is non-empty, and
/// renaming rewrites exactly these. They disagreed once — a pump's
/// efficiency curve was rewritten by a rename but invisible to the delete
/// guard, so deleting it left the pump naming a curve that no longer
/// existed, which survives into the saved file.
fn curve_references(network: &hydra::Network, curve_id: &str) -> Vec<String> {
    let names = |c: &Option<String>| c.as_deref() == Some(curve_id);
    let mut refs: Vec<String> = Vec::new();
    for n in &network.nodes {
        if let hydra::NodeKind::Tank(t) = &n.kind {
            if names(&t.volume_curve) {
                refs.push(n.base.id.clone());
            }
        }
    }
    for l in &network.links {
        let hit = match &l.kind {
            hydra::LinkKind::Pump(p) => names(&p.head_curve) || names(&p.efficiency_curve),
            hydra::LinkKind::Valve(v) => names(&v.curve),
            hydra::LinkKind::Pipe(_) => false,
        };
        if hit {
            refs.push(l.base.id.clone());
        }
    }
    refs
}

/// Delete a curve from the network.
///
/// Fails if any pump, valve, or tank still references the curve (by
/// head-curve, valve-curve, or volume-curve respectively) — the reference
/// must be cleared first so the network never ends up with a dangling curve
/// ID that would fail to parse on the next INP round-trip.
#[tauri::command(async)]
pub fn delete_curve(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        if !network.curves.iter().any(|c| c.id == id) {
            return Err(format!("curve '{}' not found", id));
        }

        let referenced_by = curve_references(network, &id);
        if !referenced_by.is_empty() {
            return Err(format!(
                "curve '{}' is still attached to {}; detach it first",
                id,
                referenced_by.join(", ")
            ));
        }

        network.curves.retain(|c| c.id != id);
        Ok(())
    })
}

/// Rename a curve in place, cascading the new ID to every reference. See
/// [`rename_curve`] for the contract. Extracted so the cascade is testable
/// without an `AppHandle`; `new_id` is assumed validated by
/// [`validate_inp_id`].
fn rename_curve_in_network(
    network: &mut hydra::Network,
    old_id: &str,
    new_id: &str,
) -> Result<(), String> {
    if !network.curves.iter().any(|c| c.id == old_id) {
        return Err(format!("curve '{old_id}' not found"));
    }
    if new_id == old_id {
        return Ok(());
    }
    if network.curves.iter().any(|c| c.id == new_id) {
        return Err(format!("curve '{new_id}' already exists"));
    }

    for c in network.curves.iter_mut() {
        if c.id == old_id {
            c.id = new_id.to_string();
        }
    }
    for l in network.links.iter_mut() {
        match &mut l.kind {
            hydra::LinkKind::Pump(p) => {
                if p.head_curve.as_deref() == Some(old_id) {
                    p.head_curve = Some(new_id.to_string());
                }
                if p.efficiency_curve.as_deref() == Some(old_id) {
                    p.efficiency_curve = Some(new_id.to_string());
                }
            }
            hydra::LinkKind::Valve(v) => {
                if v.curve.as_deref() == Some(old_id) {
                    v.curve = Some(new_id.to_string());
                }
            }
            hydra::LinkKind::Pipe(_) => {}
        }
    }
    for n in network.nodes.iter_mut() {
        if let hydra::NodeKind::Tank(t) = &mut n.kind {
            if t.volume_curve.as_deref() == Some(old_id) {
                t.volume_curve = Some(new_id.to_string());
            }
        }
    }
    Ok(())
}

/// Rename a curve, cascading the new ID to every reference: pump head and
/// efficiency curves, GPV valve curves, and tank volume curves.
///
/// `new_id` must be non-empty, contain no whitespace/`;`/quotes, and be unique
/// among curves (curves have their own ID namespace). Fails without mutating
/// anything on any violation. Renaming to the current ID is a no-op success.
#[tauri::command(async)]
pub fn rename_curve(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    old_id: String,
    new_id: String,
) -> Result<(), String> {
    let new_id = validate_inp_id(&new_id, "curve")?;
    mutate_structural(&app, &state, |network| {
        rename_curve_in_network(network, &old_id, &new_id)
    })
}

/// Convert curve points from the display units used by `get_curves` back to
/// the engine's internal SI storage units.
///
/// Reads the same [`curve_axes`] table `network_to_dto` scales outbound
/// with, and divides by exactly what that multiplied — so the two cannot
/// drift as the table gains kinds, which is the whole reason the axes are
/// declared in one place rather than as a pair of matching `if`s.
pub(crate) fn curve_points_display_to_internal(
    kind: hydra::CurveKind,
    xs: &[f64],
    ys: &[f64],
) -> Vec<hydra::CurvePoint> {
    let [ax, ay] = curve_axes(kind);
    xs.iter()
        .zip(ys.iter())
        .map(|(&x, &y)| hydra::CurvePoint {
            x: x / ax.scale(),
            y: y / ay.scale(),
        })
        .collect()
}

/// Replace all points of an existing curve.
///
/// `xs`/`ys` must be in the same display units returned by `get_curves`
/// (flow L/s and head m for pump-head curves; raw pass-through units for all
/// other curve kinds) and have equal length. Pump-head curves require at
/// least 2 points.
#[tauri::command(async)]
pub fn update_curve_points(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    xs: Vec<f64>,
    ys: Vec<f64>,
) -> Result<(), String> {
    if xs.len() != ys.len() {
        return Err("mismatched point array lengths".into());
    }
    mutate_structural(&app, &state, |network| {
        let curve = network
            .curves
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| format!("curve '{}' not found", id))?;
        if curve.kind == hydra::CurveKind::PumpHead && xs.len() < 2 {
            return Err("pump-head curves require at least 2 points".into());
        }
        curve.points = curve_points_display_to_internal(curve.kind, &xs, &ys);
        Ok(())
    })
}

/// Create a new time pattern with flat multipliers (all 1.0) at 24 hourly steps.
///
/// `id` must be unique within the network.
#[tauri::command(async)]
pub fn create_pattern(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        let id = validate_inp_id(&id, "pattern")?;
        if network.patterns.iter().any(|p| p.id == id) {
            return Err(format!("pattern '{}' already exists", id));
        }
        network.patterns.push(hydra::Pattern {
            id: id.clone(),
            factors: vec![1.0; 24],
        });
        Ok(())
    })
}

/// Replace all multipliers of an existing time pattern.
///
/// `multipliers` must have at least one entry.
#[tauri::command(async)]
pub fn update_pattern_multipliers(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
    multipliers: Vec<f64>,
) -> Result<(), String> {
    if multipliers.is_empty() {
        return Err("pattern must have at least one multiplier".into());
    }
    mutate_structural(&app, &state, |network| {
        let pattern = network
            .patterns
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("pattern '{}' not found", id))?;
        pattern.factors = multipliers;
        Ok(())
    })
}

/// Rename a time pattern, cascading the new ID to every reference:
/// junction demand categories, reservoir/tank head patterns, pump
/// speed/price patterns, and the network's global default/energy-price
/// pattern (from `[OPTIONS]`).
///
/// Fails without mutating anything if `new_id` is empty or already in use
/// by another pattern.
#[tauri::command(async)]
pub fn rename_pattern(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    old_id: String,
    new_id: String,
) -> Result<(), String> {
    let trimmed = validate_inp_id(&new_id, "pattern")?;
    mutate_structural(&app, &state, |network| {
        if !network.patterns.iter().any(|p| p.id == old_id) {
            return Err(format!("pattern '{}' not found", old_id));
        }
        if trimmed != old_id && network.patterns.iter().any(|p| p.id == trimmed) {
            return Err(format!("pattern '{}' already exists", trimmed));
        }

        for p in network.patterns.iter_mut() {
            if p.id == old_id {
                p.id = trimmed.clone();
            }
        }
        for n in network.nodes.iter_mut() {
            match &mut n.kind {
                hydra::NodeKind::Junction(j) => {
                    for d in j.demands.iter_mut() {
                        if d.pattern.as_deref() == Some(old_id.as_str()) {
                            d.pattern = Some(trimmed.clone());
                        }
                    }
                }
                hydra::NodeKind::Reservoir(r) => {
                    if r.head_pattern.as_deref() == Some(old_id.as_str()) {
                        r.head_pattern = Some(trimmed.clone());
                    }
                }
                // Tanks carry no pattern references (head patterns are
                // reservoir-only).
                hydra::NodeKind::Tank(_) => {}
            }
        }
        for l in network.links.iter_mut() {
            if let hydra::LinkKind::Pump(p) = &mut l.kind {
                if p.speed_pattern.as_deref() == Some(old_id.as_str()) {
                    p.speed_pattern = Some(trimmed.clone());
                }
                if p.price_pattern.as_deref() == Some(old_id.as_str()) {
                    p.price_pattern = Some(trimmed.clone());
                }
            }
        }
        if network.options.default_pattern.as_deref() == Some(old_id.as_str()) {
            network.options.default_pattern = Some(trimmed.clone());
        }
        if network.options.energy_price_pattern.as_deref() == Some(old_id.as_str()) {
            network.options.energy_price_pattern = Some(trimmed.clone());
        }
        Ok(())
    })
}

/// Delete a time pattern from the network.
///
/// Fails if any junction demand, reservoir head pattern, quality-source
/// pattern, pump speed/price pattern, or the global default/energy-price
/// pattern (from `[OPTIONS]`) still references it — the reference must be
/// cleared first so the network never ends up with a dangling pattern ID that
/// would fail to parse on the next INP round-trip.
#[tauri::command(async)]
pub fn delete_pattern(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    id: String,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        if !network.patterns.iter().any(|p| p.id == id) {
            return Err(format!("pattern '{}' not found", id));
        }

        let mut referenced_by: Vec<String> = Vec::new();
        for n in &network.nodes {
            match &n.kind {
                hydra::NodeKind::Junction(j) => {
                    if j.demands
                        .iter()
                        .any(|d| d.pattern.as_deref() == Some(id.as_str()))
                    {
                        referenced_by.push(n.base.id.clone());
                    }
                }
                hydra::NodeKind::Reservoir(r) => {
                    if r.head_pattern.as_deref() == Some(id.as_str()) {
                        referenced_by.push(n.base.id.clone());
                    }
                }
                // Tanks carry no pattern references (head patterns are
                // reservoir-only).
                hydra::NodeKind::Tank(_) => {}
            }
            if n.source
                .as_ref()
                .is_some_and(|s| s.pattern.as_deref() == Some(id.as_str()))
            {
                referenced_by.push(format!("{} (quality source)", n.base.id));
            }
        }
        for l in &network.links {
            if let hydra::LinkKind::Pump(p) = &l.kind {
                if p.speed_pattern.as_deref() == Some(id.as_str())
                    || p.price_pattern.as_deref() == Some(id.as_str())
                {
                    referenced_by.push(l.base.id.clone());
                }
            }
        }
        if network.options.default_pattern.as_deref() == Some(id.as_str()) {
            referenced_by.push("global default pattern (Options)".into());
        }
        if network.options.energy_price_pattern.as_deref() == Some(id.as_str()) {
            referenced_by.push("global energy price pattern (Options)".into());
        }
        if !referenced_by.is_empty() {
            return Err(format!(
                "pattern '{}' is still attached to {}; detach it first",
                id,
                referenced_by.join(", ")
            ));
        }

        network.patterns.retain(|p| p.id != id);
        Ok(())
    })
}

/// Append a new simple control to the network.
#[tauri::command(async)]
pub fn create_control(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    control: ControlDto,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        let ctrl = control_from_dto(&control, network)?;
        network.controls.push(ctrl);
        Ok(())
    })
}

/// Replace the simple control at `index` (position in `get_controls()`'s
/// response array).
#[tauri::command(async)]
pub fn update_control(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    index: usize,
    control: ControlDto,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        let ctrl = control_from_dto(&control, network)?;
        let slot = network
            .controls
            .get_mut(index)
            .ok_or_else(|| format!("control index {} out of range", index))?;
        *slot = ctrl;
        Ok(())
    })
}

/// Delete the simple control at `index`.
#[tauri::command(async)]
pub fn delete_control(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    index: usize,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        if index >= network.controls.len() {
            return Err(format!("control index {} out of range", index));
        }
        network.controls.remove(index);
        Ok(())
    })
}

/// Append a new rule-based control to the network.
#[tauri::command(async)]
pub fn create_rule(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    rule: RuleDto,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        let r = rule_from_dto(&rule, network)?;
        network.rules.push(r);
        Ok(())
    })
}

/// Replace the rule at `index` (position in `get_rules()`'s response array).
#[tauri::command(async)]
pub fn update_rule(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    index: usize,
    rule: RuleDto,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        let r = rule_from_dto(&rule, network)?;
        let slot = network
            .rules
            .get_mut(index)
            .ok_or_else(|| format!("rule index {} out of range", index))?;
        *slot = r;
        Ok(())
    })
}

/// Delete the rule at `index`.
#[tauri::command(async)]
pub fn delete_rule(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    index: usize,
) -> Result<(), String> {
    mutate_structural(&app, &state, |network| {
        if index >= network.rules.len() {
            return Err(format!("rule index {} out of range", index));
        }
        network.rules.remove(index);
        Ok(())
    })
}

/// A single patch entry passed to `preview_patches`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchItem {
    pub kind: String,
    pub id: String,
    pub field: String,
    pub value: serde_json::Value,
}

/// Apply a list of patches to a temporary clone of the in-memory network and
/// return the resulting INP text, **without** mutating `NetworkState`.
///
/// Used by the "Preview changes" diff dialog so the frontend can show a diff
/// between the on-disk file and what would be written after saving.
#[tauri::command(async)]
/// Return the INP text that would result from applying pending patches.
pub fn preview_patches(
    state: tauri::State<'_, NetworkState>,
    patches: Vec<PatchItem>,
) -> Result<String, String> {
    // A genuine deep copy: the preview mutates a throwaway network and must
    // never touch the cached one.
    let mut network: hydra::Network = {
        let guard = state.0.lock();
        match &*guard {
            NetworkStateInner::Loaded { network, .. } => (**network).clone(),
            NetworkStateInner::LoadedUds { .. } => {
                return Err(
                    "This project's engine is read-only in the GUI — editing is not \
                     available yet."
                        .into(),
                )
            }
            NetworkStateInner::Empty => return Err("no network loaded".into()),
        }
    };

    for patch in patches {
        apply_patch_to_network(
            &mut network,
            &patch.kind,
            &patch.id,
            &patch.field,
            patch.value,
        )?;
    }

    let new_bytes = hydra::write_inp(&network);
    String::from_utf8(new_bytes).map_err(|e| e.to_string())
}

/// One finding returned by `validate_network`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationFindingDto {
    /// `"error"` | `"warning"`. Every constraint the engine's `validate`
    /// checks is fatal for simulation, so all current findings are errors;
    /// the field exists so future advisory checks can be surfaced without a
    /// wire change.
    pub severity: String,
    /// Stable kebab-case code identifying the violated constraint, one per
    /// engine `ValidationError` variant (e.g. `"link-self-loop"`).
    pub code: String,
    /// Human-readable description (the engine's `Display` rendering).
    pub message: String,
    /// ID of the offending element, when the finding names one.
    pub element_id: Option<String>,
    /// `"node"` | `"link"` | `"curve"` | `"pattern"`; `None` when the
    /// offending object's kind is ambiguous (e.g. a cross-reference held by
    /// an arbitrary object) or the finding is network-wide.
    pub element_kind: Option<String>,
}

/// Map one engine [`hydra::ValidationError`] to its wire DTO. The `code`
/// mapping is exhaustive and must stay stable — the frontend keys on it.
fn validation_finding(err: &hydra::ValidationError) -> ValidationFindingDto {
    use hydra::ValidationError as V;
    let (code, element_id, element_kind): (&str, Option<String>, Option<&str>) = match err {
        V::LinkUnknownFromNode { link_id, .. } => (
            "link-unknown-from-node",
            Some(link_id.clone()),
            Some("link"),
        ),
        V::LinkUnknownToNode { link_id, .. } => {
            ("link-unknown-to-node", Some(link_id.clone()), Some("link"))
        }
        V::UnknownPatternRef { object_id, .. } => {
            ("unknown-pattern-ref", Some(object_id.clone()), None)
        }
        V::UnknownCurveRef { object_id, .. } => {
            ("unknown-curve-ref", Some(object_id.clone()), None)
        }
        V::WrongCurveKind { object_id, .. } => ("wrong-curve-kind", Some(object_id.clone()), None),
        V::MissingRequiredCurve { object_id, .. } => {
            // Only pumps and GPV/PCV valves require a curve — always a link.
            (
                "missing-required-curve",
                Some(object_id.clone()),
                Some("link"),
            )
        }
        V::UnknownNodeIdRef { object_id, .. } => {
            ("unknown-node-ref", Some(object_id.clone()), None)
        }
        V::UnknownNodeIndexRef { object_id, .. } => {
            ("unknown-node-index-ref", Some(object_id.clone()), None)
        }
        V::UnknownLinkIndexRef { object_id, .. } => {
            ("unknown-link-index-ref", Some(object_id.clone()), None)
        }
        V::LinkSelfLoop { link_id } => ("link-self-loop", Some(link_id.clone()), Some("link")),
        V::NoReservoir => ("no-reservoir", None, None),
        V::NodeNotReachable { node_id } => {
            ("node-not-reachable", Some(node_id.clone()), Some("node"))
        }
        V::TankLevelOutOfRange { node_id, .. } => (
            "tank-level-out-of-range",
            Some(node_id.clone()),
            Some("node"),
        ),
        V::PumpCurveNotDecreasing { curve_id } => (
            "pump-curve-not-decreasing",
            Some(curve_id.clone()),
            Some("curve"),
        ),
        V::EfficiencyCurveYOutOfRange { curve_id } => (
            "efficiency-curve-y-out-of-range",
            Some(curve_id.clone()),
            Some("curve"),
        ),
        V::TankVolumeCurveYNotIncreasing { curve_id } => (
            "tank-volume-curve-y-not-increasing",
            Some(curve_id.clone()),
            Some("curve"),
        ),
        V::GpvHeadlossCurveYDecreasing { curve_id } => (
            "gpv-headloss-curve-y-decreasing",
            Some(curve_id.clone()),
            Some("curve"),
        ),
        V::CurveXNotIncreasing { curve_id } => (
            "curve-x-not-increasing",
            Some(curve_id.clone()),
            Some("curve"),
        ),
        V::PatternEmpty { pattern_id } => {
            ("pattern-empty", Some(pattern_id.clone()), Some("pattern"))
        }
        V::RuleActionUnknownLink { .. } => ("rule-action-unknown-link", None, None),
        V::CurveTooFewPoints { curve_id, .. } => (
            "curve-too-few-points",
            Some(curve_id.clone()),
            Some("curve"),
        ),
        V::ControlUnknownLink { .. } => ("control-unknown-link", None, None),
        V::ValveOnFixedGradeNode { link_id, .. } => (
            "valve-on-fixed-grade-node",
            Some(link_id.clone()),
            Some("link"),
        ),
        V::ValvePlacementConflict { link_id, .. } => (
            "valve-placement-conflict",
            Some(link_id.clone()),
            Some("link"),
        ),
    };
    ValidationFindingDto {
        severity: "error".to_string(),
        code: code.to_string(),
        message: err.to_string(),
        element_id,
        element_kind: element_kind.map(str::to_string),
    }
}

/// Run the engine's network validation and map every finding to its wire DTO.
pub(crate) fn validation_findings(network: &hydra::Network) -> Vec<ValidationFindingDto> {
    match network.validate() {
        Ok(()) => Vec::new(),
        Err(errors) => errors.iter().map(validation_finding).collect(),
    }
}

/// Validate the model for `(project_id, scenario_id)` and return all findings.
///
/// Unlike `network_for_target`, a *dirty* matching cache is used as-is
/// (cloned — the cached state is never disturbed): validating the current
/// unsaved edits is exactly the point of this command, and no `results.out`
/// positional indexing is involved. When the cache does not hold the target,
/// the model is read and parsed from disk (a model that fails INP parsing —
/// which itself runs validation — surfaces as `Err`).
/// Stable kebab-case code for a uds validation kind, derived from the
/// variant name (part of the engine's public API, so as stable as the
/// engine's own semver): `AdverseSlope` → `"adverse-slope"`.
fn kebab_variant_code(kind: &hydra::uds::io::validate::ValidationKind) -> String {
    let debug = format!("{kind:?}");
    let name = debug.split([' ', '(', '{']).next().unwrap_or("finding");
    let mut out = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[tauri::command(async)]
/// Run engine validation for a project/scenario model and return the findings.
pub fn validate_network(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    scenario_id: Option<String>,
) -> Result<Vec<ValidationFindingDto>, String> {
    validate_target_ids(&project_id, scenario_id.as_deref())?;

    // Engine-dispatched: each engine's validator serves its own findings.
    // Unknown engines stay quiet instead of toasting a foreign-dialect
    // error on every open.
    {
        let app_data = app_data_dir(&app)?;
        match super::projects::project_engine_key(&app_data, &project_id).as_str() {
            "wds" => {}
            "uds" => {
                let model_path = model_path_for(&app_data, &project_id, scenario_id.as_deref());
                // No model yet: nothing to validate, and nothing wrong either.
                if read_model_bytes(&model_path)?.is_none() {
                    return Ok(Vec::new());
                }
                // The uds validator resolves as it checks (offset
                // conventions, adverse slopes), so it needs the network by
                // &mut — parse a working copy from disk rather than
                // mutating (or cloning) the shared cache.
                let raw = std::fs::read(&model_path).map_err(|e| e.to_string())?;
                let text = String::from_utf8_lossy(&raw);
                let (mut working, _import_diags) = hydra::uds::io::objects::parse_network(&text);
                let diags = hydra::uds::io::validate::validate(&mut working);
                return Ok(diags
                    .into_iter()
                    .map(|d| {
                        let severity = if d.kind.is_error() {
                            "error"
                        } else {
                            "warning"
                        };
                        ValidationFindingDto {
                            severity: severity.to_string(),
                            code: kebab_variant_code(&d.kind),
                            message: d.to_string(),
                            element_id: (!d.element.is_empty()).then(|| d.element.clone()),
                            // The diagnostic names the element without
                            // classing it — the id resolves against the
                            // live arrays frontend-side.
                            element_kind: None,
                        }
                    })
                    .collect());
            }
            _ => return Ok(Vec::new()),
        }
    }

    // Clone from the cache when it holds exactly this target (dirty allowed —
    // see the doc comment); otherwise fall back to the on-disk model.
    let cached: Option<std::sync::Arc<hydra::Network>> = {
        let guard = state.0.lock();
        match &*guard {
            NetworkStateInner::Loaded {
                network,
                owner_project_id: Some(owner),
                owner_scenario_id,
                ..
            } if owner == &project_id && owner_scenario_id.as_deref() == scenario_id.as_deref() => {
                Some(network.clone())
            }
            _ => None,
        }
    };
    let network = match cached {
        Some(n) => n,
        None => {
            let app_data = app_data_dir(&app)?;
            let model_path = model_path_for(&app_data, &project_id, scenario_id.as_deref());
            // No model yet (a project created without importing one): there
            // is nothing to validate, and nothing wrong either.
            let Some(raw) = read_model_bytes(&model_path)? else {
                return Ok(Vec::new());
            };
            // Tolerant: reporting *why* a network is not simulable is this
            // command's whole purpose, so a strict parse made it fail on
            // precisely the models it exists to describe.
            std::sync::Arc::new(
                hydra::io::parse_tolerant(&raw)
                    .map_err(format_read_error)?
                    .0,
            )
        }
    };
    Ok(validation_findings(&network))
}

#[tauri::command(async)]
/// Replace the network's `[TITLE]` lines.
///
/// EPANET's three-line title is convention, not a format rule, so any line
/// count is accepted (the GUI clamps the *display* to three lines). Trailing
/// empty lines are trimmed so clearing the editor writes an empty `[TITLE]`;
/// embedded newlines are rejected since each entry must serialise as one INP
/// line.
pub fn update_network_title(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    lines: Vec<String>,
) -> Result<(), String> {
    let normalized = normalize_title_lines(lines)?;
    mutate_structural(&app, &state, |network| {
        network.title = normalized;
        Ok(())
    })
}

/// Trim trailing whitespace per line, drop trailing empty lines, and enforce
/// single-line entries.
fn normalize_title_lines(lines: Vec<String>) -> Result<Vec<String>, String> {
    let mut trimmed: Vec<String> = lines
        .into_iter()
        .map(|l| l.trim_end().to_string())
        .collect();
    while trimmed.last().is_some_and(|l| l.is_empty()) {
        trimmed.pop();
    }
    if trimmed.iter().any(|l| l.contains('\n') || l.contains('\r')) {
        return Err("title lines must not contain newlines".into());
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_fixtures::{loaded_state, TEST_INP};

    // ── structural-mutation helper ────────────────────────────────────────

    /// The delta a patch emits must name the link's real endpoints.
    ///
    /// It resolves them by indexing `nodes` directly, which is only correct
    /// while `nodes[i].base.index == i + 1` holds. Deleting rebuilds those
    /// indices to keep it true, so this checks a link's endpoints after a
    /// delete has shifted every node above it — where an off-by-one would
    /// otherwise quietly relabel a pipe's ends in the canvas.
    #[test]
    fn a_patch_delta_names_the_links_real_endpoints() {
        let mut network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        let mut dto = network_to_dto(&network);
        // R1 is the first node, so removing it shifts J1 and T1 down one.
        delete_element_from_network(&mut network, "reservoir", "R1").expect("delete");
        let dto_after = network_to_dto(&network);

        let patched = refresh_element_dto(&network, &mut dto, "pipe", "P2").expect("refresh");
        let link = patched.link.expect("a link delta");
        assert_eq!((link.from_id.as_str(), link.to_id.as_str()), ("J1", "T1"));

        // And it agrees with the endpoints a full rebuild would report.
        let rebuilt = dto_after.links.iter().find(|l| l.id == "P2").unwrap();
        assert_eq!(
            (link.from_id, link.to_id),
            (rebuilt.from_id.clone(), rebuilt.to_id.clone())
        );
    }

    /// Every way a curve can be referenced must block its deletion.
    ///
    /// The guard exists so the model can never hold a curve id that resolves
    /// to nothing, and a pump's efficiency curve is a reference like any
    /// other — `rename_curve_in_network` already treats it as one. Left out
    /// of the guard, deleting it succeeded and the dangling id survived into
    /// the saved file.
    #[test]
    fn every_curve_reference_blocks_its_deletion() {
        let inp = TEST_INP.replace(
            "[PIPES]",
            "[PUMPS]\nPU1  R1  J1  HEAD C1\n\n[CURVES]\nC1  0  50\nC1  5  0\nE1  0  0.7\nE1  5  0.8\n\n[ENERGY]\nPump  PU1  EFFIC  E1\n\n[PIPES]",
        );
        let network = hydra::io::parse(inp.as_bytes()).expect("fixture must parse");
        let attached = network.links.iter().find_map(|l| match &l.kind {
            hydra::LinkKind::Pump(p) => p.efficiency_curve.clone(),
            _ => None,
        });
        assert_eq!(
            attached,
            Some("E1".to_string()),
            "fixture must actually attach an efficiency curve"
        );

        assert_eq!(
            curve_references(&network, "E1"),
            ["PU1"],
            "an efficiency curve is a reference too"
        );
        assert_eq!(curve_references(&network, "C1"), ["PU1"]);
        assert!(curve_references(&network, "nobody").is_empty());
    }

    /// A network whose `[REPORT]` section names two nodes and a link.
    fn reported_network() -> hydra::Network {
        // Before `[END]`, which is where the reader stops.
        let inp = TEST_INP.replace("[END]", "[REPORT]\nNODES  J1  T1\nLINKS  P1\n\n[END]");
        let network = hydra::io::parse(inp.as_bytes()).expect("fixture must parse");
        assert!(
            matches!(network.report.nodes, hydra::ReportSelection::Some(_)),
            "fixture must actually carry a report selection"
        );
        network
    }

    fn report_ids(network: &hydra::Network) -> (Vec<String>, Vec<String>) {
        let listed = |s: &hydra::ReportSelection| match s {
            hydra::ReportSelection::Some(ids) => ids.clone(),
            _ => Vec::new(),
        };
        (listed(&network.report.nodes), listed(&network.report.links))
    }

    /// `[REPORT] NODES`/`LINKS` name elements by id, so renaming one has to
    /// carry the selection with it. Missing it leaves the saved model asking
    /// for an element that no longer exists, and the element the user renamed
    /// silently drops out of its own report.
    #[test]
    fn renaming_carries_the_report_selection() {
        let mut network = reported_network();
        rename_element_in_network(&mut network, "junction", "J1", "J1a").expect("rename");
        rename_element_in_network(&mut network, "pipe", "P1", "P1a").expect("rename");

        let (nodes, links) = report_ids(&network);
        assert_eq!(nodes, ["J1a", "T1"], "renamed node should follow");
        assert_eq!(links, ["P1a"], "renamed link should follow");
    }

    /// Deleting has the same obligation in the other direction: a selection
    /// naming a deleted element round-trips a dangling id into the file.
    #[test]
    fn deleting_drops_the_report_selection() {
        let mut network = reported_network();
        // Deleting J1 cascades P1 and P2 with it, since both touch it.
        delete_element_from_network(&mut network, "junction", "J1").expect("delete");

        let (nodes, links) = report_ids(&network);
        assert_eq!(nodes, ["T1"], "deleted node should be dropped");
        assert!(
            links.is_empty(),
            "cascaded link should be dropped: {links:?}"
        );
    }

    #[test]
    fn normalize_title_lines_trims_and_rejects_newlines() {
        assert_eq!(
            normalize_title_lines(vec!["A  ".into(), "".into(), "".into()]).unwrap(),
            vec!["A"]
        );
        // Interior empty lines survive; only trailing ones trim.
        assert_eq!(
            normalize_title_lines(vec!["A".into(), "".into(), "C".into()]).unwrap(),
            vec!["A", "", "C"]
        );
        // More than three lines is allowed — EPANET treats three as
        // convention, not a format rule.
        assert_eq!(
            normalize_title_lines(vec!["1".into(), "2".into(), "3".into(), "4".into()])
                .unwrap()
                .len(),
            4
        );
        assert!(normalize_title_lines(vec!["bad\nline".into()]).is_err());
        assert!(normalize_title_lines(Vec::new()).unwrap().is_empty());
    }

    #[test]
    fn network_title_lines_trim_and_round_trip() {
        let mut state = loaded_state();
        apply_structural_mutation(&mut state, |network| {
            network.title = vec!["Main title".into(), "Detail line".into()];
            Ok(())
        })
        .unwrap();
        let NetworkStateInner::Loaded {
            network, raw_bytes, ..
        } = &state
        else {
            panic!("expected loaded state");
        };
        assert_eq!(network.title, vec!["Main title", "Detail line"]);
        // The dirty flag re-serialises on demand: title survives an INP
        // write -> parse cycle.
        let _ = raw_bytes;
        let written = hydra::io::write_inp(network);
        let reparsed = hydra::io::parse(&written).unwrap();
        assert_eq!(reparsed.title, vec!["Main title", "Detail line"]);
    }

    #[test]
    fn apply_structural_mutation_marks_dirty_and_rebuilds_dto() {
        let mut state = loaded_state();
        apply_structural_mutation(&mut state, |network| {
            network.patterns.push(hydra::Pattern {
                id: "NEW".into(),
                factors: vec![1.0; 4],
            });
            Ok(())
        })
        .unwrap();
        let NetworkStateInner::Loaded { dirty, dto, .. } = &state else {
            panic!("state must stay loaded");
        };
        assert!(*dirty, "successful mutation must mark the state dirty");
        assert!(
            dto.patterns.iter().any(|p| p.id == "NEW"),
            "cached DTO must be rebuilt after the mutation"
        );
    }

    #[test]
    fn apply_structural_mutation_error_paths() {
        // Failing mutation: the error propagates, nothing is marked dirty,
        // and the cached DTO is not rebuilt (the mutation added a pattern,
        // but the stale DTO must not pick it up).
        let mut state = loaded_state();
        let err = apply_structural_mutation(&mut state, |network| {
            network.patterns.push(hydra::Pattern {
                id: "HALF-DONE".into(),
                factors: vec![1.0],
            });
            Err("boom".into())
        })
        .unwrap_err();
        assert_eq!(err, "boom");
        let NetworkStateInner::Loaded { dirty, dto, .. } = &state else {
            panic!("state must stay loaded");
        };
        assert!(!*dirty, "failed mutation must not mark the state dirty");
        assert!(
            !dto.patterns.iter().any(|p| p.id == "HALF-DONE"),
            "DTO must not be rebuilt on failure"
        );

        // Empty state: the canonical error, and the closure never runs.
        let mut empty = NetworkStateInner::Empty;
        let err = apply_structural_mutation(&mut empty, |_| {
            panic!("mutation must not run without a loaded network")
        })
        .unwrap_err();
        assert_eq!(err, "no network loaded");
    }

    #[test]
    fn refresh_element_dto_updates_single_link_in_place() {
        let mut state = loaded_state();
        if let NetworkStateInner::Loaded { network, dto, .. } = &mut state {
            let network = std::sync::Arc::make_mut(network);
            let p2_before = dto.links.iter().find(|l| l.id == "P2").unwrap().clone();
            apply_patch_to_network(network, "pipe", "P1", "roughness", serde_json::json!(123.0))
                .unwrap();
            let patched = refresh_element_dto(network, dto, "pipe", "P1").unwrap();

            // Returned delta is the link, with endpoints resolved.
            let link = patched.link.expect("link delta");
            assert!(patched.node.is_none());
            assert_eq!(link.id, "P1");
            assert_eq!(link.from_id, "R1");
            assert_eq!(link.to_id, "J1");
            assert!((link.roughness - 123.0).abs() < 1e-9);

            // Cached DTO entry updated in place; untouched entries unchanged.
            let p1 = dto.links.iter().find(|l| l.id == "P1").unwrap();
            assert!((p1.roughness - 123.0).abs() < 1e-9);
            let p2_after = dto.links.iter().find(|l| l.id == "P2").unwrap();
            assert_eq!(p2_after.roughness, p2_before.roughness);
            assert_eq!(dto.links.len(), 2);
        } else {
            panic!("state must be loaded");
        }
    }

    #[test]
    fn refresh_element_dto_updates_single_node_in_place() {
        let mut state = loaded_state();
        if let NetworkStateInner::Loaded { network, dto, .. } = &mut state {
            let network = std::sync::Arc::make_mut(network);
            apply_patch_to_network(
                network,
                "junction",
                "J1",
                "elevation",
                serde_json::json!(42.0),
            )
            .unwrap();
            let patched = refresh_element_dto(network, dto, "junction", "J1").unwrap();

            let node = patched.node.expect("node delta");
            assert!(patched.link.is_none());
            assert_eq!(node.id, "J1");
            // A round trip, and only that: it shows the delta path and the
            // full rebuild agree, not that either is in the right unit. The
            // absolute check lives in `network_dto`'s `unit_boundary` tests,
            // which this assertion once stood in for and could not.
            assert!((node.elevation - 42.0).abs() < 1e-6);

            let j1 = dto.nodes.iter().find(|n| n.id == "J1").unwrap();
            assert!((j1.elevation - 42.0).abs() < 1e-6);
            assert_eq!(dto.nodes.len(), 3);
        } else {
            panic!("state must be loaded");
        }
    }

    /// The frontend replaces its link object wholesale with the delta DTO, so
    /// a delta must carry the fields the full snapshot ships through binary
    /// columns — a pipe's polyline vertices and its initial status. Before
    /// this was enforced, patching any pipe field silently stripped both from
    /// frontend state until the next full snapshot refetch (a "closed" pipe
    /// snapped back to showing "open" in the editor, and a polyline pipe
    /// rendered as a straight line on the canvas).
    #[test]
    fn refresh_element_dto_link_delta_carries_vertices_and_initial_status() {
        let mut state = loaded_state();
        let NetworkStateInner::Loaded { network, dto, .. } = &mut state else {
            panic!("state must be loaded");
        };
        let network = std::sync::Arc::make_mut(network);

        // Give P1 a polyline and a closed status, then patch a scalar field.
        network
            .vertices
            .insert("P1".into(), vec![(10.0, 11.0), (12.0, 13.0)]);
        apply_patch_to_network(network, "pipe", "P1", "status", serde_json::json!("Closed"))
            .unwrap();
        apply_patch_to_network(network, "pipe", "P1", "roughness", serde_json::json!(111.0))
            .unwrap();
        let patched = refresh_element_dto(network, dto, "pipe", "P1").unwrap();
        let link = patched.link.expect("link delta");
        assert_eq!(
            link.vertices.as_deref(),
            Some(&[(10.0, 11.0), (12.0, 13.0)][..])
        );
        assert_eq!(link.initial_status.as_deref(), Some("closed"));

        // CV surfaces as "cv" (check-valve flag wins over the Open status).
        apply_patch_to_network(network, "pipe", "P1", "status", serde_json::json!("CV")).unwrap();
        let patched = refresh_element_dto(network, dto, "pipe", "P1").unwrap();
        assert_eq!(patched.link.unwrap().initial_status.as_deref(), Some("cv"));

        // A vertex-less open pipe omits both optional fields (`None`), so the
        // JSON shape matches the snapshot decoder's (fields absent, not null).
        let patched = refresh_element_dto(network, dto, "pipe", "P2").unwrap();
        let link = patched.link.unwrap();
        assert_eq!(link.vertices, None);
        assert_eq!(link.initial_status.as_deref(), Some("open"));
        let json =
            serde_json::to_value(refresh_element_dto(network, dto, "pipe", "P2").unwrap()).unwrap();
        assert!(json["link"].get("vertices").is_none());
    }

    #[test]
    fn refresh_element_dto_unknown_element_errors() {
        let mut state = loaded_state();
        if let NetworkStateInner::Loaded { network, dto, .. } = &mut state {
            let network = std::sync::Arc::make_mut(network);
            assert!(refresh_element_dto(network, dto, "pipe", "NOPE").is_err());
            assert!(refresh_element_dto(network, dto, "widget", "P1").is_err());
        } else {
            panic!("state must be loaded");
        }
    }

    // ── tank elevation DTO ↔ patch round-trip ─────────────────────────────

    #[test]
    fn tank_elevation_dto_patch_round_trip_is_stable() {
        let mut state = loaded_state();
        let NetworkStateInner::Loaded { network, dto, .. } = &mut state else {
            panic!("state must be loaded");
        };
        let network = std::sync::Arc::make_mut(network);
        let t1 = network.nodes.iter().find(|n| n.base.id == "T1").unwrap();
        let internal_before = t1.base.elevation;
        let min_level = match &t1.kind {
            hydra::NodeKind::Tank(t) => t.min_level,
            _ => unreachable!("T1 is a tank"),
        };
        // Internally `base.elevation` = bottom + min_level (minimum
        // piezometric head). The DTO must report the *bottom* — the same
        // quantity the tank "elevation" patch accepts — not the raw
        // `base.elevation`.
        let dto_elev = node_to_dto(network, t1).elevation;
        assert!(
            (dto_elev - (internal_before - min_level)).abs() < 1e-9,
            "DTO must report tank bottom, got {dto_elev}"
        );

        // Round-tripping the displayed value through the elevation patch must
        // not move the tank (previously it rose by min_level per edit).
        apply_patch_to_network(
            network,
            "tank",
            "T1",
            "elevation",
            serde_json::json!(dto_elev),
        )
        .unwrap();
        let t1 = network.nodes.iter().find(|n| n.base.id == "T1").unwrap();
        assert!(
            (t1.base.elevation - internal_before).abs() < 1e-9,
            "round-trip drifted: {} -> {}",
            internal_before,
            t1.base.elevation
        );

        // And the refreshed DTO still shows the same bottom.
        let patched = refresh_element_dto(network, dto, "tank", "T1").unwrap();
        let elev_after = patched.node.expect("node delta").elevation;
        assert!((elev_after - dto_elev).abs() < 1e-9);
    }

    // ── node cascade delete: link index rebuild + control remap ───────────

    /// Like TEST_INP but with a second junction/pipe that survives deleting
    /// J1, and a control referencing the surviving pipe + tank.
    const CASCADE_INP: &str = "\
[JUNCTIONS]
J1  10  5
J2  20  0

[RESERVOIRS]
R1  100

[TANKS]
T1  50  10  5  20  40  0

[PIPES]
P1  R1  J1  1000  12  100  0  Open
P2  J1  T1  800   10  100  0  Open
P3  R1  J2  500   8   100  0  Open

[CONTROLS]
LINK P3 CLOSED IF NODE T1 ABOVE 12

[COORDINATES]
J1  1.0  2.0
J2  1.5  2.5
R1  0.0  0.0
T1  2.0  2.0

[OPTIONS]
Units  GPM

[TIMES]
Duration  0

[END]
";

    #[test]
    fn delete_node_cascade_rebuilds_link_indices_and_keeps_control_target() {
        let mut network = hydra::io::parse(CASCADE_INP.as_bytes()).unwrap();
        // Deleting J1 cascades P1 and P2; P3 (old index 3) survives.
        delete_element_from_network(&mut network, "junction", "J1").unwrap();

        assert_eq!(network.links.len(), 1);
        assert_eq!(network.links[0].base.id, "P3");
        // Surviving link indices must be contiguous 1..=n — a stale gapped
        // index corrupts the next delete's guard/remap and lets create_link
        // (links.len() + 1) mint a duplicate.
        for (i, l) in network.links.iter().enumerate() {
            assert_eq!(l.base.index, i + 1, "link {} has stale index", l.base.id);
        }

        // The control must still target P3 and trigger on T1 after the remap.
        assert_eq!(network.controls.len(), 1);
        let ctrl = &network.controls[0];
        assert_eq!(network.links[ctrl.link - 1].base.id, "P3");
        let trigger = ctrl.trigger_node.expect("level trigger keeps its node");
        assert_eq!(network.nodes[trigger - 1].base.id, "T1");

        // A follow-up delete of the surviving link must resolve it correctly.
        delete_element_from_network(&mut network, "pipe", "P3")
            .expect_err("P3 is still referenced by the control and must be protected");
        network.controls.clear();
        delete_element_from_network(&mut network, "pipe", "P3").unwrap();
        assert!(network.links.is_empty());
    }

    // ── validate_network mapping ──────────────────────────────────────────

    #[test]
    fn validation_findings_map_engine_errors_to_stable_codes() {
        let mut network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        // Parse-time validation passed, so a fresh model has no findings.
        assert!(validation_findings(&network).is_empty());

        // Introduce two findings: an empty pattern and a self-loop.
        network.patterns.push(hydra::Pattern {
            id: "EMPTY".into(),
            factors: vec![],
        });
        network.build_pattern_index();
        let to = network.links[0].base.to_node;
        network.links[0].base.from_node = to;

        let findings = validation_findings(&network);
        let empty = findings.iter().find(|f| f.code == "pattern-empty").unwrap();
        assert_eq!(empty.severity, "error");
        assert_eq!(empty.element_id.as_deref(), Some("EMPTY"));
        assert_eq!(empty.element_kind.as_deref(), Some("pattern"));
        assert!(empty.message.contains("EMPTY"));

        let self_loop = findings
            .iter()
            .find(|f| f.code == "link-self-loop")
            .unwrap();
        assert_eq!(self_loop.element_id.as_deref(), Some("P1"));
        assert_eq!(self_loop.element_kind.as_deref(), Some("link"));

        // Wire shape: camelCase keys, explicit nulls for absent element info.
        let json = serde_json::to_string(&ValidationFindingDto {
            severity: "error".into(),
            code: "no-reservoir".into(),
            message: "network has no reservoir".into(),
            element_id: None,
            element_kind: None,
        })
        .unwrap();
        assert!(json.contains("\"elementId\":null"));
        assert!(json.contains("\"elementKind\":null"));
    }

    // ── remap_index ───────────────────────────────────────────────────────

    #[test]
    fn remap_index_shifts_past_removed_entries() {
        // Removing old 1-based indices 2 and 5 from the vec they address.
        assert_eq!(remap_index(1, &[2, 5]), 1);
        assert_eq!(remap_index(3, &[2, 5]), 2);
        assert_eq!(remap_index(4, &[2, 5]), 3);
        assert_eq!(remap_index(6, &[2, 5]), 4);
        assert_eq!(remap_index(3, &[]), 3);
    }

    // ── create_link defaults (internal SI ↔ display m/mm) ─────────────────

    #[test]
    fn create_link_pipe_defaults_display_as_100m_300mm() {
        let kind = default_link_kind("pipe").unwrap();
        let link = hydra::Link {
            base: hydra::LinkBase {
                id: "P9".into(),
                index: 1,
                from_node: 1,
                to_node: 2,
                initial_status: hydra::LinkStatus::Open,
                initial_setting: None,
            },
            kind,
        };
        let dto = link_to_dto(&link, "A".into(), "B".into());
        // The documented defaults are 100 m / 300 mm — the DTO (display
        // units: m and mm) must reflect them, not 100 ft / 0.3 ft.
        assert!((dto.length - 100.0).abs() < 1e-9, "length {}", dto.length);
        assert!(
            (dto.diameter - 300.0).abs() < 1e-9,
            "diameter {}",
            dto.diameter
        );
        assert!((dto.roughness - 100.0).abs() < 1e-9);
    }

    #[test]
    fn create_link_valve_default_diameter_displays_as_300mm() {
        let kind = default_link_kind("valve").unwrap();
        let link = hydra::Link {
            base: hydra::LinkBase {
                id: "V9".into(),
                index: 1,
                from_node: 1,
                to_node: 2,
                initial_status: hydra::LinkStatus::Open,
                initial_setting: Some(0.0),
            },
            kind,
        };
        let dto = link_to_dto(&link, "A".into(), "B".into());
        assert!(
            (dto.diameter - 300.0).abs() < 1e-9,
            "diameter {}",
            dto.diameter
        );
    }

    #[test]
    fn create_link_unknown_kind_errors() {
        assert!(default_link_kind("widget").is_err());
    }

    // ── curve unit round-trip (get_curves ↔ update_curve_points) ──────────

    #[test]
    fn pump_head_curve_display_round_trip_is_value_stable() {
        // Internal points (m³/s, m) → DTO display units (L/s, m) via
        // network_to_dto's conversion, then back through the
        // update_curve_points conversion. The same scale is used in both
        // directions, so the round-trip must be stable.
        //
        // Stability is all this test can prove. It passed unchanged while
        // both directions were wrong by 28× — which is why the absolute
        // values live in `network_dto`'s `unit_boundary` tests instead.
        let mut network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        let internal = vec![
            hydra::CurvePoint { x: 0.0, y: 164.0 },
            hydra::CurvePoint { x: 0.177, y: 82.0 },
            hydra::CurvePoint { x: 0.354, y: 0.0 },
        ];
        network.curves.push(hydra::Curve {
            id: "C1".into(),
            kind: hydra::CurveKind::PumpHead,
            points: internal.clone(),
        });

        let dto = network_to_dto(&network);
        let curve = dto.curves.iter().find(|c| c.id == "C1").unwrap();
        let back = curve_points_display_to_internal(hydra::CurveKind::PumpHead, &curve.x, &curve.y);

        assert_eq!(back.len(), internal.len());
        for (b, i) in back.iter().zip(internal.iter()) {
            assert!((b.x - i.x).abs() < 1e-12, "x drifted: {} -> {}", i.x, b.x);
            assert!((b.y - i.y).abs() < 1e-12, "y drifted: {} -> {}", i.y, b.y);
        }

        // Non-pump-head kinds pass through untouched.
        let raw = curve_points_display_to_internal(hydra::CurveKind::Generic, &[1.5], &[2.5]);
        assert_eq!(raw[0].x, 1.5);
        assert_eq!(raw[0].y, 2.5);
    }

    // ── pipe status patch validation ──────────────────────────────────────

    #[test]
    fn pipe_status_patch_rejects_unknown_values() {
        let mut network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();

        // Valid values, case-insensitive.
        apply_patch_to_network(
            &mut network,
            "pipe",
            "P1",
            "status",
            serde_json::json!("Closed"),
        )
        .unwrap();
        let p1 = network.links.iter().find(|l| l.base.id == "P1").unwrap();
        assert_eq!(p1.base.initial_status, hydra::LinkStatus::Closed);
        apply_patch_to_network(
            &mut network,
            "pipe",
            "P1",
            "status",
            serde_json::json!("open"),
        )
        .unwrap();
        let p1 = network.links.iter().find(|l| l.base.id == "P1").unwrap();
        assert_eq!(p1.base.initial_status, hydra::LinkStatus::Open);

        // Unknown string: an error naming the bad value, not silently Open.
        let err = apply_patch_to_network(
            &mut network,
            "pipe",
            "P1",
            "status",
            serde_json::json!("Ajar"),
        )
        .unwrap_err();
        assert!(err.contains("Ajar"), "error must name the value: {err}");
        // Non-string: also an error, not silently Open.
        let err =
            apply_patch_to_network(&mut network, "pipe", "P1", "status", serde_json::json!(1))
                .unwrap_err();
        assert!(err.contains("expected string"), "got: {err}");
        // The failed patches must not have changed the status.
        let p1 = network.links.iter().find(|l| l.base.id == "P1").unwrap();
        assert_eq!(p1.base.initial_status, hydra::LinkStatus::Open);
    }

    #[test]
    fn pipe_status_patch_accepts_cv_and_round_trips() {
        let pipe = |network: &hydra::Network, id: &str| {
            let l = network.links.iter().find(|l| l.base.id == id).unwrap();
            let hydra::LinkKind::Pipe(p) = &l.kind else {
                panic!("{id} is a pipe");
            };
            (l.base.initial_status, p.check_valve)
        };
        let mut network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();

        // "CV" (case-insensitive) sets the check-valve flag with Open status,
        // matching how the INP reader represents a [PIPES] CV column.
        apply_patch_to_network(
            &mut network,
            "pipe",
            "P1",
            "status",
            serde_json::json!("CV"),
        )
        .unwrap();
        assert_eq!(pipe(&network, "P1"), (hydra::LinkStatus::Open, true));

        // The CV survives an INP write → parse round trip.
        let bytes = hydra::write_inp(&network);
        let reparsed = hydra::io::parse(&bytes).unwrap();
        assert_eq!(pipe(&reparsed, "P1"), (hydra::LinkStatus::Open, true));

        // Patching back to closed/open clears the check-valve flag — the INP
        // writer emits "CV" for any check-valve pipe, so a stale flag would
        // silently override the new status on the next round trip.
        apply_patch_to_network(
            &mut network,
            "pipe",
            "P1",
            "status",
            serde_json::json!("closed"),
        )
        .unwrap();
        assert_eq!(pipe(&network, "P1"), (hydra::LinkStatus::Closed, false));
        apply_patch_to_network(
            &mut network,
            "pipe",
            "P1",
            "status",
            serde_json::json!("cv"),
        )
        .unwrap();
        apply_patch_to_network(
            &mut network,
            "pipe",
            "P1",
            "status",
            serde_json::json!("open"),
        )
        .unwrap();
        assert_eq!(pipe(&network, "P1"), (hydra::LinkStatus::Open, false));
    }

    // ── element / curve rename ────────────────────────────────────────────

    #[test]
    fn validate_inp_id_rejects_empty_whitespace_and_unsafe_chars() {
        assert!(validate_inp_id("  ", "element").is_err());
        assert!(validate_inp_id("", "element").is_err());
        assert!(validate_inp_id("a b", "element").is_err());
        assert!(validate_inp_id("a\tb", "element").is_err());
        assert!(validate_inp_id("a;b", "element").is_err());
        assert!(validate_inp_id("a\"b", "element").is_err());
        assert!(validate_inp_id("a'b", "element").is_err());
        // Valid: trims and returns.
        assert_eq!(validate_inp_id("  J-42  ", "element").unwrap(), "J-42");
    }

    /// Why every id-accepting command routes through `validate_inp_id`: INP is
    /// whitespace-delimited, so an id holding a space is written as two fields
    /// and cannot be read back. Accepting one means writing a project file the
    /// app can no longer open — the corruption surfaces later, at load, far
    /// from the edit that caused it.
    #[test]
    fn an_id_containing_a_space_makes_the_written_inp_unreadable() {
        let mut network = hydra::io::parse(CASCADE_INP.as_bytes()).unwrap();
        network.patterns.push(hydra::Pattern {
            id: "Test Pattern".into(),
            factors: vec![1.0, 2.68, 2.0],
        });

        let bytes = hydra::write_inp(&network);
        let err = hydra::io::parse(&bytes)
            .expect_err("a pattern id containing a space must not round-trip");
        let msg = format!("{err}");
        assert!(
            msg.contains("Pattern"),
            "the id's second half should surface as a bad multiplier: {msg}"
        );

        // The guard that stops it reaching the writer in the first place.
        assert!(validate_inp_id("Test Pattern", "pattern").is_err());
        assert_eq!(
            validate_inp_id("Test_Pattern", "pattern").unwrap(),
            "Test_Pattern"
        );
    }

    #[test]
    fn rename_node_cascades_maps_trace_node_and_survives_round_trip() {
        let mut network = hydra::io::parse(CASCADE_INP.as_bytes()).unwrap();
        // Attach id-keyed side state to J1 that must travel with the rename.
        network.node_tags.insert("J1".into(), "zone-a".into());
        network.options.quality_mode = hydra::QualityMode::Trace;
        network.options.trace_node = Some("J1".into());
        // Capture the endpoints referencing J1 (by index) to prove they still
        // resolve after the rename.
        let j1_idx = network
            .nodes
            .iter()
            .find(|n| n.base.id == "J1")
            .unwrap()
            .base
            .index;
        let links_on_j1: Vec<String> = network
            .links
            .iter()
            .filter(|l| l.base.from_node == j1_idx || l.base.to_node == j1_idx)
            .map(|l| l.base.id.clone())
            .collect();
        assert!(!links_on_j1.is_empty(), "fixture must attach links to J1");

        rename_element_in_network(&mut network, "junction", "J1", "J1_NEW").unwrap();

        // Node id, coordinates, tag, and trace node all follow.
        assert!(network.nodes.iter().any(|n| n.base.id == "J1_NEW"));
        assert!(!network.nodes.iter().any(|n| n.base.id == "J1"));
        assert!(network.coordinates.contains_key("J1_NEW"));
        assert!(!network.coordinates.contains_key("J1"));
        assert_eq!(
            network.node_tags.get("J1_NEW").map(String::as_str),
            Some("zone-a")
        );
        assert!(!network.node_tags.contains_key("J1"));
        assert_eq!(network.options.trace_node.as_deref(), Some("J1_NEW"));

        // Endpoints referenced J1 by index, so the same links now attach to
        // the renamed node at the same index — no dangling endpoints.
        let new_idx = network
            .nodes
            .iter()
            .find(|n| n.base.id == "J1_NEW")
            .unwrap()
            .base
            .index;
        assert_eq!(new_idx, j1_idx, "index must be stable across a rename");
        for lid in &links_on_j1 {
            let l = network.links.iter().find(|l| l.base.id == *lid).unwrap();
            assert!(l.base.from_node == new_idx || l.base.to_node == new_idx);
        }

        // Round-trip: the rename survives an INP write → parse.
        let bytes = hydra::write_inp(&network);
        let reparsed = hydra::io::parse(&bytes).unwrap();
        assert!(reparsed.nodes.iter().any(|n| n.base.id == "J1_NEW"));
        assert!(reparsed.coordinates.contains_key("J1_NEW"));
        assert_eq!(
            reparsed.node_tags.get("J1_NEW").map(String::as_str),
            Some("zone-a")
        );
        assert_eq!(reparsed.options.trace_node.as_deref(), Some("J1_NEW"));
    }

    #[test]
    fn rename_link_cascades_vertices_and_tags() {
        let mut network = hydra::io::parse(CASCADE_INP.as_bytes()).unwrap();
        network
            .vertices
            .insert("P1".into(), vec![(1.5, 2.5), (1.6, 2.6)]);
        network.link_tags.insert("P1".into(), "trunk".into());

        rename_element_in_network(&mut network, "pipe", "P1", "P1_NEW").unwrap();

        assert!(network.links.iter().any(|l| l.base.id == "P1_NEW"));
        assert!(!network.links.iter().any(|l| l.base.id == "P1"));
        assert_eq!(network.vertices.get("P1_NEW").map(Vec::len), Some(2));
        assert!(!network.vertices.contains_key("P1"));
        assert_eq!(
            network.link_tags.get("P1_NEW").map(String::as_str),
            Some("trunk")
        );
        assert!(!network.link_tags.contains_key("P1"));

        let bytes = hydra::write_inp(&network);
        let reparsed = hydra::io::parse(&bytes).unwrap();
        assert!(reparsed.links.iter().any(|l| l.base.id == "P1_NEW"));
        assert_eq!(reparsed.vertices.get("P1_NEW").map(Vec::len), Some(2));
        assert_eq!(
            reparsed.link_tags.get("P1_NEW").map(String::as_str),
            Some("trunk")
        );
    }

    #[test]
    fn rename_element_uniqueness_is_per_namespace_not_shared() {
        let mut network = hydra::io::parse(CASCADE_INP.as_bytes()).unwrap();
        // Renaming a node onto another node's id fails.
        assert!(rename_element_in_network(&mut network, "junction", "J1", "J2").is_err());
        // Renaming a link onto another link's id fails.
        assert!(rename_element_in_network(&mut network, "pipe", "P1", "P2").is_err());
        // A node and a link MAY share an id — EPANET keeps node and link ids in
        // separate namespaces, and the INP parser accepts it. So renaming a
        // node onto a link's id is allowed. (Regression test: reusing an id the
        // other namespace holds was wrongly rejected as "already in use".)
        rename_element_in_network(&mut network, "junction", "J1", "P1").unwrap();
        assert!(network.nodes.iter().any(|n| n.base.id == "P1"));
        assert!(network.links.iter().any(|l| l.base.id == "P1"));
        // Failed renames leave everything untouched; unknown element errors.
        assert!(network.nodes.iter().any(|n| n.base.id == "J2"));
        assert!(rename_element_in_network(&mut network, "junction", "NOPE", "X").is_err());
        // Renaming to the current id is a no-op success.
        rename_element_in_network(&mut network, "pipe", "P1", "P1").unwrap();
        assert!(network.links.iter().any(|l| l.base.id == "P1"));
    }

    #[test]
    fn rename_curve_cascades_to_every_reference_kind() {
        let mut network = hydra::io::parse(CASCADE_INP.as_bytes()).unwrap();
        network.curves.push(hydra::Curve {
            id: "C1".into(),
            kind: hydra::CurveKind::PumpHead,
            points: vec![hydra::CurvePoint { x: 0.0, y: 1.0 }],
        });
        // Reference C1 from a tank volume curve, a pump (head + efficiency),
        // and a GPV valve.
        for n in network.nodes.iter_mut() {
            if let hydra::NodeKind::Tank(t) = &mut n.kind {
                t.volume_curve = Some("C1".into());
            }
        }
        let (r1, j1, j2) = {
            let idx = |id: &str| {
                network
                    .nodes
                    .iter()
                    .find(|n| n.base.id == id)
                    .unwrap()
                    .base
                    .index
            };
            (idx("R1"), idx("J1"), idx("J2"))
        };
        network.links.push(hydra::Link {
            base: hydra::LinkBase {
                id: "PUMP1".into(),
                index: network.links.len() + 1,
                from_node: r1,
                to_node: j1,
                initial_status: hydra::LinkStatus::Open,
                initial_setting: None,
            },
            kind: hydra::LinkKind::Pump(hydra::Pump {
                curve_type: hydra::PumpCurveType::Custom,
                head_curve: Some("C1".into()),
                power: None,
                efficiency_curve: Some("C1".into()),
                default_efficiency: 0.75,
                speed_pattern: None,
                energy_price: None,
                price_pattern: None,
            }),
        });
        network.links.push(hydra::Link {
            base: hydra::LinkBase {
                id: "V1".into(),
                index: network.links.len() + 1,
                from_node: j1,
                to_node: j2,
                initial_status: hydra::LinkStatus::Open,
                initial_setting: Some(0.0),
            },
            kind: hydra::LinkKind::Valve(hydra::Valve {
                valve_type: hydra::ValveType::Gpv,
                diameter: 1.0,
                minor_loss: 0.0,
                curve: Some("C1".into()),
            }),
        });

        rename_curve_in_network(&mut network, "C1", "CURVE_A").unwrap();

        assert!(network.curves.iter().any(|c| c.id == "CURVE_A"));
        assert!(!network.curves.iter().any(|c| c.id == "C1"));
        let tank_curve = network.nodes.iter().find_map(|n| match &n.kind {
            hydra::NodeKind::Tank(t) => t.volume_curve.clone(),
            _ => None,
        });
        assert_eq!(tank_curve.as_deref(), Some("CURVE_A"));
        let pump = network.links.iter().find(|l| l.base.id == "PUMP1").unwrap();
        if let hydra::LinkKind::Pump(p) = &pump.kind {
            assert_eq!(p.head_curve.as_deref(), Some("CURVE_A"));
            assert_eq!(p.efficiency_curve.as_deref(), Some("CURVE_A"));
        } else {
            panic!("PUMP1 is a pump");
        }
        let valve = network.links.iter().find(|l| l.base.id == "V1").unwrap();
        if let hydra::LinkKind::Valve(v) = &valve.kind {
            assert_eq!(v.curve.as_deref(), Some("CURVE_A"));
        } else {
            panic!("V1 is a valve");
        }

        // Collision + not-found guards.
        network.curves.push(hydra::Curve {
            id: "C2".into(),
            kind: hydra::CurveKind::PumpHead,
            points: vec![hydra::CurvePoint { x: 0.0, y: 1.0 }],
        });
        assert!(rename_curve_in_network(&mut network, "CURVE_A", "C2").is_err());
        assert!(rename_curve_in_network(&mut network, "NOPE", "X").is_err());
    }
}
