//! Network wire DTOs, the shared `NetworkState` cache, DTO converters
//! (nodes/links/controls/rules/premises), internal↔display unit conversions,
//! and the read-only `get_*` commands over the cached DTO.

use serde::{Deserialize, Serialize};

/// Shared unit-conversion factors between the engine's internal US-customary
/// units and the GUI's display units (see `link_setting_internal_to_display`
/// and friends). Inverse factors used by the mutation helpers are defined
/// locally in terms of these.
pub(crate) const FT_TO_M: f64 = 0.3048;
pub(crate) const FT_TO_MM: f64 = 304.8;
/// 1 ft³ = 28.316 846 6 litres — the single ft³↔litre basis used everywhere
/// in this module (flow cfs↔L/s and volume ft³↔m³ below), so display↔internal
/// round-trips through different commands can never drift.
pub(crate) const CFS_TO_LPS: f64 = 28.316_846_6;
/// ft³ → m³, derived from the same litre basis (1 m³ = 1000 L).
pub(crate) const FT3_TO_M3: f64 = CFS_TO_LPS / 1000.0;

// ── Network load commands ─────────────────────────────────────────────────────

/// Serialisable node sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    pub id: String,
    /// "junction" | "tank" | "reservoir"
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    /// Elevation in metres (converted from internal feet). For tanks this is
    /// the tank *bottom* elevation — the same value the tank "elevation"
    /// patch accepts — not the internal `base.elevation` (bottom + min_level).
    pub elevation: f64,
    /// Base demand in L/s (converted from internal ft³/s); 0 for non-junctions.
    pub base_demand: f64,
    /// Omitted until a simulation result is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<f64>,
    /// Omitted until a simulation result is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demand: Option<f64>,
    // ── Tank-only fields ─────────────────────────────────────────────────
    // All optional fields are omitted (not serialised as `null`) when absent —
    // at 46k nodes the explicit nulls dominated the snapshot payload. The
    // frontend normalises omitted fields back to `null` on receipt.
    /// Minimum water level above bottom (m); omitted for non-tanks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tank_min_level: Option<f64>,
    /// Maximum water level above bottom (m); omitted for non-tanks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tank_max_level: Option<f64>,
    /// Initial water level above bottom (m); omitted for non-tanks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tank_initial_level: Option<f64>,
    /// Tank diameter (m); omitted for non-tanks or volume-curve tanks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tank_diameter: Option<f64>,
    /// Volume curve ID; omitted when the tank uses a simple cylindrical model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tank_volume_curve: Option<String>,
    // ── Reservoir-only fields ─────────────────────────────────────────────
    /// Pattern ID modulating head over time; omitted for reservoirs without a
    /// head pattern, and omitted for junctions / tanks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_pattern: Option<String>,
}

/// Serialisable link sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDto {
    pub id: String,
    /// "pipe" | "pump" | "valve"
    #[serde(rename = "type")]
    pub kind: String,
    pub from_id: String,
    pub to_id: String,
    /// 0.0 until a simulation result is available.
    pub velocity: f64,
    /// Diameter in mm (converted from internal ft).
    pub diameter: f64,
    /// Length in metres (converted from internal ft); 0 for pumps/valves.
    pub length: f64,
    /// Hazen-Williams roughness coefficient (C); 0 for pumps/valves.
    pub roughness: f64,
    // ── Pump-only fields ──────────────────────────────────────────────────
    // Optional fields are omitted (not serialised as `null`) when absent —
    // see the matching note on `NodeDto`.
    /// Head-flow curve ID; omitted for constant-power pumps and non-pumps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pump_curve: Option<String>,
    /// Rated power in kW; omitted for curve-based pumps and non-pumps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pump_power_kw: Option<f64>,
    /// Initial relative speed (1.0 = rated speed); omitted for non-pumps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pump_speed: Option<f64>,
    // ── Valve-only fields ────────────────────────────────────────────────
    /// Valve type: `"PRV"` | `"PSV"` | `"FCV"` | `"TCV"` | `"GPV"` | `"PBV"` | `"PCV"`;
    /// omitted for non-valves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valve_type: Option<String>,
    /// Valve setting in display units: head (m) for PRV/PSV/PBV, flow (L/s) for FCV,
    /// dimensionless loss coefficient for TCV.  Omitted for GPV/PCV (curve-based) and
    /// non-valves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valve_setting: Option<f64>,
    /// Curve ID for GPV (`GpvHeadloss`) and PCV (`PcvLossRatio`) valve types;
    /// omitted for all other types.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valve_curve: Option<String>,
}

/// Serialisable pattern sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternDto {
    pub id: String,
    /// Dimensionless multipliers [F₀, F₁, …, F_{L−1}].
    pub multipliers: Vec<f64>,
}

/// Serialisable curve sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveDto {
    pub id: String,
    /// "pump-head" | "pump-efficiency" | "pump-volume" | "tank-volume" |
    /// "gpv-headloss" | "pcv-loss-ratio" | "generic"
    pub kind: String,
    /// x-axis values. Units depend on kind (flow L/s for pump curves).
    pub x: Vec<f64>,
    /// y-axis values. Units depend on kind (head m for pump-head curves).
    pub y: Vec<f64>,
}

/// Serialisable simple control (`[CONTROLS]`) sent to the frontend.
///
/// Addressed by array position (no natural ID in the INP format) — the
/// frontend uses the index within `get_controls()`'s response array when
/// calling `update_control`/`delete_control`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDto {
    pub link_id: String,
    /// "open" | "closed"; `null` when only `action_setting` is used.
    pub action_status: Option<String>,
    /// Display-unit setting value (see `LinkDto.valve_setting` for the
    /// per-valve-type unit convention; dimensionless for pumps/pipes).
    /// `null` when only `action_status` is used.
    pub action_setting: Option<f64>,
    /// "timer" | "clocktime" | "hiLevel" | "loLevel"
    pub trigger_kind: String,
    /// Seconds — elapsed sim time for "timer", seconds-from-midnight for
    /// "clocktime". `null` for "hiLevel"/"loLevel".
    pub trigger_seconds: Option<f64>,
    /// Trigger node ID for "hiLevel"/"loLevel". `null` otherwise.
    pub trigger_node_id: Option<String>,
    /// Display-unit threshold for "hiLevel"/"loLevel": tank level above
    /// bottom (m) for tanks, pressure-equivalent head (m) for junctions and
    /// reservoirs. `null` for "timer"/"clocktime".
    pub trigger_value: Option<f64>,
    pub enabled: bool,
}

