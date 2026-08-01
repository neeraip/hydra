//! The domain model (specification §2): the entities a drainage model is
//! composed of, as parsed from a predecessor file and resolved into indices.
//!
//! Quantities are SI — metres, m², m³/s — converted once at the §14 import
//! boundary. Cross-section geometry parameters are the §5 exception staged
//! deliberately: they are stored as the file carries them, because which of
//! the four parameters are lengths is a per-shape question §5's geometry
//! evaluation owns; the field name says so.

use crate::io::options::{AnalysisOptions, Date};

/// A parsed, reference-resolved drainage network.
///
/// Objects whose full records parse in later increments (curves, series,
/// parcels, transects, streets) are present as ordered identifier tables so
/// references to them are already index-resolved.
#[derive(Debug, Default)]
pub struct Network {
    /// The analysis options (§14.4).
    pub options: AnalysisOptions,
    /// Up to three title lines.
    pub title: Vec<String>,
    /// Conveyance vertices, in registration order.
    pub vertices: Vec<Vertex>,
    /// Conveyance links, in registration order.
    pub links: Vec<Link>,
    /// Curves, in registration order.
    pub curves: Vec<Curve>,
    /// Time series, in registration order.
    pub timeseries: Vec<TimeSeries>,
    /// Time patterns, in registration order.
    pub patterns: Vec<TimePattern>,
    /// Precipitation gages, in registration order.
    pub gages: Vec<Gage>,
    /// Constituents, in registration order.
    pub constituents: Vec<Constituent>,
    /// Land uses, in registration order.
    pub land_uses: Vec<LandUse>,
    /// External inflows at vertices.
    pub inflows: Vec<ExternalInflow>,
    /// Sanitary (dry-weather) inflows at vertices.
    pub dry_weather: Vec<DryWeatherInflow>,
    /// Parcels, in registration order.
    pub parcels: Vec<Parcel>,
    /// Transects, in registration order.
    pub transects: Vec<Transect>,
    /// Aquifers, in registration order.
    pub aquifers: Vec<Aquifer>,
    /// Snow pack parameter sets, in registration order.
    pub snowpacks: Vec<Snowpack>,
    /// Unit-hydrograph groups, in registration order.
    pub unit_hydrographs: Vec<UnitHydrographGroup>,
    /// Sewer-inflow (RDII) assignments at vertices.
    pub rdii: Vec<RdiiInflow>,
    /// Treatment expressions at vertices.
    pub treatments: Vec<Treatment>,
    /// Control-measure designs, in registration order.
    pub lid_controls: Vec<LidControl>,
    /// Control-measure deployments in parcels.
    pub lid_usage: Vec<LidUsage>,
    /// Street sections, in registration order.
    pub streets: Vec<Street>,
    /// Inlet designs, in registration order.
    pub inlets: Vec<InletDesign>,
    /// Inlet placements on street channels.
    pub inlet_usage: Vec<InletUsage>,
}

/// A conveyance vertex (§2.6).
#[derive(Debug, Clone, PartialEq)]
pub struct Vertex {
    /// Identifier as written.
    pub id: String,
    /// Invert elevation (m).
    pub invert: f64,
    /// The vertex kind and its parameters.
    pub kind: VertexKind,
}

/// The four vertex kinds (§2.6).
#[derive(Debug, Clone, PartialEq)]
pub enum VertexKind {
    /// An ordinary connection point.
    Junction {
        /// Maximum (ground/rim) depth (m); 0 = raised to crown at validation.
        max_depth: f64,
        /// Initial water depth (m).
        init_depth: f64,
        /// Surcharge allowance above the rim (m).
        surcharge_depth: f64,
        /// Ponded area once above full depth (m²), where ponding is enabled.
        ponded_area: f64,
    },
    /// A terminal boundary vertex.
    Outfall {
        /// The boundary condition.
        stage: OutfallStage,
        /// Gate blocking reverse flow.
        flap_gate: bool,
        /// Discharge returned onto a parcel.
        route_to_parcel: Option<usize>,
    },
    /// A vertex with significant free-surface storage.
    Storage {
        /// Maximum depth (m).
        max_depth: f64,
        /// Initial depth (m).
        init_depth: f64,
        /// Surface-area description.
        geometry: StorageGeometry,
        /// Surcharge allowance (m).
        surcharge_depth: f64,
        /// Evaporation realisation fraction (default 0).
        evap_fraction: f64,
        /// Seepage parameters, when supplied.
        seepage: Option<StorageSeepage>,
    },
    /// A flow divider — a junction under the one solver (§7.5); its rule is
    /// retained as reduced-form semantics for the import record.
    Divider {
        /// The diverted link, resolved (`*` = none named).
        diverted_link: Option<usize>,
        /// The split rule.
        rule: DividerRule,
        /// Maximum depth (m).
        max_depth: f64,
        /// Initial depth (m).
        init_depth: f64,
        /// Surcharge allowance (m).
        surcharge_depth: f64,
        /// Ponded area (m²).
        ponded_area: f64,
    },
}

/// Outfall boundary conditions (§2.6).
#[derive(Debug, Clone, PartialEq)]
pub enum OutfallStage {
    /// The smaller of critical and normal depth at the connecting channel.
    Free,
    /// Normal depth.
    Normal,
    /// A fixed stage (m).
    Fixed(f64),
    /// A repeating tidal stage curve, indexed by clock time (§14.7).
    Tidal {
        /// Stage-versus-hour curve.
        curve: usize,
    },
    /// A supplied stage series.
    Series {
        /// The time series.
        series: usize,
    },
}

