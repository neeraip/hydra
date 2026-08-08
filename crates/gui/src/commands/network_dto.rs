//! Network wire DTOs, the shared `NetworkState` cache, DTO converters
//! (nodes/links/controls/rules/premises), internal↔display unit conversions,
//! and the read-only `get_*` commands over the cached DTO.

use serde::{Deserialize, Serialize};

/// Scale factors from the engine's internal units to the GUI's display
/// units.
///
/// **The engine stores SI throughout** — metres, m³/s, m³ — and the wds
/// model spec §3 makes that a guarantee callers may rely on, not an
/// implementation detail: conversion from the file's declared unit system
/// happens inside the parser, at the I/O boundary. So a length is already
/// a length in metres by the time it reaches this module, and the only
/// factors that belong here are the two where the *display* unit is not
/// the SI base unit: millimetres for diameters, litres per second for
/// flows.
///
/// This module used to hold `FT_TO_M`, `FT_TO_MM` and `CFS_TO_LPS`,
/// converting as though the engine stored EPANET's US-customary units. It
/// does not, so every dimensional value was scaled a second time — served
/// 3.28× small for lengths and 35.3× small for demands — and the mutation
/// helpers applied the same factors inverted, writing a 3.28× wrong value
/// into the model while the GUI redisplayed whatever the user had typed.
/// Nothing caught it for the life of the repo because every test of it was
/// a round trip, and the error cancels in one. See the `unit_boundary`
/// tests at the foot of this file, which assert absolute values instead.
pub(crate) const M_TO_MM: f64 = 1000.0;
pub(crate) const M3S_TO_LPS: f64 = 1000.0;

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
    /// Base demand in L/s (scaled from the engine's m³/s); 0 for non-junctions.
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
    /// Diameter in mm (scaled from the engine's metres).
    pub diameter: f64,
    /// Length in metres, as the engine stores it; 0 for pumps/valves.
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
    // ── Delta-event-only fields ───────────────────────────────────────────
    // The full snapshot ships these through dedicated binary columns
    // (`NetworkDto::link_vertices` / `link_initial_status`), so `link_to_dto`
    // leaves them `None`. They are populated only by `refresh_element_dto`
    // (mutations.rs) so the JSON delta carried by `network-changed` events is
    // shape-complete: the frontend replaces its link object wholesale with
    // this DTO, and omitting them silently stripped a patched pipe's polyline
    // vertices and initial status from frontend state until the next full
    // snapshot refetch.
    /// Intermediate polyline vertices `[x, y]` (source CRS, endpoints
    /// excluded). Omitted for straight links and outside delta payloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertices: Option<Vec<(f64, f64)>>,
    /// Initial `[STATUS]`: `"open"` | `"closed"` | `"cv"`. Pipes only;
    /// omitted for pumps/valves and outside delta payloads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_status: Option<String>,
}

/// Serialisable pattern sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternDto {
    pub id: String,
    /// Dimensionless multipliers [F₀, F₁, …, F_{L−1}].
    pub multipliers: Vec<f64>,
}

/// One axis of a curve: what it measures, and in what.
///
/// Serialize-only, like every DTO that embeds an engine `QuantityDescriptor`
/// (the descriptor is authored by the engine and only ever travels outward).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveAxisDto {
    /// Human label for this axis, e.g. "Flow" or "Surface area".
    pub label: String,
    /// §5 quantity for the values on this axis; absent = unitless, or a
    /// curve whose purpose (and therefore units) the model never declares.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<hydra::common::QuantityDescriptor>,
}