/// A single predicate clause within a `RuleDto` (mirrors `Premise`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePremiseDto {
    /// "node" | "link" | "clock"
    pub object: String,
    /// Node ID when `object == "node"`; `null` otherwise.
    pub node_id: Option<String>,
    /// Link ID when `object == "link"`; `null` otherwise.
    pub link_id: Option<String>,
    /// "head" | "pressure" | "demand" | "level" | "flow" | "status" |
    /// "setting" | "power" | "fillTime" | "drainTime" | "clockTime" | "time"
    pub attribute: String,
    /// "eq" | "neq" | "lt" | "gt" | "le" | "ge"
    pub operator: String,
    /// Display-unit threshold. For "status" this is ignored in favour of
    /// `status_value`. Units otherwise follow `attribute`: m for
    /// head/pressure/level, L/s for demand/flow, hours for fillTime/
    /// drainTime, kW for power, seconds for clockTime/time, and the
    /// per-link-kind convention (see `ControlDto.action_setting`) for
    /// "setting".
    pub value: f64,
    /// "open" | "closed" | "active"; only meaningful when `attribute == "status"`.
    pub status_value: Option<String>,
    /// Connective joining this premise to the next; `null` for the last premise.
    /// "and" | "or"
    pub connective: Option<String>,
}

/// A single action applied by a `RuleDto`'s THEN or ELSE clause.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleActionDto {
    pub link_id: String,
    /// "open" | "closed"; `null` when `setting` is used instead.
    pub status: Option<String>,
    /// Display-unit setting value (see `ControlDto.action_setting`); `null`
    /// when `status` is used instead.
    pub setting: Option<f64>,
}

/// Serialisable rule-based control (`[RULES]`) sent to the frontend.
///
/// Addressed by array position, like `ControlDto`. `name` is a display-only
/// label synthesised from position (`R1`, `R2`, …) — the engine's `Rule`
/// struct has no name field, so custom INP rule names are not preserved.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    pub name: String,
    pub priority: f64,
    pub premises: Vec<RulePremiseDto>,
    pub then_actions: Vec<RuleActionDto>,
    pub else_actions: Vec<RuleActionDto>,
}

/// The full network payload returned to the frontend after parsing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDto {
    pub nodes: Vec<NodeDto>,
    pub links: Vec<LinkDto>,
    pub patterns: Vec<PatternDto>,
    pub curves: Vec<CurveDto>,
    pub controls: Vec<ControlDto>,
    pub rules: Vec<RuleDto>,
    /// Stem of the source file name (no directory, no extension).
    /// Empty string when the DTO was constructed without a file context.
    #[serde(default)]
    pub file_stem: String,
    /// Per-link polyline vertices from the `[VERTICES]` INP section, parallel
    /// to `links` (same order, same length; an entry is empty when the link
    /// has no vertices). Never serialised to JSON — consumed only by the
    /// binary snapshot encoder ([`encode_network_snapshot`]).
    #[serde(skip)]
    pub link_vertices: Vec<Vec<(f64, f64)>>,
    /// Per-link initial-status codes (0 = open, 1 = closed, 2 = check valve;
    /// pumps/valves always 0), parallel to `links`. Never serialised to JSON —
    /// consumed only by the binary snapshot encoder
    /// ([`encode_network_snapshot`], layout v3).
    #[serde(skip)]
    pub link_initial_status: Vec<u8>,
}

/// Inner state for `NetworkState`.
#[allow(clippy::large_enum_variant)]
#[derive(Default)]
pub enum NetworkStateInner {
    #[default]
    Empty,
    Loaded {
        /// INP bytes kept for `save_project` / `create_project`.
        ///
        /// May be stale when `dirty` is `true` — mutating commands only flag
        /// the network as dirty instead of re-serialising the whole INP on
        /// every edit. Always read these bytes through
        /// [`NetworkStateInner::up_to_date_raw_bytes`].
        raw_bytes: Vec<u8>,
        /// `true` when `network` has been mutated since `raw_bytes` was last
        /// serialised from it.
        dirty: bool,
        /// Parsed network — cached to avoid re-parsing on every `patch_element` call.
        network: hydra::Network,
        dto: NetworkDto,
        /// Project that owns this network — `Some` when loaded from a project
        /// bundle (`load_project` / `load_project_network`), `None` when loaded
        /// from the file picker (`open_and_load_network`, pre-`create_project`).
        /// `save_project` refuses to write when the caller's project id does
        /// not match, so a stale `activeProjectId` in the frontend can never
        /// silently overwrite another project's `model.inp`.
        owner_project_id: Option<String>,
        /// Scenario that owns this network — `Some(id)` when the loaded INP is
        /// a scenario's `model.inp`, `None` for the base model (or a file-picker
        /// load). Lets read commands decide whether the cached parse matches a
        /// `(project_id, scenario_id)` target without re-reading from disk.
        owner_scenario_id: Option<String>,
    },
}

impl NetworkStateInner {
    /// Return the INP bytes for the loaded network, re-serialising them from
    /// the parsed network first when mutations have occurred since the last
    /// serialisation (`dirty`). Returns `None` when no network is loaded.
    ///
    /// Serialisation happens while the caller holds the state lock, but only
    /// at consumption points (save/export/run) instead of once per mutation —
    /// mutating commands merely set `dirty`.
    pub(crate) fn up_to_date_raw_bytes(&mut self) -> Option<&Vec<u8>> {
        match self {
            NetworkStateInner::Loaded {
                raw_bytes,
                dirty,
                network,
                ..
            } => {
                if *dirty {
                    *raw_bytes = hydra::write_inp(network);
                    *dirty = false;
                }
                Some(raw_bytes)
            }
            NetworkStateInner::Empty => None,
        }
    }
}

/// Tauri managed state — holds the most recently loaded network (if any).
#[derive(Default)]
pub struct NetworkState(pub parking_lot::Mutex<NetworkStateInner>);

pub(crate) fn format_inp_parse_error(err: hydra::io::ParseError) -> String {
    match err {
        hydra::io::ParseError::ValidationFailed(errors) => {
            if errors.is_empty() {
                return "validation failed".to_string();
            }

            if let Some(summary) = summarize_unknown_pattern_refs(&errors) {
                return summary;
            }

            const PREVIEW_LIMIT: usize = 2;
            let preview: Vec<String> = errors
                .iter()
                .take(PREVIEW_LIMIT)
                .map(ToString::to_string)
                .collect();

            if errors.len() > PREVIEW_LIMIT {
                format!(
                    "validation failed ({} errors): {}; and {} more",
                    errors.len(),
                    preview.join("; "),
                    errors.len() - PREVIEW_LIMIT,
                )
            } else {
                format!("validation failed: {}", preview.join("; "))
            }
        }
        other => other.to_string(),
    }
}