/// Storage surface-area geometry (§2.6). The analytical shapes compile to
/// the common quadratic $A = a_0 + a_1 y + a_2 y^2$ at parse, as the
/// predecessor compiles them, with the shape kind retained.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageGeometry {
    /// Area from a tabulated area-versus-depth curve.
    Tabular {
        /// The storage curve.
        curve: usize,
    },
    /// $A = c + a\,y^{b}$ — converted per its exponent at import (§14.6).
    Functional {
        /// Coefficient $a$ (m^(2−b)).
        coeff: f64,
        /// Exponent $b$.
        exponent: f64,
        /// Constant $c$ (m²).
        constant: f64,
    },
    /// An analytical shape, compiled to $A = a_0 + a_1 y + a_2 y^2$ (m).
    Shape {
        /// Which shape the file declared.
        kind: StorageShapeKind,
        /// Constant term (m²).
        a0: f64,
        /// Linear coefficient (m).
        a1: f64,
        /// Quadratic coefficient (dimensionless).
        a2: f64,
    },
}

/// The four analytical storage shapes (5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageShapeKind {
    /// Elliptical cylinder.
    Cylindrical,
    /// Elliptical cone.
    Conical,
    /// Elliptical paraboloid.
    Paraboloid,
    /// Rectangular pyramid.
    Pyramidal,
}

/// Storage seepage parameters (§7.7): the Green–Ampt triple.
#[derive(Debug, Clone, PartialEq)]
pub struct StorageSeepage {
    /// Wetting-front suction head (m).
    pub suction: f64,
    /// Saturated hydraulic conductivity (m/s).
    pub conductivity: f64,
    /// Initial moisture deficit (fraction).
    pub initial_deficit: f64,
}

/// Divider split rules — reduced-form semantics (§7.5), retained for import.
#[derive(Debug, Clone, PartialEq)]
pub enum DividerRule {
    /// Divert all inflow above a threshold (m³/s).
    Cutoff {
        /// Threshold inflow (m³/s).
        min_flow: f64,
    },
    /// Diverted flow from a curve of inflow.
    Tabular {
        /// The diversion curve.
        curve: usize,
    },
    /// The weir rule.
    Weir {
        /// Minimum flow for diversion (m³/s).
        min_flow: f64,
        /// Maximum depth (m).
        max_depth: f64,
        /// Discharge coefficient (m^½/s, converted per §14.6).
        coeff: f64,
    },
    /// Divert what the non-diverted channel declines.
    Overflow,
}

/// A conveyance link (§2.7).
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// Identifier as written.
    pub id: String,
    /// Upstream vertex index (orientation defines positive flow).
    pub from: usize,
    /// Downstream vertex index.
    pub to: usize,
    /// The link kind and its parameters.
    pub kind: LinkKind,
    /// Cross-section, once `[XSECTIONS]` assigns one (channels and
    /// regulators that take one).
    pub cross_section: Option<CrossSection>,
}

/// A link invert offset, in the convention the file selected (§14.7
/// converts between them at validation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Offset {
    /// Height above the vertex invert (m).
    Depth(f64),
    /// Absolute elevation (m).
    Elevation(f64),
    /// `*` under the elevation convention: to be resolved at validation.
    Missing,
}

/// The five link kinds (§2.7).
#[derive(Debug, Clone, PartialEq)]
pub enum LinkKind {
    /// A pipe or open channel.
    Channel {
        /// Length (m).
        length: f64,
        /// Manning roughness (s·m^(-1/3)).
        roughness: f64,
        /// Upstream invert offset.
        offset1: Offset,
        /// Downstream invert offset.
        offset2: Offset,
        /// Initial flow (m³/s).
        init_flow: f64,
        /// Maximum-flow limit (m³/s); 0 = none.
        max_flow: f64,
    },
    /// A pump (§7.1).
    Pump {
        /// Pump characteristic curve; `*` = the ideal transfer pump.
        curve: Option<usize>,
        /// Initially on.
        initial_on: bool,
        /// Startup wet-well depth (m).
        startup_depth: f64,
        /// Shutoff wet-well depth (m).
        shutoff_depth: f64,
    },
    /// An orifice (§7.2).
    Orifice {
        /// Side or bottom.
        orientation: OrificeOrientation,
        /// Crest offset.
        offset: Offset,
        /// Discharge coefficient (dimensionless).
        discharge_coeff: f64,
        /// Gate blocking reverse flow.
        flap_gate: bool,
        /// Open/close rate (s to full travel); 0 = instant.
        open_close_time: f64,
    },
    /// A weir (§7.3).
    Weir {
        /// The weir form.
        form: WeirForm,
        /// Crest offset.
        offset: Offset,
        /// Discharge coefficient (m^½/s — every weir form shares the
        /// dimension, converted per §14.6).
        discharge_coeff: f64,
        /// Gate blocking reverse flow.
        flap_gate: bool,
        /// End-contraction count.
        end_contractions: f64,
        /// Second coefficient (trapezoidal end sections; m^½/s).
        end_coeff: f64,
        /// Whether the weir surcharges to its equivalent-orifice form.
        can_surcharge: bool,
        /// Road width (m), embankment weirs.
        road_width: f64,
        /// Road surface, embankment weirs.
        road_surface: RoadSurface,
        /// Head-dependent coefficient curve.
        coeff_curve: Option<usize>,
    },
    /// An outlet (§7.4).
    Outlet {
        /// Crest offset.
        offset: Offset,
        /// The rating.
        rating: OutletRating,
        /// Rating argument: upstream depth or head difference.
        head_basis: OutletHeadBasis,
        /// Gate blocking reverse flow.
        flap_gate: bool,
    },
}

