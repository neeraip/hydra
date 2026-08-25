//! The run summary as data (§11, §14.9): everything the report is
//! built from, gathered by the session, formatted by the dialect
//! tooling (format-blind extraction, phase 3).

use crate::hydraulics::routing::{LinkStats, RoutingReport, VertexStats};
use crate::hydrology::runoff::ParcelTotals;
use crate::model::Network;

/// The inputs the report draws on, gathered by the session.
/// §14.9's overland additions, present only when a run served a mesh
/// (§15): the §15.8 ledger, the §15.8 delivered pair on the network
/// side, and the §15.4.4 march counts.
#[derive(Debug, Clone)]
pub struct OverlandRpt {
    /// The §15.8 ledger, cumulative over the run (m³).
    pub ledger: crate::overland::LedgerRow,
    /// Opening surface storage (m³).
    pub initial_storage: f64,
    /// Exchange as delivered to the network ledger (m³): surface
    /// drainage in, surface spill drawn back out.
    pub delivered_in: f64,
    pub delivered_out: f64,
    /// (base substeps, macro cycles, rebuilds, min base step s,
    /// average base step s, peak active cells).
    pub march: (u64, u64, u64, f64, f64, usize),
}

pub struct ReportInputs<'a> {
    pub net: &'a Network,
    /// The §15 overland additions, when a mesh was served.
    pub overland: Option<OverlandRpt>,
    /// The §11.1 surface ledger parts, when a surface exists:
    /// (precipitation, run-on, evaporation, infiltration, runoff,
    /// ploughed snow, initial storage, final storage, error %).
    pub surface: Option<[f64; 9]>,
    /// The §11.1 subsurface parts: (infiltration, evapotranspiration,
    /// deep percolation, lateral flow, initial, final, error %).
    pub subsurface: Option<[f64; 7]>,
    /// Flow-routing parts: (sanitary, wet-weather, subsurface, sewer,
    /// external, outflow, flooding, evaporation, exfiltration, initial,
    /// final, error %).
    pub flow: [f64; 12],
    /// Per-constituent quality parts: (id, the five §11.2 admitted
    /// loads by origin, discharged, flooded, exfiltrated, reacted,
    /// initial stored, final stored, error %).
    pub quality: Vec<(String, [f64; 12])>,
    /// Per-constituent §11.1 surface-loading parts: (id, initial
    /// buildup, buildup, deposition, swept, infiltrated, BMP removed,
    /// washed off, remaining, error %).
    pub loading: Vec<(String, [f64; 9])>,
    /// Control actions: (elapsed s, link, setting, rule).
    pub actions: &'a [(f64, String, f64, String)],
    /// The whole-run numerical-performance statistics (§11.2).
    pub performance: &'a RoutingReport,
    /// §11.2 per-vertex and per-link statistics.
    pub vertex_stats: &'a [VertexStats],
    pub link_stats: &'a [LinkStats],
    /// Per-parcel §11.2 totals, parallel to the model's parcels.
    pub parcel_totals: Vec<ParcelTotals>,
    /// Per-parcel delivered washoff `[parcel][constituent]` (U).
    pub washoff_by_parcel: Option<Vec<Vec<f64>>>,
    /// Per-outfall discharged mass `[constituent][vertex]`.
    pub outfall_loads: Option<Vec<Vec<f64>>>,
    /// Per-link transported mass `[constituent][link]`.
    pub link_loads: Option<Vec<Vec<f64>>>,
    /// The top worst-error vertices: (id, accepted-step count).
    pub worst: Vec<(String, u64)>,
    /// §11.2 control-measure balances: (parcel id, control id,
    /// [inflow, evap, infil, surface, drain, initial, final] as depths
    /// over the unit's footprint (m), balance error %).
    pub lid_performance: Vec<(String, String, [f64; 7], f64)>,
}