fn summarize_unknown_pattern_refs(errors: &[hydra::ValidationError]) -> Option<String> {
    let mut refs_by_pattern: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for err in errors {
        if let hydra::ValidationError::UnknownPatternRef {
            object_id,
            pattern_id,
        } = err
        {
            refs_by_pattern
                .entry(pattern_id.clone())
                .or_default()
                .push(object_id.clone());
        }
    }

    let (pattern_id, object_ids) = refs_by_pattern
        .iter()
        .max_by_key(|(_, object_ids)| object_ids.len())?;

    let group_count = object_ids.len();
    if group_count == 0 {
        return None;
    }

    let preview_limit = 2usize;
    let preview_list = object_ids
        .iter()
        .take(preview_limit)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let remaining_in_group = group_count.saturating_sub(preview_limit);

    fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
        if count == 1 {
            singular
        } else {
            plural
        }
    }

    let mut summary = if group_count == 1 {
        format!(
            "missing pattern '{}' referenced by {}",
            pattern_id, object_ids[0]
        )
    } else {
        let mut detail = format!(
            "missing pattern '{}' referenced by {} network {} ({})",
            pattern_id,
            group_count,
            pluralize(group_count, "element", "elements"),
            preview_list,
        );
        if remaining_in_group > 0 {
            let _ = detail.pop();
            detail.push_str(&format!(", +{} more)", remaining_in_group));
        }
        detail
    };

    let remaining_errors = errors.len().saturating_sub(group_count);
    if remaining_errors > 0 {
        summary.push_str(&format!(
            "; plus {} additional validation {}",
            remaining_errors,
            pluralize(remaining_errors, "issue", "issues")
        ));
    }

    Some(summary)
}

/// Clone one collection out of the cached `NetworkDto` under the state lock,
/// returning an empty vec when no network is loaded. Shared by the read-only
/// `get_nodes` / `get_links` / `get_patterns` / `get_curves` / `get_controls`
/// / `get_rules` commands.
fn cloned_from_dto<T: Clone>(
    state: &NetworkState,
    get: impl FnOnce(&NetworkDto) -> &[T],
) -> Vec<T> {
    match &*state.0.lock() {
        NetworkStateInner::Loaded { dto, .. } => get(dto).to_vec(),
        NetworkStateInner::Empty => vec![],
    }
}

#[tauri::command(async)]
/// Return the node list for the loaded network.
pub fn get_nodes(state: tauri::State<'_, NetworkState>) -> Vec<NodeDto> {
    cloned_from_dto(&state, |dto| &dto.nodes)
}

/// Return the links of the currently loaded network, or an empty list.
#[tauri::command(async)]
/// Return the link list for the loaded network.
pub fn get_links(state: tauri::State<'_, NetworkState>) -> Vec<LinkDto> {
    cloned_from_dto(&state, |dto| &dto.links)
}

/// Return the patterns of the currently loaded network, or an empty list.
#[tauri::command(async)]
/// Return demand/head patterns for the loaded network.
pub fn get_patterns(state: tauri::State<'_, NetworkState>) -> Vec<PatternDto> {
    cloned_from_dto(&state, |dto| &dto.patterns)
}