/// Orifice orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrificeOrientation {
    /// In the vertex wall.
    Side,
    /// In the vertex floor.
    Bottom,
}

/// Weir forms (§7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeirForm {
    /// Transverse rectangular.
    Transverse,
    /// Side-flow (Engels).
    SideFlow,
    /// V-notch.
    VNotch,
    /// Rectangular centre with triangular ends.
    Trapezoidal,
    /// FHWA embankment-overtopping.
    Roadway,
}

/// Embankment-weir road surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoadSurface {
    /// Not specified.
    #[default]
    Unspecified,
    /// Paved.
    Paved,
    /// Gravel.
    Gravel,
}

/// Outlet rating (§7.4). Coefficients convert per their exponent (§14.6).
#[derive(Debug, Clone, PartialEq)]
pub enum OutletRating {
    /// $Q = a\,H^{b}$.
    Functional {
        /// Coefficient $a$ ((m³/s)/m^b).
        coeff: f64,
        /// Exponent $b$.
        exponent: f64,
    },
    /// A tabulated rating curve.
    Tabular {
        /// The rating curve.
        curve: usize,
    },
}

/// What an outlet's rating argument measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutletHeadBasis {
    /// Upstream depth above the outlet crest.
    Depth,
    /// Head difference across the outlet.
    Head,
}

/// A link cross-section assignment (`[XSECTIONS]`).
#[derive(Debug, Clone, PartialEq)]
pub struct CrossSection {
    /// The section shape.
    pub shape: XsectShape,
    /// The four geometry parameters **as the file carries them** (user
    /// units): which are lengths is a per-shape question §5's geometry
    /// evaluation owns, and conversion happens there.
    pub geom_user: [f64; 4],
    /// Identical parallel barrels (channels; ≥ 1).
    pub barrels: u32,
    /// FHWA culvert code; 0 = not a culvert.
    pub culvert_code: u32,
    /// Referent for the irregular / street / custom shapes.
    pub referent: Option<XsectReferent>,
}

/// What a referent-carrying shape points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XsectReferent {
    /// A surveyed transect (irregular sections).
    Transect(usize),
    /// A street section.
    Street(usize),
    /// A width-versus-depth shape curve (custom sections).
    Curve(usize),
}

/// The cross-section shape vocabulary, in the predecessor's table order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)] // the names are the documentation (§5)
pub enum XsectShape {
    Dummy,
    Circular,
    FilledCircular,
    RectClosed,
    RectOpen,
    Trapezoidal,
    Triangular,
    Parabolic,
    Power,
    RectTriangular,
    RectRound,
    ModBasketHandle,
    HorizEllipse,
    VertEllipse,
    Arch,
    Egg,
    Horseshoe,
    Gothic,
    Catenary,
    SemiElliptical,
    BasketHandle,
    SemiCircular,
    Irregular,
    Custom,
    ForceMain,
    Street,
}

/// A tabulated relation (§2.9): the typed role travels with the curve, and
/// point conversion at import follows the role (§14.6).
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// Identifier as written.
    pub id: String,
    /// The typed role.
    pub kind: CurveKind,
    /// The points, converted per the role's units.
    pub points: Vec<(f64, f64)>,
}

/// Curve roles, in the predecessor's vocabulary order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    /// Surface area (m²) against depth (m).
    Storage,
    /// Diverted flow against inflow (both m³/s).
    Diversion,
    /// Stage (m) against hour of day (s).
    Tidal,
    /// Outlet discharge (m³/s) against head (m).
    Rating,
    /// Control setting against controller variable (both as written).
    Control,
    /// Normalised width against depth (dimensionless).
    Shape,
    /// Weir coefficient (m^½/s) against head (m).
    WeirCoeff,
    /// Pump: flow (m³/s) stepwise against wet-well volume (m³).
    Pump1,
    /// Pump: flow stepwise against inlet depth (m).
    Pump2,
    /// Pump: flow against head difference (m).
    Pump3,
    /// Pump: flow against inlet depth (m).
    Pump4,
    /// Pump: flow against head difference at rated speed (m).
    Pump5,
}

/// A time series (§2.9). Values are stored as written: their unit depends on
/// the consumer (precipitation, stage, inflow, evaporation), which converts
/// at use.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeSeries {
    /// Identifier as written.
    pub id: String,
    /// Inline points, or an external-file reference.
    pub source: TimeSeriesSource,
}

/// Where a series' data lives.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeSeriesSource {
    /// An external data file, named by the model; acquiring its bytes is the
    /// caller's concern (this crate performs no filesystem I/O).
    External {
        /// The file name as written.
        file: String,
    },
    /// Inline timestamped points.
    Points(Vec<TimeSeriesPoint>),
}

/// One series point.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeSeriesPoint {
    /// When.
    pub time: SeriesTime,
    /// The value, in the consumer's units, as written.
    pub value: f64,
}

