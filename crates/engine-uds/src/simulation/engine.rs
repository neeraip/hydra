//! The session (§12) over the §10.1 routing-period loop.
//!
//! Load parses and validates; run advances routing period by routing
//! period, assembling lateral inflows at each period's start — external
//! inflows and sanitary base flows evaluated at the step-start date,
//! series read with the §10.1 extension contract (inflows fall to zero
//! beyond their range, with a one-time warning; stages hold their ends) —
//! and servicing every reporting boundary passed. Event windows (§10.3)
//! freeze the network between events. Results are recorded at reporting
//! times and served by object identity (§12.2).

use std::collections::HashMap;

use super::time::{civil_from_days, days_from_civil, report_start_offset, weekday};
use crate::hydraulics::routing::{Router, RouterRefusal, RoutingReport};
use crate::hydrology::groundwater::GwState;
use crate::hydrology::infiltration::InfilFactors;
use crate::hydrology::rdii::RdiiState;
use crate::hydrology::runoff::{Surface, SurfaceRefusal};
use crate::hydrology::snow::SnowClimate;
use crate::io::objects::parse_network;
use crate::io::survey::Diagnostic;
use crate::io::validate::{validate, ValidationDiagnostic};
use crate::model::{
    InflowKind, Network, OutfallStage, PatternKind, SeriesTime, TimeSeriesSource, VertexKind,
};

/// Truncation threshold for assembled lateral inflows (m³/s) — the
/// predecessor's flow tolerance, converted.
const FLOW_TOL: f64 = 2.832e-7;

/// Air temperature (°C) for a model declaring no temperature source
/// (§3.1): the predecessor's 70 °F, expressed here in the engine's own
/// unit. Operative, not decorative — a declared snow pack melts against
/// it, so a model that omits the record still has a climate.
const DEFAULT_AIR_TEMPERATURE_C: f64 = 5.0 / 9.0 * (70.0 - 32.0);

/// Why a session could not be opened (§12.1: every entry point returns a
/// typed error rather than faulting).
#[derive(Debug)]
pub enum OpenError {
    /// The file was refused by parsing; the diagnostics say where.
    Parse(Vec<Diagnostic>),
    /// The model was refused by §14.7 validation.
    Validation(Vec<ValidationDiagnostic>),
    /// The router cannot serve this model yet.
    Routing(RouterRefusal),
    /// The surface compartment cannot serve this model yet.
    Surface(SurfaceRefusal),
    /// A control rule was refused at compile (§9.1).
    Controls(String),
    /// A transport configuration this stage does not evaluate yet (§8).
    Transport(String),
}