/// Return the curves of the currently loaded network, or an empty list.
#[tauri::command(async)]
/// Return pump/GPV/volume curves for the loaded network.
pub fn get_curves(state: tauri::State<'_, NetworkState>) -> Vec<CurveDto> {
    cloned_from_dto(&state, |dto| &dto.curves)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Build the DTO for a single node. Shared by the full `network_to_dto`
/// rebuild and the single-element delta path in `patch_element`.
pub(crate) fn node_to_dto(network: &hydra::Network, n: &hydra::Node) -> NodeDto {
    use hydra::NodeKind;

    let kind = match &n.kind {
        NodeKind::Junction(_) => "junction",
        NodeKind::Reservoir(_) => "reservoir",
        NodeKind::Tank(_) => "tank",
    };
    let (x, y) = network
        .coordinates
        .get(&n.base.id)
        .copied()
        .unwrap_or((0.0, 0.0));
    // For tanks the internal `base.elevation` is bottom + min_level (the
    // minimum piezometric head); the DTO's `elevation` is consistently the
    // tank *bottom*, matching the tank "elevation" patch in
    // `apply_patch_to_network` (and `create_node`'s `elevation` input) so a
    // DTO → patch round-trip is stable instead of silently raising the tank
    // by `min_level` on every edit.
    let elevation = match &n.kind {
        NodeKind::Tank(t) => (n.base.elevation - t.min_level) * FT_TO_M,
        _ => n.base.elevation * FT_TO_M,
    };
    let base_demand = match &n.kind {
        NodeKind::Junction(j) => j.demands.iter().map(|d| d.base_demand).sum::<f64>() * CFS_TO_LPS,
        _ => 0.0,
    };
    let (tank_min_level, tank_max_level, tank_initial_level, tank_diameter, tank_volume_curve) =
        if let NodeKind::Tank(t) = &n.kind {
            (
                Some(t.min_level * FT_TO_M),
                Some(t.max_level * FT_TO_M),
                Some(t.initial_level * FT_TO_M),
                Some(t.diameter * FT_TO_M),
                t.volume_curve.clone(),
            )
        } else {
            (None, None, None, None, None)
        };
    let head_pattern = if let NodeKind::Reservoir(r) = &n.kind {
        r.head_pattern.clone()
    } else {
        None
    };
    NodeDto {
        id: n.base.id.clone(),
        kind: kind.into(),
        x,
        y,
        elevation,
        base_demand,
        pressure: None,
        demand: None,
        tank_min_level,
        tank_max_level,
        tank_initial_level,
        tank_diameter,
        tank_volume_curve,
        head_pattern,
    }
}

/// Build the DTO for a single link with pre-resolved endpoint IDs. Shared by
/// the full `network_to_dto` rebuild and the single-element delta path.
pub(crate) fn link_to_dto(l: &hydra::Link, from_id: String, to_id: String) -> LinkDto {
    use hydra::LinkKind;

    let (kind, diameter, length, roughness) = match &l.kind {
        LinkKind::Pipe(p) => (
            "pipe",
            p.diameter * FT_TO_MM,
            p.length * FT_TO_M,
            p.roughness,
        ),
        LinkKind::Pump(_) => ("pump", 0.0, 0.0, 0.0),
        LinkKind::Valve(v) => ("valve", v.diameter * FT_TO_MM, 0.0, 0.0),
    };
    let (pump_curve, pump_power_kw, pump_speed) = if let LinkKind::Pump(p) = &l.kind {
        // power is stored in Watts; convert to kW for the DTO
        let kw = p.power.map(|pw| pw / 1000.0);
        // initial_setting on the base is the initial relative speed (ω); default 1.0
        let speed = l.base.initial_setting.or(Some(1.0));
        (p.head_curve.clone(), kw, speed)
    } else {
        (None, None, None)
    };
    let (valve_type, valve_setting, valve_curve) = if let LinkKind::Valve(v) = &l.kind {
        use hydra::ValveType;
        let vt = match v.valve_type {
            ValveType::Prv => "PRV",
            ValveType::Psv => "PSV",
            ValveType::Fcv => "FCV",
            ValveType::Tcv => "TCV",
            ValveType::Gpv => "GPV",
            ValveType::Pcv => "PCV",
            ValveType::Pbv => "PBV",
        };
        // Convert setting from internal ft/cfs/dimensionless to display units.
        let setting = match v.valve_type {
            ValveType::Prv | ValveType::Psv | ValveType::Pbv => {
                l.base.initial_setting.map(|s| s * FT_TO_M)
            }
            ValveType::Fcv => l.base.initial_setting.map(|s| s * CFS_TO_LPS),
            ValveType::Tcv => l.base.initial_setting,
            ValveType::Gpv | ValveType::Pcv => None,
        };
        (Some(vt.to_string()), setting, v.curve.clone())
    } else {
        (None, None, None)
    };
    LinkDto {
        id: l.base.id.clone(),
        kind: kind.into(),
        from_id,
        to_id,
        velocity: 0.0,
        diameter,
        length,
        roughness,
        pump_curve,
        pump_power_kw,
        pump_speed,
        valve_type,
        valve_setting,
        valve_curve,
    }
}

/// Snapshot initial-status code for one link: `0` = open, `1` = closed,
/// `2` = check valve (a pipe with `check_valve` set — CV pipes always parse
/// with `initial_status == Open`, so the CV bit takes precedence).
/// Pumps and valves are always `0`.
pub(crate) fn link_initial_status_code(l: &hydra::Link) -> u8 {
    match &l.kind {
        hydra::LinkKind::Pipe(p) if p.check_valve => 2,
        hydra::LinkKind::Pipe(_) if l.base.initial_status == hydra::LinkStatus::Closed => 1,
        _ => 0,
    }
}

pub(crate) fn network_to_dto(network: &hydra::Network) -> NetworkDto {
    // Build a node-index → node-id map for resolving link endpoints.
    let node_id_by_index: std::collections::HashMap<usize, &str> = network
        .nodes
        .iter()
        .map(|n| (n.base.index, n.base.id.as_str()))
        .collect();

    let nodes = network
        .nodes
        .iter()
        .map(|n| node_to_dto(network, n))
        .collect();

    let links = network
        .links
        .iter()
        .map(|l| {
            let from_id = node_id_by_index
                .get(&l.base.from_node)
                .map(|s| s.to_string())
                .unwrap_or_default();
            let to_id = node_id_by_index
                .get(&l.base.to_node)
                .map(|s| s.to_string())
                .unwrap_or_default();
            link_to_dto(l, from_id, to_id)
        })
        .collect();

    // `[VERTICES]` polylines in link order, parallel to `links`.
    let link_vertices = network
        .links
        .iter()
        .map(|l| {
            network
                .vertices
                .get(&l.base.id)
                .cloned()
                .unwrap_or_default()
        })
        .collect();

    let patterns = network
        .patterns
        .iter()
        .map(|p| PatternDto {
            id: p.id.clone(),
            multipliers: p.factors.clone(),
        })
        .collect();

    let curves = network
        .curves
        .iter()
        .map(|c| {
            use hydra::CurveKind;
            let kind = match c.kind {
                CurveKind::PumpHead => "pump-head",
                CurveKind::PumpEfficiency => "pump-efficiency",
                CurveKind::PumpVolume => "pump-volume",
                CurveKind::TankVolume => "tank-volume",
                CurveKind::GpvHeadloss => "gpv-headloss",
                CurveKind::PcvLossRatio => "pcv-loss-ratio",
                CurveKind::Generic => "generic",
            };
            // Pump-head: x = flow (cfs → L/s), y = head (ft → m).
            // All others: pass raw values through (unit conversion is
            // context-dependent; the frontend labels accordingly).
            let (xs, ys): (Vec<f64>, Vec<f64>) = if c.kind == CurveKind::PumpHead {
                c.points
                    .iter()
                    .map(|p| (p.x * CFS_TO_LPS, p.y * FT_TO_M))
                    .unzip()
            } else {
                c.points.iter().map(|p| (p.x, p.y)).unzip()
            };
            CurveDto {
                id: c.id.clone(),
                kind: kind.into(),
                x: xs,
                y: ys,
            }
        })
        .collect();

    NetworkDto {
        nodes,
        links,
        patterns,
        curves,
        controls: network
            .controls
            .iter()
            .map(|c| control_to_dto(c, network))
            .collect(),
        rules: network
            .rules
            .iter()
            .enumerate()
            .map(|(i, r)| rule_to_dto(i, r, network))
            .collect(),
        file_stem: String::new(),
        link_vertices,
        link_initial_status: network.links.iter().map(link_initial_status_code).collect(),
    }
}

/// Convert a link's setting from internal units to the display units used
/// throughout the GUI: dimensionless for pumps/pipes, head (m) for
/// PRV/PSV/PBV, flow (L/s) for FCV, dimensionless loss coefficient for TCV,
/// and raw (curve-based; caller should not use this) for GPV/PCV.
pub(crate) fn link_setting_internal_to_display(link: &hydra::Link, internal: f64) -> f64 {
    match &link.kind {
        hydra::LinkKind::Valve(v) => match v.valve_type {
            hydra::ValveType::Prv | hydra::ValveType::Psv | hydra::ValveType::Pbv => {
                internal * FT_TO_M
            }
            hydra::ValveType::Fcv => internal * CFS_TO_LPS,
            _ => internal,
        },
        _ => internal,
    }
}

/// Inverse of [`link_setting_internal_to_display`].
pub(crate) fn link_setting_display_to_internal(link: &hydra::Link, display: f64) -> f64 {
    match &link.kind {
        hydra::LinkKind::Valve(v) => match v.valve_type {
            hydra::ValveType::Prv | hydra::ValveType::Psv | hydra::ValveType::Pbv => {
                display / FT_TO_M
            }
            hydra::ValveType::Fcv => display / CFS_TO_LPS,
            _ => display,
        },
        _ => display,
    }
}

/// Convert a HiLevel/LowLevel trigger grade from internal absolute hydraulic
/// grade (ft) to the display threshold shown to the user: level above bottom
/// (m) for tanks, pressure-equivalent head (m) for junctions/reservoirs.
/// Mirrors `inp_writer`'s `[CONTROLS]` emission.
pub(crate) fn node_grade_internal_to_display(node: &hydra::Node, internal_grade: f64) -> f64 {
    match &node.kind {
        hydra::NodeKind::Tank(t) => {
            let bottom = node.base.elevation - t.min_level;
            (internal_grade - bottom) * FT_TO_M
        }
        _ => (internal_grade - node.base.elevation) * FT_TO_M,
    }
}

/// Inverse of [`node_grade_internal_to_display`].
pub(crate) fn node_grade_display_to_internal(node: &hydra::Node, display: f64) -> f64 {
    match &node.kind {
        hydra::NodeKind::Tank(t) => {
            let bottom = node.base.elevation - t.min_level;
            display / FT_TO_M + bottom
        }
        _ => display / FT_TO_M + node.base.elevation,
    }
}

fn link_status_to_str(status: hydra::LinkStatus) -> Option<&'static str> {
    match status {
        hydra::LinkStatus::Open => Some("open"),
        hydra::LinkStatus::Closed => Some("closed"),
        hydra::LinkStatus::Active => Some("active"),
        _ => None,
    }
}