/// Serialisable curve sent to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveDto {
    pub id: String,
    /// "generic" | "pump-head" | "pump-efficiency" | "tank-volume" |
    /// "gpv-headloss" | "pcv-loss-ratio"
    ///
    /// A kind the engine gains after this build is reported as "generic",
    /// since that is what an unrecognised purpose already means here.
    pub kind: String,
    /// x-axis values, in the SI display unit of this kind's first axis
    /// (see `list_curve_axes`).
    pub x: Vec<f64>,
    /// y-axis values, in the SI display unit of this kind's second axis.
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
// Serialize-only: this DTO is built from a parsed network and sent to the
// frontend, never read back. Its `Deserialize` was vestigial, and became
// impossible once curves began carrying engine `QuantityDescriptor`s.
#[derive(Debug, Clone, Default, Serialize)]
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
    /// binary snapshot encoder
    /// ([`encode_network_snapshot`](super::encode_network_snapshot)).
    #[serde(skip)]
    pub link_vertices: Vec<Vec<(f64, f64)>>,
    /// Per-link initial-status codes (0 = open, 1 = closed, 2 = check valve;
    /// pumps/valves always 0), parallel to `links`. Never serialised to JSON —
    /// consumed only by the binary snapshot encoder
    /// ([`encode_network_snapshot`](super::encode_network_snapshot),
    /// layout v3).
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
        /// Parsed network — cached to avoid re-parsing on every mutating call.
        ///
        /// Behind an `Arc` so read commands that need the whole network while
        /// *not* holding the state lock (report rendering, validation) take a
        /// pointer copy instead of a deep clone: at 46k nodes + 46k links that
        /// clone showed up on every project/scenario switch. Mutations go
        /// through `Arc::make_mut`, which only copies while another reader is
        /// still holding the previous version.
        network: std::sync::Arc<hydra::Network>,
        dto: NetworkDto,
        /// Project that owns this network — `Some` when loaded from a project
        /// bundle (`load_project_network`), `None` when loaded
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
    /// A loaded urban-drainage model: viewable and runnable, never editable
    /// (mutating commands refuse this variant), so it is never dirty and its
    /// text is always current. The viewer DTO/snapshot arrives with the
    /// descriptor-driven snapshot layout; until then the canvas shows an
    /// empty network for these projects.
    LoadedUds {
        /// The model text as imported — served back for save/create.
        raw_text: String,
        /// Parsed uds network, for validation findings and future viewing.
        network: std::sync::Arc<hydra::uds::model::Network>,
        /// Same ownership semantics as `Loaded`.
        owner_project_id: Option<String>,
        /// Same ownership semantics as `Loaded`.
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
            NetworkStateInner::LoadedUds { .. } => None,
            NetworkStateInner::Empty => None,
        }
    }

    /// The uds model text when a uds network is loaded — the read-only
    /// counterpart of [`Self::up_to_date_raw_bytes`].
    pub(crate) fn uds_raw_text(&self) -> Option<&str> {
        match self {
            NetworkStateInner::LoadedUds { raw_text, .. } => Some(raw_text),
            _ => None,
        }
    }

    /// Current model bytes for save/create, whichever engine is loaded:
    /// wds re-serialises when dirty, uds text is always current.
    pub(crate) fn current_model_bytes(&mut self) -> Option<Vec<u8>> {
        if let Some(text) = self.uds_raw_text() {
            return Some(text.as_bytes().to_vec());
        }
        self.up_to_date_raw_bytes().cloned()
    }
}

/// Tauri managed state — holds the most recently loaded network (if any).
#[derive(Default)]
pub struct NetworkState(pub parking_lot::Mutex<NetworkStateInner>);

/// Render a read failure for display.
///
/// A foreign dialect is deliberately worded as an engine mismatch rather than a
/// fault: the file is a sound model, just not one this engine reads, and
/// telling the user their network is invalid would be simply untrue (model spec
/// §4.1.2).
///
/// When the named tool's engine is GUI-openable, the message says how to
/// open it (create a project under that engine); otherwise it points at
/// the CLI, which runs every available engine.
pub(crate) fn format_read_error(err: hydra::io::ReadError) -> String {
    match err {
        hydra::io::ReadError::ForeignDialect { tool, section } => {
            let openable_engine = hydra::common::ENGINES.iter().find(|e| {
                e.import.iter().any(|f| f.label.contains(tool))
                    && super::projects::GUI_OPENABLE_ENGINES.contains(&e.key)
            });
            let advice = match openable_engine {
                Some(e) => format!(
                    "To open it here, create a new {} project and import this file.",
                    e.label
                ),
                None => {
                    format!("The GUI cannot open {tool} models yet — the hydra CLI can run them.")
                }
            };
            format!(
                "This is a {tool} model, not a water-distribution one. \
                 It declares a [{section}] section, which EPANET has no concept of. {advice}"
            )
        }
        other => other.to_string(),
    }
}