/// One readable line per refusal, so applications can show the error
/// without reaching for `Debug`. Multi-finding refusals name the first
/// finding and count the rest; the full lists stay on the variants.
impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn first_and_rest(
            f: &mut std::fmt::Formatter<'_>,
            what: &str,
            shown: Option<String>,
            total: usize,
        ) -> std::fmt::Result {
            match shown {
                Some(first) if total > 1 => {
                    write!(f, "{what}: {first} (and {} more findings)", total - 1)
                }
                Some(first) => write!(f, "{what}: {first}"),
                None => write!(f, "{what}"),
            }
        }
        match self {
            OpenError::Parse(diags) => {
                let errors: Vec<_> = diags.iter().filter(|d| d.kind.is_error()).collect();
                first_and_rest(
                    f,
                    "the model was refused by parsing",
                    errors.first().map(|d| d.to_string()),
                    errors.len(),
                )
            }
            OpenError::Validation(findings) => {
                let errors: Vec<_> = findings.iter().filter(|v| v.kind.is_error()).collect();
                first_and_rest(
                    f,
                    "the model was refused by validation",
                    errors.first().map(|v| v.to_string()),
                    errors.len(),
                )
            }
            OpenError::Routing(r) => write!(f, "{r}"),
            OpenError::Surface(s) => write!(f, "{s}"),
            OpenError::Controls(msg) | OpenError::Transport(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for OpenError {}

#[cfg(test)]
mod open_error_display_tests {
    use super::*;

    /// The GUI shows these lines in toasts; a `Debug`-shaped string with
    /// braces and variant names reaching a user is the defect this pins.
    #[test]
    fn every_variant_displays_as_prose() {
        let cases: Vec<OpenError> = vec![
            OpenError::Controls("rule R1 names a vanished element".into()),
            OpenError::Transport("a treatment expression is not served yet".into()),
        ];
        for e in cases {
            let line = e.to_string();
            assert!(!line.contains('{') && !line.contains("(\""), "{line:?}");
            assert!(!line.is_empty());
        }
    }

    #[test]
    fn a_parse_refusal_displays_as_prose() {
        let (_, diags) = parse_network("not a model at all");
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

/// One recorded reporting boundary: the full §14.9 record set, by index
/// into the model (§12.2 serves them by identity).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Simulation time (s from start).
    pub t: f64,
    /// Vertex depths (m).
    pub depths: Vec<f64>,
    /// Link flows (m³/s), in the user's orientation.
    pub flows: Vec<f64>,
    /// Vertex heads (m), volumes (m³), lateral inflows, total inflows,
    /// and flooding rates (m³/s).
    pub node_head: Vec<f64>,
    pub node_volume: Vec<f64>,
    pub node_lateral: Vec<f64>,
    pub node_inflow: Vec<f64>,
    pub node_flooding: Vec<f64>,
    /// Vertex concentrations `[constituent][vertex]`.
    pub node_quality: Vec<Vec<f64>>,
    /// Link depths (m), velocities (m/s), volumes (m³), and capacity
    /// fractions.
    pub link_depth: Vec<f64>,
    pub link_velocity: Vec<f64>,
    pub link_volume: Vec<f64>,
    pub link_capacity: Vec<f64>,
    /// Link concentrations `[constituent][link]`.
    pub link_quality: Vec<Vec<f64>>,
    /// Per-parcel surface records.
    pub subcatch: Vec<SubcatchRecord>,
    /// The fifteen §14.9 system series, SI.
    pub system: [f64; 15],
}

/// One parcel's §14.9 record.
#[derive(Debug, Clone, Default)]
pub struct SubcatchRecord {
    /// Precipitation rate (m/s).
    pub rain: f64,
    /// Snow water equivalent (m).
    pub snow_depth: f64,
    /// Evaporation rate exerted (m/s).
    pub evap: f64,
    /// Infiltration rate (m/s).
    pub infil: f64,
    /// Runoff rate (m³/s).
    pub runoff: f64,
    /// Subsurface lateral discharge (m³/s).
    pub gw_flow: f64,
    /// Water-table elevation (m).
    pub gw_elev: f64,
    /// Unsaturated-zone moisture content.
    pub soil_moisture: f64,
    /// Runoff concentrations per constituent.
    pub washoff: Vec<f64>,
}

/// A reported parcel record as an interface-file row, still in engine
/// units (§14.8.2). The file's field order is its own, so the mapping is
/// written out rather than derived from either type's layout.
fn parcel_replay(r: &SubcatchRecord) -> crate::io::iface::ParcelReplay {
    crate::io::iface::ParcelReplay {
        rainfall: r.rain,
        snow_depth: r.snow_depth,
        evap: r.evap,
        infil: r.infil,
        runoff: r.runoff,
        gw_flow: r.gw_flow,
        gw_elev: r.gw_elev,
        soil_moisture: r.soil_moisture,
        washoff: r.washoff.clone(),
    }
}

/// One §11.1 balance: the accumulated inflow and outflow sides and the
/// error statistic ε = 100(1 − O/I), sign-mirrored when the ledger has
/// outflow but no inflow, zero within the agreement threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ledger {
    /// The accumulated inflow side.
    pub inflow: f64,
    /// The accumulated outflow side.
    pub outflow: f64,
    /// The §11.1 error statistic (percent).
    pub error_percent: f64,
}

fn balance(inflow: f64, outflow: f64, tol: f64) -> Ledger {
    let error_percent = if (inflow - outflow).abs() <= tol {
        0.0
    } else if inflow > 0.0 {
        100.0 * (1.0 - outflow / inflow)
    } else if outflow > 0.0 {
        -100.0 * (1.0 - inflow / outflow)
    } else {
        0.0
    };
    Ledger {
        inflow,
        outflow,
        error_percent,
    }
}

/// The five §11.1 conservation balances.
#[derive(Debug, Clone)]
pub struct Ledgers {
    /// The surface water balance, where a surface compartment exists.
    pub surface: Option<Ledger>,
    /// The subsurface balance, where aquifers exist.
    pub subsurface: Option<Ledger>,
    /// The network flow balance.
    pub network: Ledger,
    /// The constituent balances, by identity.
    pub constituents: Vec<(String, Ledger)>,
    /// The surface-loading balances, by identity.
    pub loading: Vec<(String, Ledger)>,
}

/// A run-time notice: the engine reporting on the run as it happens.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeNotice {
    /// Simulation time (s from start).
    pub t: f64,
    /// What happened.
    pub message: String,
}

/// The §3.1 day-state over supplied climate records: daily min/max
/// temperatures with their sinusoidal clock, the 7-day Hargreaves
/// window, and the pan-scaled file evaporation. All temperatures °F
/// internally, per the relations' native units.
#[derive(Debug, Clone, Default)]
struct ClimateDayState {
    /// The civil day last analysed.
    last_day: i64,
    tmin: f64,
    tmax: f64,
    tave: f64,
    trng: f64,
    /// Previous day's max minus today's min, the overnight limb.
    trng1: f64,
    prev_tmax: Option<f64>,
    /// Sunrise, effective sunset (3 h early), and their derived spans.
    hrsr: f64,
    hrss: f64,
    hrday: f64,
    dhrdy: f64,
    dydif: f64,
    /// The 7-day moving window on daily average and range.
    ma_ta: Vec<f64>,
    ma_tr: Vec<f64>,
    front: usize,
    t_ave7: f64,
    t_rng7: f64,
    /// Today's Hargreaves rate (m/s).
    hargreaves: f64,
    /// Today's file evaporation (m/s), pan-scaled.
    file_evap: f64,
    /// Today's wind (same unit the monthly table uses).
    wind: f64,
}

struct EventWindow {
    start: f64,
    end: f64,
}

/// A loaded, runnable model (§12.1).
pub struct Simulation {
    net: Network,
    router: Router,
    /// The §3 surface compartment, when the model has one.
    surface: Option<Surface>,
    /// §4.1 aquifers by parcel index.
    aquifers: Vec<(usize, GwState)>,
    /// §4.3 sewer-inflow convolutions.
    rdii: Vec<RdiiState>,
    /// Hydrology clock (s from start).
    hydro_t: f64,
    /// Bracketing hydrology laterals for §10.1 linear interpolation.
    hydro_prev: (f64, Vec<f64>),
    hydro_now: (f64, Vec<f64>),
    hydro_degraded_warned: bool,
    /// Epoch instant of simulation start (s).
    start_epoch: f64,
    /// Run duration (s).
    duration: f64,
    report_step: f64,
    next_report: f64,
    routing_period: f64,
    events: Vec<EventWindow>,
    vertex_by_id: HashMap<String, usize>,
    link_by_id: HashMap<String, usize>,
    /// Per-series exhaustion warnings already issued.
    series_warned: Vec<bool>,
    /// Per-vertex lateral overrides (§12.4 boundary forcing).
    lateral_override: HashMap<usize, f64>,
    /// §12.4: injected outfall stages and link settings, held so a rule
    /// cannot move an element a caller has taken over, and so a restored
    /// checkpoint takes it over again.
    stage_override: HashMap<usize, f64>,
    setting_override: HashMap<usize, f64>,
    /// §12.4: injected channel loss coefficients and flow caps, held so a
    /// restored checkpoint takes them over again.
    loss_override: HashMap<usize, (f64, f64, f64)>,
    limit_override: HashMap<usize, f64>,
    /// §12.4: the concentrations an injected inflow carries, per vertex.
    concentration_override: HashMap<usize, Vec<f64>>,
    /// §12.4: injected underdrains, by `(parcel, unit)`.
    drain_override: HashMap<(usize, usize), crate::hydrology::lid::DrainSetting>,
    /// The compiled §9 control system, when the model has rules.
    controls: Option<super::controls::Controls>,
    /// §8.4 network quality, when the model declares constituents.
    quality: Option<crate::transport::NetworkQuality>,
    /// §8.2–§8.3 surface quality, when parcels carry land uses.
    surface_quality: Option<crate::transport::SurfaceQuality>,
    /// §7.8 street-inlet capture, when the model places inlets.
    inlets: Option<crate::hydraulics::inlets::Inlets>,
    /// Supplied daily climate records (§3.1), chronological.
    climate_records: Vec<crate::model::DailyClimate>,
    climate_state: ClimateDayState,
    /// A supplied routing interface inflow file (§14.8).
    iface_in: Option<crate::io::iface::RoutingInterface>,
    /// A supplied RDII interface file (§14.8.1). When present it *is* the
    /// RDII hydrograph and the convolutions are not run.
    rdii_in: Option<crate::io::iface::RdiiInterface>,
    /// Convolved RDII for `SAVE` (§14.8.1): `(epoch s, flow per assignment)`
    /// at each hydrology step. Collected only when the model asks for a
    /// file, since a long run holds one row per step per assignment.
    rdii_out: Option<Vec<(f64, Vec<f64>)>>,
    /// §14.8.3: when the model asks to save a rainfall interface file,
    /// every file-sourced gage's record normalised to interval depths in
    /// inches. Fixed at load: the cache is of the records, and no part of
    /// a run changes them.
    rain_out: Option<Vec<crate::io::iface::RainGageRecord>>,
    /// §14.8.2: when the model asks to save a runoff interface file
    /// (`SAVE`), one `(step length, per-parcel record)` per hydrology step,
    /// in engine units. Held rather than streamed so the header's step
    /// count is the number of records the run actually produced. `USE` is
    /// the other half of the same option and fills `runoff_in` instead;
    /// a run does one or the other, never both.
    runoff_out: Option<Vec<(f64, Vec<crate::io::iface::ParcelReplay>)>>,
    /// §12.3: a fingerprint of each interface file supplied to this run,
    /// as `(what it is, hash of its bytes)`. A checkpoint carries these so
    /// a restored run given a different file, or none, is refused rather
    /// than continued on inflows it was never receiving.
    supplied: Vec<(&'static str, u64)>,
    /// A supplied runoff interface file (§14.8.2) and the cursor into it.
    /// When present the surface is not stepped at all and the hydrology
    /// clock follows the file's own steps.
    runoff_in: Option<(crate::io::iface::RunoffInterface, usize)>,
    /// Said once when a replayed file runs out before the run does.
    runoff_exhausted: bool,
    /// The replayed record now in force, per parcel, in engine units.
    ///
    /// The reported parcel series are built from the live surface, which a
    /// replayed run never steps, so without this a replay routed the
    /// file's flows correctly and reported every parcel as producing
    /// nothing (§14.8.2).
    runoff_now: Vec<crate::io::iface::ParcelReplay>,
    /// Bracketing hydrology lateral mass rates `[p][v]` (unit·m³/s).
    hydro_mass_prev: Vec<Vec<Vec<f64>>>,
    /// Hydrology lateral mass by origin, in HYDRO_SOURCES order:
    /// `[source][constituent][vertex]` (§11.2).
    hydro_mass_now: Vec<Vec<Vec<f64>>>,
    /// The next rule-evaluation boundary (s) under the rule-step option;
    /// zero rule step evaluates every routing step.
    next_rule_t: f64,
    /// The last assembled lateral vector and its category totals
    /// (external, sanitary), for the §14.9 records.
    last_lat: Vec<f64>,
    /// The §7.8 inlet-transfer share of the last laterals, for the
    /// period-end reporting snapshot.
    last_inlet_lat: Vec<f64>,
    last_ext_total: f64,
    last_dwf_total: f64,
    /// §14.5 per-category inflow volumes (m³): sanitary, external,
    /// wet-weather, subsurface, and sewer inflow.
    vol_dwf: f64,
    vol_ext: f64,
    vol_wet: f64,
    vol_gw: f64,
    vol_rdii: f64,
    /// Recorded reporting boundaries.
    pub snapshots: Vec<Snapshot>,
    /// The §14.9 results file, written as the run produces it.
    ///
    /// Attached by [`Self::begin_results`] and closed by
    /// [`Self::finish_results`]; absent when the caller wants no results
    /// file. Not state: a resumed run attaches its own sink, and the
    /// bytes already written are in the file the caller holds.
    out: Option<crate::io::out_writer::OutStream<Box<dyn std::io::Write + Send>>>,
    /// The first write failure the stream met, held until the caller
    /// closes it. A step is not the place to fail on a results file.
    out_error: Option<std::io::Error>,
    /// Whether reporting instants are kept after being written.
    ///
    /// §12.3 makes a checkpoint carry the results so far, so a run that
    /// may be asked for one has to keep them; that set is the largest
    /// thing a long run owns. A run that will not be asked keeps none,
    /// and pays nothing for a guarantee it never uses. Kept regardless
    /// when the model saves a routing outflow file, which is written from
    /// the same instants at the end (§14.8).
    retain_snapshots: bool,
    /// Run-time notices, in time order.
    pub notices: Vec<RuntimeNotice>,
}

impl Simulation {
    /// Load a model from its input text: parse, validate (§14.7 mutations
    /// applied), and build the router. Warning-class diagnostics from
    /// both passes are returned alongside the session.
    pub fn open(
        input: &str,
    ) -> Result<(Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>), OpenError> {
        Simulation::open_with_files(input, Vec::new(), Vec::new())
    }

    /// Load a model together with daily climate records (§3.1) — the
    /// caller owns reading the climate file; `io::climate` parses its
    /// text. Records serve file-sourced temperature, evaporation, wind,
    /// and the Hargreaves relation.
    pub fn open_with_climate(
        input: &str,
        climate_records: Vec<crate::model::DailyClimate>,
    ) -> Result<(Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>), OpenError> {
        Simulation::open_with_files(input, climate_records, Vec::new())
    }

    /// Load a model together with every auxiliary record the caller read
    /// for it (§12.1): daily climate records (§3.1), and external rain
    /// records (§14.12) as `(file name, parsed readings)` — `io::rain`
    /// parses their text. File-sourced gages are realised as the
    /// equivalent series at load; a gage naming a file not supplied here
    /// refuses the load with the file named.
    pub fn open_with_files(
        input: &str,
        climate_records: Vec<crate::model::DailyClimate>,
        rain_files: Vec<(String, Vec<crate::io::rain::RainReading>)>,
    ) -> Result<(Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>), OpenError> {
        let records = rain_files
            .into_iter()
            .map(|(name, readings)| (name, crate::io::rain::RainRecords::Station(readings)))
            .collect();
        Simulation::open_inner(input, climate_records, records, None)
    }

    /// Load a model together with rain files in whichever layout each was
    /// written in (§14.12, §14.12.1) — `io::rain::parse_any_rain_file`
    /// recognises them, and a gage reads whichever its file turned out to
    /// be without declaring anything.
    pub fn open_with_rain_records(
        input: &str,
        climate_records: Vec<crate::model::DailyClimate>,
        rain_files: Vec<(String, crate::io::rain::RainRecords)>,
    ) -> Result<(Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>), OpenError> {
        Simulation::open_inner(input, climate_records, rain_files, None)
    }

    /// Load a model whose file-sourced gages read a rainfall interface
    /// file (§14.8.3) rather than their own records.
    ///
    /// The file caches records already parsed and normalised, so it stands
    /// in for every external record the model would otherwise need: a gage
    /// is matched to it by station identifier, and one whose station the
    /// file does not carry refuses the load by name.
    pub fn open_with_rain_interface(
        input: &str,
        climate_records: Vec<crate::model::DailyClimate>,
        rain_interface: &[u8],
    ) -> Result<(Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>), OpenError> {
        let iface = crate::io::iface::parse_rain_iface(rain_interface)
            .map_err(|e| OpenError::Transport(format!("rainfall interface file: {e}")))?;
        Simulation::open_inner(input, climate_records, Vec::new(), Some(iface))
    }

    fn open_inner(
        input: &str,
        climate_records: Vec<crate::model::DailyClimate>,
        rain_files: Vec<(String, crate::io::rain::RainRecords)>,
        rain_interface: Option<crate::io::iface::RainInterface>,
    ) -> Result<(Simulation, Vec<Diagnostic>, Vec<ValidationDiagnostic>), OpenError> {
        let (mut net, diags) = parse_network(input);
        if diags.iter().any(|d| d.kind.is_error()) {
            return Err(OpenError::Parse(diags));
        }
        // Before validation, not after: §3.1 says a realised record is
        // treated exactly as if its series had been written in the model,
        // and validation is where §5's mutations and every gage/series
        // finding happen — the wet-step reduction to a finer gage interval
        // among them. Realising afterwards left file-sourced gages
        // silently skipping all of it, running a coarser hydrology step
        // than the same model with the record inlined.
        // §14.8.3: a model asking to read the cache must read the cache.
        // Falling through to each gage's own record would run a model on
        // inputs it did not ask for, and produce results silently unlike
        // the ones the file holds.
        if let (Some((crate::model::FileMode::Use, name)), None) =
            (&net.interface_files.rainfall, &rain_interface)
        {
            if net
                .gages
                .iter()
                .any(|g| matches!(g.source, crate::model::GageSource::File { .. }))
            {
                return Err(OpenError::Transport(format!(
                    "this model reads its rainfall from the interface file {name:?}, \
                     which was not supplied (§14.8.3)"
                )));
            }
        }
        let rain_records = realise_file_gages(&mut net, &rain_files, rain_interface.as_ref())
            .map_err(OpenError::Surface)?;
        let findings = validate(&mut net);
        if findings.iter().any(|f| f.kind.is_error()) {
            return Err(OpenError::Validation(findings));
        }
        let router = Router::build(&net).map_err(OpenError::Routing)?;
        let start_epoch_for_surface =
            days_from_civil(net.options.start_date) as f64 * 86_400.0 + net.options.start_time;
        let surface = Surface::build(&net, start_epoch_for_surface).map_err(OpenError::Surface)?;
        let mut aquifers = Vec::new();
        for (pi, p) in net.parcels.iter().enumerate() {
            if let Some(gw) = &p.groundwater {
                let invert = net.vertices[gw.vertex].invert;
                let state = GwState::build(
                    gw,
                    &net.aquifers[gw.aquifer],
                    invert,
                    p.area,
                    net.options.flow_units.is_us(),
                )
                .map_err(|e| {
                    OpenError::Surface(SurfaceRefusal::Incomplete(format!("{}: {e}", p.id)))
                })?;
                aquifers.push((pi, state));
            }
        }
        let rdii = RdiiState::build_all(&net);

        let start_epoch =
            days_from_civil(net.options.start_date) as f64 * 86_400.0 + net.options.start_time;
        let end_epoch =
            days_from_civil(net.options.end_date) as f64 * 86_400.0 + net.options.end_time;
        let duration = (end_epoch - start_epoch).max(0.0);
        let report_step = net.options.report_step.max(1.0);
        let next_report = match net.options.report_start {
            Some((d, s)) => {
                let offset = days_from_civil(d) as f64 * 86_400.0 + s - start_epoch;
                if offset <= 0.0 {
                    report_step
                } else {
                    offset
                }
            }
            None => report_step,
        };

        // Event windows in run time, sorted, overlaps clipped to the next
        // start (§10.3).
        let mut events: Vec<EventWindow> = net
            .events
            .iter()
            .map(|e| EventWindow {
                start: days_from_civil(e.start_date) as f64 * 86_400.0 + e.start_time - start_epoch,
                end: days_from_civil(e.end_date) as f64 * 86_400.0 + e.end_time - start_epoch,
            })
            .collect();
        events.sort_by(|a, b| a.start.total_cmp(&b.start));
        for i in 0..events.len().saturating_sub(1) {
            let next_start = events[i + 1].start;
            if events[i].end > next_start {
                events[i].end = next_start;
            }
        }

        let vertex_by_id = net
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| (v.id.clone(), i))
            .collect();
        let link_by_id = net
            .links
            .iter()
            .enumerate()
            .map(|(i, l)| (l.id.clone(), i))
            .collect();
        let n_series = net.timeseries.len();
        let routing_period = net.options.routing_step.max(0.5);

        // §8: network transport, surface accumulation-mobilisation and
        // treatment all evaluate here.
        if climate_records.is_empty() {
            if matches!(
                net.climate.evaporation,
                crate::model::EvaporationSource::Temperature
                    | crate::model::EvaporationSource::File { .. }
            ) {
                return Err(OpenError::Surface(SurfaceRefusal::Unsupported(
                    "Hargreaves and climate-file evaporation need supplied climate records \
                     (open_with_climate)",
                )));
            }
            if net.climate.snowmelt.is_some()
                && matches!(
                    net.climate.temperature,
                    Some(crate::model::TemperatureSource::File { .. })
                )
            {
                return Err(OpenError::Surface(SurfaceRefusal::Unsupported(
                    "snowmelt from file temperatures needs supplied climate records \
                     (open_with_climate)",
                )));
            }
        }
        // §14.8: the rainfall, runoff, and RDII interface formats arrive with
        // a follow-up stage. The two modes are not the same failure.
        //
        // USE is an input the results depend on, so the model is refused. SAVE
        // and SCRATCH ask for an output artifact: in the predecessor the
        // rainfall file is built on every run and merely kept rather than
        // deleted (`rain.c`, default mode SCRATCH_FILE), so a run without it
        // produces the same results. Refusing those would block a model that
        // is otherwise perfectly runnable, and saying nothing — which is what
        // happened before — leaves a modeller waiting for a file that will
        // never appear. Loud and non-destructive, per §1.8.
        // One routing file never serves both roles in a run (§14.8).
        if let (Some(a), Some(b)) = (&net.interface_files.inflows, &net.interface_files.outflows) {
            if a == b {
                return Err(OpenError::Transport(
                    "one routing interface file cannot serve both roles (§14.8)".into(),
                ));
            }
        }
        let quality = if net.constituents.is_empty() {
            None
        } else {
            Some(
                crate::transport::NetworkQuality::build(&router, &net)
                    .map_err(OpenError::Transport)?,
            )
        };
        let surface_quality = if net.constituents.is_empty() || net.parcels.is_empty() {
            None
        } else {
            Some(crate::transport::SurfaceQuality::build(&net))
        };
        let inlets = crate::hydraulics::inlets::Inlets::build(&net, &router);
        let mut router = router;
        // §11.2: per-object statistics gate on the report start date.
        router.stats_start = report_start_offset(&net.options);

        // §9.1: compile the control rules; never-true premises warn.
        let mut rule_advisories = Vec::new();
        let controls = super::controls::Controls::compile(&net, &mut rule_advisories)
            .map_err(|e| OpenError::Controls(e.0))?;
        let mut notices: Vec<RuntimeNotice> = rule_advisories
            .into_iter()
            .map(|m| RuntimeNotice { t: 0.0, message: m })
            .collect();
        let _ = &mut notices;

        let nv = net.vertices.len();
        Ok((
            Simulation {
                router,
                surface,
                aquifers,
                rdii,
                hydro_t: 0.0,
                hydro_prev: (0.0, vec![0.0; nv]),
                hydro_now: (0.0, vec![0.0; nv]),
                hydro_degraded_warned: false,
                start_epoch,
                duration,
                report_step,
                next_report,
                routing_period,
                events,
                vertex_by_id,
                link_by_id,
                series_warned: vec![false; n_series],
                lateral_override: HashMap::new(),
                stage_override: HashMap::new(),
                setting_override: HashMap::new(),
                loss_override: HashMap::new(),
                limit_override: HashMap::new(),
                concentration_override: HashMap::new(),
                drain_override: HashMap::new(),
                controls,
                quality,
                surface_quality,
                inlets,
                climate_records,
                supplied: Vec::new(),
                iface_in: None,
                rdii_in: None,
                runoff_in: None,
                runoff_exhausted: false,
                runoff_now: Vec::new(),
                rdii_out: matches!(
                    net.interface_files.rdii,
                    Some((
                        crate::model::FileMode::Save | crate::model::FileMode::Scratch,
                        _
                    ))
                )
                .then(Vec::new),
                runoff_out: matches!(
                    net.interface_files.runoff,
                    Some((crate::model::FileMode::Save, _))
                )
                .then(Vec::new),
                rain_out: matches!(
                    net.interface_files.rainfall,
                    Some((crate::model::FileMode::Save, _))
                )
                .then(|| rain_records.clone()),
                climate_state: ClimateDayState {
                    last_day: i64::MIN,
                    ..ClimateDayState::default()
                },
                last_lat: vec![0.0; nv],
                last_inlet_lat: vec![0.0; nv],
                last_ext_total: 0.0,
                last_dwf_total: 0.0,
                vol_dwf: 0.0,
                vol_ext: 0.0,
                vol_wet: 0.0,
                vol_gw: 0.0,
                vol_rdii: 0.0,
                hydro_mass_prev: Vec::new(),
                hydro_mass_now: Vec::new(),
                next_rule_t: 0.0,
                snapshots: Vec::new(),
                out: None,
                out_error: None,
                retain_snapshots: true,
                notices,
                net,
            },
            diags,
            findings,
        ))
    }

    /// Current simulation time (s from start).
    pub fn time(&self) -> f64 {
        self.router.time()
    }

    /// The run duration (s).
    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// The routing ledger to date.
    pub fn report(&self) -> &RoutingReport {
        &self.router.report
    }

    /// Depth at a vertex, by identity (m).
    pub fn depth(&self, id: &str) -> Option<f64> {
        self.vertex_by_id.get(id).map(|&v| self.router.depth(v))
    }

    /// Flow in a link, by identity (m³/s), in the user's orientation.
    pub fn flow(&self, id: &str) -> Option<f64> {
        self.link_by_id
            .get(id)
            .map(|&l| self.router.flow(l, &self.net))
    }

    /// Set a control measure's underdrain mid-run; `None` releases it
    /// back to the design the model gives it (§12.4).
    ///
    /// Addressed as the model addresses a placement: the parcel hosting
    /// it and the control design deployed there. `false` for a pair this
    /// model does not place, or a design with no drain.
    pub fn set_drain(
        &mut self,
        parcel_id: &str,
        control_id: &str,
        setting: Option<crate::hydrology::lid::DrainSetting>,
    ) -> bool {
        let Some((pi, unit)) = self.placement_of(parcel_id, control_id) else {
            return false;
        };
        let value = match setting {
            Some(s) => s,
            // Releasing restores the design's own drain, which the model
            // still holds: the unit's copy is what was changed.
            None => match self.model_drain(parcel_id, control_id) {
                Some(s) => s,
                None => return false,
            },
        };
        match setting {
            Some(_) => {
                self.drain_override.insert((pi, unit), value);
            }
            None => {
                self.drain_override.remove(&(pi, unit));
            }
        }
        self.surface
            .as_mut()
            .is_some_and(|s| s.set_drain_setting(pi, unit, value))
    }

    /// A control measure's underdrain as it stands now.
    pub fn drain(
        &self,
        parcel_id: &str,
        control_id: &str,
    ) -> Option<crate::hydrology::lid::DrainSetting> {
        let (pi, unit) = self.placement_of(parcel_id, control_id)?;
        self.surface.as_ref()?.drain_setting(pi, unit)
    }

    /// Which parcel, and which of its units, a `(parcel, control)` pair
    /// names — the address `[LID_USAGE]` itself uses.
    fn placement_of(&self, parcel_id: &str, control_id: &str) -> Option<(usize, usize)> {
        let pi = self
            .net
            .parcels
            .iter()
            .position(|p| p.id.eq_ignore_ascii_case(parcel_id))?;
        let ci = self
            .net
            .lid_controls
            .iter()
            .position(|c| c.id.eq_ignore_ascii_case(control_id))?;
        // A parcel's units are built in the order its usages are listed,
        // so the unit's index is its position among that parcel's own.
        let unit = self
            .net
            .lid_usage
            .iter()
            .filter(|u| u.parcel == pi)
            .position(|u| u.control == ci)?;
        Some((pi, unit))
    }

    /// The drain the model's design gives a placement, for releasing an
    /// injection back to it.
    fn model_drain(
        &self,
        parcel_id: &str,
        control_id: &str,
    ) -> Option<crate::hydrology::lid::DrainSetting> {
        let (_, _) = self.placement_of(parcel_id, control_id)?;
        let ci = self
            .net
            .lid_controls
            .iter()
            .position(|c| c.id.eq_ignore_ascii_case(control_id))?;
        let d = self.net.lid_controls[ci].drain.as_ref()?;
        Some(crate::hydrology::lid::DrainSetting {
            coeff: d.coeff,
            exponent: d.exponent,
            offset: d.offset,
            delay: d.delay,
            h_open: d.h_open,
            h_close: d.h_close,
        })
    }

    /// Inject the constituent concentrations carried by the lateral
    /// inflow injected at a vertex; `None` releases them (§12.4).
    ///
    /// One value per constituent in model order. They apply to the
    /// injected inflow and to nothing else: the model's own inflows carry
    /// what the model gives them.
    pub fn set_inflow_concentrations(&mut self, id: &str, values: Option<Vec<f64>>) -> bool {
        let Some(&v) = self.vertex_by_id.get(id) else {
            return false;
        };
        match values {
            Some(cs) => {
                if cs.len() != self.net.constituents.len() {
                    return false;
                }
                self.concentration_override.insert(v, cs);
            }
            None => {
                self.concentration_override.remove(&v);
            }
        }
        true
    }

    /// Inject a channel's entry, exit and average loss coefficients;
    /// `None` releases them back to the model's own (§12.4).
    ///
    /// `false` for a link this model does not carry as a channel: a loss
    /// coefficient on a pump is not a thing to set.
    pub fn set_losses(&mut self, id: &str, losses: Option<(f64, f64, f64)>) -> bool {
        let Some(&li) = self.link_by_id.get(id) else {
            return false;
        };
        let Some(from_model) = self.model_losses(li) else {
            return false;
        };
        let (inlet, outlet, average) = losses.unwrap_or(from_model);
        match losses {
            Some(_) => {
                self.loss_override.insert(li, (inlet, outlet, average));
            }
            None => {
                self.loss_override.remove(&li);
            }
        }
        self.router.set_losses(li, inlet, outlet, average).is_some()
    }

    /// Cap the flow a channel carries (m³/s); `None` releases the cap
    /// back to the model's own, and zero is no cap (§12.4).
    pub fn set_flow_limit(&mut self, id: &str, q_limit: Option<f64>) -> bool {
        let Some(&li) = self.link_by_id.get(id) else {
            return false;
        };
        let Some(from_model) = self.model_flow_limit(li) else {
            return false;
        };
        let value = q_limit.unwrap_or(from_model);
        match q_limit {
            Some(_) => {
                self.limit_override.insert(li, value);
            }
            None => {
                self.limit_override.remove(&li);
            }
        }
        self.router.set_flow_limit(li, value).is_some()
    }

    /// The loss coefficients the model gives a link, for releasing an
    /// injection back to them.
    fn model_losses(&self, li: usize) -> Option<(f64, f64, f64)> {
        let l = self.net.links.get(li)?;
        match &l.kind {
            crate::model::LinkKind::Channel {
                loss_inlet,
                loss_outlet,
                loss_avg,
                ..
            } => Some((*loss_inlet, *loss_outlet, *loss_avg)),
            _ => None,
        }
    }

    /// The flow cap the model gives a link, likewise.
    fn model_flow_limit(&self, li: usize) -> Option<f64> {
        let l = self.net.links.get(li)?;
        match &l.kind {
            crate::model::LinkKind::Channel { max_flow, .. } => Some(*max_flow),
            _ => None,
        }
    }

    /// Inject an intensity at a gage (m/s), superseding its record for
    /// every parcel it drives; `None` releases it (§12.4).
    ///
    /// `false` for a gage this model does not carry, or for a run with no
    /// surface to rain on.
    pub fn set_precipitation(&mut self, id: &str, rate: Option<f64>) -> bool {
        let Some(gi) = self
            .net
            .gages
            .iter()
            .position(|g| g.id.eq_ignore_ascii_case(id))
        else {
            return false;
        };
        self.surface
            .as_mut()
            .is_some_and(|s| s.set_precipitation(gi, rate))
    }

    /// Inject a boundary stage at an outfall (m, as an elevation);
    /// `None` releases it (§12.4).
    ///
    /// `false` for a vertex this model does not carry, or one that is not
    /// an outfall: a stage on a junction is not a thing to set, and
    /// accepting it silently would leave a caller believing it had.
    pub fn set_outfall_stage(&mut self, id: &str, elevation: Option<f64>) -> bool {
        let Some(&v) = self.vertex_by_id.get(id) else {
            return false;
        };
        if !matches!(
            self.net.vertices[v].kind,
            crate::model::VertexKind::Outfall { .. }
        ) {
            return false;
        }
        match elevation {
            Some(e) => {
                self.stage_override.insert(v, e);
                self.router.force_outfall_stage(v, Some(e), &self.net);
            }
            None => {
                self.stage_override.remove(&v);
                // Releasing returns the outfall to the boundary its model
                // declares, whatever that was.
                self.router.force_outfall_stage(v, None, &self.net);
            }
        }
        true
    }

    /// Inject a target setting on a controllable link; `None` releases it
    /// (§12.4).
    ///
    /// While an injection stands, no rule moves this link: a caller
    /// injecting a setting is replacing the model's decision for it, not
    /// entering a priority contest. The change is logged in the action
    /// record beside the rules', named as an injection.
    pub fn set_link_setting(&mut self, id: &str, value: Option<f64>) -> bool {
        let Some(&li) = self.link_by_id.get(id) else {
            return false;
        };
        match value {
            Some(v) => {
                if self.router.set_setting(li, v).is_none() {
                    return false;
                }
                self.setting_override.insert(li, v);
                let t = self.router.time();
                if let Some(controls) = &mut self.controls {
                    controls.log.push((
                        t,
                        self.net.links[li].id.clone(),
                        v,
                        "injected (§12.4)".to_string(),
                    ));
                }
            }
            None => {
                self.setting_override.remove(&li);
            }
        }
        true
    }

    /// Override the lateral inflow at a vertex (§12.4 boundary forcing);
    /// `None` restores the model's own forcing.
    pub fn set_lateral_inflow(&mut self, id: &str, q: Option<f64>) -> bool {
        let Some(&v) = self.vertex_by_id.get(id) else {
            return false;
        };
        match q {
            Some(q) => {
                self.lateral_override.insert(v, q);
            }
            None => {
                self.lateral_override.remove(&v);
            }
        }
        true
    }

    /// Replay a runoff interface file in place of the surface (§14.8.2).
    ///
    /// The clock is the file's. Each of its records carries the length of
    /// the step that produced it, so following them is what makes the
    /// replay reproduce the run that was written rather than resample it;
    /// the model's own wet and dry steps decide nothing here.
    ///
    /// RDII still runs. It is a separate file and a separate computation
    /// in the predecessor too, so a model may replay its surface and
    /// convolve its sewer inflow in the same run.
    fn replay_hydrology(&mut self, period_end: f64, routing_active: bool) {
        let nv = self.net.vertices.len();
        let np = self.net.constituents.len();
        let cv = crate::io::iface::flow_cv_of(self.net.options.flow_units);
        let Some(surface) = self.surface.take() else {
            return;
        };
        while self.hydro_t < period_end - 1e-9 {
            let step = self.runoff_in.as_mut().and_then(|(ifc, cursor)| {
                let s = ifc.steps.get(*cursor).cloned();
                if s.is_some() {
                    *cursor += 1;
                }
                s
            });
            // A file covering less of the clock than the run asks for is
            // not the end of the run: the surface stops contributing and
            // the clock falls back to the model's own step, because RDII
            // is a separate computation and goes on either way.
            let (dt, parcels) = match step {
                Some((dt, parcels)) => (dt.max(1e-9), Some(parcels)),
                None => {
                    if !self.runoff_exhausted {
                        self.runoff_exhausted = true;
                        self.notices.push(RuntimeNotice {
                            t: self.hydro_t,
                            message: concat!(
                                "the runoff interface file ended before the run did; ",
                                "the surface contributes nothing from here on (§14.8.2)"
                            )
                            .to_string(),
                        });
                    }
                    (self.net.options.wet_step.max(1e-9), None)
                }
            };
            let mut lats = vec![0.0; nv];
            let mut mass = vec![vec![vec![0.0; nv]; np]; 3];
            let us = self.net.options.flow_units.is_us();
            self.runoff_now = parcels
                .as_ref()
                .map(|rows| rows.iter().map(|r| r.to_si(us, cv)).collect())
                .unwrap_or_default();
            // Parcels draining to another parcel report nothing at the
            // vertex boundary: the receiving parcel's own runoff already
            // carries what they sent it, exactly as it does when the
            // surface computes the cascade itself.
            for (pi, parcel) in self.net.parcels.iter().enumerate() {
                let Some(r) = self.runoff_now.get(pi) else {
                    continue;
                };
                let q = r.runoff;
                if let crate::model::ParcelOutlet::Vertex(v) = parcel.outlet {
                    lats[v] += q;
                    // §8.2–§8.3: the runoff stream joins at the washoff
                    // concentrations the file recorded for it.
                    for (ci, row) in mass[0].iter_mut().enumerate() {
                        row[v] += r.washoff.get(ci).copied().unwrap_or(0.0) * q;
                    }
                }
            }
            if routing_active {
                self.vol_wet += lats.iter().sum::<f64>() * dt;
            }
            // §4.1: groundwater comes from the file as a flow, where the
            // computed path produces a rate the parcel's area scales.
            let gw_flows: Vec<(usize, f64)> = self
                .aquifers
                .iter()
                .filter_map(|(pi, gw)| self.runoff_now.get(*pi).map(|r| (gw.vertex, r.gw_flow)))
                .collect();
            for (vertex, q) in gw_flows {
                lats[vertex] += q;
                if routing_active {
                    self.vol_gw += q * dt;
                }
                for (ci, c) in self.net.constituents.iter().enumerate() {
                    mass[1][ci][vertex] += q.max(0.0) * c.c_groundwater;
                }
            }
            let epoch = self.start_epoch + self.hydro_t;
            let month = civil_from_days((epoch / 86_400.0).floor() as i64).month;
            let rain_factor = self.net.climate.adjust_rainfall[(month - 1) as usize];
            let rdii_flows: Vec<(usize, f64)> = match &self.rdii_in {
                Some(ifc) => ifc.inflows_at(epoch),
                None => self
                    .rdii
                    .iter_mut()
                    .map(|r| {
                        let rain = r
                            .gage(&self.net)
                            .map_or(0.0, |g| surface.gage_rate(g, epoch))
                            * rain_factor;
                        let q = r.step(&self.net, rain, month, dt);
                        (r.vertex, q)
                    })
                    .collect(),
            };
            if let Some(rows) = &mut self.rdii_out {
                rows.push((epoch, rdii_flows.iter().map(|(_, q)| *q).collect()));
            }
            for (vertex, q) in rdii_flows {
                lats[vertex] += q;
                if routing_active {
                    self.vol_rdii += q * dt;
                }
                for (ci, c) in self.net.constituents.iter().enumerate() {
                    mass[2][ci][vertex] += q * c.c_rdii;
                }
            }
            self.hydro_t += dt;
            self.hydro_prev = std::mem::replace(&mut self.hydro_now, (self.hydro_t, lats));
            self.hydro_mass_prev = std::mem::replace(&mut self.hydro_mass_now, mass);
        }
        self.surface = Some(surface);
    }

    /// Advance the §3 surface compartment in whole hydrology steps until
    /// it covers `period_end` (§10.1): the wet step while precipitation
    /// or ponded water exists, the dry step otherwise, truncated at gage
    /// recording boundaries.
    fn advance_hydrology(&mut self, period_end: f64, routing_active: bool) {
        if self.runoff_in.is_some() {
            return self.replay_hydrology(period_end, routing_active);
        }
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let nv = self.net.vertices.len();
        while self.hydro_t < period_end - 1e-9 {
            let epoch = self.start_epoch + self.hydro_t;
            // A supplied file leaves the convolution states at zero, so
            // asking them whether it is wet would run every RDII event on
            // the dry step (§14.8.1).
            let rdii_wet = match &self.rdii_in {
                Some(ifc) => ifc.inflows_at(epoch).iter().any(|(_, q)| *q > 0.0),
                None => self.rdii.iter().any(|r| r.flow > 0.0),
            };
            let wet = surface.is_wet(epoch) || rdii_wet;
            let mut dt = if wet {
                self.net.options.wet_step
            } else {
                self.net.options.dry_step
            };
            if let Some(b) = surface.next_gage_boundary(epoch) {
                let to_boundary = b - epoch;
                if to_boundary > 1e-9 {
                    dt = dt.min(to_boundary);
                }
            }
            // §14.4 refuses zero steps, so dt is positive; this floor is a
            // backstop so no future dt source can stall the clock.
            let dt = dt.max(1e-9);
            let month = {
                let epoch_days = ((self.start_epoch + self.hydro_t) / 86_400.0).floor() as i64;
                civil_from_days(epoch_days).month
            };
            let m = (month - 1) as usize;
            let evap = self.evaporation_rate(month);
            let rain_factor = self.net.climate.adjust_rainfall[m];
            self.update_climate_day(self.hydro_t);
            // §3.1/§3.3: the monthly conductivity adjustment and the
            // recovery pattern both ride the infiltration factors.
            let fac = InfilFactors {
                conductivity: self.net.climate.adjust_conductivity[m],
                recovery: self.pattern_factor(self.net.climate.recovery_pattern, self.hydro_t),
            };
            let dry_only = self.net.climate.evaporate_dry_only;
            let snow_cl = self.snow_climate(m);
            // §4.1: storability caps on this step's infiltration.
            let mut infil_caps = vec![f64::MAX; self.net.parcels.len()];
            for (pi, gw) in &self.aquifers {
                let frac_perv = 1.0 - self.net.parcels[*pi].frac_imperv;
                if frac_perv > 0.0 {
                    infil_caps[*pi] = (gw.max_infil_depth(frac_perv) / dt).max(0.0);
                }
            }
            let t_now = self.hydro_t;
            let patterns = |pat: Option<usize>| self.pattern_factor(pat, t_now);
            surface.step(
                epoch,
                dt,
                evap,
                dry_only,
                rain_factor,
                fac,
                snow_cl.as_ref(),
                &infil_caps,
                &patterns,
            );
            // §4.1: each aquifer advances on the same clock, reading the
            // routed stage lagged one step (§10.1), its discharge joining
            // the vertex laterals.
            let mut lats = surface.vertex_laterals(nv);
            if routing_active {
                self.vol_wet += lats.iter().sum::<f64>() * dt;
            }
            let np = self.net.constituents.len();
            // One plane per hydrology origin, in HYDRO_SOURCES order:
            // wet weather, subsurface, sewer (§11.2).
            let mut mass = vec![vec![vec![0.0; lats.len()]; np]; 3];
            // §8.2–§8.3: surface quality advances on the same clock; the
            // runoff and drain streams join the lateral mass at their
            // parcels' concentrations.
            if let Some(sq) = &mut self.surface_quality {
                let day = (self.start_epoch + self.hydro_t + dt) / 86_400.0;
                let doy = {
                    let d = civil_from_days(day as i64);
                    let jan1 = crate::io::options::Date {
                        year: d.year,
                        month: 1,
                        day: 1,
                    };
                    (day as i64 - days_from_civil(jan1) + 1) as u32
                };
                let (ss, se) = (self.net.options.sweep_start, self.net.options.sweep_end);
                let in_season = if ss <= se {
                    doy >= ss && doy <= se
                } else {
                    doy >= ss || doy <= se
                };
                let qsteps: Vec<_> = (0..self.net.parcels.len())
                    .map(|pi| surface.qstep(pi))
                    .collect();
                let net = &self.net;
                let start_epoch = self.start_epoch;
                let t_now = self.hydro_t + dt;
                let sv = move |si: usize| series_value_pure(net, start_epoch, si, t_now, false);
                sq.step(net, &qsteps, day, in_season, &sv);
                for (pi, parcel) in self.net.parcels.iter().enumerate() {
                    let q_out = surface.parcel_runoff(pi);
                    if let crate::model::ParcelOutlet::Vertex(v) = parcel.outlet {
                        for (ci, row) in mass[0].iter_mut().enumerate() {
                            row[v] += sq.conc[pi][ci] * q_out;
                        }
                    }
                    // Control-measure drains carry the parent parcel's
                    // concentration, less any drain removal (§8.1).
                    let removals = surface.lid_drain_removals(pi);
                    for &(v, qd) in surface.lid_drains(pi) {
                        for (ci, row) in mass[0].iter_mut().enumerate() {
                            let keep = 1.0 - removals.get(ci).copied().unwrap_or(0.0);
                            row[v] += sq.conc[pi][ci] * qd * keep;
                        }
                    }
                }
            }
            // §4.1 aquifer ET patterns, resolved before the mutable pass.
            let evap_pats: Vec<f64> = self
                .aquifers
                .iter()
                .map(|(pi, _)| {
                    let ai = self.net.parcels[*pi]
                        .groundwater
                        .as_ref()
                        .map_or(0, |g| g.aquifer);
                    self.pattern_factor(self.net.aquifers[ai].evap_pattern, self.hydro_t)
                })
                .collect();
            for (ai, (pi, gw)) in self.aquifers.iter_mut().enumerate() {
                let (infil, evap_used) = surface.parcel_infil_evap(*pi);
                let p = &self.net.parcels[*pi];
                let frac_perv = 1.0 - p.frac_imperv;
                let max_evap = evap * frac_perv;
                let stage = self.net.vertices[gw.vertex].invert + self.router.depth(gw.vertex);
                let q = gw.step(dt, infil, evap_used, max_evap, stage, evap_pats[ai]);
                lats[gw.vertex] += q * p.area;
                if routing_active {
                    self.vol_gw += q * p.area * dt;
                }
                // §8.1: subsurface inflow at its constant concentration.
                for (ci, c) in self.net.constituents.iter().enumerate() {
                    mass[1][ci][gw.vertex] += (q * p.area).max(0.0) * c.c_groundwater;
                }
                // §9.3: a domain-guarded custom relation announces itself
                // once without changing any result.
                for which in gw.guard_events.drain(..) {
                    self.notices.push(RuntimeNotice {
                        t: self.hydro_t,
                        message: format!(
                            "{}: the custom {which} groundwater relation was domain-guarded \
                             to zero at least once (§9.3)",
                            p.id
                        ),
                    });
                }
            }
            // §4.3: RDII convolutions on the same clock, with the monthly
            // rainfall adjustment applied during preprocessing (§3.1).
            //
            // §14.8.1: a supplied interface file *is* the hydrograph. The
            // convolution is precisely what such a file exists to avoid
            // recomputing, so the two are alternatives and never a sum.
            let rdii_flows: Vec<(usize, f64)> = match &self.rdii_in {
                Some(ifc) => ifc.inflows_at(epoch),
                None => self
                    .rdii
                    .iter_mut()
                    .map(|r| {
                        let rain = r
                            .gage(&self.net)
                            .map_or(0.0, |g| surface.gage_rate(g, epoch))
                            * rain_factor;
                        let q = r.step(&self.net, rain, month, dt);
                        (r.vertex, q)
                    })
                    .collect(),
            };
            if let Some(rows) = &mut self.rdii_out {
                rows.push((epoch, rdii_flows.iter().map(|(_, q)| *q).collect()));
            }
            for (vertex, q) in rdii_flows {
                lats[vertex] += q;
                if routing_active {
                    self.vol_rdii += q * dt;
                }
                // §8.1: sewer inflow at its constant concentration.
                for (ci, c) in self.net.constituents.iter().enumerate() {
                    mass[2][ci][vertex] += q * c.c_rdii;
                }
            }
            // §14.8.2: a saved runoff file holds what the results file
            // reports for each parcel, at every hydrology step rather than
            // at the reporting cadence, so the file can replay the run it
            // came from.
            if self.runoff_out.is_some() {
                let rows: Vec<_> = self
                    .parcel_records(&surface, &|pi| surface.qstep(pi).rain_rate)
                    .iter()
                    .map(parcel_replay)
                    .collect();
                if let Some(out) = &mut self.runoff_out {
                    out.push((dt, rows));
                }
            }
            self.hydro_t += dt;
            self.hydro_prev = std::mem::replace(&mut self.hydro_now, (self.hydro_t, lats));
            self.hydro_mass_prev = std::mem::replace(&mut self.hydro_mass_now, mass);
            if surface.degraded && !self.hydro_degraded_warned {
                self.hydro_degraded_warned = true;
                self.notices.push(RuntimeNotice {
                    t: self.hydro_t,
                    message: "surface integration ran at its floor below tolerance (§3.5)"
                        .to_string(),
                });
            }
        }
        self.surface = Some(surface);
    }

    /// The §4.2 snow climate for this step, when the model melts snow:
    /// air temperature from its series with the monthly offset, wind from
    /// the monthly averages, and the seasonal melt sweep.
    fn snow_climate(&mut self, month_index: usize) -> Option<SnowClimate> {
        use crate::model::WindSource;
        let sm = self.net.climate.snowmelt.clone()?;
        let t = self.hydro_t;
        let ta = self.air_temperature(t, month_index);
        let wind = match &self.net.climate.wind {
            WindSource::Monthly(w) => w[month_index],
            WindSource::File => self.climate_state.wind,
        };
        // Day of year for the seasonal sweep.
        let epoch_days = ((self.start_epoch + t) / 86_400.0).floor() as i64;
        let date = civil_from_days(epoch_days);
        let jan1 = crate::io::options::Date {
            year: date.year,
            month: 1,
            day: 1,
        };
        let day = (epoch_days - days_from_civil(jan1) + 1) as f64;
        Some(SnowClimate {
            ta,
            wind,
            snow_temp: sm.snow_temp,
            ati_weight: sm.ati_weight,
            rnm: sm.negative_melt_ratio,
            elevation: sm.elevation,
            season: (0.017_261_5 * (day - 81.0)).sin(),
            adc_impervious: self.net.climate.adc_impervious,
            adc_pervious: self.net.climate.adc_pervious,
        })
    }

    /// Advance the §3.1 climate day-state to the day containing run
    /// time `t`: pull the daily record (missing values inheriting the
    /// most recent), place the sinusoidal min/max clock, roll the 7-day
    /// Hargreaves window, and pan-scale the file evaporation.
    fn update_climate_day(&mut self, t: f64) {
        if self.climate_records.is_empty() {
            return;
        }
        let day = ((self.start_epoch + t) / 86_400.0).floor() as i64;
        if day <= self.climate_state.last_day {
            return;
        }
        let us = self.net.options.flow_units.is_us();
        let to_f = |v: f64| if us { v } else { v * 1.8 + 32.0 };
        let date = civil_from_days(day);
        // The most recent record at or before today, inheriting missing
        // values from earlier days (§3.1).
        let (mut tmax, mut tmin, mut evap, mut wind) = (None, None, None, None);
        for r in &self.climate_records {
            if days_from_civil(r.date) > day {
                break;
            }
            tmax = r.tmax.or(tmax);
            tmin = r.tmin.or(tmin);
            evap = r.evap.or(evap);
            wind = r.wind.or(wind);
        }
        let m = (date.month - 1) as usize;
        let adj = self.net.climate.adjust_temperature[m];
        let adj_f = if us { adj } else { adj * 1.8 };
        let mut lo = to_f(tmin.unwrap_or(0.0)) + adj_f;
        let mut hi = to_f(tmax.unwrap_or(0.0)) + adj_f;
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        let st = &mut self.climate_state;
        st.tmin = lo;
        st.tmax = hi;
        st.tave = (lo + hi) / 2.0;
        st.trng = (hi - lo) / 2.0;
        st.trng1 = match st.prev_tmax {
            Some(p) => p - lo,
            None => hi - lo,
        };
        st.prev_tmax = Some(hi);
        // Min at sunrise, max three hours before sunset (§3.1).
        let jan1 = crate::io::options::Date {
            year: date.year,
            month: 1,
            day: 1,
        };
        let doy = (day - days_from_civil(jan1) + 1) as f64;
        let decl = 0.40928 * (0.017202 * (172.0 - doy)).cos();
        // §3.1: without a [SNOWMELT] declaration the site latitude is the
        // predecessor's 40°N default — 0° would flatten day-length (and
        // Hargreaves ET) seasonally.
        let lat = self
            .net
            .climate
            .snowmelt
            .as_ref()
            .map_or(40.0, |sm| sm.latitude);
        let arg = -decl.tan() * (lat.to_radians()).tan();
        let arg = if arg <= -1.0 {
            std::f64::consts::PI
        } else if arg >= 1.0 {
            0.0
        } else {
            arg.acos()
        };
        let hrang = 3.8197 * arg;
        st.hrsr = 12.0 - hrang;
        st.hrss = 12.0 + hrang - 3.0;
        st.dhrdy = st.hrsr - st.hrss;
        st.dydif = 24.0 + st.hrsr - st.hrss;
        st.hrday = (st.hrsr + st.hrss) / 2.0;
        st.last_day = day;
        // The 7-day Hargreaves window (§3.1).
        let (ta, tr) = ((lo + hi) / 2.0, (hi - lo).abs());
        if st.ma_ta.len() == 7 {
            let n = 7.0;
            st.t_ave7 = (st.t_ave7 * n + ta - st.ma_ta[st.front]) / n;
            st.t_rng7 = (st.t_rng7 * n + tr - st.ma_tr[st.front]) / n;
            st.ma_ta[st.front] = ta;
            st.ma_tr[st.front] = tr;
            st.front = (st.front + 1) % 7;
        } else {
            let n = st.ma_ta.len() as f64;
            st.t_ave7 = (st.t_ave7 * n + ta) / (n + 1.0);
            st.t_rng7 = (st.t_rng7 * n + tr) / (n + 1.0);
            st.ma_ta.push(ta);
            st.ma_tr.push(tr);
            st.front = st.ma_ta.len() % 7;
        }
        // Hargreaves (§3.1), evaluated in its fitted units.
        {
            let a = 2.0 * std::f64::consts::PI / 365.0;
            let ta_c = (st.t_ave7 - 32.0) * 5.0 / 9.0;
            let tr_c = st.t_rng7 * 5.0 / 9.0;
            let lamda = 2.50 - 0.002361 * ta_c;
            let dr = 1.0 + 0.033 * (a * doy).cos();
            let phi = lat.to_radians();
            let del = 0.4093 * (a * (284.0 + doy)).sin();
            let cos_omega = (-phi.tan() * del.tan()).clamp(-1.0, 1.0);
            let omega = cos_omega.acos();
            let ra =
                37.6 * dr * (omega * phi.sin() * del.sin() + phi.cos() * del.cos() * omega.sin());
            let e_mm_day = (0.0023 * ra / lamda * tr_c.max(0.0).sqrt() * (ta_c + 17.8)).max(0.0);
            st.hargreaves = e_mm_day * 1.0e-3 / 86_400.0;
        }
        // Pan-scaled file evaporation (§3.1).
        if let crate::model::EvaporationSource::File { pan } = &self.net.climate.evaporation {
            let e = evap.unwrap_or(0.0) * pan[m];
            st.file_evap = e * if us { 0.0254 } else { 1.0e-3 } / 86_400.0;
        }
        // §14.14: the climate file's wind column is miles per hour in
        // both unit systems, unlike a monthly declaration, which arrives
        // already converted. Both feed the same rain-melt relation, so
        // both have to reach it in the same unit.
        st.wind = wind.unwrap_or(0.0) * 0.447_04;
    }

    /// The air temperature in force at run time `t` (°C), from whichever
    /// source the model declares (§3.1).
    ///
    /// Separate from [`Self::snow_climate`] because the two answer
    /// different questions and the answers differ. A run always has an air
    /// temperature — a model declaring no source runs at the predecessor's
    /// constant, and melts a declared pack against it — while a snow
    /// climate exists only for a model that declares snowmelt. Reading the
    /// temperature off the snow climate reported zero for every model
    /// without a `[SNOWMELT]` block, including ones carrying a full
    /// temperature record.
    fn air_temperature(&mut self, t: f64, month_index: usize) -> f64 {
        use crate::model::TemperatureSource;
        match &self.net.climate.temperature {
            Some(TemperatureSource::Series(ts)) => {
                let ts = *ts;
                self.series_value(ts, t, true) + self.net.climate.adjust_temperature[month_index]
            }
            // File temperatures interpolate sinusoidally between the
            // daily extremes (§3.1); the adjustment rode the extremes.
            Some(TemperatureSource::File { .. }) if !self.climate_records.is_empty() => {
                self.climate_temperature(t)
            }
            _ => DEFAULT_AIR_TEMPERATURE_C,
        }
    }

    /// The climate temperature (°C) at run time `t` from daily records:
    /// the §3.1 three-branch sinusoidal interpolation, overnight limb
    /// spanning from the previous day's maximum.
    fn climate_temperature(&self, t: f64) -> f64 {
        let st = &self.climate_state;
        let hour = ((self.start_epoch + t) / 3600.0) % 24.0;
        let pi = std::f64::consts::PI;
        let ta_f = if hour < st.hrsr {
            st.tmin + st.trng1 / 2.0 * (pi / st.dydif * (st.hrsr - hour)).sin()
        } else if hour <= st.hrss {
            st.tave + st.trng * (pi / st.dhrdy * (st.hrday - hour)).sin()
        } else {
            st.tmax - st.trng * (pi / st.dydif * (hour - st.hrss)).sin()
        };
        (ta_f - 32.0) / 1.8
    }

    /// The potential surface evaporation rate (m/s) for a month, from the
    /// §3.1 sources this stage evaluates, plus the monthly adjustment.
    fn evaporation_rate(&self, month: u32) -> f64 {
        self.evaporation_rate_at(month, self.hydro_t)
    }

    /// The potential evaporation rate (m/s) at run time `t`: constant,
    /// monthly, or the §3.1 step-function series — deliberately holding
    /// each entry's rate until the next timestamp, where every other
    /// series interpolates.
    fn evaporation_rate_at(&self, month: u32, t: f64) -> f64 {
        use crate::model::EvaporationSource;
        let m = (month - 1) as usize;
        let base = match &self.net.climate.evaporation {
            EvaporationSource::Constant(e) => *e,
            EvaporationSource::Monthly(ms) => ms[m],
            EvaporationSource::Series(si) => {
                let raw = series_step_value(&self.net, self.start_epoch, *si, t);
                // Values are written in the file's evaporation unit.
                raw * if self.net.options.flow_units.is_us() {
                    0.0254
                } else {
                    1.0e-3
                } / 86_400.0
            }
            EvaporationSource::Temperature => self.climate_state.hargreaves,
            EvaporationSource::File { .. } => self.climate_state.file_evap,
        };
        (base + self.net.climate.adjust_evaporation[m]).max(0.0)
    }

    /// Whether run time `t` lies inside an event window; with no events
    /// declared, everything does.
    fn in_event(&self, t: f64) -> bool {
        if self.events.is_empty() {
            return true;
        }
        self.events.iter().any(|e| t >= e.start && t < e.end)
    }

    /// Advance one routing period (§10.1): assemble forcing at the
    /// period's start, update dynamic boundaries, route — or freeze,
    /// between events — and service any reporting boundary passed.
    /// Returns false once the run is complete.
    pub fn step(&mut self) -> bool {
        let t = self.router.time();
        if t >= self.duration - 1e-9 {
            return false;
        }
        let period_end = (t + self.routing_period)
            .min(self.duration)
            .min(self.next_report);

        // §10.3: hydrology (and the climate day-state) continue between
        // events; only routing freezes.
        let routing_active = self.in_event(t);
        self.advance_hydrology(period_end, routing_active);
        if routing_active {
            self.update_boundary_stages(t);
            // §7.7: channels evaporate at the session's potential rate.
            let month = self.calendar(t).0;
            self.router.evap_rate = self.evaporation_rate(month);
            let (base, base_mass) = self.assemble_lateral(t);
            self.vol_dwf += self.last_dwf_total * (period_end - t);
            self.vol_ext += self.last_ext_total * (period_end - t);
            // §10.1: hydrology outputs interpolate linearly to routing
            // times between the bracketing hydrology results.
            let (t0, l0) = (self.hydro_prev.0, self.hydro_prev.1.clone());
            let (t1, l1) = (self.hydro_now.0, self.hydro_now.1.clone());
            // §12.4: an override replaces the vertex's entire lateral,
            // hydrology terms included.
            let overrides: Vec<(usize, f64)> = self
                .lateral_override
                .iter()
                .map(|(&v, &q)| (v, q))
                .collect();
            let interp = move |tt: f64, lat: &mut [f64]| {
                let f = if t1 > t0 {
                    ((tt - t0) / (t1 - t0)).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                for (i, l) in lat.iter_mut().enumerate() {
                    *l = base[i] + l0[i] + f * (l1[i] - l0[i]);
                }
                for &(v, q) in &overrides {
                    if v < lat.len() {
                        lat[v] = q;
                    }
                }
            };
            if self.controls.is_some() || self.quality.is_some() || self.inlets.is_some() {
                // §9.1: rules evaluate at every routing step — or on the
                // fixed rule-step clock, whose boundaries the stepper
                // lands on — before the step's trials begin; §8.4 quality
                // updates after each accepted step.
                let rule_step = self.net.options.rule_step;
                let np = self.net.constituents.len();
                let nv = self.net.vertices.len();
                let (mt0, mt1) = (self.hydro_prev.0, self.hydro_now.0);
                let mut lat = vec![0.0; nv];
                let mut mass = crate::transport::quality::SourceMass::new(np, nv);
                while self.router.time() < period_end - 1e-9 {
                    let tt = self.router.time();
                    interp(tt, &mut lat);
                    // §7.8: inlets shift lateral flow (and mass) from
                    // bypass to sewer vertices before the step's trials.
                    if let Some(mut inlets) = self.inlets.take() {
                        let f = if mt1 > mt0 {
                            ((tt - mt0) / (mt1 - mt0)).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        mass.compose(&base_mass, &self.hydro_mass_prev, &self.hydro_mass_now, f);
                        let quality = self.quality.as_ref();
                        let conc =
                            move |p: usize, v: usize| quality.map_or(0.0, |q| q.c_vertex[p][v]);
                        let before = lat.clone();
                        inlets.apply(&self.router, &self.net, &mut lat, &mut mass, np, &conc);
                        self.inlets = Some(inlets);
                        // §7.8: the capture transfer is part of each
                        // vertex's lateral for reporting — record the
                        // delta so the period-end snapshot carries it.
                        for (d, (a, b)) in self
                            .last_inlet_lat
                            .iter_mut()
                            .zip(lat.iter().zip(before.iter()))
                        {
                            *d = a - b;
                        }
                        self.last_lat.clone_from(&lat);
                    }
                    if self.controls.is_some() {
                        let mut cap = period_end;
                        if rule_step > 0.0 {
                            if tt + 1e-9 >= self.next_rule_t {
                                self.apply_controls(tt, &lat);
                                self.next_rule_t = ((tt / rule_step).floor() + 1.0) * rule_step;
                            }
                            cap = cap.min(self.next_rule_t);
                        } else {
                            self.apply_controls(tt, &lat);
                        }
                        self.router.step_once(cap, &lat);
                    } else {
                        self.router.step_once(period_end, &lat);
                    }
                    if let Some(mut q) = self.quality.take() {
                        // §8.1 lateral mass: period-start base plus the
                        // hydrology terms interpolated like their flows —
                        // already assembled (and inlet-shifted) when
                        // inlets ran this step.
                        if self.inlets.is_none() {
                            let f = if mt1 > mt0 {
                                ((tt - mt0) / (mt1 - mt0)).clamp(0.0, 1.0)
                            } else {
                                1.0
                            };
                            mass.compose(
                                &base_mass,
                                &self.hydro_mass_prev,
                                &self.hydro_mass_now,
                                f,
                            );
                        }
                        q.update(&self.router, &self.net, &lat, &mass, self.router.last_dt());
                        self.quality = Some(q);
                    }
                }
            } else {
                self.router.advance(period_end, &interp);
            }
            // Retain the end-of-period laterals for the §14.9 records —
            // §7.8 inlet transfers included, or an inlet-fed vertex would
            // report zero inflow while its depth rises.
            let mut lat_now = vec![0.0; self.net.vertices.len()];
            interp(self.router.time(), &mut lat_now);
            for (l, d) in lat_now.iter_mut().zip(&self.last_inlet_lat) {
                *l += d;
            }
            self.last_lat = lat_now;
        } else {
            // Between events the network state freezes and no lateral
            // inflows apply (§10.3) — but rules are operator forcing,
            // not routed state: they evaluate on their §9.1 clock
            // through the gap so time-triggered settings are in place
            // the instant routing resumes.
            if self.controls.is_some() {
                let lat = vec![0.0; self.net.vertices.len()];
                let rule_step = self.net.options.rule_step;
                if rule_step > 0.0 {
                    while self.next_rule_t <= period_end + 1e-9 {
                        let tt = self.next_rule_t;
                        self.apply_controls(tt, &lat);
                        self.next_rule_t = ((tt / rule_step).floor() + 1.0) * rule_step;
                    }
                } else {
                    self.apply_controls(t, &lat);
                }
            }
            self.router.skip_to(period_end);
            self.last_lat = vec![0.0; self.net.vertices.len()];
        }

        while self.next_report <= self.router.time() + 1e-9 {
            let snap = self.record_snapshot(self.next_report);
            // Streamed here rather than gathered and written at the end:
            // an instant is complete the moment it is recorded, and a run
            // long enough to matter cannot hold them all (§14.9).
            if let Some(out) = self.out.as_mut() {
                // A results file that cannot be written is the caller's
                // to hear about; the run itself is unaffected, so the
                // error is carried to `finish_results` rather than
                // stopping the simulation mid-step.
                if let Err(e) = out.append(&snap) {
                    self.out_error.get_or_insert(e);
                }
            }
            if self.retain_snapshots {
                self.snapshots.push(snap);
            }
            // §11.2: the reported maximum is the maximum over these
            // instants alone, which is what a reader finds in the
            // results file.
            self.router.record_reported_depths();
            self.next_report += self.report_step;
        }
        true
    }

    /// The nine reported quantities for every parcel, from live surface
    /// and subsurface state (§14.9).
    ///
    /// Named and extracted because two callers need the identical vector:
    /// the reporting snapshot, and the runoff interface file a run saves
    /// (§14.8.2). The file is defined as holding what the results file
    /// reports, so the two cannot be allowed to drift.
    ///
    /// `rain` is the one field they disagree about, so each caller states
    /// it. A reporting record carries the gage's rate at the reporting
    /// instant (§14.9); an interface row carries the rate the step it
    /// replays actually ran on (§14.8.2). Reading one from the other is
    /// how the reported series came to sit an interval early.
    fn parcel_records(
        &self,
        surface: &Surface,
        rain: &dyn Fn(usize) -> f64,
    ) -> Vec<SubcatchRecord> {
        let np = self.net.constituents.len();
        let mut out = Vec::with_capacity(self.net.parcels.len());
        for pi in 0..self.net.parcels.len() {
            let q = surface.qstep(pi);
            let (infil, evap) = surface.parcel_infil_evap(pi);
            let washoff = self
                .surface_quality
                .as_ref()
                .map_or_else(|| vec![0.0; np], |sq| sq.conc[pi].clone());
            let mut rec = SubcatchRecord {
                rain: rain(pi),
                snow_depth: q.snow_depth,
                evap,
                infil,
                runoff: surface.parcel_runoff(pi),
                gw_flow: 0.0,
                gw_elev: 0.0,
                soil_moisture: 0.0,
                washoff,
            };
            if let Some((_, gw)) = self.aquifers.iter().find(|(p, _)| *p == pi) {
                rec.gw_flow = gw.flow * self.net.parcels[pi].area;
                rec.gw_elev = gw.table_elevation();
                rec.soil_moisture = gw.theta;
            }
            out.push(rec);
        }
        out
    }

    /// Assemble the full §14.9 record set at a reporting boundary.
    fn record_snapshot(&mut self, t: f64) -> Snapshot {
        let month = self.calendar(t).0;
        let air_temp = self.air_temperature(t, (month - 1) as usize);
        let nv = self.net.vertices.len();
        let nl = self.net.links.len();
        let np = self.net.constituents.len();
        let r = &self.router;
        let depths: Vec<f64> = (0..nv).map(|v| r.depth(v)).collect();
        let flows: Vec<f64> = (0..nl).map(|l| r.flow(l, &self.net)).collect();
        // Total inflow per vertex: laterals plus arriving link flows.
        let mut inflow = self.last_lat.clone();
        for &(_, from, to, q, _) in &r.channel_transport() {
            if q > 0.0 {
                inflow[to] += q;
            } else {
                inflow[from] += -q;
            }
        }
        for &(_, from, to, q) in &r.structure_transport() {
            if q > 0.0 {
                inflow[to] += q;
            } else {
                inflow[from] += -q;
            }
        }
        // Link geometry records.
        let mut link_volume = vec![0.0; nl];
        let mut link_capacity = vec![0.0; nl];
        for &(li, _, _, _, vol) in &r.channel_transport() {
            link_volume[li] = vol;
            if let Some((_, y_full, _, _)) = r.chan_full_attrs(li) {
                if y_full > 0.0 {
                    link_capacity[li] = (r.link_depth(li).unwrap_or(0.0) / y_full).clamp(0.0, 1.0);
                }
            }
        }
        for &(li, _, _, _) in &r.structure_transport() {
            link_capacity[li] = r.setting(li).unwrap_or(1.0);
        }
        // Surface and subsurface records.
        let mut subcatch = Vec::with_capacity(self.net.parcels.len());
        // §14.8.2: a replayed run never steps the surface, so its live
        // state says nothing. The record in force is what the file put
        // there, already in engine units.
        if self.runoff_in.is_some() {
            for pi in 0..self.net.parcels.len() {
                let r = self.runoff_now.get(pi);
                subcatch.push(SubcatchRecord {
                    rain: r.map_or(0.0, |r| r.rainfall),
                    snow_depth: r.map_or(0.0, |r| r.snow_depth),
                    evap: r.map_or(0.0, |r| r.evap),
                    infil: r.map_or(0.0, |r| r.infil),
                    runoff: r.map_or(0.0, |r| r.runoff),
                    gw_flow: r.map_or(0.0, |r| r.gw_flow),
                    gw_elev: r.map_or(0.0, |r| r.gw_elev),
                    soil_moisture: r.map_or(0.0, |r| r.soil_moisture),
                    washoff: r.map_or_else(|| vec![0.0; np], |r| r.washoff.clone()),
                });
            }
        } else if let Some(surface) = &self.surface {
            // §14.9: precipitation is looked up at the reporting instant,
            // under the monthly adjustment in force there.
            let epoch = self.start_epoch + t;
            let factor = self.net.climate.adjust_rainfall[(month - 1) as usize];
            subcatch =
                self.parcel_records(surface, &|pi| surface.parcel_rain_at(pi, epoch) * factor);
        }
        // The fifteen system series (§14.9), SI.
        let total_area: f64 = self.net.parcels.iter().map(|p| p.area).sum();
        let wmean = |f: &dyn Fn(&SubcatchRecord, f64) -> f64| -> f64 {
            if total_area <= 0.0 {
                return 0.0;
            }
            subcatch
                .iter()
                .zip(&self.net.parcels)
                .map(|(rec, p)| f(rec, p.area))
                .sum::<f64>()
                / total_area
        };
        let system = [
            air_temp,
            wmean(&|rec, a| rec.rain * a),
            wmean(&|rec, a| rec.snow_depth * a),
            wmean(&|rec, a| rec.infil * a),
            subcatch.iter().map(|rec| rec.runoff).sum(),
            self.last_dwf_total,
            subcatch.iter().map(|rec| rec.gw_flow).sum(),
            self.rdii.iter().map(|x| x.flow).sum(),
            self.last_ext_total,
            self.last_lat.iter().sum(),
            (0..nv).map(|v| r.flood_rate(v)).sum(),
            r.outflow_rate(),
            (0..nv).map(|v| r.vertex_volume_now(v)).sum::<f64>()
                + r.channel_transport().iter().map(|c| c.4).sum::<f64>(),
            wmean(&|rec, a| rec.evap * a),
            self.evaporation_rate(month),
        ];
        Snapshot {
            t,
            node_head: (0..nv).map(|v| r.vertex_invert(v) + depths[v]).collect(),
            node_volume: (0..nv).map(|v| r.vertex_volume_now(v)).collect(),
            node_lateral: self.last_lat.clone(),
            node_inflow: inflow,
            node_flooding: (0..nv).map(|v| r.flood_rate(v)).collect(),
            node_quality: self
                .quality
                .as_ref()
                .map_or_else(|| vec![vec![0.0; nv]; np], |q| q.c_vertex.clone()),
            link_depth: (0..nl).map(|l| r.link_depth(l).unwrap_or(0.0)).collect(),
            link_velocity: (0..nl).map(|l| r.link_velocity(l).unwrap_or(0.0)).collect(),
            link_volume,
            link_capacity,
            link_quality: self.quality.as_ref().map_or_else(
                || vec![vec![0.0; nl]; np],
                |q| {
                    (0..np)
                        .map(|p| {
                            (0..nl)
                                .map(|l| q.link_concentration(r, p, l).unwrap_or(0.0))
                                .collect()
                        })
                        .collect()
                },
            ),
            subcatch,
            depths,
            flows,
            system,
        }
    }

    /// Run to completion.
    pub fn run(&mut self) {
        while self.step() {}
    }

    /// The calendar decomposition of run time `t`: (month 1–12, weekday
    /// Sunday = 0, hour 0–23, seconds past midnight).
    fn calendar(&self, t: f64) -> (u32, u32, u32, f64) {
        let epoch = self.start_epoch + t;
        let days = (epoch / 86_400.0).floor() as i64;
        let secs = epoch - days as f64 * 86_400.0;
        let date = civil_from_days(days);
        (date.month, weekday(days), (secs / 3600.0) as u32, secs)
    }

    /// The combined factor of a sanitary-inflow pattern set (§2.9): the
    /// slots multiply, except that on a weekend day a weekend-hourly
    /// pattern *replaces* any hourly one — the predecessor's semantics,
    /// where multiplying both would double-scale weekend flow.
    fn dwf_pattern_factor(&self, patterns: [Option<usize>; 4], t: f64) -> f64 {
        let (_, wday, _, _) = self.calendar(t);
        let weekend = wday == 0 || wday == 6;
        let has_weekend = patterns.iter().flatten().any(|&p| {
            matches!(
                self.net.patterns[p].kind,
                crate::model::PatternKind::Weekend
            )
        });
        let mut f = 1.0;
        for p in patterns.iter().flatten() {
            if weekend
                && has_weekend
                && matches!(
                    self.net.patterns[*p].kind,
                    crate::model::PatternKind::Hourly
                )
            {
                continue;
            }
            f *= self.pattern_factor(Some(*p), t);
        }
        f
    }

    /// A pattern's factor at run time `t`; a missing slot is 1.
    fn pattern_factor(&self, pattern: Option<usize>, t: f64) -> f64 {
        let Some(p) = pattern else {
            return 1.0;
        };
        let pat = &self.net.patterns[p];
        let (month, wday, hour, _) = self.calendar(t);
        let idx = match pat.kind {
            PatternKind::Monthly => (month - 1) as usize,
            PatternKind::Daily => wday as usize,
            PatternKind::Hourly => hour as usize,
            PatternKind::Weekend => {
                if wday == 0 || wday == 6 {
                    hour as usize
                } else {
                    return 1.0;
                }
            }
        };
        pat.factors.get(idx).copied().unwrap_or(1.0)
    }

    /// Evaluate the §9 rules at the current state and apply the winning
    /// per-link settings; fired constant actions land in the action log.
    fn apply_controls(&mut self, t: f64, lat: &[f64]) {
        let Some(mut controls) = self.controls.take() else {
            return;
        };
        let epoch = self.start_epoch + t;
        let (month, _, _, _) = self.calendar(t);
        let rain_factor = self.net.climate.adjust_rainfall[(month - 1) as usize];
        let surface = self.surface.as_ref();
        let net = &self.net;
        let start_epoch = self.start_epoch;
        let gage_intensity =
            move |g: usize| surface.map_or(0.0, |s| s.gage_rate(g, epoch)) * rain_factor;
        let gage_past = move |g: usize, n: u32| {
            surface.map_or(0.0, |s| s.gage_past_depth(g, epoch, n)) * rain_factor
        };
        let series_value = move |si: usize| series_value_pure(net, start_epoch, si, t, true);
        let applied = controls.evaluate(&super::controls::ControlView {
            router: &self.router,
            net: &self.net,
            gage_intensity: &gage_intensity,
            gage_past: &gage_past,
            laterals: lat,
            series_value: &series_value,
            elapsed: t,
            date_days: (self.start_epoch + t) / 86_400.0,
            dt: self.router.last_dt(),
        });
        for (li, v, ai) in applied {
            // §12.4: an injection has taken this link over until it is
            // released, so a rule acting on it does nothing at all rather
            // than fighting it step by step.
            if self.setting_override.contains_key(&li) {
                continue;
            }
            if self.router.set_setting(li, v) == Some(true) {
                controls.log_action(t, ai, &self.net.links[li].id, v);
            }
        }
        // §9.3: a domain-guarded rule expression announces itself once.
        for name in controls.guard_events.drain(..) {
            self.notices.push(RuntimeNotice {
                t,
                message: format!(
                    "rule expression {name} was domain-guarded to zero at least once (§9.3)"
                ),
            });
        }
        self.controls = Some(controls);
    }

    /// The §11.1 conservation ledgers over the run so far.
    pub fn ledgers(&self) -> Ledgers {
        const VOL_TOL: f64 = 0.0283;
        const MASS_TOL: f64 = 0.001;
        // §14.8.2: a replayed run has no surface or subsurface balance to
        // report. The file carries the flows those compartments produced,
        // not the rainfall, storage and losses a balance is made of, so
        // their accumulators are untouched and a balance built from them
        // would read as a perfectly conserving compartment that never ran.
        let replaying = self.runoff_in.is_some();
        let surface = self.surface.as_ref().filter(|_| !replaying).map(|s| {
            balance(
                s.rainfall + s.runon_in + s.initial_storage,
                s.evap_vol + s.infil_vol + s.runoff_out + s.snow_plowed + s.stored_volume(),
                VOL_TOL,
            )
        });
        let subsurface = if self.aquifers.is_empty() || replaying {
            None
        } else {
            let (mut i, mut o) = (0.0, 0.0);
            for (_, gw) in &self.aquifers {
                i += gw.infil_in + gw.initial_storage;
                o += gw.evap_out + gw.perc_out + gw.lateral_out + gw.stored_volume();
            }
            Some(balance(i, o, VOL_TOL))
        };
        let r = &self.router.report;
        let stored_now: f64 = (0..self.net.vertices.len())
            .map(|v| self.router.vertex_volume_now(v))
            .sum::<f64>()
            + self
                .router
                .channel_transport()
                .iter()
                .map(|c| c.4)
                .sum::<f64>();
        let network = balance(
            r.inflow + r.initial_storage,
            r.outflow + r.flooding + r.losses + r.negative_out + stored_now,
            VOL_TOL,
        );
        let mut constituents = Vec::new();
        if let Some(q) = &self.quality {
            for (p, c) in self.net.constituents.iter().enumerate() {
                let mut i = q.initial_mass[p] + q.inflow_mass[p];
                let mut o = q.outfall_mass[p]
                    + q.flooded_mass[p]
                    + q.reacted[p]
                    + q.final_storage[p]
                    + q.seepage_mass[p]
                    + q.stored_mass(p);
                // Count-unit constituents report on the log scale (§11.1).
                if c.units == crate::model::ConcentrationUnits::CountPerL {
                    i = i.max(1.0).log10();
                    o = o.max(1.0).log10();
                }
                constituents.push((c.id.clone(), balance(i, o, MASS_TOL)));
            }
        }
        let mut loading = Vec::new();
        if let Some(sq) = &self.surface_quality {
            for (p, c) in self.net.constituents.iter().enumerate() {
                let i = sq.initial_buildup[p] + sq.buildup_in[p] + sq.deposition[p];
                let o = sq.swept[p]
                    + sq.infiltrated[p]
                    + sq.bmp_removed[p]
                    + sq.washed_off[p]
                    + sq.to_final[p]
                    + sq.stored_mass(p);
                loading.push((c.id.clone(), balance(i, o, MASS_TOL)));
            }
        }
        Ledgers {
            surface,
            subsurface,
            network,
            constituents,
            loading,
        }
    }

    /// Save a predecessor hotstart file (`SWMM5-HOTSTART4`, §14.8) to
    /// `w`: the runoff block (sub-area depths, infiltration, subsurface,
    /// snow, and quality state) then the routing block, in the
    /// predecessor's internal units — including its multi-pollutant
    /// buildup write asymmetry, reproduced so its readers behave
    /// identically on this engine's files.
    pub fn save_hotstart(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        const FT: f64 = 0.3048;
        const CFS: f64 = 0.028_316_846_592;
        let put_i = |w: &mut dyn std::io::Write, v: i32| w.write_all(&v.to_le_bytes());
        let put_f = |w: &mut dyn std::io::Write, v: f64| w.write_all(&(v as f32).to_le_bytes());
        let put_d = |w: &mut dyn std::io::Write, v: f64| w.write_all(&v.to_le_bytes());
        w.write_all(b"SWMM5-HOTSTART4")?;
        let np = self.net.constituents.len();
        put_i(w, self.net.parcels.len() as i32)?;
        put_i(w, self.net.land_uses.len() as i32)?;
        put_i(w, self.net.vertices.len() as i32)?;
        put_i(w, self.net.links.len() as i32)?;
        put_i(w, np as i32)?;
        put_i(w, self.net.options.flow_units as i32)?;

        // ── Runoff block ────────────────────────────────────────────────
        if let Some(surface) = &self.surface {
            for pi in 0..self.net.parcels.len() {
                let d = surface.subarea_depths(pi);
                for v in d {
                    put_d(w, v / FT)?;
                }
                put_d(w, surface.parcel_runoff(pi) / CFS)?;
                let x = surface.infil_state(pi).unwrap_or([0.0; 6]);
                for (slot, v) in x.into_iter().enumerate() {
                    put_d(w, v / infil_slot_ft(self.net.options.infiltration, slot))?;
                }
                if let Some((_, gw)) = self.aquifers.iter().find(|(p, _)| *p == pi) {
                    let (theta, elev, flow, accept) = gw.hotstart_get();
                    put_d(w, theta)?;
                    put_d(w, elev / FT)?;
                    put_d(w, flow / FT)?;
                    put_d(w, accept / FT)?;
                }
                if let Some(snow) = surface.snow_state(pi) {
                    for sf in snow {
                        put_d(w, sf[0] / FT)?;
                        put_d(w, sf[1] / FT)?;
                        put_d(w, sf[2] / FT)?;
                        put_d(w, sf[3])?;
                        put_d(w, sf[4])?;
                    }
                }
                if np > 0 {
                    let (ponded, slots) = self
                        .surface_quality
                        .as_ref()
                        .map(|sq| sq.hotstart_get(pi))
                        .unwrap_or((vec![0.0; np], Vec::new()));
                    let conc = self
                        .surface_quality
                        .as_ref()
                        .map(|sq| sq.conc[pi].clone())
                        .unwrap_or(vec![0.0; np]);
                    for v in conc {
                        put_d(w, v)?;
                    }
                    for (ci, v) in ponded.iter().enumerate() {
                        put_d(w, v / self.mass_cv(ci))?;
                    }
                    for k in 0..self.net.land_uses.len() {
                        let (row, swept) = slots.get(k).cloned().unwrap_or((vec![0.0; np], 0.0));
                        for (ci, b) in row.iter().enumerate() {
                            // The predecessor writes np doubles per pair.
                            for _ in 0..np {
                                put_d(w, b / self.mass_cv(ci))?;
                            }
                        }
                        put_d(w, swept + 25_569.0)?;
                    }
                }
            }
        }

        // ── Routing block ───────────────────────────────────────────────
        let (depths, links) = self.router.hotstart_get(self.net.links.len());
        for (vi, d) in depths.iter().enumerate() {
            put_f(w, d / FT)?;
            put_f(w, self.last_lat.get(vi).copied().unwrap_or(0.0) / CFS)?;
            if self.router.is_storage(vi) {
                let hrt = self.quality.as_ref().map_or(0.0, |q| q.hrt[vi]);
                put_f(w, hrt)?;
            }
            for p in 0..np {
                let c = self.quality.as_ref().map_or(0.0, |q| q.c_vertex[p][vi]);
                put_f(w, c)?;
            }
        }
        for (li, &(q, d, setting)) in links.iter().enumerate() {
            put_f(w, q / CFS)?;
            put_f(w, d / FT)?;
            put_f(w, setting)?;
            for p in 0..np {
                let c = self
                    .quality
                    .as_ref()
                    .and_then(|qq| qq.link_concentration(&self.router, p, li))
                    .unwrap_or(0.0);
                put_f(w, c)?;
            }
        }
        Ok(())
    }

    fn mass_cv(&self, ci: usize) -> f64 {
        use crate::model::ConcentrationUnits;
        let us = self.net.options.flow_units.is_us();
        match self.net.constituents[ci].units {
            ConcentrationUnits::MgPerL => {
                if us {
                    453.592_37
                } else {
                    1000.0
                }
            }
            ConcentrationUnits::UgPerL => {
                if us {
                    453_592.37
                } else {
                    1.0e6
                }
            }
            ConcentrationUnits::CountPerL => 1.0e-3,
        }
    }

    /// Load a predecessor hotstart file (§14.8), versions 3 and 4, restoring
    /// what the format carries; what it cannot carry is named in the
    /// notices. Object counts must match the model.
    pub fn load_hotstart(&mut self, bytes: &[u8]) -> Result<(), String> {
        const FT: f64 = 0.3048;
        const CFS: f64 = 0.028_316_846_592;
        let mut pos;
        let version = if bytes.len() >= 15 && &bytes[..14] == b"SWMM5-HOTSTART" {
            let v = (bytes[14] as char).to_digit(10);
            match v {
                Some(v) => {
                    pos = 15;
                    v
                }
                None => {
                    pos = 14;
                    1
                }
            }
        } else {
            return Err("not a SWMM5-HOTSTART file".into());
        };
        // §14.8: versions 1–2 carry layouts this engine does not read
        // (per-node constituent tails, version-2 groundwater prefixes);
        // misreading them would silently misalign every later record.
        if version < 3 {
            return Err(format!(
                "hotstart version {version} is not supported. Resave as version 3 or 4"
            ));
        }
        let get_i = |pos: &mut usize| -> Result<i32, String> {
            let b: [u8; 4] = bytes
                .get(*pos..*pos + 4)
                .ok_or("truncated hotstart file")?
                .try_into()
                .unwrap();
            *pos += 4;
            Ok(i32::from_le_bytes(b))
        };
        let np = self.net.constituents.len();
        let n_sub = if version >= 2 {
            get_i(&mut pos)? as usize
        } else {
            self.net.parcels.len()
        };
        let n_land = if version >= 3 {
            get_i(&mut pos)? as usize
        } else {
            self.net.land_uses.len()
        };
        let n_nodes = get_i(&mut pos)? as usize;
        let n_links = get_i(&mut pos)? as usize;
        let n_pollut = get_i(&mut pos)? as usize;
        let flow_units = get_i(&mut pos)?;
        if n_sub != self.net.parcels.len()
            || n_land != self.net.land_uses.len()
            || n_nodes != self.net.vertices.len()
            || n_links != self.net.links.len()
            || n_pollut != np
            || flow_units != self.net.options.flow_units as i32
        {
            return Err("hotstart object counts do not match the model".into());
        }
        let get_d = |pos: &mut usize| -> Result<f64, String> {
            let b: [u8; 8] = bytes
                .get(*pos..*pos + 8)
                .ok_or("truncated hotstart file")?
                .try_into()
                .unwrap();
            *pos += 8;
            let v = f64::from_le_bytes(b);
            if v.is_nan() {
                return Err("hotstart file carries NaN state".into());
            }
            Ok(v)
        };
        let get_f = |pos: &mut usize| -> Result<f64, String> {
            let b: [u8; 4] = bytes
                .get(*pos..*pos + 4)
                .ok_or("truncated hotstart file")?
                .try_into()
                .unwrap();
            *pos += 4;
            let v = f32::from_le_bytes(b);
            if v.is_nan() {
                return Err("hotstart file carries NaN state".into());
            }
            Ok(f64::from(v))
        };

        // ── Runoff block (version ≥ 3) ──────────────────────────────────
        if version >= 3 {
            if let Some(mut surface) = self.surface.take() {
                for pi in 0..self.net.parcels.len() {
                    let mut d = [0.0; 3];
                    for v in &mut d {
                        *v = get_d(&mut pos)? * FT;
                    }
                    let runoff = get_d(&mut pos)? * CFS;
                    surface.hotstart_set(pi, d, runoff);
                    let mut x = [0.0; 6];
                    for (slot, v) in x.iter_mut().enumerate() {
                        *v = get_d(&mut pos)? * infil_slot_ft(self.net.options.infiltration, slot);
                    }
                    surface.set_infil_state(pi, x);
                    if let Some((_, gw)) = self.aquifers.iter_mut().find(|(p, _)| *p == pi) {
                        let theta = get_d(&mut pos)?;
                        let elev = get_d(&mut pos)? * FT;
                        let flow = get_d(&mut pos)? * FT;
                        let _accept = get_d(&mut pos)?;
                        gw.hotstart_set(theta, elev, flow);
                    }
                    if surface.snow_state(pi).is_some() {
                        let mut snow = [[0.0; 5]; 3];
                        for sf in &mut snow {
                            sf[0] = get_d(&mut pos)? * FT;
                            sf[1] = get_d(&mut pos)? * FT;
                            sf[2] = get_d(&mut pos)? * FT;
                            sf[3] = get_d(&mut pos)?;
                            sf[4] = get_d(&mut pos)?;
                        }
                        surface.set_snow_state(pi, snow);
                    }
                    if np > 0 {
                        let mut conc = vec![0.0; np];
                        for v in &mut conc {
                            *v = get_d(&mut pos)?;
                        }
                        let mut ponded = vec![0.0; np];
                        for (ci, v) in ponded.iter_mut().enumerate() {
                            *v = get_d(&mut pos)? * self.mass_cv(ci);
                        }
                        let mut slots = Vec::new();
                        for _ in 0..self.net.land_uses.len() {
                            let mut row = vec![0.0; np];
                            for (ci, b) in row.iter_mut().enumerate() {
                                *b = get_d(&mut pos)? * self.mass_cv(ci);
                                // §14.8: the writer emits np doubles per
                                // slot; the leading one is the value.
                                for _ in 1..np {
                                    let _ = get_d(&mut pos)?;
                                }
                            }
                            let swept = get_d(&mut pos)? - 25_569.0;
                            slots.push((row, swept));
                        }
                        if let Some(sq) = &mut self.surface_quality {
                            sq.conc[pi] = conc;
                            sq.hotstart_set(pi, ponded, slots);
                        }
                    }
                }
                // §11.1: restored storage is this run's starting storage
                // — the same rebasing the router applies below. Without
                // it the restored ponded water drains as runoff that was
                // never an inflow, and the surface ledger carries the
                // difference all run.
                surface.initial_storage = surface.stored_volume();
                self.surface = Some(surface);
                for (_, gw) in &mut self.aquifers {
                    gw.initial_storage = gw.stored_volume();
                }
            }
        }

        // ── Routing block ───────────────────────────────────────────────
        let nv = self.net.vertices.len();
        let mut depths = vec![0.0; nv];
        for (vi, d) in depths.iter_mut().enumerate() {
            *d = get_f(&mut pos)? * FT;
            let _lat = get_f(&mut pos)?;
            // The storage residence time is a version-4 addition; reading
            // it from a version-3 file would misalign the stream.
            if version >= 4 && self.router.is_storage(vi) {
                let hrt = get_f(&mut pos)?;
                if let Some(q) = &mut self.quality {
                    q.hrt[vi] = hrt;
                }
            }
            for p in 0..np {
                let c = get_f(&mut pos)?;
                if let Some(q) = &mut self.quality {
                    q.c_vertex[p][vi] = c;
                }
            }
        }
        let mut links = vec![(0.0, 0.0, 1.0); self.net.links.len()];
        let chan_slots: Vec<(usize, usize)> = self
            .router
            .channel_transport()
            .iter()
            .enumerate()
            .map(|(k, c)| (c.0, k))
            .collect();
        for (li, slot) in links.iter_mut().enumerate() {
            let q = get_f(&mut pos)? * CFS;
            let d = get_f(&mut pos)? * FT;
            let setting = get_f(&mut pos)?;
            *slot = (q, d, setting);
            for p in 0..np {
                let c = get_f(&mut pos)?;
                if let Some(qq) = &mut self.quality {
                    if let Some(&(_, k)) = chan_slots.iter().find(|(l, _)| *l == li) {
                        qq.c_channel[p][k] = c;
                    }
                }
            }
        }
        self.router.hotstart_apply(&depths, &links);

        // §11.1/§8.4: what was restored is what this run starts with. The
        // water ledgers rebase above; the quality state has to as well —
        // its opening mass, its surface buildup baseline, and the previous
        // volumes the mixing form reads to tell a full vertex from an
        // empty one. Ordered after `hotstart_apply` because the volumes it
        // reads are the restored ones.
        if let Some(q) = &mut self.quality {
            q.rebase_to_restored_state(&self.router);
        }
        if let Some(sq) = &mut self.surface_quality {
            sq.rebase_initial_buildup();
        }

        // What the format cannot carry is named (§14.8).
        if self.net.lid_usage.iter().len() > 0 {
            self.notices.push(RuntimeNotice {
                t: 0.0,
                message: "hotstart restored: the predecessor format carries no \
                          control-measure layer state; units start from their \
                          build state (§14.8)"
                    .to_string(),
            });
        }
        Ok(())
    }

    /// Supply the routing interface inflow file's text (§14.8); the
    /// caller owns reading it. Values interpolate between bracketing
    /// periods and add as boundary inflows at their vertices.
    pub fn supply_routing_inflows(&mut self, text: &str) -> Result<(), String> {
        self.iface_in = Some(crate::io::iface::parse_routing_file(text, &self.net)?);
        self.note_supplied("routing inflows", text.as_bytes());
        Ok(())
    }

    /// Supply the runoff interface file's bytes (§14.8.2); the caller owns
    /// reading it. The surface is then not run at all: the file replays
    /// what it produced, and the hydrology clock follows the file's steps
    /// rather than the model's wet and dry ones.
    pub fn supply_runoff(&mut self, bytes: &[u8]) -> Result<(), String> {
        // §14.8.2: a run either replays a hydrology or records the one it
        // computes. Accepting both would write a file that only copies its
        // input, under a name that claims to be this run's own hydrology.
        if self.runoff_out.is_some() {
            return Err(
                "this run saves a runoff interface file, so it cannot also replay one \
                 (§14.8.2)"
                    .into(),
            );
        }
        let ifc = crate::io::iface::parse_runoff_file(bytes, &self.net)?;
        self.runoff_in = Some((ifc, 0));
        self.note_supplied("runoff", bytes);
        Ok(())
    }

    /// Supply the RDII interface file's bytes (§14.8.1); the caller owns
    /// reading it. Either of the predecessor's encodings is accepted, and
    /// the file replaces the RDII convolutions rather than adding to them.
    pub fn supply_rdii(&mut self, bytes: &[u8]) -> Result<(), String> {
        let cv = crate::io::iface::flow_cv_of(self.net.options.flow_units);
        self.rdii_in = Some(crate::io::iface::parse_rdii_file(bytes, &self.net, cv)?);
        self.note_supplied("sewer inflow", bytes);
        Ok(())
    }

    /// Write a checkpoint of this session (§12.3).
    ///
    /// A model using a capability whose state is not yet carried is
    /// refused with the capability named: restoring it from a default
    /// would continue plausibly and wrongly, which is the one outcome a
    /// checkpoint must not have.
    pub fn save_checkpoint(&self, w: &mut impl std::io::Write) -> Result<(), String> {
        // §12.3: a checkpoint carries the results so far, so that a run
        // restored from it writes the whole run's results. A session that
        // said it would not be checkpointed kept none of them, and a
        // checkpoint written from what is left would silently lose every
        // instant before this one.
        if !self.retain_snapshots {
            return Err(
                "this run was opened without checkpointing, so the reporting \
                        instants a checkpoint carries were not kept"
                    .into(),
            );
        }
        use crate::simulation::checkpoint as cp;
        // Exhaustive on purpose, and the last type here to become so.
        // Every other state type the checkpoint touches is read through a
        // destructure like this, so a field added to one fails the build
        // until it is written or declared a parameter; the session was
        // the exception, and a field added to it escaped silently. Two
        // did, in the change that published mid-run forcing.
        //
        // The bindings are used below rather than `self.field`, which is
        // what makes this a check and not a comment: a state field bound
        // and then not written is an unused variable, and unused
        // variables fail this build.
        let Simulation {
            // Parameters: the model builds these, and opening it again
            // builds them the same.
            net: _,
            start_epoch: _,
            duration: _,
            report_step: _,
            routing_period: _,
            events: _,
            vertex_by_id: _,
            link_by_id: _,
            climate_records: _,
            // Output in flight, and not state: a resumed run attaches
            // its own sink, and what has already been written is in the
            // file the caller holds, not in the checkpoint.
            out: _,
            out_error: _,
            retain_snapshots: _,
            // Supplied by the caller, who must supply them again: their
            // contents are not copied here, only fingerprinted.
            iface_in: _,
            rdii_in: _,
            // The rainfall cache is fixed at load from those records, so
            // a restored session already holds it.
            rain_out: _,
            // State.
            router,
            surface,
            aquifers,
            rdii,
            hydro_t,
            hydro_prev,
            hydro_now,
            hydro_degraded_warned,
            next_report,
            next_rule_t,
            series_warned,
            lateral_override,
            stage_override,
            setting_override,
            loss_override,
            limit_override,
            concentration_override,
            drain_override,
            controls,
            quality,
            surface_quality,
            inlets,
            climate_state,
            rdii_out,
            runoff_out,
            supplied,
            runoff_in,
            runoff_exhausted,
            runoff_now,
            hydro_mass_prev,
            hydro_mass_now,
            last_lat,
            last_inlet_lat,
            last_ext_total,
            last_dwf_total,
            vol_dwf,
            vol_ext,
            vol_wet,
            vol_gw,
            vol_rdii,
            snapshots,
            notices,
        } = self;
        let io = |e: std::io::Error| e.to_string();
        w.write_all(cp::STAMP).map_err(io)?;
        w.write_all(&cp::VERSION.to_le_bytes()).map_err(io)?;
        cp::put_u(w, self.model_fingerprint()).map_err(io)?;
        // Clocks and the accounting the ledgers are built from.
        for v in [
            *hydro_t,
            *next_report,
            *next_rule_t,
            *vol_dwf,
            *vol_ext,
            *vol_wet,
            *vol_gw,
            *vol_rdii,
            *last_ext_total,
            *last_dwf_total,
        ] {
            cp::put_f(w, v).map_err(io)?;
        }
        for vs in [last_lat, last_inlet_lat] {
            cp::put_fs(w, vs).map_err(io)?;
        }
        for (at, lats) in [hydro_prev, hydro_now] {
            cp::put_f(w, *at).map_err(io)?;
            cp::put_fs(w, lats).map_err(io)?;
        }
        // Written with its own shape rather than flattened: the nesting is
        // source by constituent by vertex, and a flat run of values cannot
        // say which of the three is empty when a model has no constituents.
        for field in [hydro_mass_prev, hydro_mass_now] {
            cp::put_u(w, field.len() as u64).map_err(io)?;
            for rows in field.iter() {
                cp::put_rows(w, rows).map_err(io)?;
            }
        }
        // Latches: a restored run must neither repeat a warning nor
        // swallow one it has not issued (§12.3).
        cp::put_b(w, *hydro_degraded_warned).map_err(io)?;
        cp::put_b(w, *runoff_exhausted).map_err(io)?;
        cp::put_u(w, series_warned.len() as u64).map_err(io)?;
        for flag in series_warned {
            cp::put_b(w, *flag).map_err(io)?;
        }
        // §12.4: an injection standing when a checkpoint is taken stands
        // when it is restored, or the restored run quietly returns the
        // element to the model and diverges from the run it continues.
        for held in [stage_override, setting_override, limit_override] {
            cp::put_u(w, held.len() as u64).map_err(io)?;
            let mut rows: Vec<_> = held.iter().collect();
            rows.sort_by_key(|(k, _)| **k);
            for (at, v) in rows {
                cp::put_u(w, *at as u64).map_err(io)?;
                cp::put_f(w, *v).map_err(io)?;
            }
        }
        cp::put_u(w, drain_override.len() as u64).map_err(io)?;
        let mut drains: Vec<_> = drain_override.iter().collect();
        drains.sort_by_key(|((p, u), _)| (*p, *u));
        for ((pi, unit), d) in drains {
            cp::put_u(w, *pi as u64).map_err(io)?;
            cp::put_u(w, *unit as u64).map_err(io)?;
            for v in [d.coeff, d.exponent, d.offset, d.delay, d.h_open, d.h_close] {
                cp::put_f(w, v).map_err(io)?;
            }
        }
        cp::put_u(w, concentration_override.len() as u64).map_err(io)?;
        let mut concs: Vec<_> = concentration_override.iter().collect();
        concs.sort_by_key(|(k, _)| **k);
        for (v, cs) in concs {
            cp::put_u(w, *v as u64).map_err(io)?;
            cp::put_fs(w, cs).map_err(io)?;
        }
        cp::put_u(w, loss_override.len() as u64).map_err(io)?;
        let mut losses: Vec<_> = loss_override.iter().collect();
        losses.sort_by_key(|(k, _)| **k);
        for (li, (inlet, outlet, average)) in losses {
            cp::put_u(w, *li as u64).map_err(io)?;
            for v in [inlet, outlet, average] {
                cp::put_f(w, *v).map_err(io)?;
            }
        }
        cp::put_u(w, lateral_override.len() as u64).map_err(io)?;
        // Sorted so a checkpoint of one state is one file: a map's own
        // order is not part of the state and must not reach the bytes.
        let mut overrides: Vec<_> = lateral_override.iter().collect();
        overrides.sort_by_key(|(k, _)| **k);
        for (vertex, q) in overrides {
            cp::put_u(w, *vertex as u64).map_err(io)?;
            cp::put_f(w, *q).map_err(io)?;
        }
        climate_state.checkpoint_put(w).map_err(io)?;
        // The output so far, so a restored run writes the whole run's
        // results rather than the part after the checkpoint (§12.3).
        cp::put_u(w, snapshots.len() as u64).map_err(io)?;
        for snap in snapshots {
            snap.checkpoint_put(w).map_err(io)?;
        }
        cp::put_u(w, notices.len() as u64).map_err(io)?;
        for notice in notices {
            cp::put_f(w, notice.t).map_err(io)?;
            cp::put_u(w, notice.message.len() as u64).map_err(io)?;
            w.write_all(notice.message.as_bytes()).map_err(io)?;
        }
        // §12.3: the interface-file records collected so far. The rainfall
        // cache is not among them: it is fixed at load from the records the
        // caller supplied, so a restored session already has it.
        cp::put_u(w, rdii_out.as_ref().map_or(0, Vec::len) as u64).map_err(io)?;
        for (at, flows) in rdii_out.iter().flatten() {
            cp::put_f(w, *at).map_err(io)?;
            cp::put_fs(w, flows).map_err(io)?;
        }
        cp::put_u(w, runoff_out.as_ref().map_or(0, Vec::len) as u64).map_err(io)?;
        for (dt, rows) in runoff_out.iter().flatten() {
            cp::put_f(w, *dt).map_err(io)?;
            cp::put_u(w, rows.len() as u64).map_err(io)?;
            for rec in rows {
                for v in [
                    rec.rainfall,
                    rec.snow_depth,
                    rec.evap,
                    rec.infil,
                    rec.runoff,
                    rec.gw_flow,
                    rec.gw_elev,
                    rec.soil_moisture,
                ] {
                    cp::put_f(w, v).map_err(io)?;
                }
                cp::put_fs(w, &rec.washoff).map_err(io)?;
            }
        }
        // §12.3: which interface files this run was given, and how far it
        // has read the one that is read sequentially. The files themselves
        // are the caller's and are not copied here.
        cp::put_u(w, supplied.len() as u64).map_err(io)?;
        for (role, hash) in supplied {
            cp::put_u(w, role.len() as u64).map_err(io)?;
            w.write_all(role.as_bytes()).map_err(io)?;
            cp::put_u(w, *hash).map_err(io)?;
        }
        cp::put_u(w, runoff_in.as_ref().map_or(0, |(_, at)| *at as u64)).map_err(io)?;
        cp::put_u(w, runoff_now.len() as u64).map_err(io)?;
        for rec in runoff_now {
            for v in [
                rec.rainfall,
                rec.snow_depth,
                rec.evap,
                rec.infil,
                rec.runoff,
                rec.gw_flow,
                rec.gw_elev,
                rec.soil_moisture,
            ] {
                cp::put_f(w, v).map_err(io)?;
            }
            cp::put_fs(w, &rec.washoff).map_err(io)?;
        }
        // Controls, sewer inflow and street inlets (§12.3).
        cp::put_b(w, controls.is_some()).map_err(io)?;
        if let Some(controls) = &controls {
            controls.checkpoint_put(w).map_err(io)?;
        }
        cp::put_u(w, rdii.len() as u64).map_err(io)?;
        for state in rdii {
            state.checkpoint_put(w).map_err(io)?;
        }
        cp::put_b(w, inlets.is_some()).map_err(io)?;
        if let Some(inlets) = &inlets {
            inlets.checkpoint_put(w).map_err(io)?;
        }
        // Constituent state, on the surface and in the network (§12.3).
        cp::put_b(w, surface_quality.is_some()).map_err(io)?;
        if let Some(sq) = &surface_quality {
            sq.checkpoint_put(w).map_err(io)?;
        }
        cp::put_b(w, quality.is_some()).map_err(io)?;
        if let Some(q) = &quality {
            q.checkpoint_put(w).map_err(io)?;
        }
        // The surface compartment and the aquifers beneath it (§12.3).
        cp::put_b(w, surface.is_some()).map_err(io)?;
        if let Some(surface) = &surface {
            surface.checkpoint_put(w).map_err(io)?;
        }
        cp::put_u(w, aquifers.len() as u64).map_err(io)?;
        for (parcel, gw) in aquifers {
            cp::put_u(w, *parcel as u64).map_err(io)?;
            gw.checkpoint_put(w).map_err(io)?;
        }
        router.checkpoint_put(w).map_err(io)?;
        Ok(())
    }

    /// Restore a checkpoint over this session (§12.3).
    ///
    /// The session must be one this model opened: the fingerprint refuses
    /// a checkpoint from another model, or from the same model with its
    /// elements reordered, which would otherwise restore every value onto
    /// the wrong element and run.
    pub fn load_checkpoint(&mut self, bytes: &[u8]) -> Result<(), String> {
        use crate::simulation::checkpoint as cp;
        let mut r = cp::Reader::new(bytes);
        r.tag(cp::STAMP)?;
        let version = r.u32()?;
        if version != cp::VERSION {
            return Err(format!(
                "checkpoint is version {version} and this build writes version {}",
                cp::VERSION
            ));
        }
        let fingerprint = r.u()?;
        if fingerprint != self.model_fingerprint() {
            return Err(
                "this checkpoint was taken from a different model, or from this \
                 one before its elements were renamed or reordered (§12.3)"
                    .into(),
            );
        }
        for slot in [
            &mut self.hydro_t,
            &mut self.next_report,
            &mut self.next_rule_t,
            &mut self.vol_dwf,
            &mut self.vol_ext,
            &mut self.vol_wet,
            &mut self.vol_gw,
            &mut self.vol_rdii,
            &mut self.last_ext_total,
            &mut self.last_dwf_total,
        ] {
            *slot = r.f()?;
        }
        for slot in [&mut self.last_lat, &mut self.last_inlet_lat] {
            *slot = r.fs()?;
        }
        for slot in [&mut self.hydro_prev, &mut self.hydro_now] {
            slot.0 = r.f()?;
            slot.1 = r.fs()?;
        }
        for slot in [&mut self.hydro_mass_prev, &mut self.hydro_mass_now] {
            let n = r.u()? as usize;
            *slot = (0..n).map(|_| r.rows()).collect::<Result<_, _>>()?;
        }
        self.hydro_degraded_warned = r.b()?;
        self.runoff_exhausted = r.b()?;
        let n = r.u()? as usize;
        if n != self.series_warned.len() {
            return Err(format!(
                "checkpoint holds {n} series latches where this model has {}",
                self.series_warned.len()
            ));
        }
        for flag in self.series_warned.iter_mut() {
            *flag = r.b()?;
        }
        for held in [
            &mut self.stage_override,
            &mut self.setting_override,
            &mut self.limit_override,
        ] {
            let n = r.u()? as usize;
            held.clear();
            for _ in 0..n {
                let at = r.u()? as usize;
                held.insert(at, r.f()?);
            }
        }
        let n = r.u()? as usize;
        self.drain_override.clear();
        for _ in 0..n {
            let pi = r.u()? as usize;
            let unit = r.u()? as usize;
            let d = crate::hydrology::lid::DrainSetting {
                coeff: r.f()?,
                exponent: r.f()?,
                offset: r.f()?,
                delay: r.f()?,
                h_open: r.f()?,
                h_close: r.f()?,
            };
            self.drain_override.insert((pi, unit), d);
        }
        // A restored drain reaches the unit as well as the map.
        for ((pi, unit), d) in self.drain_override.clone() {
            if let Some(s) = self.surface.as_mut() {
                s.set_drain_setting(pi, unit, d);
            }
        }
        let n = r.u()? as usize;
        self.concentration_override.clear();
        for _ in 0..n {
            let v = r.u()? as usize;
            self.concentration_override.insert(v, r.fs()?);
        }
        let n = r.u()? as usize;
        self.loss_override.clear();
        for _ in 0..n {
            let li = r.u()? as usize;
            let (a, b, c) = (r.f()?, r.f()?, r.f()?);
            self.loss_override.insert(li, (a, b, c));
        }
        // A restored injection has to reach the router as well as the
        // map, or the run continues with it recorded and not applied.
        for (li, (a, b, c)) in self.loss_override.clone() {
            self.router.set_losses(li, a, b, c);
        }
        for (li, q) in self.limit_override.clone() {
            self.router.set_flow_limit(li, q);
        }
        for (v, e) in self.stage_override.clone() {
            self.router.force_outfall_stage(v, Some(e), &self.net);
        }
        for (li, v) in self.setting_override.clone() {
            self.router.set_setting(li, v);
        }
        let n = r.u()? as usize;
        self.lateral_override.clear();
        for _ in 0..n {
            let vertex = r.u()? as usize;
            self.lateral_override.insert(vertex, r.f()?);
        }
        self.climate_state.checkpoint_get(&mut r)?;
        let n = r.u()? as usize;
        self.snapshots = (0..n)
            .map(|_| Snapshot::checkpoint_get(&mut r))
            .collect::<Result<Vec<_>, _>>()?;
        let n = r.u()? as usize;
        self.notices = Vec::with_capacity(n);
        for _ in 0..n {
            let t = r.f()?;
            self.notices.push(RuntimeNotice {
                t,
                message: r.text()?,
            });
        }
        let n = r.u()? as usize;
        let mut rdii_rows = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let at = r.f()?;
            rdii_rows.push((at, r.fs()?));
        }
        if self.rdii_out.is_some() {
            self.rdii_out = Some(rdii_rows);
        } else if n > 0 {
            return Err("checkpoint collected an RDII file this run does not write".into());
        }
        let n = r.u()? as usize;
        let mut runoff_rows = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let dt = r.f()?;
            let rows = r.u()? as usize;
            let mut recs = Vec::with_capacity(rows.min(4096));
            for _ in 0..rows {
                recs.push(crate::io::iface::ParcelReplay {
                    rainfall: r.f()?,
                    snow_depth: r.f()?,
                    evap: r.f()?,
                    infil: r.f()?,
                    runoff: r.f()?,
                    gw_flow: r.f()?,
                    gw_elev: r.f()?,
                    soil_moisture: r.f()?,
                    washoff: r.fs()?,
                });
            }
            runoff_rows.push((dt, recs));
        }
        if self.runoff_out.is_some() {
            self.runoff_out = Some(runoff_rows);
        } else if n > 0 {
            return Err("checkpoint collected a runoff file this run does not write".into());
        }
        let n = r.u()? as usize;
        let mut want: Vec<(String, u64)> = Vec::with_capacity(n.min(8));
        for _ in 0..n {
            let role = r.text()?;
            want.push((role, r.u()?));
        }
        let have: Vec<(String, u64)> = self
            .supplied
            .iter()
            .map(|(role, hash)| ((*role).to_string(), *hash))
            .collect();
        if want != have {
            let names = |v: &[(String, u64)]| {
                if v.is_empty() {
                    "none".to_string()
                } else {
                    v.iter()
                        .map(|(r, _)| r.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            };
            return Err(format!(
                "this checkpoint was taken while reading {} and this run was \
                 given {}; the same files must be supplied before restoring \
                 (§12.3)",
                names(&want),
                names(&have)
            ));
        }
        let at = r.u()? as usize;
        if let Some((_, cursor)) = &mut self.runoff_in {
            *cursor = at;
        }
        let n = r.u()? as usize;
        self.runoff_now = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let rec = crate::io::iface::ParcelReplay {
                rainfall: r.f()?,
                snow_depth: r.f()?,
                evap: r.f()?,
                infil: r.f()?,
                runoff: r.f()?,
                gw_flow: r.f()?,
                gw_elev: r.f()?,
                soil_moisture: r.f()?,
                washoff: r.fs()?,
            };
            self.runoff_now.push(rec);
        }
        if r.b()? {
            match &mut self.controls {
                Some(controls) => controls.checkpoint_get(&mut r)?,
                None => return Err("checkpoint holds control state this model has not".into()),
            }
        } else if self.controls.is_some() {
            return Err("this model has controls the checkpoint has not".into());
        }
        let n = r.u()? as usize;
        if n != self.rdii.len() {
            return Err(format!(
                "checkpoint holds {n} sewer inflow assignments where this model \
                 has {}",
                self.rdii.len()
            ));
        }
        for state in &mut self.rdii {
            state.checkpoint_get(&mut r)?;
        }
        if r.b()? {
            match &mut self.inlets {
                Some(inlets) => inlets.checkpoint_get(&mut r)?,
                None => return Err("checkpoint holds street inlets this model has not".into()),
            }
        } else if self.inlets.is_some() {
            return Err("this model has street inlets the checkpoint has not".into());
        }
        if r.b()? {
            match &mut self.surface_quality {
                Some(sq) => sq.checkpoint_get(&mut r)?,
                None => return Err("checkpoint holds surface loading this model has not".into()),
            }
        } else if self.surface_quality.is_some() {
            return Err("this model tracks surface loading the checkpoint does not".into());
        }
        if r.b()? {
            match &mut self.quality {
                Some(q) => q.checkpoint_get(&mut r)?,
                None => return Err("checkpoint holds network quality this model has not".into()),
            }
        } else if self.quality.is_some() {
            return Err("this model tracks network quality the checkpoint does not".into());
        }
        if r.b()? {
            match &mut self.surface {
                Some(surface) => surface.checkpoint_get(&mut r)?,
                None => return Err("checkpoint holds a surface this model has not".into()),
            }
        } else if self.surface.is_some() {
            return Err("this model has a surface the checkpoint has not".into());
        }
        let n = r.u()? as usize;
        if n != self.aquifers.len() {
            return Err(format!(
                "checkpoint holds {n} aquifers where this model has {}",
                self.aquifers.len()
            ));
        }
        for (parcel, gw) in &mut self.aquifers {
            let at = r.u()? as usize;
            if at != *parcel {
                return Err("checkpoint puts an aquifer under a different parcel".into());
            }
            gw.checkpoint_get(&mut r)?;
        }
        self.router.checkpoint_get(&mut r)?;
        // Bytes left over mean a layout this build does not share, which
        // the version alone did not catch.
        if !r.at_end() {
            return Err("checkpoint holds more than this build reads".into());
        }
        Ok(())
    }

    /// Record that an interface file was supplied, by what it is and a
    /// hash of its bytes (§12.3).
    fn note_supplied(&mut self, role: &'static str, bytes: &[u8]) {
        let mut h = crate::simulation::checkpoint::Fnv::new();
        h.write(bytes);
        let hash = h.finish();
        match self.supplied.iter_mut().find(|(r, _)| *r == role) {
            // Supplying twice replaces, as the field it fills does.
            Some(slot) => slot.1 = hash,
            None => self.supplied.push((role, hash)),
        }
        self.supplied.sort_by_key(|(r, _)| *r);
    }

    /// A hash of the model's shape and every element identifier, in model
    /// order (§12.3).
    fn model_fingerprint(&self) -> u64 {
        let mut h = crate::simulation::checkpoint::Fnv::new();
        for n in [
            self.net.parcels.len(),
            self.net.vertices.len(),
            self.net.links.len(),
            self.net.constituents.len(),
            self.net.land_uses.len(),
        ] {
            h.write(&(n as u64).to_le_bytes());
        }
        // Order matters as much as membership: the predecessor compares
        // counts alone and says so, and a reordered model then restores
        // every value onto the wrong element.
        for v in &self.net.vertices {
            h.write(v.id.as_bytes());
        }
        for l in &self.net.links {
            h.write(l.id.as_bytes());
        }
        for p in &self.net.parcels {
            h.write(p.id.as_bytes());
        }
        for c in &self.net.constituents {
            h.write(c.id.as_bytes());
        }
        h.finish()
    }

    /// Write the rainfall interface file for this model's gages, if it
    /// asked for one (§14.8.3). `false` when it did not.
    ///
    /// The file caches the records rather than anything the run computed,
    /// so it is complete from the moment the model opens.
    pub fn write_rain(&self, w: &mut impl std::io::Write) -> std::io::Result<bool> {
        let Some(gages) = &self.rain_out else {
            return Ok(false);
        };
        crate::io::iface::write_rain_iface(gages, w)?;
        Ok(true)
    }

    /// Write the runoff interface file this run recorded, if it recorded
    /// one (§14.8.2). `false` when the model asked for no such file.
    pub fn write_runoff(&self, w: &mut impl std::io::Write) -> std::io::Result<bool> {
        let Some(rows) = &self.runoff_out else {
            return Ok(false);
        };
        crate::io::iface::write_runoff_file(&self.net, rows, w)?;
        Ok(true)
    }

    /// Write the RDII interface file (§14.8.1) in the text form, if the
    /// model asked for one. `false` when it did not.
    ///
    /// The declared step is the longer of the two hydrology steps, which
    /// bounds every gap between the records, so the written hydrograph
    /// leaves no instant of the run uncovered.
    pub fn write_rdii(&self, w: &mut impl std::io::Write) -> std::io::Result<bool> {
        let Some(rows) = &self.rdii_out else {
            return Ok(false);
        };
        let vertices: Vec<usize> = self.rdii.iter().map(|r| r.vertex).collect();
        let step = self.net.options.wet_step.max(self.net.options.dry_step);
        crate::io::iface::write_rdii_file(&self.net, &vertices, step, rows, w)?;
        Ok(true)
    }

    /// Write the routing interface outflow file (§14.8): outlet
    /// vertices' inflows and concentrations per reporting period.
    pub fn write_routing_outflows(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        crate::io::iface::write_routing_file(
            &self.net,
            &self.snapshots,
            self.start_epoch,
            self.report_step,
            w,
        )
    }

    /// Write the §14.9 text report to `w`, drawing on the §11 ledgers,
    /// the control-action log, and the routing performance counters.
    pub fn write_report(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        let led = self.ledgers();
        let surface = self.surface.as_ref().map(|s| {
            let err = led.surface.map_or(0.0, |l| l.error_percent);
            [
                s.rainfall,
                s.runon_in,
                s.evap_vol,
                s.infil_vol,
                s.runoff_out,
                s.snow_plowed,
                s.initial_storage,
                s.stored_volume(),
                err,
            ]
        });
        let subsurface = if self.aquifers.is_empty() {
            None
        } else {
            let (mut infil, mut evap, mut perc, mut lat, mut init, mut fin) =
                (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
            for (_, gw) in &self.aquifers {
                infil += gw.infil_in;
                evap += gw.evap_out;
                perc += gw.perc_out;
                lat += gw.lateral_out;
                init += gw.initial_storage;
                fin += gw.stored_volume();
            }
            let err = led.subsurface.map_or(0.0, |l| l.error_percent);
            Some([infil, evap, perc, lat, init, fin, err])
        };
        let r = &self.router.report;
        let stored_now: f64 = (0..self.net.vertices.len())
            .map(|v| self.router.vertex_volume_now(v))
            .sum::<f64>()
            + self
                .router
                .channel_transport()
                .iter()
                .map(|c| c.4)
                .sum::<f64>();
        let flow = [
            self.vol_dwf,
            self.vol_wet,
            self.vol_gw,
            self.vol_rdii,
            self.vol_ext,
            r.outflow + r.negative_out,
            r.flooding,
            r.evaporation,
            r.losses - r.evaporation,
            r.initial_storage,
            stored_now,
            led.network.error_percent,
        ];
        let mut quality = Vec::new();
        if let Some(q) = &self.quality {
            for (p, (id, l)) in self
                .net
                .constituents
                .iter()
                .map(|c| c.id.clone())
                .zip(led.constituents.iter().map(|(_, l)| *l))
                .enumerate()
            {
                let by = q.inflow_by_source[p];
                quality.push((
                    id,
                    [
                        by[0],
                        by[1],
                        by[2],
                        by[3],
                        by[4],
                        q.outfall_mass[p],
                        q.flooded_mass[p],
                        q.seepage_mass[p],
                        q.reacted[p],
                        q.initial_mass[p],
                        q.final_storage[p] + q.stored_mass(p),
                        l.error_percent,
                    ],
                ));
            }
        }
        // The §11.1 surface-loading ledger, in the report's row order.
        let mut loading = Vec::new();
        if let Some(sq) = &self.surface_quality {
            for (p, (id, l)) in self
                .net
                .constituents
                .iter()
                .map(|c| c.id.clone())
                .zip(led.loading.iter().map(|(_, l)| *l))
                .enumerate()
            {
                loading.push((
                    id,
                    [
                        sq.initial_buildup[p],
                        sq.buildup_in[p],
                        sq.deposition[p],
                        sq.swept[p],
                        sq.infiltrated[p],
                        sq.bmp_removed[p],
                        sq.washed_off[p],
                        sq.to_final[p] + sq.stored_mass(p),
                        l.error_percent,
                    ],
                ));
            }
        }
        // §11.2 top-five governing vertices.
        let mut worst: Vec<(String, u64)> = self
            .router
            .worst_counts
            .iter()
            .enumerate()
            .filter(|(_, &n)| n > 0)
            .map(|(vi, &n)| (self.net.vertices[vi].id.clone(), n))
            .collect();
        worst.sort_by_key(|x| std::cmp::Reverse(x.1));
        worst.truncate(5);
        let parcel_totals = match &self.surface {
            Some(sf) => (0..self.net.parcels.len())
                .map(|pi| sf.parcel_totals(pi))
                .collect(),
            None => Vec::new(),
        };
        crate::io::rpt_writer::write_rpt(
            &crate::io::rpt_writer::ReportInputs {
                net: &self.net,
                surface,
                subsurface,
                flow,
                quality,
                loading,
                actions: self.control_actions(),
                performance: &self.router.report,
                vertex_stats: &self.router.vertex_stats,
                link_stats: &self.router.link_stats,
                parcel_totals,
                washoff_by_parcel: self
                    .surface_quality
                    .as_ref()
                    .map(|sq| sq.washed_by_parcel.clone()),
                outfall_loads: self.quality.as_ref().map(|q| q.outfall_load.clone()),
                link_loads: self.quality.as_ref().map(|q| q.link_load.clone()),
                worst,
            },
            w,
        )
    }

    /// Stream the §14.9 results to `sink` as the run produces them.
    ///
    /// The alternative is [`Self::write_out`], which writes the whole run
    /// at the end from the gathered instants. Streaming exists because
    /// that set is the largest thing a long run owns.
    ///
    /// Attach before stepping: the header records where the first
    /// reporting instant falls, so instants already produced would be
    /// missing from a file opened late.
    pub fn begin_results(
        &mut self,
        sink: Box<dyn std::io::Write + Send>,
        may_checkpoint: bool,
    ) -> std::io::Result<()> {
        // The header backdates from the run's *first* instant, which on a
        // run resumed from a checkpoint (§12.3) is one the checkpoint
        // restored rather than the next one falling due. Those restored
        // instants are written straight away, so a resumed run still
        // writes the whole run's results and not merely the tail.
        let first = self.snapshots.first().map_or(self.next_report, |s| s.t);
        let mut out = crate::io::out_writer::OutStream::begin(
            sink,
            &self.net,
            self.start_epoch,
            self.report_step,
            first,
        )?;
        for snap in &self.snapshots {
            out.append(snap)?;
        }
        self.out = Some(out);
        // A routing outflow file is written from the same instants once
        // the run ends (§14.8), so a model that saves one keeps them
        // whatever the caller says about checkpoints.
        let saves_outflows = self.net.interface_files.outflows.is_some();
        self.retain_snapshots = may_checkpoint || saves_outflows;
        if !self.retain_snapshots {
            self.snapshots = Vec::new();
        }
        Ok(())
    }

    /// Close a streamed results file, reporting the first write failure
    /// the run met if there was one. A no-op without one attached.
    pub fn finish_results(&mut self) -> std::io::Result<()> {
        if let Some(e) = self.out_error.take() {
            self.out = None;
            return Err(e);
        }
        match self.out.take() {
            Some(out) => out.finish().map(|_| ()),
            None => Ok(()),
        }
    }

    /// Write the §14.9 binary results to `w`; the caller owns where the
    /// bytes go (§12.2).
    pub fn write_out(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        crate::io::out_writer::write_out(
            &self.net,
            &self.snapshots,
            self.start_epoch,
            self.report_step,
            w,
        )
    }

    /// The §9.1 control-action log: (time s, link, setting, rule).
    pub fn control_actions(&self) -> &[(f64, String, f64, String)] {
        self.controls.as_ref().map_or(&[], |c| &c.log)
    }

    /// Constituent index by identity.
    fn constituent_index(&self, pollutant: &str) -> Option<usize> {
        self.net.constituents.iter().position(|c| c.id == pollutant)
    }

    /// Concentration of `pollutant` at vertex `id`, in its declared unit
    /// (§8.4, §12.2).
    pub fn node_concentration(&self, id: &str, pollutant: &str) -> Option<f64> {
        let q = self.quality.as_ref()?;
        let p = self.constituent_index(pollutant)?;
        let v = *self.vertex_by_id.get(id)?;
        Some(q.c_vertex[p][v])
    }

    /// Concentration of `pollutant` in link `id` (§8.4, §12.2).
    pub fn link_concentration(&self, id: &str, pollutant: &str) -> Option<f64> {
        let q = self.quality.as_ref()?;
        let p = self.constituent_index(pollutant)?;
        let l = *self.link_by_id.get(id)?;
        q.link_concentration(&self.router, p, l)
    }

    /// The §8 mass ledger for `pollutant`: (admitted, discharged,
    /// reacted, final storage), each in the declared unit times m³.
    pub fn quality_ledger(&self, pollutant: &str) -> Option<(f64, f64, f64, f64)> {
        let q = self.quality.as_ref()?;
        let p = self.constituent_index(pollutant)?;
        Some((
            q.inflow_mass[p],
            q.outfall_mass[p],
            q.reacted[p],
            q.final_storage[p],
        ))
    }

    /// The §11.2 origin split of `pollutant`'s admitted load, indexed by
    /// [`crate::transport::MassSource`]. Sums to `quality_ledger`'s admitted
    /// term.
    pub fn quality_inflow_by_source(
        &self,
        pollutant: &str,
    ) -> Option<[f64; crate::transport::quality::MassSource::COUNT]> {
        let q = self.quality.as_ref()?;
        Some(q.inflow_by_source[self.constituent_index(pollutant)?])
    }

    /// A series value at run time `t` under the §10.1 extension contract.
    /// `hold_ends` holds the first/last value outside the range (stages);
    /// otherwise the value falls to zero, with a one-time warning when the
    /// series ends before the run does.
    fn series_value(&mut self, si: usize, t: f64, hold_ends: bool) -> f64 {
        let TimeSeriesSource::Points(points) = &self.net.timeseries[si].source else {
            // External-file series need the caller to supply bytes; the
            // model carries only the name (§2.9). Absent data reads as an
            // empty series.
            return 0.0;
        };
        if points.is_empty() {
            return 0.0;
        }
        let epoch = self.start_epoch + t;
        let at = |st: &SeriesTime| -> f64 {
            match st {
                SeriesTime::Elapsed(s) => *s,
                SeriesTime::Absolute { date, seconds } => {
                    days_from_civil(*date) as f64 * 86_400.0 + seconds - self.start_epoch
                }
            }
        };
        let x = match points[0].time {
            SeriesTime::Elapsed(_) => t,
            SeriesTime::Absolute { .. } => epoch - self.start_epoch,
        };
        let first = at(&points[0].time);
        let last = at(&points[points.len() - 1].time);
        if x < first {
            return if hold_ends { points[0].value } else { 0.0 };
        }
        if x > last {
            if hold_ends {
                return points[points.len() - 1].value;
            }
            if !self.series_warned[si] {
                self.series_warned[si] = true;
                self.notices.push(RuntimeNotice {
                    t,
                    message: format!(
                        "series {} exhausted before the run ends; its consumers read zero",
                        self.net.timeseries[si].id
                    ),
                });
            }
            return 0.0;
        }
        // Linear interpolation between bracketing points.
        for w in points.windows(2) {
            let (t0, t1) = (at(&w[0].time), at(&w[1].time));
            if x <= t1 {
                if t1 <= t0 {
                    return w[1].value;
                }
                let f = (x - t0) / (t1 - t0);
                return w[0].value + f * (w[1].value - w[0].value);
            }
        }
        points[points.len() - 1].value
    }

    /// Assemble the lateral inflow vector at a period start (§10.1):
    /// external inflows and sanitary base flows, evaluated at the
    /// step-start date, near-zero values truncated; §12.4 overrides win.
    /// The mass planes come back in [`BASE_SOURCES`] order — sanitary
    /// first, external second — so the origin split of §11.2 is fixed
    /// where the load is assembled rather than inferred later.
    fn assemble_lateral(&mut self, t: f64) -> (Vec<f64>, Vec<Vec<Vec<f64>>>) {
        let nv = self.net.vertices.len();
        let np = self.net.constituents.len();
        let mut lat = vec![0.0; nv];
        let mut ext_flow = vec![0.0; nv];
        let mut dwf_flow = vec![0.0; nv];
        let mut dwf_mass = vec![vec![0.0; nv]; np];
        let mut ext_mass = vec![vec![0.0; nv]; np];

        for i in 0..self.net.inflows.len() {
            let inflow = &self.net.inflows[i];
            if inflow.kind != InflowKind::Flow {
                continue;
            }
            let (vertex, series, scale, baseline, base_pattern) = (
                inflow.vertex,
                inflow.series,
                inflow.scale,
                inflow.baseline,
                inflow.base_pattern,
            );
            let mut q = baseline * self.pattern_factor(base_pattern, t);
            if let Some(si) = series {
                q += self.series_value(si, t, false) * scale;
            }
            lat[vertex] += q;
            ext_flow[vertex] += q;
        }
        self.last_ext_total = ext_flow.iter().sum();

        for d in 0..self.net.dry_weather.len() {
            let dwf = &self.net.dry_weather[d];
            if dwf.constituent.is_some() {
                continue;
            }
            let (vertex, average, patterns) = (dwf.vertex, dwf.average, dwf.patterns);
            let q = average * self.dwf_pattern_factor(patterns, t);
            lat[vertex] += q;
            dwf_flow[vertex] += q;
        }
        self.last_dwf_total = dwf_flow.iter().sum();

        // §8.1 mass sources riding those flows: constituent inflows as a
        // concentration on the external flow or a flow-free mass rate,
        // sanitary flow at its global and per-vertex concentrations.
        if np > 0 {
            for i in 0..self.net.inflows.len() {
                let inflow = &self.net.inflows[i];
                let Some(ci) = inflow.constituent else {
                    continue;
                };
                let (vertex, series, scale, baseline, base_pattern, kind, units_factor) = (
                    inflow.vertex,
                    inflow.series,
                    inflow.scale,
                    inflow.baseline,
                    inflow.base_pattern,
                    inflow.kind,
                    inflow.units_factor,
                );
                let mut v = baseline * self.pattern_factor(base_pattern, t);
                if let Some(si) = series {
                    v += self.series_value(si, t, false) * scale;
                }
                match kind {
                    InflowKind::Concentration => {
                        ext_mass[ci][vertex] += v * ext_flow[vertex].max(0.0);
                    }
                    InflowKind::Mass => {
                        ext_mass[ci][vertex] += v * units_factor;
                    }
                    InflowKind::Flow => {}
                }
            }
            for d in 0..self.net.dry_weather.len() {
                let dwf = &self.net.dry_weather[d];
                let Some(ci) = dwf.constituent else {
                    continue;
                };
                let (vertex, average, patterns) = (dwf.vertex, dwf.average, dwf.patterns);
                let c = average * self.dwf_pattern_factor(patterns, t);
                dwf_mass[ci][vertex] += c * dwf_flow[vertex];
            }
            for (ci, c) in self.net.constituents.iter().enumerate() {
                if c.c_dwf != 0.0 {
                    for v in 0..nv {
                        dwf_mass[ci][v] += c.c_dwf * dwf_flow[v];
                    }
                }
            }
        }

        // §14.8 routing interface inflows, interpolated between the
        // file's bracketing periods, carrying their concentrations.
        if let Some(ifc) = &self.iface_in {
            for (vi, q, conc) in ifc.inflows_at(self.start_epoch + t, np) {
                lat[vi] += q;
                for (p, c) in conc.iter().enumerate() {
                    ext_mass[p][vi] += q.max(0.0) * c;
                }
            }
        }
        for (&v, &q) in &self.lateral_override {
            lat[v] = q;
            // §12.4: an override replaces the vertex's whole lateral, so
            // it replaces the mass arriving with it too. An injection
            // given no concentration carries none, which is what an
            // inflow of clean water is.
            for (ci, row) in ext_mass.iter_mut().enumerate() {
                let c = self
                    .concentration_override
                    .get(&v)
                    .and_then(|cs| cs.get(ci).copied())
                    .unwrap_or(0.0);
                row[v] = q.max(0.0) * c;
            }
            for row in dwf_mass.iter_mut() {
                row[v] = 0.0;
            }
        }
        for l in &mut lat {
            if l.abs() < FLOW_TOL {
                *l = 0.0;
            }
        }
        (lat, vec![dwf_mass, ext_mass])
    }

    /// Update tidal and series outfall stages for the period (§2.6):
    /// tides indexed by clock time (§14.7), series holding their ends.
    fn update_boundary_stages(&mut self, t: f64) {
        for vi in 0..self.net.vertices.len() {
            let VertexKind::Outfall { stage, .. } = &self.net.vertices[vi].kind else {
                continue;
            };
            match stage {
                OutfallStage::Tidal { curve } => {
                    let (_, _, _, secs) = self.calendar(t);
                    let elev = tidal_stage(&self.net.curves[*curve].points, secs);
                    self.router.set_outfall_stage(vi, elev);
                }
                OutfallStage::Series { series } => {
                    let elev = self.series_value(*series, t, true);
                    self.router.set_outfall_stage(vi, elev);
                }
                _ => {}
            }
        }
    }
}

/// A tidal curve's stage at a clock time (s past midnight): linear in the
/// curve — whose abscissae import converted to seconds — wrapping over
/// the day.
/// Realise file-sourced gages as the equivalent series (§3.1): each
/// supplied record's station readings, unit-converted, become an appended
/// series the gage then points at, so everything downstream treats the
/// gage exactly as if the series had been written in the model. A gage
/// naming a file the caller did not supply refuses the load.
///
/// A supplied name matches the gage's declaration as written, or failing
/// that by trailing file name — models carry paths from the machine they
/// were authored on, and the caller supplies the file it actually found.
fn realise_file_gages(
    net: &mut Network,
    rain_files: &[(String, crate::io::rain::RainRecords)],
    rain_interface: Option<&crate::io::iface::RainInterface>,
) -> Result<Vec<crate::io::iface::RainGageRecord>, crate::hydrology::runoff::SurfaceRefusal> {
    use crate::hydrology::runoff::SurfaceRefusal;
    use crate::model::{GageSource, RainFileUnit};

    let mut cached = Vec::new();

    let basename = |name: &str| {
        name.rsplit(['/', '\\'])
            .next()
            .unwrap_or(name)
            .to_ascii_lowercase()
    };
    for gi in 0..net.gages.len() {
        let GageSource::File {
            ref file,
            ref station,
            unit,
        } = net.gages[gi].source
        else {
            continue;
        };
        // §14.8.3: an interface file caches records already parsed and
        // normalised, so it stands in for the record this gage names.
        if let Some(iface) = rain_interface {
            let Some(rec) = iface.station(station) else {
                return Err(SurfaceRefusal::Incomplete(format!(
                    "gage {}: the rainfall interface file holds no station \
                     {station:?}. Check the station name, or supply the file \
                     that has it",
                    net.gages[gi].id
                )));
            };
            if rec.readings.is_empty() {
                return Err(SurfaceRefusal::Incomplete(format!(
                    "gage {}: the rainfall interface file holds station \
                     {station:?} with no readings at all",
                    net.gages[gi].id
                )));
            }
            // Readings on the file are interval depths in inches whatever
            // the gage declared, so the gage reads as a volume record at
            // the file's own interval (§14.8.3).
            let to_model = if net.options.flow_units.is_us() {
                1.0
            } else {
                25.4
            };
            let points: Vec<crate::model::TimeSeriesPoint> = rec
                .readings
                .iter()
                .map(|(day, depth)| {
                    let (date, seconds) = civil_from_decimal_day(*day);
                    crate::model::TimeSeriesPoint {
                        time: crate::model::SeriesTime::Absolute { date, seconds },
                        value: depth * to_model,
                    }
                })
                .collect();
            let series = net.timeseries.len();
            net.timeseries.push(crate::model::TimeSeries {
                id: format!("[rain interface {station}]"),
                source: crate::model::TimeSeriesSource::Points(points),
            });
            net.gages[gi].form = crate::model::RainForm::Volume;
            if rec.interval > 0.0 {
                net.gages[gi].interval = rec.interval;
            }
            net.gages[gi].source = GageSource::Series { series };
            cached.push(rec.clone());
            continue;
        }
        let supplied = rain_files
            .iter()
            .find(|(name, _)| name == file)
            .or_else(|| {
                rain_files
                    .iter()
                    .find(|(name, _)| basename(name) == basename(file))
            });
        let Some((_, supplied)) = supplied else {
            return Err(SurfaceRefusal::Incomplete(format!(
                "gage {}: external rain record {file:?} was not supplied. \
                 Provide the file, or inline the record as a [TIMESERIES] section",
                net.gages[gi].id
            )));
        };
        // §14.12.1: an archival record arrives already normalised to
        // depths in inches over the interval its own header declares, so
        // it is realised exactly as a cache of one is, and the gage's
        // declared form describes its record rather than the file.
        let readings = match supplied {
            crate::io::rain::RainRecords::Station(readings) => readings,
            crate::io::rain::RainRecords::Archive(rec) => {
                let to_model = if net.options.flow_units.is_us() {
                    1.0
                } else {
                    25.4
                };
                let points: Vec<crate::model::TimeSeriesPoint> = rec
                    .readings
                    .iter()
                    .map(|(day, depth)| {
                        let (date, seconds) = civil_from_decimal_day(*day);
                        crate::model::TimeSeriesPoint {
                            time: crate::model::SeriesTime::Absolute { date, seconds },
                            value: depth * to_model,
                        }
                    })
                    .collect();
                if points.is_empty() {
                    return Err(SurfaceRefusal::Incomplete(format!(
                        "gage {}: the archival record {file:?} holds no rainfall",
                        net.gages[gi].id
                    )));
                }
                let series = net.timeseries.len();
                net.timeseries.push(crate::model::TimeSeries {
                    id: format!("[archival record {file}]"),
                    source: crate::model::TimeSeriesSource::Points(points),
                });
                net.gages[gi].form = crate::model::RainForm::Volume;
                if rec.interval > 0.0 {
                    net.gages[gi].interval = rec.interval;
                }
                net.gages[gi].source = GageSource::Series { series };
                cached.push(rec.clone());
                continue;
            }
        };
        // The record's declared depth unit, converted to the model's; an
        // undeclared unit already reads in the model's (§14.12).
        let scale = match (unit, net.options.flow_units.is_us()) {
            (Some(RainFileUnit::Inches), false) => 25.4,
            (Some(RainFileUnit::Millimetres), true) => 1.0 / 25.4,
            _ => 1.0,
        };
        // Station ids compare without case, as every other id in this
        // engine does — and as the predecessor's own `strcomp` does. A
        // record written `STA7` for a gage declaring `sta7` is the same
        // station, not a dry run.
        let mut points: Vec<crate::model::TimeSeriesPoint> = readings
            .iter()
            .filter(|r| r.station.eq_ignore_ascii_case(station))
            .map(|r| crate::model::TimeSeriesPoint {
                time: crate::model::SeriesTime::Absolute {
                    date: r.date,
                    seconds: r.seconds,
                },
                value: r.value * scale,
            })
            .collect();
        // A supplied record holding nothing for this station is a missing
        // input wearing a supplied file's clothes: the wrong file, a
        // mistyped station, or a `FILE` line that named none at all.
        // Silence here is the dry model the deferral existed to prevent.
        if points.is_empty() {
            return Err(SurfaceRefusal::Incomplete(if station.is_empty() {
                format!(
                    "gage {}: its FILE line names no station, so no reading in \
                     {file:?} can belong to it. Name the station after the file",
                    net.gages[gi].id
                )
            } else {
                format!(
                    "gage {}: {file:?} holds no readings for station {station:?}. \
                     Check the station name, or supply the record that has it",
                    net.gages[gi].id
                )
            }));
        }
        points.sort_by(|a, b| {
            let key = |p: &crate::model::TimeSeriesPoint| match &p.time {
                crate::model::SeriesTime::Absolute { date, seconds } => {
                    days_from_civil(*date) as f64 * 86_400.0 + seconds
                }
                crate::model::SeriesTime::Elapsed(s) => *s,
            };
            key(a).total_cmp(&key(b))
        });
        let series = net.timeseries.len();
        net.timeseries.push(crate::model::TimeSeries {
            id: format!("[rain record {file}:{station}]"),
            source: crate::model::TimeSeriesSource::Points(points),
        });
        cached.push(cacheable_record(
            station,
            net.gages[gi].interval,
            net.gages[gi].form,
            unit,
            net.options.flow_units.is_us(),
            readings,
        ));
        net.gages[gi].source = GageSource::Series { series };
    }
    Ok(cached)
}

/// One gage's record as a rainfall interface file holds it (§14.8.3):
/// interval depths in inches, whatever form and unit the record declared.
///
/// This is the normalisation the predecessor performs while building the
/// file, and it is why a gage reading one back reads volumes: the form a
/// gage declares describes its record, not the cache.
fn cacheable_record(
    station: &str,
    interval: f64,
    form: crate::model::RainForm,
    unit: Option<crate::model::RainFileUnit>,
    model_is_us: bool,
    readings: &[crate::io::rain::RainReading],
) -> crate::io::iface::RainGageRecord {
    use crate::model::{RainFileUnit, RainForm};
    // The record's own declared unit, not the model's: an undeclared unit
    // is the model unit system's depth unit (§14.12).
    let to_inches = match unit {
        Some(RainFileUnit::Inches) => 1.0,
        Some(RainFileUnit::Millimetres) => 1.0 / 25.4,
        None if model_is_us => 1.0,
        None => 1.0 / 25.4,
    };
    let mut accum = 0.0;
    let mut out = Vec::new();
    for r in readings
        .iter()
        .filter(|r| r.station.eq_ignore_ascii_case(station))
    {
        let depth = match form {
            RainForm::Volume => r.value,
            RainForm::Intensity => r.value * interval / 3600.0,
            RainForm::Cumulative => {
                // A total that falls is a record that restarted, and the
                // reading is then the increment itself.
                if r.value >= accum {
                    let d = r.value - accum;
                    accum += d;
                    d
                } else {
                    accum = r.value;
                    r.value
                }
            }
        };
        let day = SWMM_EPOCH_DAYS_ENGINE + days_from_civil(r.date) as f64 + r.seconds / 86_400.0;
        out.push((day, depth * to_inches));
    }
    crate::io::iface::RainGageRecord {
        station: station.to_string(),
        interval,
        readings: out,
    }
}

/// A decimal day as a calendar date and seconds past its midnight.
///
/// The encoding cannot hold whole seconds exactly — a quarter hour reads
/// back as 899.9999999 s — so the second is rounded to the nearest
/// millisecond, far above the encoding's noise and far below any interval
/// a record uses.
fn civil_from_decimal_day(day: f64) -> (crate::io::options::Date, f64) {
    let since_epoch = day - SWMM_EPOCH_DAYS_ENGINE;
    let days = since_epoch.floor();
    let seconds = ((since_epoch - days) * 86_400.0 * 1000.0).round() / 1000.0;
    // A second rounded up to the day's end belongs to the next day.
    if seconds >= 86_400.0 {
        return (civil_from_days(days as i64 + 1), 0.0);
    }
    (civil_from_days(days as i64), seconds)
}

/// Days between the predecessor's date origin and the Unix epoch, as
/// `io::iface` states it.
const SWMM_EPOCH_DAYS_ENGINE: f64 = 25_569.0;

fn tidal_stage(points: &[(f64, f64)], secs: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    if points.len() == 1 {
        return points[0].1;
    }
    let secs = secs.rem_euclid(86_400.0);
    for w in points.windows(2) {
        if secs >= w[0].0 && secs <= w[1].0 {
            let f = (secs - w[0].0) / (w[1].0 - w[0].0).max(1e-9);
            return w[0].1 + f * (w[1].1 - w[0].1);
        }
    }
    // Wrap: between the last point and the first, one day on.
    let (t0, v0) = points[points.len() - 1];
    let (t1, v1) = (points[0].0 + 86_400.0, points[0].1);
    if secs >= t0 && t1 > t0 {
        let f = (secs - t0) / (t1 - t0);
        return v0 + f * (v1 - v0);
    }
    points[0].1
}

/// The internal-unit scale (metres per foot-based slot) for each §14.8
/// infiltration state slot under the session's model.
fn infil_slot_ft(model: crate::io::options::InfiltrationModel, slot: usize) -> f64 {
    use crate::io::options::InfiltrationModel as M;
    const FT: f64 = 0.3048;
    match model {
        // tp (s), Fe (ft), rest unused.
        M::Horton | M::ModifiedHorton => [1.0, FT, FT, 1.0, 1.0, 1.0][slot],
        // IMD (–), F (ft), Fu (ft), Sat flag, T (s), unused.
        M::GreenAmpt | M::ModifiedGreenAmpt => [1.0, FT, FT, 1.0, 1.0, 1.0][slot],
        // S, P, F (ft), T (s), Se (ft), f (ft/s).
        M::CurveNumber => [FT, FT, FT, 1.0, FT, FT][slot],
    }
}

/// A raw series value at run time `t` as a step function: each entry's
/// value holds until the next timestamp (§3.1 evaporation), zero before
/// the first and the last value held beyond the end.
fn series_step_value(net: &Network, start_epoch: f64, si: usize, t: f64) -> f64 {
    let TimeSeriesSource::Points(points) = &net.timeseries[si].source else {
        return 0.0;
    };
    let at = |st: &SeriesTime| -> f64 {
        match st {
            SeriesTime::Elapsed(s) => *s,
            SeriesTime::Absolute { date, seconds } => {
                days_from_civil(*date) as f64 * 86_400.0 + seconds - start_epoch
            }
        }
    };
    let mut v = 0.0;
    for pt in points {
        if at(&pt.time) > t {
            break;
        }
        v = pt.value;
    }
    v
}

/// A raw series value at run time `t`, warning-free: ends held for the
/// §9.1 rule-driven contract, zero beyond the range for §8.2 external
/// accumulation (§10.1).
fn series_value_pure(net: &Network, start_epoch: f64, si: usize, t: f64, hold_ends: bool) -> f64 {
    let TimeSeriesSource::Points(points) = &net.timeseries[si].source else {
        return 0.0;
    };
    if points.is_empty() {
        return 0.0;
    }
    let at = |st: &SeriesTime| -> f64 {
        match st {
            SeriesTime::Elapsed(s) => *s,
            SeriesTime::Absolute { date, seconds } => {
                days_from_civil(*date) as f64 * 86_400.0 + seconds - start_epoch
            }
        }
    };
    let x = t;
    if x <= at(&points[0].time) {
        return if hold_ends { points[0].value } else { 0.0 };
    }
    if x >= at(&points[points.len() - 1].time) {
        return if hold_ends {
            points[points.len() - 1].value
        } else {
            0.0
        };
    }
    for w in points.windows(2) {
        let (t0, t1) = (at(&w[0].time), at(&w[1].time));
        if x <= t1 {
            if t1 <= t0 {
                return w[1].value;
            }
            return w[0].value + (w[1].value - w[0].value) * (x - t0) / (t1 - t0);
        }
    }
    points[points.len() - 1].value
}

#[cfg(test)]
mod interface_file_modes {
    use super::*;

    fn model_with(files: &str) -> String {
        let base = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/uds/runoff_parcel.inp"),
        )
        .expect("fixture readable");
        format!("{base}\n{files}\n")
    }

    /// Every interface format §14.8 declares is now served, so declaring
    /// one neither refuses the model nor reports anything about it. The
    /// two tests this replaced asserted the opposite for rainfall.
    #[test]
    fn declaring_an_interface_file_neither_refuses_nor_reports() {
        for files in [
            "[FILES]\nSAVE RAINFALL rain.rff",
            "[FILES]\nSAVE RUNOFF runoff.bin",
            "[FILES]\nSAVE RDII rdii.txt",
            "",
        ] {
            let Ok((_sim, _diags, findings)) = Simulation::open(&model_with(files)) else {
                panic!("a served format must not block the model: {files:?}");
            };
            let errors: Vec<_> = findings.iter().filter(|f| f.kind.is_error()).collect();
            assert!(errors.is_empty(), "{files:?}: {errors:?}");
        }
    }

    /// `USE RAINFALL` names an input the results depend on. A caller who
    /// does not supply it is refused, rather than the run quietly falling
    /// back to each gage's own record and producing different results.
    #[test]
    fn reading_a_rainfall_file_without_its_bytes_is_refused() {
        let file_gage = model_with("[FILES]\nUSE RAINFALL rain.rff").replace(
            "G1  INTENSITY  0:15  1.0  TIMESERIES  RAIN1",
            "G1  INTENSITY  0:15  1.0  FILE  \"rain.dat\"  STA01  IN",
        );
        let Err(err) = Simulation::open(&file_gage) else {
            panic!("a model reading an unsupplied cache must not open");
        };
        let text = err.to_string();
        assert!(text.contains("rain.rff"), "{text}");
    }

    /// A model with no file-sourced gage has nothing to read from the
    /// cache, so declaring one is harmless and opens.
    #[test]
    fn a_rainfall_file_no_gage_reads_does_not_block_the_model() {
        assert!(
            Simulation::open(&model_with("[FILES]\nUSE RAINFALL rain.rff")).is_ok(),
            "a cache nothing reads must not refuse the model"
        );
    }
}

impl Snapshot {
    /// Write one reporting record (§12.3). A restored run must be able to
    /// write the whole run's results, not the part after the checkpoint.
    fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_f, put_fs, put_u};
        let Snapshot {
            t,
            depths,
            flows,
            node_head,
            node_volume,
            node_lateral,
            node_inflow,
            node_flooding,
            node_quality,
            link_depth,
            link_velocity,
            link_volume,
            link_capacity,
            link_quality,
            subcatch,
            system,
        } = self;
        put_f(w, *t)?;
        for vs in [
            depths,
            flows,
            node_head,
            node_volume,
            node_lateral,
            node_inflow,
            node_flooding,
            link_depth,
            link_velocity,
            link_volume,
            link_capacity,
        ] {
            put_fs(w, vs)?;
        }
        for rows in [node_quality, link_quality] {
            put_u(w, rows.len() as u64)?;
            for row in rows {
                put_fs(w, row)?;
            }
        }
        put_u(w, subcatch.len() as u64)?;
        for rec in subcatch {
            rec.checkpoint_put(w)?;
        }
        for v in system {
            put_f(w, *v)?;
        }
        Ok(())
    }

    fn checkpoint_get(
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<Snapshot, String> {
        let t = r.f()?;
        let mut fs = || r.fs();
        let depths = fs()?;
        let flows = fs()?;
        let node_head = fs()?;
        let node_volume = fs()?;
        let node_lateral = fs()?;
        let node_inflow = fs()?;
        let node_flooding = fs()?;
        let link_depth = fs()?;
        let link_velocity = fs()?;
        let link_volume = fs()?;
        let link_capacity = fs()?;
        let mut rows = || -> Result<Vec<Vec<f64>>, String> {
            let n = r.u()? as usize;
            (0..n).map(|_| r.fs()).collect()
        };
        let node_quality = rows()?;
        let link_quality = rows()?;
        let n = r.u()? as usize;
        let subcatch = (0..n)
            .map(|_| SubcatchRecord::checkpoint_get(r))
            .collect::<Result<Vec<_>, _>>()?;
        let mut system = [0.0; 15];
        for v in &mut system {
            *v = r.f()?;
        }
        Ok(Snapshot {
            t,
            depths,
            flows,
            node_head,
            node_volume,
            node_lateral,
            node_inflow,
            node_flooding,
            node_quality,
            link_depth,
            link_velocity,
            link_volume,
            link_capacity,
            link_quality,
            subcatch,
            system,
        })
    }
}

impl SubcatchRecord {
    fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_f, put_fs};
        let SubcatchRecord {
            rain,
            snow_depth,
            evap,
            infil,
            runoff,
            gw_flow,
            gw_elev,
            soil_moisture,
            washoff,
        } = self;
        for v in [
            rain,
            snow_depth,
            evap,
            infil,
            runoff,
            gw_flow,
            gw_elev,
            soil_moisture,
        ] {
            put_f(w, *v)?;
        }
        put_fs(w, washoff)
    }

    fn checkpoint_get(
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<SubcatchRecord, String> {
        Ok(SubcatchRecord {
            rain: r.f()?,
            snow_depth: r.f()?,
            evap: r.f()?,
            infil: r.f()?,
            runoff: r.f()?,
            gw_flow: r.f()?,
            gw_elev: r.f()?,
            soil_moisture: r.f()?,
            washoff: r.fs()?,
        })
    }
}