fn link_status_from_str(s: &str) -> Option<hydra::LinkStatus> {
    match s {
        "open" => Some(hydra::LinkStatus::Open),
        "closed" => Some(hydra::LinkStatus::Closed),
        "active" => Some(hydra::LinkStatus::Active),
        _ => None,
    }
}

fn control_to_dto(ctrl: &hydra::SimpleControl, network: &hydra::Network) -> ControlDto {
    let link = network.links.get(ctrl.link.saturating_sub(1));
    let link_id = link.map(|l| l.base.id.clone()).unwrap_or_default();
    let action_status = ctrl
        .action_status
        .and_then(link_status_to_str)
        .map(Into::into);
    let action_setting = match (link, ctrl.action_setting) {
        (Some(l), Some(s)) => Some(link_setting_internal_to_display(l, s)),
        _ => None,
    };
    let (trigger_kind, trigger_seconds, trigger_node_id, trigger_value) = match ctrl.trigger_type {
        hydra::TriggerType::Timer => ("timer", ctrl.trigger_time, None, None),
        hydra::TriggerType::TimeOfDay => ("clocktime", ctrl.trigger_time, None, None),
        hydra::TriggerType::HiLevel | hydra::TriggerType::LowLevel => {
            let kind = if ctrl.trigger_type == hydra::TriggerType::HiLevel {
                "hiLevel"
            } else {
                "loLevel"
            };
            let node = ctrl
                .trigger_node
                .and_then(|idx| network.nodes.get(idx.saturating_sub(1)));
            let node_id = node.map(|n| n.base.id.clone());
            let value = match (node, ctrl.trigger_grade) {
                (Some(n), Some(g)) => Some(node_grade_internal_to_display(n, g)),
                _ => None,
            };
            (kind, None, node_id, value)
        }
    };
    ControlDto {
        link_id,
        action_status,
        action_setting,
        trigger_kind: trigger_kind.into(),
        trigger_seconds,
        trigger_node_id,
        trigger_value,
        enabled: ctrl.enabled,
    }
}

fn premise_attribute_to_str(a: hydra::PremiseAttribute) -> &'static str {
    match a {
        hydra::PremiseAttribute::Head => "head",
        hydra::PremiseAttribute::Pressure => "pressure",
        hydra::PremiseAttribute::Demand => "demand",
        hydra::PremiseAttribute::Level => "level",
        hydra::PremiseAttribute::Flow => "flow",
        hydra::PremiseAttribute::Status => "status",
        hydra::PremiseAttribute::Setting => "setting",
        hydra::PremiseAttribute::Power => "power",
        hydra::PremiseAttribute::FillTime => "fillTime",
        hydra::PremiseAttribute::DrainTime => "drainTime",
        hydra::PremiseAttribute::ClockTime => "clockTime",
        hydra::PremiseAttribute::Time => "time",
    }
}

fn premise_attribute_from_str(s: &str) -> Result<hydra::PremiseAttribute, String> {
    Ok(match s {
        "head" => hydra::PremiseAttribute::Head,
        "pressure" => hydra::PremiseAttribute::Pressure,
        "demand" => hydra::PremiseAttribute::Demand,
        "level" => hydra::PremiseAttribute::Level,
        "flow" => hydra::PremiseAttribute::Flow,
        "status" => hydra::PremiseAttribute::Status,
        "setting" => hydra::PremiseAttribute::Setting,
        "power" => hydra::PremiseAttribute::Power,
        "fillTime" => hydra::PremiseAttribute::FillTime,
        "drainTime" => hydra::PremiseAttribute::DrainTime,
        "clockTime" => hydra::PremiseAttribute::ClockTime,
        "time" => hydra::PremiseAttribute::Time,
        other => return Err(format!("unknown premise attribute '{other}'")),
    })
}

fn premise_operator_to_str(o: hydra::PremiseOperator) -> &'static str {
    match o {
        hydra::PremiseOperator::Eq => "eq",
        hydra::PremiseOperator::Neq => "neq",
        hydra::PremiseOperator::Lt => "lt",
        hydra::PremiseOperator::Gt => "gt",
        hydra::PremiseOperator::Le => "le",
        hydra::PremiseOperator::Ge => "ge",
    }
}

fn premise_operator_from_str(s: &str) -> Result<hydra::PremiseOperator, String> {
    Ok(match s {
        "eq" => hydra::PremiseOperator::Eq,
        "neq" => hydra::PremiseOperator::Neq,
        "lt" => hydra::PremiseOperator::Lt,
        "gt" => hydra::PremiseOperator::Gt,
        "le" => hydra::PremiseOperator::Le,
        "ge" => hydra::PremiseOperator::Ge,
        other => return Err(format!("unknown premise operator '{other}'")),
    })
}

/// Convert a premise/action threshold from internal units to display units,
/// given the attribute and (for node/link-scoped attributes) the referenced
/// object. See `RulePremiseDto.value` for the per-attribute unit convention.
fn premise_value_internal_to_display(
    attribute: hydra::PremiseAttribute,
    object: hydra::PremiseObject,
    value: f64,
    network: &hydra::Network,
) -> f64 {
    use hydra::{PremiseAttribute, PremiseObject};
    match attribute {
        PremiseAttribute::Head | PremiseAttribute::Pressure | PremiseAttribute::Level => {
            value * FT_TO_M
        }
        PremiseAttribute::Demand | PremiseAttribute::Flow => value * CFS_TO_LPS,
        PremiseAttribute::FillTime | PremiseAttribute::DrainTime => value / 3600.0,
        PremiseAttribute::Setting => {
            if let PremiseObject::Link(idx) = object {
                if let Some(link) = network.links.get(idx.saturating_sub(1)) {
                    return link_setting_internal_to_display(link, value);
                }
            }
            value
        }
        _ => value,
    }
}

/// Inverse of [`premise_value_internal_to_display`].
fn premise_value_display_to_internal(
    attribute: hydra::PremiseAttribute,
    object: hydra::PremiseObject,
    value: f64,
    network: &hydra::Network,
) -> f64 {
    use hydra::{PremiseAttribute, PremiseObject};
    match attribute {
        PremiseAttribute::Head | PremiseAttribute::Pressure | PremiseAttribute::Level => {
            value / FT_TO_M
        }
        PremiseAttribute::Demand | PremiseAttribute::Flow => value / CFS_TO_LPS,
        PremiseAttribute::FillTime | PremiseAttribute::DrainTime => value * 3600.0,
        PremiseAttribute::Setting => {
            if let PremiseObject::Link(idx) = object {
                if let Some(link) = network.links.get(idx.saturating_sub(1)) {
                    return link_setting_display_to_internal(link, value);
                }
            }
            value
        }
        _ => value,
    }
}