pub(crate) fn format_inp_parse_error(err: hydra::io::ParseError) -> String {
    match err {
        hydra::io::ParseError::Read(err) => format_read_error(err),
        hydra::io::ParseError::NotSimulable(errors) => {
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
/// `get_patterns` / `get_curves` / `get_controls` / `get_rules` commands.
fn cloned_from_dto<T: Clone>(
    state: &NetworkState,
    get: impl FnOnce(&NetworkDto) -> &[T],
) -> Vec<T> {
    match &*state.0.lock() {
        NetworkStateInner::Loaded { dto, .. } => get(dto).to_vec(),
        NetworkStateInner::LoadedUds { .. } | NetworkStateInner::Empty => vec![],
    }
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

#[tauri::command(async)]
/// Return the loaded network's `[TITLE]` lines (empty when no network is
/// loaded or the model has no title).
pub fn get_network_title(state: tauri::State<'_, NetworkState>) -> Vec<String> {
    match &*state.0.lock() {
        NetworkStateInner::Loaded { network, .. } => network.title.clone(),
        NetworkStateInner::LoadedUds { network, .. } => network.title.clone(),
        NetworkStateInner::Empty => Vec::new(),
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Build the DTO for a single node. Shared by the full `network_to_dto`
/// rebuild and the single-element delta path in `patch_elements`.
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
    // Lengths pass through: the engine's metres are the DTO's metres.
    let elevation = match &n.kind {
        NodeKind::Tank(t) => n.base.elevation - t.min_level,
        _ => n.base.elevation,
    };
    let base_demand = match &n.kind {
        NodeKind::Junction(j) => j.demands.iter().map(|d| d.base_demand).sum::<f64>() * M3S_TO_LPS,
        _ => 0.0,
    };
    let (tank_min_level, tank_max_level, tank_initial_level, tank_diameter, tank_volume_curve) =
        if let NodeKind::Tank(t) = &n.kind {
            (
                Some(t.min_level),
                Some(t.max_level),
                Some(t.initial_level),
                Some(t.diameter),
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
        LinkKind::Pipe(p) => ("pipe", p.diameter * M_TO_MM, p.length, p.roughness),
        LinkKind::Pump(_) => ("pump", 0.0, 0.0, 0.0),
        LinkKind::Valve(v) => ("valve", v.diameter * M_TO_MM, 0.0, 0.0),
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
        // A valve's setting means a different quantity per type: a pressure
        // or head in metres, a flow, or a dimensionless coefficient.
        let setting = match v.valve_type {
            ValveType::Prv | ValveType::Psv | ValveType::Pbv => l.base.initial_setting,
            ValveType::Fcv => l.base.initial_setting.map(|s| s * M3S_TO_LPS),
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
        // Delta-event-only — see the field docs; populated by
        // `refresh_element_dto`, never here (the snapshot carries these in
        // its dedicated binary columns instead).
        vertices: None,
        initial_status: None,
    }
}

/// Human-readable initial-status value for one snapshot status code —
/// the string form the frontend `Link.initialStatus` field and the pipe
/// "status" patch arm use.
pub(crate) fn link_initial_status_str(code: u8) -> &'static str {
    match code {
        1 => "closed",
        2 => "cv",
        _ => "open",
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
            let kind = curve_kind_id(c.kind);
            // Values are scaled here; what the axes *are* is served once
            // per engine by `list_curve_axes`, keyed on this `kind`, rather
            // than repeated on every curve in the network.
            let [ax, ay] = curve_axes(c.kind);
            let (xs, ys): (Vec<f64>, Vec<f64>) = c
                .points
                .iter()
                .map(|p| (p.x * ax.scale, p.y * ay.scale))
                .unzip();
            CurveDto {
                id: c.id.clone(),
                kind: kind.to_string(),
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

/// One curve axis: what it measures, the §5 quantity its values carry, and
/// the scale from the engine's internal SI to that quantity's SI display
/// unit.
pub(crate) struct CurveAxis {
    label: &'static str,
    quantity: Option<&'static str>,
    scale: f64,
}

impl CurveAxis {
    /// Internal SI → this axis's display unit. Callers going the other way
    /// divide by it.
    pub(crate) fn scale(&self) -> f64 {
        self.scale
    }

    fn dto(&self) -> CurveAxisDto {
        CurveAxisDto {
            label: self.label.to_string(),
            quantity: self
                .quantity
                .and_then(|k| hydra::descriptors::QUANTITIES.iter().find(|q| q.key == k))
                .copied(),
        }
    }
}

const fn axis(label: &'static str, quantity: Option<&'static str>, scale: f64) -> CurveAxis {
    CurveAxis {
        label,
        quantity,
        scale,
    }
}

/// Wire id for a curve kind — the key `list_curve_axes` is looked up by.
pub(crate) fn curve_kind_id(kind: hydra::CurveKind) -> &'static str {
    use hydra::CurveKind::*;
    match kind {
        PumpHead => "pump-head",
        PumpEfficiency => "pump-efficiency",
        TankVolume => "tank-volume",
        GpvHeadloss => "gpv-headloss",
        PcvLossRatio => "pcv-loss-ratio",
        // `Generic`, and any kind added to the engine after this build:
        // both mean "purpose unknown here", which is what `generic`
        // already denotes — unlabelled, unscaled axes rather than a guess.
        _ => "generic",
    }
}

/// The axes of every curve kind this engine's models can contain.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveKindAxesDto {
    /// Matches `CurveDto.kind`.
    pub kind: String,
    pub axes: [CurveAxisDto; 2],
}

/// What each curve kind's axes are — served once per engine rather than
/// repeated on every curve.
///
/// Keyed by kind because that is what determines the answer, which also
/// means the editor can ask about a curve that does not exist yet: a curve
/// staged in the draft has no server-side counterpart to read axes from,
/// and hand-writing them in the frontend meant a newly created curve
/// rendered without units and stored what the user typed as though it were
/// already SI.
///
/// Empty for engines whose curves this GUI does not edit. Drainage serves
/// its curve axes alongside the values, in `get_collection_detail` — it can,
/// because those curves are only ever read.
#[tauri::command]
pub fn list_curve_axes(engine: String) -> Vec<CurveKindAxesDto> {
    if engine != "wds" {
        return Vec::new();
    }
    // The engine's own list, not a copy of it: `CurveKind` is
    // `#[non_exhaustive]`, so a kind added there would never make the
    // compiler look at this function.
    hydra::CurveKind::ALL
        .iter()
        .map(|&k| {
            let [x, y] = curve_axes(k);
            CurveKindAxesDto {
                kind: curve_kind_id(k).to_string(),
                axes: [x.dto(), y.dto()],
            }
        })
        .collect()
}

/// What a curve's two axes are, by what the curve is *for*.
///
/// The single authority for both directions: `network_to_dto` scales
/// outbound by `scale` and `curve_points_display_to_internal` divides by the
/// same, so a get → update round-trip cannot drift however the table grows.
///
/// A curve's kind is inferred from what references it (wds model spec §2.3),
/// so these are facts about the model, not preferences. Before this table
/// the editor named every axis flow-and-head and converted as though every
/// curve were a pump curve, which rendered a tank's volume curve in the
/// wrong units under the wrong names.
pub(crate) fn curve_axes(kind: hydra::CurveKind) -> [CurveAxis; 2] {
    use hydra::CurveKind::*;
    match kind {
        PumpHead => [
            axis("Flow", Some("flow"), M3S_TO_LPS),
            axis("Head", Some("head"), 1.0),
        ],
        PumpEfficiency => [
            axis("Flow", Some("flow"), M3S_TO_LPS),
            // Stored as a percentage already (§2.3 bounds it to (0, 100]).
            axis("Efficiency", Some("percent"), 1.0),
        ],
        TankVolume => [
            axis("Level", Some("length"), 1.0),
            axis("Volume", Some("volume"), 1.0),
        ],
        GpvHeadloss => [
            axis("Flow", Some("flow"), M3S_TO_LPS),
            axis("Head loss", Some("head"), 1.0),
        ],
        // Both axes are percentages: the solver evaluates this curve at the
        // valve's percent-open setting and divides the result by 100 to get
        // the loss ratio it applies.
        PcvLossRatio => [
            axis("Position", Some("percent"), 1.0),
            axis("Loss ratio", Some("percent"), 1.0),
        ],
        // A curve nothing references. Its purpose is unknown, so no unit
        // interpretation may be imposed on it — the importer does not
        // convert its points either, so they are still in whatever units
        // the source file used. Naming a quantity here would be a guess
        // that changes the numbers.
        //
        // A kind added to the engine after this build lands here for the
        // same reason: unknown purpose, so unlabelled and unscaled.
        _ => [axis("X", None, 1.0), axis("Y", None, 1.0)],
    }
}

/// Convert a link's setting from internal units to the display units used
/// throughout the GUI: dimensionless for pumps/pipes, head (m) for
/// PRV/PSV/PBV, flow (L/s) for FCV, dimensionless loss coefficient for TCV,
/// and raw (curve-based; caller should not use this) for GPV/PCV.
pub(crate) fn link_setting_internal_to_display(link: &hydra::Link, internal: f64) -> f64 {
    match &link.kind {
        hydra::LinkKind::Valve(v) => match v.valve_type {
            hydra::ValveType::Prv | hydra::ValveType::Psv | hydra::ValveType::Pbv => internal,
            hydra::ValveType::Fcv => internal * M3S_TO_LPS,
            _ => internal,
        },
        _ => internal,
    }
}

/// Inverse of [`link_setting_internal_to_display`].
pub(crate) fn link_setting_display_to_internal(link: &hydra::Link, display: f64) -> f64 {
    match &link.kind {
        hydra::LinkKind::Valve(v) => match v.valve_type {
            hydra::ValveType::Prv | hydra::ValveType::Psv | hydra::ValveType::Pbv => display,
            hydra::ValveType::Fcv => display / M3S_TO_LPS,
            _ => display,
        },
        _ => display,
    }
}

/// Convert a HiLevel/LowLevel trigger grade from internal absolute hydraulic
/// grade (m) to the display threshold shown to the user: level above bottom
/// (m) for tanks, pressure-equivalent head (m) for junctions/reservoirs.
/// Mirrors `inp_writer`'s `[CONTROLS]` emission.
pub(crate) fn node_grade_internal_to_display(node: &hydra::Node, internal_grade: f64) -> f64 {
    match &node.kind {
        hydra::NodeKind::Tank(t) => {
            let bottom = node.base.elevation - t.min_level;
            internal_grade - bottom
        }
        _ => internal_grade - node.base.elevation,
    }
}

/// Inverse of [`node_grade_internal_to_display`].
pub(crate) fn node_grade_display_to_internal(node: &hydra::Node, display: f64) -> f64 {
    match &node.kind {
        hydra::NodeKind::Tank(t) => {
            let bottom = node.base.elevation - t.min_level;
            display + bottom
        }
        _ => display + node.base.elevation,
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
        PremiseAttribute::Head | PremiseAttribute::Pressure | PremiseAttribute::Level => value,
        PremiseAttribute::Demand | PremiseAttribute::Flow => value * M3S_TO_LPS,
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
        PremiseAttribute::Head | PremiseAttribute::Pressure | PremiseAttribute::Level => value,
        PremiseAttribute::Demand | PremiseAttribute::Flow => value / M3S_TO_LPS,
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

        // Mutate the network the way `patch_elements` does: apply + mark dirty.
        if let NetworkStateInner::Loaded { network, dirty, .. } = &mut state {
            let network = std::sync::Arc::make_mut(network);
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
            _ => panic!("state must stay loaded"),
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
        // Expectations are written as literals, not as `internal * FACTOR`.
        // Phrasing them in terms of the conversion constant is how this test
        // passed while every value it checked was wrong: it asserted that the
        // code agreed with itself.
        for (vt, internal, display) in [
            // A pressure/head setting is already metres internally.
            (hydra::ValveType::Prv, 100.0, 100.0),
            (hydra::ValveType::Psv, 50.0, 50.0),
            (hydra::ValveType::Pbv, 25.0, 25.0),
            // A flow setting is m³/s internally, L/s on the wire.
            (hydra::ValveType::Fcv, 2.0, 2000.0),
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
        assert!((display - 10.0).abs() < 1e-9, "10 m above bottom is 10 m");
        // Junction display value is head above the node elevation in metres.
        let j1 = network.nodes.iter().find(|n| n.base.id == "J1").unwrap();
        let display = node_grade_internal_to_display(j1, j1.base.elevation + 10.0);
        assert!((display - 10.0).abs() < 1e-9, "10 m of head is 10 m");
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
            format_inp_parse_error(hydra::io::ParseError::NotSimulable(vec![])),
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
        let msg = format_inp_parse_error(hydra::io::ParseError::NotSimulable(errs));
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
    fn format_inp_parse_error_explains_a_swmm_file_picked_by_mistake() {
        // Both engines' models are `.inp`, so the picker cannot stop this —
        // the message is the only thing telling the user what went wrong.
        let inp = b"[JUNCTIONS]\nJ1  12.0  3.0  0  0  0\n\n\
                    [CONDUITS]\nC1  J1  J2  400  0.01  0  0  0\n";
        let err = hydra::io::parse(inp).expect_err("a SWMM model must not load as EPANET");
        let msg = format_inp_parse_error(err);
        assert!(msg.contains("SWMM"), "got: {msg}");
        assert!(msg.contains("[CONDUITS]"), "got: {msg}");
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

/// The unit boundary between the engine and the GUI, asserted against
/// **known absolute values** rather than round trips.
///
/// Every test here failed when it was written. The GUI had converted as
/// though the engine stored EPANET's US-customary units, but the engine
/// stores SI throughout (wds model spec §3, "any layer above the I/O
/// boundary may rely on all model quantities being in SI"), so every
/// dimensional value was scaled a second time: lengths, elevations and
/// diameters served 3.28× small, demands 35.3× small — and, because the
/// write path applied the same factors inverted, every edit wrote a value
/// 3.28× wrong into the model while the GUI redisplayed what the user had
/// typed.
///
/// It survived for the life of the repo because every test of it was a
/// round trip, and the error is perfectly symmetric under one: patch 42,
/// read back 42, both halves wrong by the same factor. That is why these
/// tests name a real model with real numbers and check the arithmetic
/// against the source file, and why none of them may be rewritten as a
/// round trip.
#[cfg(test)]
mod unit_boundary {
    use super::*;

    /// A US-customary model, deliberately: for a metric model the internal
    /// SI value and the file's own number coincide, so a metric fixture
    /// cannot tell a correct conversion from a missing one.
    ///
    /// J1: elevation 100 ft (= 30.48 m), demand 50 gpm (= 3.1545 L/s).
    /// P1: 1000 ft (= 304.8 m) long, 12 in (= 304.8 mm) across.
    /// T1: bottom 80 ft, min level 5 ft (= 1.524 m), max 20 ft (= 6.096 m).
    const US_MODEL: &str = "\
[JUNCTIONS]
J1  100  50

[RESERVOIRS]
R1  200

[TANKS]
T1  80  10  5  20  60  0

[PIPES]
P1  R1  J1  1000  12  100  0  Open

[OPTIONS]
Units  GPM
Headloss  H-W

[TIMES]
Duration  0
";

    fn dto() -> NetworkDto {
        network_to_dto(&hydra::io::parse(US_MODEL.as_bytes()).expect("fixture parses"))
    }

    /// Relative comparison, because the expected values here are exact
    /// textbook conversions and the engine's are EPANET's rounded ones
    /// (3.28084 ft/m, 35.315 cfs per m³/s). A 1000 ft pipe therefore comes
    /// back as 304.8037 m, not 304.8 — right to about five significant
    /// figures, and wrong by 3.28× if this fix regresses. The tolerance is
    /// far tighter than any unit error and far looser than that rounding.
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-4 * b.abs().max(1.0)
    }

    #[test]
    fn elevations_are_served_in_metres() {
        let dto = dto();
        let j1 = dto.nodes.iter().find(|n| n.id == "J1").unwrap();
        // 100 ft = 30.48 m. Serving 9.29 would be metres scaled by 0.3048
        // a second time.
        assert!(
            close(j1.elevation, 30.48),
            "J1 is at 100 ft = 30.48 m, got {}",
            j1.elevation
        );
    }

    #[test]
    fn demands_are_served_in_litres_per_second() {
        let dto = dto();
        let j1 = dto.nodes.iter().find(|n| n.id == "J1").unwrap();
        // 50 gpm = 3.1545 L/s.
        assert!(
            close(j1.base_demand, 3.1545),
            "J1 demands 50 gpm = 3.1545 L/s, got {}",
            j1.base_demand
        );
    }

    #[test]
    fn pipe_length_is_metres_and_diameter_is_millimetres() {
        let dto = dto();
        let p1 = dto.links.iter().find(|l| l.id == "P1").unwrap();
        assert!(
            close(p1.length, 304.8),
            "P1 is 1000 ft = 304.8 m long, got {}",
            p1.length
        );
        // The two happen to share a number — 1000 ft in metres and 12 in in
        // millimetres are both 304.8 — which is a coincidence of the
        // fixture, not a shared factor. Both are checked because they take
        // different conversions.
        assert!(
            close(p1.diameter, 304.8),
            "P1 is 12 in = 304.8 mm across, got {}",
            p1.diameter
        );
    }

    #[test]
    fn tank_levels_are_served_in_metres() {
        let dto = dto();
        let t1 = dto.nodes.iter().find(|n| n.id == "T1").unwrap();
        assert!(
            close(t1.tank_min_level.unwrap(), 1.524),
            "T1's min level is 5 ft = 1.524 m, got {:?}",
            t1.tank_min_level
        );
        assert!(
            close(t1.tank_max_level.unwrap(), 6.096),
            "T1's max level is 20 ft = 6.096 m, got {:?}",
            t1.tank_max_level
        );
    }

    /// The one that matters most: an edit must reach the model as the value
    /// the user meant, not merely come back out looking like it.
    #[test]
    fn an_edited_elevation_reaches_the_model_and_the_exported_file() {
        let mut network = hydra::io::parse(US_MODEL.as_bytes()).unwrap();
        crate::commands::mutations::apply_patch_to_network(
            &mut network,
            "junction",
            "J1",
            "elevation",
            serde_json::json!(42.0),
        )
        .unwrap();

        // The engine stores SI, so "42 m" is 42.
        let j1 = network.nodes.iter().find(|n| n.base.id == "J1").unwrap();
        assert!(
            close(j1.base.elevation, 42.0),
            "an elevation set to 42 m must be stored as 42 m, got {}",
            j1.base.elevation
        );

        // And the file the user exports must say so. The model is GPM, so
        // [JUNCTIONS] carries feet: 42 m = 137.795 ft. Writing 452 ft here
        // was the corruption, invisible because the DTO scaled it back.
        let inp = String::from_utf8_lossy(&hydra::io::write_inp(&network)).into_owned();
        let j1_line = inp
            .lines()
            .find(|l| l.trim_start().starts_with("J1 "))
            .expect("J1 is written");
        let elev: f64 = j1_line.split_whitespace().nth(1).unwrap().parse().unwrap();
        assert!(
            close(elev, 137.795),
            "42 m must export as 137.795 ft, got {elev} (line: {j1_line})"
        );
    }
}

/// What each curve kind's axes are, and that the outbound scale and its
/// inverse agree — checked against absolute values, the way the unit
/// boundary above is.
#[cfg(test)]
mod curve_axis_boundary {
    use super::*;
    use crate::commands::mutations::curve_points_display_to_internal;

    fn axes_of(kind: hydra::CurveKind) -> [CurveAxisDto; 2] {
        let [x, y] = curve_axes(kind);
        [x.dto(), y.dto()]
    }

    fn quantity_key(a: &CurveAxisDto) -> Option<&str> {
        a.quantity.as_ref().map(|q| q.key)
    }

    /// The whole point of serving axes: what a curve's numbers *are*
    /// depends on what the curve is for. A single flow/head assumption
    /// described four of the six kinds wrongly.
    #[test]
    fn each_kind_names_its_own_axes() {
        use hydra::CurveKind::*;
        for (kind, x_label, x_q, y_label, y_q) in [
            (PumpHead, "Flow", Some("flow"), "Head", Some("head")),
            (
                PumpEfficiency,
                "Flow",
                Some("flow"),
                "Efficiency",
                Some("percent"),
            ),
            (
                TankVolume,
                "Level",
                Some("length"),
                "Volume",
                Some("volume"),
            ),
            (GpvHeadloss, "Flow", Some("flow"), "Head loss", Some("head")),
            (
                PcvLossRatio,
                "Position",
                Some("percent"),
                "Loss ratio",
                Some("percent"),
            ),
            (Generic, "X", None, "Y", None),
        ] {
            let [x, y] = axes_of(kind);
            assert_eq!(x.label, x_label, "{kind:?} x label");
            assert_eq!(y.label, y_label, "{kind:?} y label");
            assert_eq!(quantity_key(&x), x_q, "{kind:?} x quantity");
            assert_eq!(quantity_key(&y), y_q, "{kind:?} y quantity");
        }
    }

    /// `CurveKind` is `#[non_exhaustive]`, so this crate's two matches over
    /// it now end in a wildcard — and a wildcard is exactly where a new
    /// kind can be silently mislabelled instead of caught by the compiler.
    ///
    /// What the wildcard must do is decline to guess: an unrecognised kind
    /// is unlabelled and unscaled, the same treatment `Generic` gets, so
    /// its points are shown as the raw numbers they are. The failure this
    /// guards against is someone giving the wildcard a real quantity to
    /// make some future kind render nicely, which would silently convert
    /// every *other* future kind's numbers.
    #[test]
    fn an_unrecognised_kind_is_never_given_units() {
        let [x, y] = axes_of(hydra::CurveKind::Generic);
        assert_eq!(x.label, "X");
        assert_eq!(y.label, "Y");
        assert_eq!(quantity_key(&x), None);
        assert_eq!(quantity_key(&y), None);

        // Unscaled as well as unlabelled: the importer leaves such a
        // curve's points in whatever units the source file used, so any
        // factor here would change numbers it cannot interpret.
        let [raw_x, raw_y] = curve_axes(hydra::CurveKind::Generic);
        assert_eq!(raw_x.scale(), 1.0);
        assert_eq!(raw_y.scale(), 1.0);

        assert_eq!(curve_kind_id(hydra::CurveKind::Generic), "generic");
    }

    /// The command the frontend actually reads: one entry per kind a model
    /// can contain, keyed by the same string `CurveDto.kind` carries — so a
    /// curve staged in the draft, which has no DTO at all, resolves its
    /// axes exactly as a saved one does.
    #[test]
    fn the_served_table_covers_every_kind_a_curve_dto_can_report() {
        let served = list_curve_axes("wds".into());
        let keys: Vec<&str> = served.iter().map(|r| r.kind.as_str()).collect();
        // Driven by the engine's list rather than a copy of it.
        for &kind in hydra::CurveKind::ALL {
            let id = curve_kind_id(kind);
            assert!(keys.contains(&id), "{id} is missing from the served table");
        }

        // And the keys are distinct — which is the assertion that actually
        // catches a kind this crate has not labelled yet, now that
        // `#[non_exhaustive]` has taken that job from the compiler. The
        // loop above cannot: `curve_kind_id` routes an unrecognised kind to
        // "generic", so an unlabelled kind would find that key already
        // present and pass. What it cannot do is avoid colliding with the
        // real generic entry.
        let mut distinct = keys.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            keys.len(),
            "two kinds served under one key — a new `CurveKind` needs an id \
             and axes in this crate"
        );

        // Each entry says what `curve_axes` says — one authority, two
        // readers (the DTO's value scaling and this table's labels).
        for row in &served {
            let kind = hydra::CurveKind::ALL
                .iter()
                .find(|&&k| curve_kind_id(k) == row.kind)
                .copied()
                .expect("served kind is one the engine publishes");
            let [x, y] = curve_axes(kind);
            assert_eq!(row.axes[0].label, x.dto().label);
            assert_eq!(row.axes[1].label, y.dto().label);
        }
    }

    /// The one fact still stated on both sides of the IPC boundary: a
    /// curve created in the GUI is a pump-head curve, so the editor stages
    /// the add with `role: "pump-head"` and looks its axes up under that
    /// key. If `create_curve` ever makes something else, the staged add
    /// would render under the wrong axes until it was saved.
    ///
    /// The frontend half is `stagedCurveRole` in `CurveEditor.tsx`.
    #[test]
    fn a_created_curve_is_the_kind_the_editor_stages_it_as() {
        let mut network =
            hydra::io::parse(crate::commands::test_fixtures::TEST_INP.as_bytes()).unwrap();
        crate::commands::mutations::create_curve_in_network(&mut network, "NEW1").unwrap();
        let created = network.curves.iter().find(|c| c.id == "NEW1").unwrap();
        assert_eq!(
            curve_kind_id(created.kind),
            "pump-head",
            "the editor stages new curves as pump-head; keep the two in step"
        );
    }

    /// Engines whose curves this GUI does not edit publish none, and must
    /// say so as an empty list rather than by serving wds's table.
    #[test]
    fn engines_without_editable_curves_serve_no_axes() {
        assert!(list_curve_axes("uds".into()).is_empty());
        assert!(list_curve_axes("och".into()).is_empty());
    }

    /// A US reader must see the number their own file carries. An INP
    /// expresses volumes in cubic feet, so a 5000 ft³ minimum volume has to
    /// read as 5000 — it read as 37 401 while the catalog said gallons.
    #[test]
    fn volume_is_cubic_feet_in_us_display() {
        let volume = hydra::descriptors::QUANTITIES
            .iter()
            .find(|q| q.key == "volume")
            .expect("the catalog declares volume");
        assert_eq!(volume.us_label, "ft³");
        let five_thousand_cubic_feet_in_m3 = 5000.0 / 35.314_667;
        let displayed = five_thousand_cubic_feet_in_m3 * volume.si_to_us_scale;
        assert!(
            (displayed - 5000.0).abs() < 1e-6,
            "5000 ft³ must display as 5000, got {displayed}"
        );
    }

    /// Every quantity the table names must exist in the engine's §5
    /// catalog, or the axis reaches the frontend with no unit at all —
    /// silently, since an unknown key resolves to `None`.
    #[test]
    fn every_named_quantity_resolves_in_the_engine_catalog() {
        use hydra::CurveKind::*;
        for kind in [
            PumpHead,
            PumpEfficiency,
            TankVolume,
            GpvHeadloss,
            PcvLossRatio,
            Generic,
        ] {
            for (i, a) in axes_of(kind).iter().enumerate() {
                let named = curve_axes(kind)[i].quantity.is_some();
                assert_eq!(
                    named,
                    a.quantity.is_some(),
                    "{kind:?} axis {i} names a quantity the catalog does not define"
                );
            }
        }
    }

    /// Flows are the only curve values that change scale. Everything else
    /// is already in its display unit, and scaling it would be the old bug
    /// in a new place.
    #[test]
    fn only_flow_axes_are_scaled() {
        use hydra::CurveKind::*;
        assert_eq!(curve_axes(PumpHead)[0].scale(), 1000.0, "m³/s → L/s");
        assert_eq!(curve_axes(PumpHead)[1].scale(), 1.0, "head is metres");
        assert_eq!(curve_axes(TankVolume)[0].scale(), 1.0, "level is metres");
        assert_eq!(curve_axes(TankVolume)[1].scale(), 1.0, "volume is m³");
        assert_eq!(curve_axes(PcvLossRatio)[0].scale(), 1.0, "percent");
        assert_eq!(curve_axes(PcvLossRatio)[1].scale(), 1.0, "percent");
        assert_eq!(curve_axes(Generic)[0].scale(), 1.0, "unknown units");
    }

    /// A tank volume curve read from a US-customary model, checked against
    /// the file. This is the case the old code got most visibly wrong: it
    /// labelled the axes flow and head and converted both as though they
    /// were, so a 10 ft / 5000 ft³ point rendered as 10 L/s by 5000 m.
    #[test]
    fn a_tank_volume_curve_arrives_in_metres_and_cubic_metres() {
        let inp = "\
[JUNCTIONS]
J1  100  0

[RESERVOIRS]
R1  200

[TANKS]
T1  80  10  5  20  60  0  TV1

[PIPES]
P1  R1  J1  1000  12  100  0  Open

[CURVES]
TV1  0     0
TV1  10    5000

[OPTIONS]
Units  GPM

[TIMES]
Duration  0
";
        let network = hydra::io::parse(inp.as_bytes()).unwrap();
        let dto = network_to_dto(&network);
        let tv = dto.curves.iter().find(|c| c.id == "TV1").unwrap();
        assert_eq!(tv.kind, "tank-volume");
        // 10 ft = 3.048 m; 5000 ft³ = 141.584 m³.
        assert!(
            (tv.x[1] - 3.048).abs() < 1e-3,
            "10 ft of level is 3.048 m, got {}",
            tv.x[1]
        );
        assert!(
            (tv.y[1] - 141.584).abs() < 1e-2,
            "5000 ft³ is 141.584 m³, got {}",
            tv.y[1]
        );
    }

    /// The inverse reads the same table, so an edit must land back exactly
    /// where it came from — for every kind, not just the one the editor was
    /// originally written for.
    #[test]
    fn display_to_internal_inverts_every_kind() {
        use hydra::CurveKind::*;
        let internal_x = [0.0, 0.177, 0.354];
        let internal_y = [50.0, 25.0, 0.0];
        for kind in [
            PumpHead,
            PumpEfficiency,
            TankVolume,
            GpvHeadloss,
            PcvLossRatio,
            Generic,
        ] {
            let [ax, ay] = curve_axes(kind);
            let xs: Vec<f64> = internal_x.iter().map(|v| v * ax.scale()).collect();
            let ys: Vec<f64> = internal_y.iter().map(|v| v * ay.scale()).collect();
            let back = curve_points_display_to_internal(kind, &xs, &ys);
            for (i, p) in back.iter().enumerate() {
                assert!(
                    (p.x - internal_x[i]).abs() < 1e-12,
                    "{kind:?} x drifted: {} → {}",
                    internal_x[i],
                    p.x
                );
                assert!(
                    (p.y - internal_y[i]).abs() < 1e-12,
                    "{kind:?} y drifted: {} → {}",
                    internal_y[i],
                    p.y
                );
            }
        }
    }
}