impl ClimateDayState {
    fn checkpoint_put(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        use crate::simulation::checkpoint::{put_b, put_f, put_fs, put_u};
        let ClimateDayState {
            last_day,
            tmin,
            tmax,
            tave,
            trng,
            trng1,
            prev_tmax,
            hrsr,
            hrss,
            hrday,
            dhrdy,
            dydif,
            ma_ta,
            ma_tr,
            front,
            t_ave7,
            t_rng7,
            hargreaves,
            file_evap,
            wind,
        } = self;
        // A day index, written as the bits of its own type so a day
        // before the epoch survives.
        w.write_all(&last_day.to_le_bytes())?;
        for v in [
            tmin, tmax, tave, trng, trng1, hrsr, hrss, hrday, dhrdy, dydif, t_ave7, t_rng7,
            hargreaves, file_evap, wind,
        ] {
            put_f(w, *v)?;
        }
        put_b(w, prev_tmax.is_some())?;
        put_f(w, prev_tmax.unwrap_or(0.0))?;
        put_fs(w, ma_ta)?;
        put_fs(w, ma_tr)?;
        put_u(w, *front as u64)?;
        Ok(())
    }

    fn checkpoint_get(
        &mut self,
        r: &mut crate::simulation::checkpoint::Reader<'_>,
    ) -> Result<(), String> {
        self.last_day = r.i64()?;
        for slot in [
            &mut self.tmin,
            &mut self.tmax,
            &mut self.tave,
            &mut self.trng,
            &mut self.trng1,
            &mut self.hrsr,
            &mut self.hrss,
            &mut self.hrday,
            &mut self.dhrdy,
            &mut self.dydif,
            &mut self.t_ave7,
            &mut self.t_rng7,
            &mut self.hargreaves,
            &mut self.file_evap,
            &mut self.wind,
        ] {
            *slot = r.f()?;
        }
        let has = r.b()?;
        let v = r.f()?;
        self.prev_tmax = has.then_some(v);
        self.ma_ta = r.fs()?;
        self.ma_tr = r.fs()?;
        self.front = r.u()? as usize;
        Ok(())
    }
}