fn premise_to_dto(p: &hydra::Premise, network: &hydra::Network) -> RulePremiseDto {
    let (object, node_id, link_id) = match p.object {
        hydra::PremiseObject::Node(idx) => (
            "node",
            network
                .nodes
                .get(idx.saturating_sub(1))
                .map(|n| n.base.id.clone()),
            None,
        ),
        hydra::PremiseObject::Link(idx) => (
            "link",
            None,
            network
                .links
                .get(idx.saturating_sub(1))
                .map(|l| l.base.id.clone()),
        ),
        hydra::PremiseObject::Clock => ("clock", None, None),
    };
    let status_value = if p.attribute == hydra::PremiseAttribute::Status {
        // Status thresholds are encoded as 0/1/2 (closed/open/active) per
        // `parse_premise_value`.
        match p.value as i32 {
            1 => Some("open".to_string()),
            2 => Some("active".to_string()),
            _ => Some("closed".to_string()),
        }
    } else {
        None
    };
    RulePremiseDto {
        object: object.into(),
        node_id,
        link_id,
        attribute: premise_attribute_to_str(p.attribute).into(),
        operator: premise_operator_to_str(p.operator).into(),
        value: premise_value_internal_to_display(p.attribute, p.object, p.value, network),
        status_value,
        connective: p.connective.map(|c| match c {
            hydra::LogicOp::And => "and".into(),
            hydra::LogicOp::Or => "or".into(),
        }),
    }
}

fn rule_action_to_dto(a: &hydra::RuleAction, network: &hydra::Network) -> RuleActionDto {
    let link = network.links.get(a.link.saturating_sub(1));
    let link_id = link.map(|l| l.base.id.clone()).unwrap_or_default();
    let (status, setting) = match &a.value {
        hydra::ActionValue::Status(s) => (link_status_to_str(*s).map(Into::into), None),
        hydra::ActionValue::Setting(v) => (
            None,
            Some(link.map_or(*v, |l| link_setting_internal_to_display(l, *v))),
        ),
    };
    RuleActionDto {
        link_id,
        status,
        setting,
    }
}

fn rule_to_dto(index: usize, rule: &hydra::Rule, network: &hydra::Network) -> RuleDto {
    RuleDto {
        name: format!("R{}", index + 1),
        priority: rule.priority,
        premises: rule
            .premises
            .iter()
            .map(|p| premise_to_dto(p, network))
            .collect(),
        then_actions: rule
            .then_actions
            .iter()
            .map(|a| rule_action_to_dto(a, network))
            .collect(),
        else_actions: rule
            .else_actions
            .iter()
            .map(|a| rule_action_to_dto(a, network))
            .collect(),
    }
}

// ── Controls & rules ──────────────────────────────────────────────────────────

fn resolve_node_id(network: &hydra::Network, id: &str) -> Result<usize, String> {
    network
        .nodes
        .iter()
        .position(|n| n.base.id == id)
        .map(|p| p + 1)
        .ok_or_else(|| format!("node '{}' not found", id))
}

fn resolve_link_id(network: &hydra::Network, id: &str) -> Result<usize, String> {
    network
        .links
        .iter()
        .position(|l| l.base.id == id)
        .map(|p| p + 1)
        .ok_or_else(|| format!("link '{}' not found", id))
}

pub(crate) fn control_from_dto(
    dto: &ControlDto,
    network: &hydra::Network,
) -> Result<hydra::SimpleControl, String> {
    let link_idx = resolve_link_id(network, &dto.link_id)?;
    let link = &network.links[link_idx - 1];

    let action_status = dto
        .action_status
        .as_deref()
        .map(|s| link_status_from_str(s).ok_or_else(|| format!("invalid action status '{}'", s)))
        .transpose()?;
    let action_setting = dto
        .action_setting
        .map(|v| link_setting_display_to_internal(link, v));
    if action_status.is_none() && action_setting.is_none() {
        return Err("control must set an action status or setting".into());
    }

    let (trigger_type, trigger_time, trigger_node, trigger_grade) = match dto.trigger_kind.as_str()
    {
        "timer" => (
            hydra::TriggerType::Timer,
            Some(
                dto.trigger_seconds
                    .ok_or("timer trigger requires trigger_seconds")?,
            ),
            None,
            None,
        ),
        "clocktime" => (
            hydra::TriggerType::TimeOfDay,
            Some(
                dto.trigger_seconds
                    .ok_or("clocktime trigger requires trigger_seconds")?,
            ),
            None,
            None,
        ),
        "hiLevel" | "loLevel" => {
            let node_id = dto
                .trigger_node_id
                .as_deref()
                .ok_or("node-level trigger requires trigger_node_id")?;
            let node_idx = resolve_node_id(network, node_id)?;
            let node = &network.nodes[node_idx - 1];
            let value = dto
                .trigger_value
                .ok_or("node-level trigger requires trigger_value")?;
            let kind = if dto.trigger_kind == "hiLevel" {
                hydra::TriggerType::HiLevel
            } else {
                hydra::TriggerType::LowLevel
            };
            (
                kind,
                None,
                Some(node_idx),
                Some(node_grade_display_to_internal(node, value)),
            )
        }
        other => return Err(format!("unknown trigger kind '{}'", other)),
    };

    Ok(hydra::SimpleControl {
        link: link_idx,
        trigger_type,
        trigger_time,
        trigger_node,
        trigger_grade,
        action_status,
        action_setting,
        enabled: dto.enabled,
    })
}

fn premise_from_dto(
    dto: &RulePremiseDto,
    network: &hydra::Network,
) -> Result<hydra::Premise, String> {
    let object = match dto.object.as_str() {
        "node" => {
            let id = dto
                .node_id
                .as_deref()
                .ok_or("node premise requires node_id")?;
            hydra::PremiseObject::Node(resolve_node_id(network, id)?)
        }
        "link" => {
            let id = dto
                .link_id
                .as_deref()
                .ok_or("link premise requires link_id")?;
            hydra::PremiseObject::Link(resolve_link_id(network, id)?)
        }
        "clock" => hydra::PremiseObject::Clock,
        other => return Err(format!("unknown premise object '{}'", other)),
    };
    let attribute = premise_attribute_from_str(&dto.attribute)?;
    let operator = premise_operator_from_str(&dto.operator)?;
    let value = if attribute == hydra::PremiseAttribute::Status {
        match dto.status_value.as_deref() {
            Some("open") => 1.0,
            Some("active") => 2.0,
            _ => 0.0,
        }
    } else {
        premise_value_display_to_internal(attribute, object, dto.value, network)
    };
    let connective = match dto.connective.as_deref() {
        Some("and") => Some(hydra::LogicOp::And),
        Some("or") => Some(hydra::LogicOp::Or),
        _ => None,
    };
    Ok(hydra::Premise {
        object,
        attribute,
        operator,
        value,
        connective,
    })
}