/// A series timestamp: elapsed, or anchored to a calendar date. A date seen
/// on any line anchors every later time until the next date, per the
/// predecessor's reader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SeriesTime {
    /// Seconds from simulation start.
    Elapsed(f64),
    /// A calendar date and seconds past its midnight.
    Absolute {
        /// The date.
        date: Date,
        /// Seconds past midnight.
        seconds: f64,
    },
}

/// A repeating multiplier set (§2.9).
#[derive(Debug, Clone, PartialEq)]
pub struct TimePattern {
    /// Identifier as written.
    pub id: String,
    /// The period.
    pub kind: PatternKind,
    /// The multipliers, at most the period's count.
    pub factors: Vec<f64>,
}

/// Pattern periods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    /// Twelve monthly factors.
    Monthly,
    /// Seven daily factors, Sunday first.
    Daily,
    /// Twenty-four hourly factors.
    Hourly,
    /// Twenty-four weekend hourly factors.
    Weekend,
}

/// A precipitation gage (§2.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Gage {
    /// Identifier as written.
    pub id: String,
    /// How the record's values are expressed.
    pub form: RainForm,
    /// Recording interval (s).
    pub interval: f64,
    /// Snow catch factor.
    pub catch_factor: f64,
    /// Where the record comes from.
    pub source: GageSource,
}

/// Precipitation record forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RainForm {
    /// Rate over each interval.
    Intensity,
    /// Depth per interval.
    Volume,
    /// Running cumulative depth.
    Cumulative,
}

/// A gage's data source.
#[derive(Debug, Clone, PartialEq)]
pub enum GageSource {
    /// A supplied time series.
    Series {
        /// The series.
        series: usize,
    },
    /// An external record file; acquiring bytes is the caller's concern.
    File {
        /// File name as written.
        file: String,
        /// Station identifier within the file.
        station: String,
    },
}

/// A parcel (§2.4), assembled from its three sections.
#[derive(Debug, Clone, PartialEq)]
pub struct Parcel {
    /// Identifier as written.
    pub id: String,
    /// The precipitation gage.
    pub gage: usize,
    /// Where runoff discharges.
    pub outlet: ParcelOutlet,
    /// Area (m²).
    pub area: f64,
    /// Impervious fraction, capped at 1 (§14.7).
    pub frac_imperv: f64,
    /// Characteristic width (m).
    pub width: f64,
    /// Surface slope (fraction).
    pub slope: f64,
    /// Curb length (m), for per-curb accumulation normalisation.
    pub curb_length: f64,
    /// Snow pack parameter set, when assigned.
    pub snowpack: Option<usize>,
    /// Land-use cover: (land use, fraction) pairs from `[COVERAGES]`.
    pub land_cover: Vec<(usize, f64)>,
    /// Initial surface buildup: (constituent, areal load as written).
    pub init_buildup: Vec<(usize, f64)>,
    /// Sub-area parameters, once `[SUBAREAS]` supplies them.
    pub subareas: Option<Subareas>,
    /// Infiltration parameters, once `[INFILTRATION]` supplies them.
    pub infiltration: Option<Infiltration>,
    /// Groundwater connection, once `[GROUNDWATER]` supplies one.
    pub groundwater: Option<GroundwaterLink>,
}

/// A parcel's discharge target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParcelOutlet {
    /// A conveyance vertex.
    Vertex(usize),
    /// Another parcel (overland cascade).
    Parcel(usize),
}

/// The three sub-areas' parameters (§3.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Subareas {
    /// Manning roughness of the impervious sub-areas.
    pub n_imperv: f64,
    /// Manning roughness of the pervious sub-area.
    pub n_perv: f64,
    /// Depression storage, impervious (m).
    pub dstore_imperv: f64,
    /// Depression storage, pervious (m).
    pub dstore_perv: f64,
    /// Fraction of the impervious area with no depression storage.
    pub frac_zero_store: f64,
    /// Internal re-routing target.
    pub routing: SubareaRouting,
    /// Fraction of runoff re-routed (1 = all).
    pub frac_routed: f64,
}

/// Internal sub-area re-routing (§3.2): mutually exclusive directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubareaRouting {
    /// Both sub-areas discharge to the outlet (no re-routing).
    Outlet,
    /// Pervious runoff routes onto the impervious sub-area.
    Impervious,
    /// Impervious runoff routes onto the pervious sub-area.
    Pervious,
}

/// Per-parcel infiltration parameters (§3.3), in SI.
#[derive(Debug, Clone, PartialEq)]
pub enum Infiltration {
    /// Horton or modified Horton, per the model selection.
    Horton {
        /// Initial capacity (m/s).
        f0: f64,
        /// Equilibrium capacity (m/s).
        f_min: f64,
        /// Decay coefficient (1/s).
        decay: f64,
        /// Drying time (s).
        dry_time: f64,
        /// Optional total-volume cap (m); 0 = none.
        f_max: f64,
    },
    /// Green–Ampt or modified Green–Ampt, per the model selection.
    GreenAmpt {
        /// Wetting-front suction head (m).
        suction: f64,
        /// Saturated conductivity (m/s).
        conductivity: f64,
        /// Initial moisture deficit (fraction).
        initial_deficit: f64,
    },
    /// The SCS relation; the curve number is dimensionless.
    CurveNumber {
        /// Curve number, clamped to [10, 99] at validation.
        curve_number: f64,
        /// Drying time (s).
        dry_time: f64,
    },
}

