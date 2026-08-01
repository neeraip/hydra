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
    /// Parcel identifiers, in registration order.
    pub parcel_ids: Vec<String>,
    /// Transect identifiers, in registration order.
    pub transect_ids: Vec<String>,
    /// Street-section identifiers, in registration order.
    pub street_ids: Vec<String>,
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