fn rule_action_from_dto(
    dto: &RuleActionDto,
    network: &hydra::Network,
) -> Result<hydra::RuleAction, String> {
    let link_idx = resolve_link_id(network, &dto.link_id)?;
    let link = &network.links[link_idx - 1];
    let value = match (&dto.status, dto.setting) {
        (Some(s), _) => hydra::ActionValue::Status(
            link_status_from_str(s).ok_or_else(|| format!("invalid action status '{}'", s))?,
        ),
        (None, Some(v)) => hydra::ActionValue::Setting(link_setting_display_to_internal(link, v)),
        (None, None) => return Err("rule action must set a status or setting".into()),
    };
    Ok(hydra::RuleAction {
        link: link_idx,
        value,
    })
}

pub(crate) fn rule_from_dto(
    dto: &RuleDto,
    network: &hydra::Network,
) -> Result<hydra::Rule, String> {
    if dto.premises.is_empty() {
        return Err("rule must have at least one premise".into());
    }
    let premises = dto
        .premises
        .iter()
        .map(|p| premise_from_dto(p, network))
        .collect::<Result<Vec<_>, _>>()?;
    let then_actions = dto
        .then_actions
        .iter()
        .map(|a| rule_action_from_dto(a, network))
        .collect::<Result<Vec<_>, _>>()?;
    let else_actions = dto
        .else_actions
        .iter()
        .map(|a| rule_action_from_dto(a, network))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(hydra::Rule {
        priority: dto.priority,
        premises,
        then_actions,
        else_actions,
    })
}

/// Return the simple controls (`[CONTROLS]`) of the loaded network, or an empty list.
#[tauri::command(async)]
pub fn get_controls(state: tauri::State<'_, NetworkState>) -> Vec<ControlDto> {
    cloned_from_dto(&state, |dto| &dto.controls)
}