/// A constituent (§2.8). Concentrations stay in their declared unit; the
/// decay coefficient converts from per-day to per-second.
#[derive(Debug, Clone, PartialEq)]
pub struct Constituent {
    /// Identifier as written.
    pub id: String,
    /// Concentration unit.
    pub units: ConcentrationUnits,
    /// Background concentration in precipitation.
    pub c_rain: f64,
    /// Background concentration in groundwater.
    pub c_groundwater: f64,
    /// Background concentration in sewer inflow.
    pub c_rdii: f64,
    /// First-order decay coefficient (1/s); negative models growth.
    pub decay: f64,
    /// Accumulates only under snow cover.
    pub snow_only: bool,
    /// Co-pollutant relation: this constituent's load gains a fraction of
    /// another's.
    pub co_constituent: Option<usize>,
    /// The co-pollutant fraction (may exceed 1: it bridges units).
    pub co_fraction: f64,
    /// Background concentration in sanitary flow.
    pub c_dwf: f64,
    /// Initial network concentration.
    pub c_init: f64,
}

/// Concentration units (independent of the unit system).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConcentrationUnits {
    /// Milligrams per litre.
    MgPerL,
    /// Micrograms per litre.
    UgPerL,
    /// Organism counts per litre.
    CountPerL,
}

/// A land use (§2.8), owning per-constituent accumulation and mobilisation
/// relations. Accumulation masses and their normalisers stay in the file's
/// units — the §8 relations own their interpretation.
#[derive(Debug, Clone, PartialEq)]
pub struct LandUse {
    /// Identifier as written.
    pub id: String,
    /// Street-cleaning interval (days); 0 = never.
    pub sweep_interval: f64,
    /// Fraction of buildup a cleaning pass can remove.
    pub sweep_removal: f64,
    /// Days since the last cleaning at simulation start.
    pub sweep_days_since: f64,
    /// Per-constituent accumulation, indexed by constituent.
    pub buildup: Vec<Option<Buildup>>,
    /// Per-constituent mobilisation, indexed by constituent.
    pub washoff: Vec<Option<Washoff>>,
}

/// An accumulation relation (§8.2), coefficients as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Buildup {
    /// The functional form.
    pub form: BuildupForm,
    /// The three coefficients, in the file's column order.
    pub coeffs: [f64; 3],
    /// Per-area or per-curb-length normalisation.
    pub normalizer: BuildupNormalizer,
}

/// Accumulation forms (§8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildupForm {
    /// No accumulation.
    None,
    /// Power growth capped at a maximum.
    Power,
    /// Exponential approach to a maximum.
    Exponential,
    /// Michaelis–Menten saturation.
    Saturation,
    /// A scaled external loading series.
    External,
}

/// What accumulation normalises by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildupNormalizer {
    /// Mass per unit area.
    PerArea,
    /// Mass per unit curb length.
    PerCurb,
}

/// A mobilisation relation (§8.3), coefficients as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Washoff {
    /// The functional form.
    pub form: WashoffForm,
    /// Washoff coefficient.
    pub coeff: f64,
    /// Washoff exponent.
    pub exponent: f64,
    /// Cleaning removal efficiency (%).
    pub sweep_efficiency: f64,
    /// BMP removal efficiency (%).
    pub bmp_efficiency: f64,
}

/// Mobilisation forms (§8.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WashoffForm {
    /// No mobilisation.
    None,
    /// Source-limited exponential in remaining buildup.
    Exponential,
    /// Rating on the land-use share of flow.
    RatingCurve,
    /// Constant event-mean concentration.
    Emc,
}

/// A direct external inflow at a vertex (§2.6, §8.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalInflow {
    /// The receiving vertex.
    pub vertex: usize,
    /// The constituent; `None` is a flow inflow.
    pub constituent: Option<usize>,
    /// The supplied series, when given.
    pub series: Option<usize>,
    /// How the series and baseline are read.
    pub kind: InflowKind,
    /// User units factor (mass inflows).
    pub units_factor: f64,
    /// Series scale factor.
    pub scale: f64,
    /// Constant baseline — m³/s for flow inflows, concentration or mass
    /// rate as written otherwise.
    pub baseline: f64,
    /// The baseline's periodic modulation.
    pub base_pattern: Option<usize>,
}

/// External-inflow interpretations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InflowKind {
    /// A water inflow.
    Flow,
    /// A concentration riding the accompanying flow.
    Concentration,
    /// A mass rate needing no flow.
    Mass,
}

/// A sanitary inflow at a vertex (§2.6): an average modulated by up to four
/// patterns, whose slot-versus-declared-type rules §14.7 flags.
#[derive(Debug, Clone, PartialEq)]
pub struct DryWeatherInflow {
    /// The receiving vertex.
    pub vertex: usize,
    /// The constituent; `None` is the flow inflow.
    pub constituent: Option<usize>,
    /// Average value — m³/s for flow, concentration as written otherwise.
    pub average: f64,
    /// The four pattern slots (monthly, daily, hourly, weekend), as given.
    pub patterns: [Option<usize>; 4],
}

/// A surveyed transect (§5.6). Roughness is a property of the transect —
/// the reader's rolling NC state resolves at parse, a transect is complete
/// when its record ends, and the meander factor belongs to this transect
/// alone (the predecessor's shared-state defects, closed by construction).
#[derive(Debug, Clone, PartialEq)]
pub struct Transect {
    /// Identifier as written.
    pub id: String,
    /// Left-overbank Manning roughness.
    pub n_left: f64,
    /// Right-overbank Manning roughness.
    pub n_right: f64,
    /// Main-channel Manning roughness.
    pub n_channel: f64,
    /// Left-overbank station (m, multiplier applied).
    pub x_left: f64,
    /// Right-overbank station (m, multiplier applied).
    pub x_right: f64,
    /// Meander modifier: valley-to-channel length ratio; 1 = none.
    pub meander_factor: f64,
    /// Survey points as (elevation, station), both m, multiplier and
    /// offset applied.
    pub stations: Vec<(f64, f64)>,
}

/// An aquifer parameter set (§4.1), SI except the unit-dependent lateral
/// coefficients, which live on the groundwater link.
#[derive(Debug, Clone, PartialEq)]
pub struct Aquifer {
    /// Identifier as written.
    pub id: String,
    /// Porosity (fraction).
    pub porosity: f64,
    /// Wilting point (fraction).
    pub wilting_point: f64,
    /// Field capacity (fraction).
    pub field_capacity: f64,
    /// Saturated conductivity (m/s).
    pub conductivity: f64,
    /// Conductivity slope (dimensionless HCO).
    pub conductivity_slope: f64,
    /// Tension slope (m).
    pub tension_slope: f64,
    /// Upper-zone fraction of potential evapotranspiration.
    pub upper_evap_frac: f64,
    /// Lower-zone evapotranspiration cutoff depth (m).
    pub lower_evap_depth: f64,
    /// Deep-percolation coefficient (m/s).
    pub lower_loss_coeff: f64,
    /// Aquifer bottom elevation (m).
    pub bottom_elev: f64,
    /// Initial water-table elevation (m).
    pub water_table_elev: f64,
    /// Initial upper-zone moisture (fraction).
    pub upper_moisture: f64,
    /// Monthly pattern on the upper evapotranspiration fraction.
    pub evap_pattern: Option<usize>,
}

/// A parcel's groundwater connection (§4.1).
#[derive(Debug, Clone, PartialEq)]
pub struct GroundwaterLink {
    /// The aquifer parameter set.
    pub aquifer: usize,
    /// The receiving vertex.
    pub vertex: usize,
    /// Ground surface elevation over the aquifer (m).
    pub surface_elev: f64,
    /// Lateral-relation coefficient A1 (user units, §14.6).
    pub a1: f64,
    /// Exponent B1.
    pub b1: f64,
    /// Coefficient A2 (user units).
    pub a2: f64,
    /// Exponent B2.
    pub b2: f64,
    /// Interaction coefficient A3 (user units).
    pub a3: f64,
    /// Fixed surface-water depth (m); 0 = use the live routed stage.
    pub fixed_surface_depth: f64,
    /// Threshold elevation override (m); `None` = the vertex invert.
    pub threshold_elev: Option<f64>,
    /// Aquifer bottom override (m).
    pub bottom_elev: Option<f64>,
    /// Initial water-table override (m).
    pub water_table_elev: Option<f64>,
    /// Initial moisture override (fraction).
    pub upper_moisture: Option<f64>,
    /// Custom lateral-flow expression, added to the power relation
    /// (§4.1); kept as written, evaluated per §14.6.
    pub lateral_expression: Option<String>,
    /// Custom deep-percolation expression, replacing the linear reservoir.
    pub deep_expression: Option<String>,
}

/// A snow pack parameter set (§4.2): per-surface melt parameters over the
/// three-way split, plus the removal (plowing) rule.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snowpack {
    /// Identifier as written.
    pub id: String,
    /// Plowable-surface parameters.
    pub plowable: Option<SnowSurface>,
    /// Impervious-surface parameters.
    pub impervious: Option<SnowSurface>,
    /// Pervious-surface parameters.
    pub pervious: Option<SnowSurface>,
    /// Fraction of impervious area that is plowable (from the plowable
    /// line's last parameter).
    pub plow_fraction: f64,
    /// The removal rule, when supplied.
    pub removal: Option<SnowRemoval>,
}

/// One surface's melt parameters (§4.2), SI (°C, m, m/s per °C).
#[derive(Debug, Clone, PartialEq)]
pub struct SnowSurface {
    /// Minimum (21 December) degree-day melt coefficient (m/s per °C).
    pub dh_min: f64,
    /// Maximum (21 June) melt coefficient (m/s per °C).
    pub dh_max: f64,
    /// Base melt temperature (°C).
    pub t_base: f64,
    /// Free-water holding capacity, as a fraction of pack depth.
    pub fw_frac: f64,
    /// Initial pack depth, water equivalent (m).
    pub init_depth: f64,
    /// Initial free water (m), clamped to capacity at parse as the
    /// predecessor clamps it.
    pub init_free_water: f64,
    /// Depth at 100 % areal cover (m); `None` on the plowable surface,
    /// which is always fully covered.
    pub full_cover_depth: Option<f64>,
}

/// The plowing rule (§4.2): beyond the trigger depth, the surface's whole
/// depth redistributes by five fractions.
#[derive(Debug, Clone, PartialEq)]
pub struct SnowRemoval {
    /// Trigger depth (m).
    pub trigger_depth: f64,
    /// The five redistribution fractions, in the file's order.
    pub fractions: [f64; 5],
    /// Receiving parcel for the transfer fraction.
    pub to_parcel: Option<usize>,
}