/// Return the rule-based controls (`[RULES]`) of the loaded network, or an empty list.
#[tauri::command(async)]
pub fn get_rules(state: tauri::State<'_, NetworkState>) -> Vec<RuleDto> {
    cloned_from_dto(&state, |dto| &dto.rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::binary_codec::encode_network_snapshot;
    use crate::commands::mutations::apply_patch_to_network;
    use crate::commands::test_fixtures::{loaded_state, TEST_INP};

    // ── dirty flag / delta patching ───────────────────────────────────────
    #[test]
    fn up_to_date_raw_bytes_reserialises_only_when_dirty() {
        let mut state = loaded_state();

        // Clean state: returns the original bytes untouched.
        let before = state.up_to_date_raw_bytes().unwrap().clone();
        assert_eq!(before, TEST_INP.as_bytes());

        // Mutate the network the way `patch_element` does: apply + mark dirty.
        if let NetworkStateInner::Loaded { network, dirty, .. } = &mut state {
            apply_patch_to_network(network, "pipe", "P1", "roughness", serde_json::json!(140.0))
                .unwrap();
            *dirty = true;
        }

        // The refreshed bytes must reflect the patch...
        let after = state.up_to_date_raw_bytes().unwrap().clone();
        assert_ne!(after, before);
        let reparsed = hydra::io::parse(&after).unwrap();
        let p1 = reparsed
            .links
            .iter()
            .find(|l| l.base.id == "P1")
            .expect("P1 present");
        match &p1.kind {
            hydra::LinkKind::Pipe(p) => assert!((p.roughness - 140.0).abs() < 1e-9),
            other => panic!("expected pipe, got {other:?}"),
        }
        // ...and the dirty flag must be cleared so the next read is free.
        match &state {
            NetworkStateInner::Loaded { dirty, .. } => assert!(!dirty),
            NetworkStateInner::Empty => panic!("state must stay loaded"),
        }
    }

    #[test]
    fn up_to_date_raw_bytes_none_when_empty() {
        let mut state = NetworkStateInner::Empty;
        assert!(state.up_to_date_raw_bytes().is_none());
    }

    #[test]
    fn network_to_dto_carries_link_vertices_in_link_order() {
        const VERTS_INP: &str = "\
[JUNCTIONS]
J1  10  5

[RESERVOIRS]
R1  100

[PIPES]
P1  R1  J1  1000  12  100  0  Open
P2  J1  R1  800   10  100  0  Open

[COORDINATES]
J1  1.0  2.0
R1  0.0  0.0

[VERTICES]
P2  5.5  6.5
P2  7.5  8.5

[OPTIONS]
Units  GPM

[TIMES]
Duration  0

[END]
";
        let network = hydra::io::parse(VERTS_INP.as_bytes()).unwrap();
        let dto = network_to_dto(&network);
        assert_eq!(dto.link_vertices.len(), dto.links.len());
        let p1 = dto.links.iter().position(|l| l.id == "P1").unwrap();
        let p2 = dto.links.iter().position(|l| l.id == "P2").unwrap();
        assert!(dto.link_vertices[p1].is_empty(), "P1 has no vertices");
        assert_eq!(dto.link_vertices[p2], vec![(5.5, 6.5), (7.5, 8.5)]);

        // The encoded snapshot totals match.
        let buf = encode_network_snapshot(&dto);
        assert_eq!(
            u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            2,
            "total_verts"
        );
    }

    // ── optional DTO fields are omitted, not null ─────────────────────────

    #[test]
    fn node_link_dtos_skip_absent_optional_fields() {
        let network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        let dto = network_to_dto(&network);

        let j1 = dto.nodes.iter().find(|n| n.id == "J1").unwrap();
        let json = serde_json::to_string(j1).unwrap();
        assert!(!json.contains("null"), "junction JSON has nulls: {json}");
        assert!(!json.contains("tankMinLevel"));
        assert!(!json.contains("pressure"));

        let t1 = dto.nodes.iter().find(|n| n.id == "T1").unwrap();
        let json = serde_json::to_string(t1).unwrap();
        assert!(json.contains("tankMinLevel"), "tank keeps tank fields");

        let p1 = dto.links.iter().find(|l| l.id == "P1").unwrap();
        let json = serde_json::to_string(p1).unwrap();
        assert!(!json.contains("null"), "pipe JSON has nulls: {json}");
        assert!(!json.contains("pumpCurve"));
        assert!(!json.contains("valveType"));

        // Round-trip: omitted fields deserialise back to `None`.
        let back: NodeDto = serde_json::from_str(&serde_json::to_string(j1).unwrap()).unwrap();
        assert!(back.tank_min_level.is_none());
        assert!(back.pressure.is_none());
    }

    // ── display-unit conversions ──────────────────────────────────────────

    fn valve_link(vt: hydra::ValveType) -> hydra::Link {
        hydra::Link {
            base: hydra::LinkBase {
                id: "V1".into(),
                index: 1,
                from_node: 1,
                to_node: 2,
                initial_status: hydra::LinkStatus::Open,
                initial_setting: None,
            },
            kind: hydra::LinkKind::Valve(hydra::Valve {
                valve_type: vt,
                diameter: 1.0,
                minor_loss: 0.0,
                curve: None,
            }),
        }
    }

    #[test]
    fn link_setting_conversion_round_trips_per_valve_type() {
        for (vt, internal, display) in [
            (hydra::ValveType::Prv, 100.0, 100.0 * FT_TO_M),
            (hydra::ValveType::Psv, 50.0, 50.0 * FT_TO_M),
            (hydra::ValveType::Pbv, 25.0, 25.0 * FT_TO_M),
            (hydra::ValveType::Fcv, 2.0, 2.0 * CFS_TO_LPS),
            (hydra::ValveType::Tcv, 7.5, 7.5), // dimensionless: identity
        ] {
            let link = valve_link(vt);
            let d = link_setting_internal_to_display(&link, internal);
            assert!((d - display).abs() < 1e-9, "{vt:?} to display");
            let back = link_setting_display_to_internal(&link, d);
            assert!((back - internal).abs() < 1e-9, "{vt:?} round-trip");
        }
        // Non-valve links: identity in both directions.
        let network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        let pipe = network.links.iter().find(|l| l.base.id == "P1").unwrap();
        assert_eq!(link_setting_internal_to_display(pipe, 3.5), 3.5);
        assert_eq!(link_setting_display_to_internal(pipe, 3.5), 3.5);
    }

    #[test]
    fn node_grade_conversion_round_trips_for_tank_and_junction() {
        let network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
        for id in ["T1", "J1", "R1"] {
            let node = network.nodes.iter().find(|n| n.base.id == id).unwrap();
            let internal = node.base.elevation + 12.0;
            let display = node_grade_internal_to_display(node, internal);
            let back = node_grade_display_to_internal(node, display);
            assert!((back - internal).abs() < 1e-9, "{id} round-trip");
        }
        // Tank display value is the level above the tank *bottom* in metres.
        let tank = network.nodes.iter().find(|n| n.base.id == "T1").unwrap();
        let hydra::NodeKind::Tank(t) = &tank.kind else {
            unreachable!("T1 is a tank");
        };
        let bottom = tank.base.elevation - t.min_level;
        let display = node_grade_internal_to_display(tank, bottom + 10.0);
        assert!((display - 10.0 * FT_TO_M).abs() < 1e-9);
        // Junction display value is head above the node elevation in metres.
        let j1 = network.nodes.iter().find(|n| n.base.id == "J1").unwrap();
        let display = node_grade_internal_to_display(j1, j1.base.elevation + 10.0);
        assert!((display - 10.0 * FT_TO_M).abs() < 1e-9);
    }

    // ── INP parse-error summarisation ─────────────────────────────────────

    fn unknown_pattern_err(object_id: &str, pattern_id: &str) -> hydra::ValidationError {
        hydra::ValidationError::UnknownPatternRef {
            object_id: object_id.into(),
            pattern_id: pattern_id.into(),
        }
    }

    #[test]
    fn summarize_unknown_pattern_refs_single_and_grouped() {
        // Single reference: no counts, just the pair.
        let errors = vec![unknown_pattern_err("J1", "PAT1")];
        assert_eq!(
            summarize_unknown_pattern_refs(&errors).unwrap(),
            "missing pattern 'PAT1' referenced by J1"
        );

        // Largest group is summarised with a 2-element preview + "+N more",
        // and leftover errors (other patterns) are counted separately.
        let errors = vec![
            unknown_pattern_err("J1", "PAT1"),
            unknown_pattern_err("J2", "PAT1"),
            unknown_pattern_err("J3", "PAT1"),
            unknown_pattern_err("J9", "PAT2"),
        ];
        assert_eq!(
            summarize_unknown_pattern_refs(&errors).unwrap(),
            "missing pattern 'PAT1' referenced by 3 network elements (J1, J2, +1 more); \
             plus 1 additional validation issue"
        );

        // No unknown-pattern errors: no summary.
        assert!(summarize_unknown_pattern_refs(&[]).is_none());
    }

    #[test]
    fn format_inp_parse_error_previews_generic_validation_errors() {
        assert_eq!(
            format_inp_parse_error(hydra::io::ParseError::ValidationFailed(vec![])),
            "validation failed"
        );

        let errs = vec![
            hydra::ValidationError::LinkUnknownFromNode {
                link_id: "P1".into(),
                node_index: 9,
            },
            hydra::ValidationError::LinkUnknownFromNode {
                link_id: "P2".into(),
                node_index: 9,
            },
            hydra::ValidationError::LinkUnknownFromNode {
                link_id: "P3".into(),
                node_index: 9,
            },
        ];
        let msg = format_inp_parse_error(hydra::io::ParseError::ValidationFailed(errs));
        assert!(
            msg.starts_with("validation failed (3 errors):"),
            "got: {msg}"
        );
        assert!(msg.ends_with("and 1 more"), "got: {msg}");
    }

    #[test]
    fn format_inp_parse_error_renders_section_and_line_for_reader_errors() {
        // A real reader error (malformed junction elevation) must surface the
        // section name, the 1-based line number, and the offending value.
        let inp = b"[JUNCTIONS]\nJ1    not-a-number    10\n\n[RESERVOIRS]\nR1    100\n\n\
                    [PIPES]\nP1    R1    J1    1000    12    100    0    Open\n\n\
                    [OPTIONS]\nUnits    GPM\nHeadloss    H-W\n";
        let err = hydra::io::parse(inp).expect_err("malformed elevation must fail");
        let msg = format_inp_parse_error(err);
        assert!(msg.contains("[JUNCTIONS] line 2"), "got: {msg}");
        assert!(msg.contains("not-a-number"), "got: {msg}");
    }

    #[test]
    fn format_inp_parse_error_renders_duplicate_id() {
        let inp = b"[JUNCTIONS]\nJ1    0    10\nJ1    0    20\n\n[RESERVOIRS]\nR1    100\n\n\
                    [PIPES]\nP1    R1    J1    1000    12    100    0    Open\n\n\
                    [OPTIONS]\nUnits    GPM\nHeadloss    H-W\n";
        let err = hydra::io::parse(inp).expect_err("duplicate node ID must fail");
        let msg = format_inp_parse_error(err);
        assert!(msg.contains("duplicate node ID 'J1'"), "got: {msg}");
    }
}