/// A unit-hydrograph group (§4.3): up to three triangular responses per
/// calendar month, plus the gage assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitHydrographGroup {
    /// Identifier as written.
    pub id: String,
    /// The precipitation gage driving the group.
    pub gage: Option<usize>,
    /// Per-month responses: `months[m][k]` is month `m+1`'s response of
    /// duration class `k` (short, medium, long).
    pub months: Box<[[Option<UhResponse>; 3]; 12]>,
}

/// One triangular response (§4.3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UhResponse {
    /// Fraction of rainfall volume entering the sewer.
    pub r: f64,
    /// Time to peak (s).
    pub t_peak: f64,
    /// Recession-to-peak ratio.
    pub k: f64,
    /// Initial-abstraction capacity (m).
    pub ia_max: f64,
    /// Initial abstraction already depleted at start (m).
    pub ia_init: f64,
    /// Abstraction recovery rate, as written (per day).
    pub ia_recovery: f64,
}

/// A sewer-inflow assignment (§4.3).
#[derive(Debug, Clone, PartialEq)]
pub struct RdiiInflow {
    /// The receiving vertex.
    pub vertex: usize,
    /// The unit-hydrograph group.
    pub group: usize,
    /// Sewershed area (m²).
    pub area: f64,
}

/// A treatment expression at a vertex (§8.5), retained as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Treatment {
    /// The vertex.
    pub vertex: usize,
    /// The constituent treated.
    pub constituent: usize,
    /// Result kind: a removal fraction or a resulting concentration.
    pub kind: TreatmentKind,
    /// The expression text after the `=`, as written (§14.6: expressions
    /// evaluate in the file's unit system).
    pub expression: String,
}

/// What a treatment expression computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreatmentKind {
    /// A fractional removal applied to the influent.
    Removal,
    /// The resulting concentration.
    Concentration,
}

/// A control-measure design (§3.4), assembled from its layer lines. Depths
/// are m, rates m/s; the underdrain coefficient and exponent stay as
/// written (§14.6).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LidControl {
    /// Identifier as written.
    pub id: String,
    /// The unit type.
    pub kind: Option<LidKind>,
    /// Surface layer.
    pub surface: Option<LidSurface>,
    /// Soil layer.
    pub soil: Option<LidSoil>,
    /// Pavement layer.
    pub pavement: Option<LidPavement>,
    /// Storage layer.
    pub storage: Option<LidStorage>,
    /// Underdrain.
    pub drain: Option<LidDrain>,
    /// Green-roof drainage mat.
    pub drain_mat: Option<LidDrainMat>,
    /// Per-constituent drain-load removal fractions.
    pub removals: Vec<(usize, f64)>,
}

/// The eight unit types (§3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidKind {
    /// Bio-retention cell.
    BioRetention,
    /// Rain garden.
    RainGarden,
    /// Green roof.
    GreenRoof,
    /// Infiltration trench.
    InfiltrationTrench,
    /// Permeable pavement.
    PermeablePavement,
    /// Rain barrel.
    RainBarrel,
    /// Rooftop disconnection.
    RooftopDisconnection,
    /// Vegetative swale.
    VegetativeSwale,
}

/// Surface layer parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct LidSurface {
    /// Berm height (m).
    pub thickness: f64,
    /// Void fraction (1 − the file's vegetative volume fraction, as the
    /// predecessor stores it).
    pub void_frac: f64,
    /// Manning roughness.
    pub roughness: f64,
    /// Surface slope (fraction).
    pub slope: f64,
    /// Swale side slope (run per rise).
    pub side_slope: f64,
}

/// Soil layer parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct LidSoil {
    /// Thickness (m).
    pub thickness: f64,
    /// Porosity.
    pub porosity: f64,
    /// Field capacity.
    pub field_capacity: f64,
    /// Wilting point.
    pub wilting_point: f64,
    /// Saturated conductivity (m/s).
    pub k_sat: f64,
    /// Conductivity slope.
    pub k_slope: f64,
    /// Suction head (m).
    pub suction: f64,
}

/// Pavement layer parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct LidPavement {
    /// Thickness (m).
    pub thickness: f64,
    /// Void fraction (the file's void ratio, converted x/(x+1)).
    pub void_frac: f64,
    /// Impervious paver fraction.
    pub imperv_frac: f64,
    /// Permeability (m/s).
    pub k_sat: f64,
    /// Clogging factor (void volumes of treated inflow); 0 = none.
    pub clog_factor: f64,
    /// Regeneration interval (days); 0 = none.
    pub regen_days: f64,
    /// Regeneration degree in [0, 1].
    pub regen_degree: f64,
}

/// Storage layer parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct LidStorage {
    /// Thickness (m).
    pub thickness: f64,
    /// Void fraction (ratio converted x/(x+1)).
    pub void_frac: f64,
    /// Native-soil exfiltration rate (m/s).
    pub k_sat: f64,
    /// Clogging factor; 0 = none.
    pub clog_factor: f64,
    /// Rain barrels: covered against direct rainfall.
    pub covered: bool,
}

/// Underdrain parameters. The power-relation coefficient arrives in the
/// file's rain-rate/rain-depth units and is converted to SI-dimensional
/// form per its exponent (§14.6).
#[derive(Debug, Clone, PartialEq)]
pub struct LidDrain {
    /// Coefficient: m/s per m^exponent of head.
    pub coeff: f64,
    /// Exponent.
    pub exponent: f64,
    /// Offset height (m).
    pub offset: f64,
    /// Drain delay (s; rain barrels).
    pub delay: f64,
    /// Open-threshold head (m); 0 = none.
    pub h_open: f64,
    /// Close-threshold head (m); 0 = none.
    pub h_close: f64,
    /// Optional flow-multiplier curve against head.
    pub curve: Option<usize>,
}

/// Green-roof drainage mat.
#[derive(Debug, Clone, PartialEq)]
pub struct LidDrainMat {
    /// Thickness (m).
    pub thickness: f64,
    /// Void fraction.
    pub void_frac: f64,
    /// Manning roughness.
    pub roughness: f64,
}

/// A control-measure deployment (§3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct LidUsage {
    /// The hosting parcel.
    pub parcel: usize,
    /// The design deployed.
    pub control: usize,
    /// Replicate units.
    pub count: u32,
    /// Area per unit (m²).
    pub area: f64,
    /// Surface width per unit (m).
    pub width: f64,
    /// Initial saturation (fraction).
    pub init_saturation: f64,
    /// Fraction of the parcel's impervious runoff captured.
    pub from_impervious: f64,
    /// Fraction of the parcel's pervious runoff captured.
    pub from_pervious: f64,
    /// Surface outflow returns to the parcel's pervious sub-area rather
    /// than leaving the parcel.
    pub to_pervious: bool,
    /// Detailed per-unit report file, as written; the caller owns I/O.
    pub report_file: Option<String>,
    /// Drain routing override: a parcel or vertex.
    pub drain_to: Option<ParcelOutlet>,
}

/// A street cross-section (§7.8), compiled to a transect at validation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Street {
    /// Identifier as written.
    pub id: String,
    /// Curb-to-crown width (m).
    pub crown_width: f64,
    /// Curb height (m).
    pub curb_height: f64,
    /// Roadway cross slope (fraction).
    pub cross_slope: f64,
    /// Roadway Manning roughness.
    pub roughness: f64,
    /// Depressed-gutter height (m).
    pub gutter_depression: f64,
    /// Depressed-gutter width (m).
    pub gutter_width: f64,
    /// One- or two-sided (default two).
    pub sides: u8,
    /// Backing width (m).
    pub backing_width: f64,
    /// Backing slope (fraction).
    pub backing_slope: f64,
    /// Backing Manning roughness.
    pub backing_roughness: f64,
}

/// An inlet design (§7.8): a combination inlet is one design carrying both
/// a grate and a curb opening.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InletDesign {
    /// Identifier as written.
    pub id: String,
    /// Grate opening.
    pub grate: Option<GrateInlet>,
    /// Curb opening.
    pub curb: Option<CurbInlet>,
    /// Slotted drain.
    pub slotted: Option<SlottedInlet>,
    /// Custom capture/diversion curve.
    pub custom_curve: Option<usize>,
    /// The grate is a drop inlet.
    pub drop_grate: bool,
    /// The curb opening is a drop inlet.
    pub drop_curb: bool,
}

/// A grate opening.
#[derive(Debug, Clone, PartialEq)]
pub struct GrateInlet {
    /// Length (m).
    pub length: f64,
    /// Width (m).
    pub width: f64,
    /// The grate family.
    pub grate: GrateKind,
    /// Generic grates: open-area ratio.
    pub area_ratio: f64,
    /// Generic grates: splash-over velocity (m/s); 0 = none.
    pub splash_velocity: f64,
}

/// The seven standard grate families plus the generic (§7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)] // the names are FHWA designations
pub enum GrateKind {
    PBar50,
    PBar50x100,
    PBar30,
    CurvedVane,
    TiltBar45,
    TiltBar30,
    Reticuline,
    Generic,
}

/// A curb opening.
#[derive(Debug, Clone, PartialEq)]
pub struct CurbInlet {
    /// Length (m).
    pub length: f64,
    /// Opening height (m).
    pub height: f64,
    /// Throat geometry.
    pub throat: ThroatAngle,
}

/// Curb-opening throat geometries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThroatAngle {
    /// Horizontal throat.
    Horizontal,
    /// Inclined throat.
    Inclined,
    /// Vertical throat (the default).
    Vertical,
}

/// A slotted drain.
#[derive(Debug, Clone, PartialEq)]
pub struct SlottedInlet {
    /// Length (m).
    pub length: f64,
    /// Slot width (m).
    pub width: f64,
}

/// An inlet placement on a street channel (§7.8).
#[derive(Debug, Clone, PartialEq)]
pub struct InletUsage {
    /// The street channel carrying the inlet.
    pub link: usize,
    /// The design.
    pub design: usize,
    /// The sewer vertex receiving captured flow.
    pub capture_vertex: usize,
    /// Replicate count.
    pub count: u32,
    /// Clogged percentage.
    pub pct_clogged: f64,
    /// Per-inlet capture cap (m³/s); 0 = none.
    pub flow_limit: f64,
    /// Local gutter depression (m).
    pub local_depression: f64,
    /// Local depression width (m).
    pub local_width: f64,
    /// Placement resolution.
    pub placement: InletPlacement,
}

/// Inlet placement (§7.8): automatic resolves by the bypass vertex's
/// topology at validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InletPlacement {
    /// Resolve at validation.
    #[default]
    Automatic,
    /// Flow-driven capture.
    OnGrade,
    /// Depth-driven capture.
    OnSag,
}
