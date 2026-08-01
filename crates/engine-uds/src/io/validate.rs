//! The §14.7 validation and mutation pass: the post-validation model is
//! what a predecessor file *means*, so validation both refuses and
//! rewrites — and every rewrite of a user-authored value is reported with
//! the element's name.
//!
//! Covered here: curve and series monotonicity; parcel-outlet ambiguity;
//! groundwater elevation ordering; gage consistency (co-gage formats,
//! shared series, recording intervals, the wet-step reduction); unit
//! hydrograph bounds; cyclic treatment dependencies; link offset
//! conversion and zeroing; regulator shape rules and crest raising; crown
//! raising with its exemption ladder; channel slopes (floors, the
//! drop-exceeds-length fallback, adverse reversal); storage volume;
//! divider and pump rules; and the user-dimensioned-ellipse advisory.

use crate::hydraulics::section::{
    build_section, build_street_section, build_transect_section, BuildError, Section, SectionBuild,
};
use crate::model::{
    LinkKind, Network, Offset, OrificeOrientation, OutfallStage, SeriesTime, StorageGeometry,
    TimeSeriesSource, TreatmentKind, VertexKind, WeirForm, XsectReferent, XsectShape,
};

use super::options::LinkOffsets;

/// The predecessor's minimum elevation change for a channel (0.001 ft).
const MIN_DELTA_Z: f64 = 0.001 * 0.3048;

/// A validation finding, named for the element it concerns.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationDiagnostic {
    /// The element's identifier as written.
    pub element: String,
    /// What was found.
    pub kind: ValidationKind,
}

/// The §14.7 findings: fatal refusals, reported mutations, and advisories.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationKind {
    // ── Fatal ────────────────────────────────────────────────────────────
    /// Curve abscissae or series timestamps fail to increase.
    NonIncreasingCurve,
    /// See [`ValidationKind::NonIncreasingCurve`].
    NonIncreasingSeries,
    /// A parcel outlet name naming both a vertex and a parcel.
    AmbiguousParcelOutlet,
    /// Ground surface below the initial water table.
    GroundBelowWaterTable,
    /// Two gages share a series but declare different record forms.
    CoGageFormatConflict,
    /// A gage series also feeding temperature or evaporation.
    GageSeriesShared,
    /// A recording interval coarser than the series it reads.
    GageIntervalCoarserThanSeries,
    /// A unit-hydrograph response with a negative time to peak.
    NegativeTimeToPeak,
    /// A month's unit-hydrograph fractions summing above 1.01.
    ResponseFractionsAboveUnity,
    /// Treatment expressions that depend on each other's removals.
    CyclicTreatment,
    /// A channel or regulator with no cross-section.
    MissingCrossSection,
    /// Refused section geometry, with the §5 reason.
    BadSectionGeometry(&'static str),
    /// An orifice or weir whose section shape its form forbids.
    RegulatorShape,
    /// A storage vertex draining through a zero-geometry channel.
    StorageDummyOutflow,
    /// A non-positive channel length.
    BadLength,
    /// A non-positive channel roughness.
    BadRoughness,
    /// An initial depth above the maximum plus surcharge.
    InitDepthAboveMax,
    /// Negative integrated storage volume at full depth.
    NegativeStorageVolume,
    /// A pump curve that is not a pump curve, or missing.
    BadPumpCurve,
    /// A startup depth at or below the shutoff depth.
    BadPumpDepths,
    /// A divider whose diverted link is absent or unattached.
    DividerLinkDetached,
    /// Weir-divider parameters that cannot form a rating.
    BadWeirDivider,
    // ── Mutations, each applied and reported ─────────────────────────────
    /// A vertex's maximum depth raised to the crown of its highest
    /// connecting link (m).
    MaxDepthRaised {
        /// The new depth (m).
        to: f64,
    },
    /// A regulator crest below its downstream vertex's invert raised to
    /// that invert — unconditionally, where the predecessor applies it
    /// under dynamic wave only (§14.7).
    CrestRaised {
        /// The new offset above the upstream invert (m).
        to: f64,
    },
    /// A negative invert offset zeroed.
    NegativeOffsetZeroed,
    /// An elevation drop below the minimum treated as the minimum.
    NegligibleDrop,
    /// An elevation drop at or beyond the length: slope falls back to
    /// drop over length.
    DropExceedsLength,
    /// A slope floored at the minimum-slope option.
    SlopeFloored {
        /// The floor applied.
        to: f64,
    },
    /// An adverse-slope channel reversed internally; reported flows carry
    /// the direction multiplier.
    ChannelReversed,
    /// An infeasible bottom radius enlarged to its geometric minimum (m).
    RadiusRaised {
        /// The new radius (m).
        to: f64,
    },
    /// The wet-weather hydrology step reduced to a gage's finer recording
    /// interval (s).
    WetStepReduced {
        /// The new step (s).
        to: f64,
    },
    /// An inlet placed on a channel its design cannot serve, removed
    /// (§7.8): custom inlets need a diversion or rating curve, drop
    /// inlets a trapezoidal or open-rectangular channel, all others a
    /// street section.
    InletPlacementRemoved,
    // ── Advisories ───────────────────────────────────────────────────────
    /// A gage recording interval finer than its series' spacing.
    GageIntervalFinerThanSeries,
    /// A user-dimensioned ellipse, which the predecessor evaluated at
    /// fixed proportions regardless of the entered width (§5.4).
    UserDimensionedEllipse,
    /// A channel short enough to Courant-limit the run at full flow
    /// (§6.5): its cost is small steps, visible rather than hidden in
    /// the retired lengthening transform.
    StubChannel,
    /// A rule mixing `AND` and `OR` premises: firing may depend on the
    /// §9.1 precedence correction.
    RuleMixesAndOr,
    /// A sanitary-inflow pattern whose declared type does not match the
    /// slot it occupies — it contributes its own type's multiplier from
    /// wherever it sits (§14.7).
    DwfPatternSlotMismatch,
    /// A tidal outfall under a non-midnight start: this engine indexes
    /// the tide by clock time where the predecessor used elapsed time,
    /// so results differ (§14.7).
    TidalCurveClockIndexed,
}

impl ValidationKind {
    /// Whether this finding refuses the model rather than reporting on it.
    pub fn is_error(&self) -> bool {
        !matches!(
            self,
            ValidationKind::MaxDepthRaised { .. }
                | ValidationKind::CrestRaised { .. }
                | ValidationKind::NegativeOffsetZeroed
                | ValidationKind::NegligibleDrop
                | ValidationKind::DropExceedsLength
                | ValidationKind::SlopeFloored { .. }
                | ValidationKind::ChannelReversed
                | ValidationKind::RadiusRaised { .. }
                | ValidationKind::WetStepReduced { .. }
                | ValidationKind::InletPlacementRemoved
                | ValidationKind::GageIntervalFinerThanSeries
                | ValidationKind::UserDimensionedEllipse
                | ValidationKind::StubChannel
                | ValidationKind::RuleMixesAndOr
                | ValidationKind::DwfPatternSlotMismatch
                | ValidationKind::TidalCurveClockIndexed
        )
    }
}

fn push(d: &mut Vec<ValidationDiagnostic>, element: &str, kind: ValidationKind) {
    d.push(ValidationDiagnostic {
        element: element.to_string(),
        kind,
    });
}

/// Run the §14.7 pass over a parsed network, applying its mutations and
/// returning every finding — exhaustively; the predecessor's 100-error
/// reporting cap is not carried.
pub fn validate(net: &mut Network) -> Vec<ValidationDiagnostic> {
    let mut d = Vec::new();
    validate_tables(net, &mut d);
    validate_parcels(net, &mut d);
    validate_gages(net, &mut d);
    validate_hydrographs(net, &mut d);
    validate_treatment(net, &mut d);
    // Links before vertices: crown raising rewrites vertex depths that
    // the vertex checks then judge.
    let originals = vertex_depths(net);
    validate_links(net, &mut d);
    validate_vertices(net, &originals, &mut d);
    validate_inlets(net, &mut d);
    validate_rules(net, &mut d);
    validate_dwf(net, &mut d);
    validate_tidal(net, &mut d);
    d
}

/// Inlet placements against their channels (§7.8): invalid placements
/// are removed, each with a notice naming the channel.
fn validate_inlets(net: &mut Network, d: &mut Vec<ValidationDiagnostic>) {
    let mut removed = Vec::new();
    net.inlet_usage.retain(|u| {
        let design = &net.inlets[u.design];
        let shape = net.links[u.link].cross_section.as_ref().map(|x| x.shape);
        let ok = if let Some(c) = design.custom_curve {
            matches!(
                net.curves[c].kind,
                crate::model::CurveKind::Diversion | crate::model::CurveKind::Rating
            )
        } else if design.drop_grate || design.drop_curb {
            matches!(shape, Some(XsectShape::Trapezoidal | XsectShape::RectOpen))
        } else {
            shape == Some(XsectShape::Street)
        };
        if !ok {
            removed.push(net.links[u.link].id.clone());
        }
        ok
    });
    for id in removed {
        push(d, &id, ValidationKind::InletPlacementRemoved);
    }
}

/// Rules mixing `AND` and `OR` among their premises (§9.1 advisory).
fn validate_rules(net: &Network, d: &mut Vec<ValidationDiagnostic>) {
    for rule in &net.controls.rules {
        let (mut in_premises, mut has_and, mut has_or) = (false, false, false);
        for line in &rule.lines {
            let first = line
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if first.starts_with("IF") {
                in_premises = true;
            } else if first.starts_with("THEN") || first.starts_with("ELSE") {
                in_premises = false;
            } else if in_premises {
                if first.starts_with("AND") {
                    has_and = true;
                }
                if first.starts_with("OR") {
                    has_or = true;
                }
            }
        }
        if has_and && has_or {
            push(d, &rule.name, ValidationKind::RuleMixesAndOr);
        }
    }
}

/// Sanitary-inflow patterns judged against the slots they occupy.
fn validate_dwf(net: &Network, d: &mut Vec<ValidationDiagnostic>) {
    use crate::model::PatternKind;
    const SLOTS: [PatternKind; 4] = [
        PatternKind::Monthly,
        PatternKind::Daily,
        PatternKind::Hourly,
        PatternKind::Weekend,
    ];
    for dwf in &net.dry_weather {
        for (slot, pat) in dwf.patterns.iter().enumerate() {
            if let Some(p) = pat {
                if net.patterns[*p].kind != SLOTS[slot] {
                    push(
                        d,
                        &net.vertices[dwf.vertex].id,
                        ValidationKind::DwfPatternSlotMismatch,
                    );
                }
            }
        }
    }
}

/// Tidal outfalls under a non-midnight start (§14.7).
fn validate_tidal(net: &Network, d: &mut Vec<ValidationDiagnostic>) {
    if net.options.start_time == 0.0 {
        return;
    }
    for v in &net.vertices {
        if let VertexKind::Outfall {
            stage: OutfallStage::Tidal { .. },
            ..
        } = v.kind
        {
            push(d, &v.id, ValidationKind::TidalCurveClockIndexed);
        }
    }
}

/// A comparable scalar for a series timestamp (s).
fn series_instant(t: &SeriesTime) -> f64 {
    match t {
        SeriesTime::Elapsed(s) => *s,
        SeriesTime::Absolute { date, seconds } => {
            // Days from civil epoch, Howard Hinnant's algorithm.
            let y = i64::from(date.year) - i64::from(date.month <= 2);
            let era = if y >= 0 { y } else { y - 399 } / 400;
            let yoe = (y - era * 400) as f64;
            let m = f64::from(date.month);
            let doy = (153.0 * (m + if m > 2.0 { -3.0 } else { 9.0 }) + 2.0) / 5.0
                + f64::from(date.day)
                - 1.0;
            let doe = yoe * 365.0 + (yoe / 4.0).floor() - (yoe / 100.0).floor() + doy.floor();
            (era as f64 * 146_097.0 + doe) * 86_400.0 + seconds
        }
    }
}

fn validate_tables(net: &Network, d: &mut Vec<ValidationDiagnostic>) {
    for c in &net.curves {
        if c.points.windows(2).any(|w| w[1].0 <= w[0].0) {
            push(d, &c.id, ValidationKind::NonIncreasingCurve);
        }
    }
    for ts in &net.timeseries {
        if let TimeSeriesSource::Points(points) = &ts.source {
            let mut prev: Option<f64> = None;
            for p in points {
                let now = series_instant(&p.time);
                if let Some(p) = prev {
                    if now <= p {
                        push(d, &ts.id, ValidationKind::NonIncreasingSeries);
                        break;
                    }
                }
                prev = Some(now);
            }
        }
    }
}

fn validate_parcels(net: &Network, d: &mut Vec<ValidationDiagnostic>) {
    for p in &net.parcels {
        // An outlet name naming both a vertex and a parcel is ambiguous:
        // the parse resolved the vertex, but the file meant neither.
        if let crate::model::ParcelOutlet::Vertex(v) = p.outlet {
            let name = &net.vertices[v].id;
            if net.parcels.iter().any(|q| &q.id == name) {
                push(d, &p.id, ValidationKind::AmbiguousParcelOutlet);
            }
        }
        if let Some(gw) = &p.groundwater {
            let table = net.aquifers[gw.aquifer].water_table_elev;
            if gw.surface_elev < table {
                push(d, &p.id, ValidationKind::GroundBelowWaterTable);
            }
        }
    }
}

/// The minimum positive spacing of a series (s), if it has one.
fn series_min_interval(net: &Network, ts: usize) -> Option<f64> {
    let TimeSeriesSource::Points(points) = &net.timeseries[ts].source else {
        return None;
    };
    let mut min: Option<f64> = None;
    for w in points.windows(2) {
        let dt = series_instant(&w[1].time) - series_instant(&w[0].time);
        if dt > 0.0 && min.is_none_or(|m| dt < m) {
            min = Some(dt);
        }
    }
    min
}

fn validate_gages(net: &mut Network, d: &mut Vec<ValidationDiagnostic>) {
    use crate::model::{EvaporationSource, GageSource, TemperatureSource};
    let mut findings = Vec::new();
    for (j, g) in net.gages.iter().enumerate() {
        let GageSource::Series { series: ts } = g.source else {
            continue;
        };
        // A gage sharing a series with an earlier gage is its co-gage:
        // both must record the same form.
        if let Some(first) = net.gages[..j]
            .iter()
            .find(|h| matches!(h.source, GageSource::Series { series } if series == ts))
        {
            if first.form != g.form {
                findings.push((g.id.clone(), ValidationKind::CoGageFormatConflict));
            }
            continue;
        }
        // The series may serve no other consumer.
        let shared = matches!(net.climate.temperature, Some(TemperatureSource::Series(t)) if t == ts)
            || matches!(net.climate.evaporation, EvaporationSource::Series(t) if t == ts);
        if shared {
            findings.push((g.id.clone(), ValidationKind::GageSeriesShared));
        }
        if let Some(dt) = series_min_interval(net, ts) {
            let dt = dt.round();
            if dt > 0.0 && g.interval > dt {
                findings.push((g.id.clone(), ValidationKind::GageIntervalCoarserThanSeries));
            }
            if g.interval < dt {
                findings.push((g.id.clone(), ValidationKind::GageIntervalFinerThanSeries));
            }
        }
        // A gage finer than the wet step pulls the wet step down (§14.7).
        if g.interval < net.options.wet_step {
            net.options.wet_step = g.interval;
            findings.push((
                g.id.clone(),
                ValidationKind::WetStepReduced { to: g.interval },
            ));
        }
    }
    for (id, kind) in findings {
        push(d, &id, kind);
    }
}

fn validate_hydrographs(net: &Network, d: &mut Vec<ValidationDiagnostic>) {
    for uh in &net.unit_hydrographs {
        for month in uh.months.iter() {
            let mut r_sum = 0.0;
            for resp in month.iter().flatten() {
                if resp.t_peak < 0.0 {
                    push(d, &uh.id, ValidationKind::NegativeTimeToPeak);
                }
                r_sum += resp.r;
            }
            if r_sum > 1.01 {
                push(d, &uh.id, ValidationKind::ResponseFractionsAboveUnity);
                break;
            }
        }
    }
}

fn validate_treatment(net: &Network, d: &mut Vec<ValidationDiagnostic>) {
    // Per vertex: an edge from constituent p to q where p's expression
    // reads q's removal (`R_<name>`); a cycle is fatal.
    let n = net.constituents.len();
    let vertices: std::collections::BTreeSet<usize> =
        net.treatments.iter().map(|t| t.vertex).collect();
    for v in vertices {
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
        for t in net.treatments.iter().filter(|t| t.vertex == v) {
            if t.kind != TreatmentKind::Removal {
                continue;
            }
            let upper = t.expression.to_ascii_uppercase();
            for (q, c) in net.constituents.iter().enumerate() {
                if q != t.constituent && upper.contains(&format!("R_{}", c.id.to_ascii_uppercase()))
                {
                    edges[t.constituent].push(q);
                }
            }
        }
        // Depth-first cycle detection.
        let mut state = vec![0_u8; n]; // 0 new, 1 open, 2 done
        fn dfs(p: usize, edges: &[Vec<usize>], state: &mut [u8]) -> bool {
            state[p] = 1;
            for &q in &edges[p] {
                if state[q] == 1 || (state[q] == 0 && dfs(q, edges, state)) {
                    return true;
                }
            }
            state[p] = 2;
            false
        }
        for p in 0..n {
            if state[p] == 0 && dfs(p, &edges, &mut state) {
                push(d, &net.vertices[v].id, ValidationKind::CyclicTreatment);
                break;
            }
        }
    }
}

fn vertex_depths(net: &Network) -> Vec<f64> {
    net.vertices
        .iter()
        .map(|v| match &v.kind {
            VertexKind::Junction { max_depth, .. } | VertexKind::Storage { max_depth, .. } => {
                *max_depth
            }
            _ => 0.0,
        })
        .collect()
}

/// The mutable full-depth slot of a vertex, where one exists.
fn full_depth_mut(kind: &mut VertexKind) -> Option<&mut f64> {
    match kind {
        VertexKind::Junction { max_depth, .. } | VertexKind::Storage { max_depth, .. } => {
            Some(max_depth)
        }
        _ => None,
    }
}

fn surcharge_of(kind: &VertexKind) -> f64 {
    match kind {
        VertexKind::Junction {
            surcharge_depth, ..
        }
        | VertexKind::Storage {
            surcharge_depth, ..
        } => *surcharge_depth,
        _ => 0.0,
    }
}

/// Resolve one offset to a height above the vertex invert, per the file's
/// convention, warning on a zeroed negative (§14.7).
fn resolve_offset(
    offset: Offset,
    convention: LinkOffsets,
    invert: f64,
    is_pump: bool,
    id: &str,
    d: &mut Vec<ValidationDiagnostic>,
) -> f64 {
    if is_pump {
        return 0.0;
    }
    match (convention, offset) {
        (_, Offset::Missing) => 0.0,
        (LinkOffsets::Depth, Offset::Depth(h)) | (LinkOffsets::Depth, Offset::Elevation(h)) => {
            if h < 0.0 {
                push(d, id, ValidationKind::NegativeOffsetZeroed);
                0.0
            } else {
                h
            }
        }
        (LinkOffsets::Elevation, Offset::Elevation(e))
        | (LinkOffsets::Elevation, Offset::Depth(e)) => {
            let h = e - invert;
            if h >= 0.0 {
                h
            } else if h >= -MIN_DELTA_Z {
                // Within the minimum drop: zeroed without comment, as the
                // predecessor does.
                0.0
            } else {
                push(d, id, ValidationKind::NegativeOffsetZeroed);
                0.0
            }
        }
    }
}

/// Build the §5 section for a link, routing referents. Public because
/// the §6 router assembles the same geometry.
pub fn build_for_link(
    net: &Network,
    li: usize,
    len: f64,
) -> Option<Result<SectionBuild, BuildError>> {
    let xs = net.links[li].cross_section.as_ref()?;
    Some(match (xs.shape, xs.referent) {
        (XsectShape::Irregular, Some(XsectReferent::Transect(t))) => {
            build_transect_section(&net.transects[t])
        }
        (XsectShape::Street, Some(XsectReferent::Street(st))) => {
            build_street_section(&net.streets[st])
        }
        _ => {
            let curve = match xs.referent {
                Some(XsectReferent::Curve(c)) => Some(net.curves[c].points.as_slice()),
                _ => None,
            };
            build_section(xs.shape, xs.geom_user, len, curve)
        }
    })
}

fn validate_links(net: &mut Network, d: &mut Vec<ValidationDiagnostic>) {
    let convention = net.options.link_offsets;
    let len = if net.options.flow_units.is_us() {
        0.3048
    } else {
        1.0
    };
    let min_slope = net.options.min_slope;

    for li in 0..net.links.len() {
        let id = net.links[li].id.clone();
        let from = net.links[li].from;
        let to = net.links[li].to;
        let invert1 = net.vertices[from].invert;
        let invert2 = net.vertices[to].invert;

        // ── Section ─────────────────────────────────────────────────────
        let needs_section = matches!(
            net.links[li].kind,
            LinkKind::Channel { .. } | LinkKind::Orifice { .. } | LinkKind::Weir { .. }
        );
        let built = build_for_link(net, li, len);
        let section: Option<Section> = match built {
            Some(Ok(b)) => {
                if let Some(r) = b.radius_raised {
                    push(d, &id, ValidationKind::RadiusRaised { to: r });
                }
                Some(b.section)
            }
            Some(Err(BuildError::BadGeometry(why))) => {
                push(d, &id, ValidationKind::BadSectionGeometry(why));
                None
            }
            Some(Err(BuildError::Unsupported(why))) => {
                push(d, &id, ValidationKind::BadSectionGeometry(why));
                None
            }
            None => {
                if needs_section {
                    push(d, &id, ValidationKind::MissingCrossSection);
                }
                None
            }
        };
        let shape = net.links[li].cross_section.as_ref().map(|x| x.shape);

        // The user-dimensioned ellipse advisory (§5.4).
        if let Some(xs) = &net.links[li].cross_section {
            if matches!(xs.shape, XsectShape::HorizEllipse | XsectShape::VertEllipse)
                && xs.geom_user[1] > 0.0
                && xs.geom_user[2] == 0.0
            {
                push(d, &id, ValidationKind::UserDimensionedEllipse);
            }
        }

        // ── Regulator shape rules ───────────────────────────────────────
        match &net.links[li].kind {
            LinkKind::Orifice { .. }
                if !matches!(shape, Some(XsectShape::Circular | XsectShape::RectClosed)) =>
            {
                push(d, &id, ValidationKind::RegulatorShape);
            }
            LinkKind::Weir { form, .. } => {
                let ok = match form {
                    WeirForm::Transverse | WeirForm::SideFlow | WeirForm::Roadway => {
                        matches!(shape, Some(XsectShape::RectOpen))
                    }
                    WeirForm::VNotch => matches!(shape, Some(XsectShape::Triangular)),
                    WeirForm::Trapezoidal => matches!(shape, Some(XsectShape::Trapezoidal)),
                };
                if !ok {
                    push(d, &id, ValidationKind::RegulatorShape);
                }
            }
            _ => {}
        }

        // ── Offsets to heights above inverts ────────────────────────────
        let is_pump = matches!(net.links[li].kind, LinkKind::Pump { .. });
        match &mut net.links[li].kind {
            LinkKind::Channel {
                offset1, offset2, ..
            } => {
                let mut h1 = resolve_offset(*offset1, convention, invert1, false, &id, d);
                let mut h2 = resolve_offset(*offset2, convention, invert2, false, &id, d);
                // A sediment-filled invert sits above the pipe invert.
                if shape == Some(XsectShape::FilledCircular) {
                    if let Some(xs) = &net.links[li].cross_section {
                        h1 += xs.geom_user[1] * len;
                        h2 += xs.geom_user[1] * len;
                    }
                }
                let LinkKind::Channel {
                    offset1, offset2, ..
                } = &mut net.links[li].kind
                else {
                    unreachable!()
                };
                *offset1 = Offset::Depth(h1);
                *offset2 = Offset::Depth(h2);
            }
            LinkKind::Orifice { offset, .. }
            | LinkKind::Weir { offset, .. }
            | LinkKind::Outlet { offset, .. } => {
                let h = resolve_offset(*offset, convention, invert1, false, &id, d);
                set_link_offset1(&mut net.links[li].kind, h);
            }
            LinkKind::Pump { .. } => {
                let _ = is_pump;
            }
        }

        // ── Per-kind rules ──────────────────────────────────────────────
        match &net.links[li].kind {
            LinkKind::Channel {
                length, roughness, ..
            } => {
                if shape == Some(XsectShape::Dummy)
                    && matches!(net.vertices[from].kind, VertexKind::Storage { .. })
                {
                    push(
                        d,
                        &net.vertices[from].id.clone(),
                        ValidationKind::StorageDummyOutflow,
                    );
                }
                if *length <= 0.0 {
                    push(d, &id, ValidationKind::BadLength);
                }
                // Transect and street channels take the survey's roughness.
                let uses_survey_n =
                    matches!(shape, Some(XsectShape::Irregular | XsectShape::Street));
                if *roughness <= 0.0 && !uses_survey_n {
                    push(d, &id, ValidationKind::BadRoughness);
                }
            }
            LinkKind::Pump {
                curve,
                startup_depth,
                shutoff_depth,
                ..
            } => {
                if let Some(c) = curve {
                    if !matches!(
                        net.curves[*c].kind,
                        crate::model::CurveKind::Pump1
                            | crate::model::CurveKind::Pump2
                            | crate::model::CurveKind::Pump3
                            | crate::model::CurveKind::Pump4
                            | crate::model::CurveKind::Pump5
                    ) {
                        push(d, &id, ValidationKind::BadPumpCurve);
                    }
                }
                if *startup_depth > 0.0 && *startup_depth <= *shutoff_depth {
                    push(d, &id, ValidationKind::BadPumpDepths);
                }
            }
            _ => {}
        }

        // ── Regulator crest raising, unconditional (§14.7) ──────────────
        if matches!(
            net.links[li].kind,
            LinkKind::Orifice { .. } | LinkKind::Weir { .. } | LinkKind::Outlet { .. }
        ) {
            let h = link_offset1(&net.links[li].kind);
            if invert1 + h < invert2 {
                let new = invert2 - invert1;
                set_link_offset1(&mut net.links[li].kind, new);
                push(d, &id, ValidationKind::CrestRaised { to: new });
            }
        }

        // ── Crown raising with the exemption ladder ─────────────────────
        let exempt = matches!(net.links[li].kind, LinkKind::Pump { .. })
            || matches!(
                net.links[li].kind,
                LinkKind::Orifice {
                    orientation: OrificeOrientation::Bottom,
                    ..
                }
            );
        if !exempt {
            if let Some(sec) = &section {
                let y_full = sec.y_full();
                let h1 = link_offset1(&net.links[li].kind);
                raise_crown(net, from, h1 + y_full);
                if let LinkKind::Channel { offset2, .. } = &net.links[li].kind {
                    let Offset::Depth(h2) = offset2 else {
                        unreachable!()
                    };
                    let target = h2 + y_full;
                    raise_crown(net, to, target);
                }
            }
        }

        // ── Channel slope and adverse reversal ──────────────────────────
        if let LinkKind::Channel { length, .. } = &net.links[li].kind {
            let length = *length;
            if length > 0.0 && section.is_some() {
                let h1 = link_offset1(&net.links[li].kind);
                let LinkKind::Channel { offset2, .. } = &net.links[li].kind else {
                    unreachable!()
                };
                let Offset::Depth(h2) = *offset2 else {
                    unreachable!()
                };
                let elev1 = invert1 + h1;
                let elev2 = invert2 + h2;
                // Meander: the effective length is the shorter valley
                // length (§5.6).
                let eff_len = match (shape, &net.links[li].cross_section) {
                    (Some(XsectShape::Irregular), Some(xs)) => match xs.referent {
                        Some(XsectReferent::Transect(t)) => {
                            let m = net.transects[t].meander_factor;
                            if m > 0.0 {
                                length / m
                            } else {
                                length
                            }
                        }
                        _ => length,
                    },
                    _ => length,
                };
                let mut delta = (elev1 - elev2).abs();
                if delta < MIN_DELTA_Z {
                    push(d, &id, ValidationKind::NegligibleDrop);
                    delta = MIN_DELTA_Z;
                }
                let mut slope = if delta >= eff_len {
                    push(d, &id, ValidationKind::DropExceedsLength);
                    delta / eff_len
                } else {
                    delta / (eff_len * eff_len - delta * delta).sqrt()
                };
                if min_slope > 0.0 && slope < min_slope {
                    push(d, &id, ValidationKind::SlopeFloored { to: min_slope });
                    slope = min_slope;
                }
                // A stub channel: its full-flow Courant length exceeds
                // its own length, so it will limit the accuracy-driven
                // step (§6.5) at the configured routing step.
                if let Some(sec) = &section {
                    if sec.y_full() > 0.0 && sec.a_full() > 0.0 {
                        let LinkKind::Channel { roughness, .. } = &net.links[li].kind else {
                            unreachable!()
                        };
                        let n = if *roughness > 0.0 { *roughness } else { 0.013 };
                        let y_eff = if sec.is_closed() {
                            sec.y_full()
                        } else {
                            let w = sec.top_width(sec.y_full());
                            if w > 0.0 {
                                sec.a_full() / w
                            } else {
                                sec.y_full()
                            }
                        };
                        let v_full = sec.psi(sec.y_full()) * slope.sqrt() / (n * sec.a_full());
                        let courant_len = ((crate::hydraulics::GRAVITY * y_eff).sqrt() + v_full)
                            * net.options.routing_step;
                        if courant_len > eff_len {
                            push(d, &id, ValidationKind::StubChannel);
                        }
                    }
                }
                // Adverse: reverse the channel internally (§14.7).
                if elev1 < elev2 && shape != Some(XsectShape::Dummy) {
                    let link = &mut net.links[li];
                    std::mem::swap(&mut link.from, &mut link.to);
                    if let LinkKind::Channel {
                        offset1,
                        offset2,
                        reversed,
                        ..
                    } = &mut link.kind
                    {
                        std::mem::swap(offset1, offset2);
                        *reversed = !*reversed;
                    }
                    push(d, &id, ValidationKind::ChannelReversed);
                }
            }
        }
    }
}

fn link_offset1(kind: &LinkKind) -> f64 {
    let off = match kind {
        LinkKind::Channel { offset1, .. } => offset1,
        LinkKind::Orifice { offset, .. }
        | LinkKind::Weir { offset, .. }
        | LinkKind::Outlet { offset, .. } => offset,
        LinkKind::Pump { .. } => return 0.0,
    };
    match off {
        Offset::Depth(h) => *h,
        _ => 0.0,
    }
}

fn set_link_offset1(kind: &mut LinkKind, h: f64) {
    match kind {
        LinkKind::Channel { offset1, .. } => *offset1 = Offset::Depth(h),
        LinkKind::Orifice { offset, .. }
        | LinkKind::Weir { offset, .. }
        | LinkKind::Outlet { offset, .. } => *offset = Offset::Depth(h),
        LinkKind::Pump { .. } => {}
    }
}

/// Raise a vertex's full depth to a connecting link's crown, unless the
/// vertex is storage without a surcharge allowance (§14.7).
fn raise_crown(net: &mut Network, v: usize, crown: f64) {
    let kind = &mut net.vertices[v].kind;
    if matches!(kind, VertexKind::Storage { .. }) && surcharge_of(kind) <= 0.0 {
        return;
    }
    if let Some(depth) = full_depth_mut(kind) {
        if crown > *depth {
            *depth = crown;
        }
    }
}

/// Integrated storage volume at a given depth (m³).
fn storage_volume(net: &Network, geometry: &StorageGeometry, y: f64) -> f64 {
    match geometry {
        StorageGeometry::Functional {
            coeff,
            exponent,
            constant,
        } => constant * y + coeff * y.powf(exponent + 1.0) / (exponent + 1.0),
        StorageGeometry::Shape { a0, a1, a2, .. } => {
            a0 * y + a1 * y * y / 2.0 + a2 * y * y * y / 3.0
        }
        StorageGeometry::Tabular { curve } => {
            // Trapezoidal integration of the area curve, extended flat.
            let pts = &net.curves[*curve].points;
            let mut vol = 0.0;
            let mut y0 = 0.0;
            let mut a0 = pts.first().map_or(0.0, |p| p.1);
            for &(yy, aa) in pts {
                if yy <= 0.0 {
                    a0 = aa;
                    continue;
                }
                let y1 = yy.min(y);
                if y1 > y0 {
                    let a1 = a0 + (aa - a0) * (y1 - y0) / (yy - y0);
                    vol += 0.5 * (a0 + a1) * (y1 - y0);
                    y0 = y1;
                }
                a0 = aa;
                if y0 >= y {
                    break;
                }
            }
            if y > y0 {
                vol += a0 * (y - y0);
            }
            vol
        }
    }
}

fn validate_vertices(net: &Network, originals: &[f64], d: &mut Vec<ValidationDiagnostic>) {
    for (vi, v) in net.vertices.iter().enumerate() {
        match &v.kind {
            VertexKind::Junction {
                max_depth,
                init_depth,
                surcharge_depth,
                ..
            } => {
                // Warn only when a user-authored depth was overridden.
                if *max_depth > originals[vi] && originals[vi] > 0.0 {
                    push(d, &v.id, ValidationKind::MaxDepthRaised { to: *max_depth });
                }
                if *init_depth > max_depth + surcharge_depth {
                    push(d, &v.id, ValidationKind::InitDepthAboveMax);
                }
            }
            VertexKind::Storage {
                max_depth,
                init_depth,
                surcharge_depth,
                geometry,
                ..
            } => {
                if *max_depth > originals[vi] && originals[vi] > 0.0 {
                    push(d, &v.id, ValidationKind::MaxDepthRaised { to: *max_depth });
                }
                if *init_depth > max_depth + surcharge_depth {
                    push(d, &v.id, ValidationKind::InitDepthAboveMax);
                }
                if storage_volume(net, geometry, *max_depth) < 0.0 {
                    push(d, &v.id, ValidationKind::NegativeStorageVolume);
                }
            }
            VertexKind::Divider {
                diverted_link,
                rule,
                ..
            } => {
                match diverted_link {
                    Some(l) => {
                        if net.links[*l].from != vi && net.links[*l].to != vi {
                            push(d, &v.id, ValidationKind::DividerLinkDetached);
                        }
                    }
                    None => push(d, &v.id, ValidationKind::DividerLinkDetached),
                }
                if let crate::model::DividerRule::Weir {
                    min_flow,
                    max_depth,
                    coeff,
                } = rule
                {
                    if *max_depth <= 0.0 || *coeff <= 0.0 {
                        push(d, &v.id, ValidationKind::BadWeirDivider);
                    } else {
                        let q_max = coeff * max_depth.powf(1.5);
                        if *min_flow > q_max {
                            push(d, &v.id, ValidationKind::BadWeirDivider);
                        }
                    }
                }
            }
            VertexKind::Outfall { stage, .. } => {
                let _ = stage;
                let _: &OutfallStage = stage;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::objects::parse_network;

    fn validated(input: &str) -> (Network, Vec<ValidationDiagnostic>) {
        let (mut net, diags) = parse_network(input);
        assert!(
            !diags.iter().any(|d| d.kind.is_error()),
            "parse refused: {:?}",
            diags
                .iter()
                .filter(|d| d.kind.is_error())
                .collect::<Vec<_>>()
        );
        let v = validate(&mut net);
        (net, v)
    }

    fn has(
        v: &[ValidationDiagnostic],
        element: &str,
        pred: impl Fn(&ValidationKind) -> bool,
    ) -> bool {
        v.iter().any(|d| d.element == element && pred(&d.kind))
    }

    const BASE: &str = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  4
J2  98   4

[OUTFALLS]
O1  95  FREE

[CONDUITS]
C1  J1  J2  400  0.013  0  0
C2  J2  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0
C2  CIRCULAR  1.5  0  0  0
";

    #[test]
    fn a_clean_network_validates_clean() {
        let (_, v) = validated(BASE);
        assert!(
            v.iter().all(|d| !d.kind.is_error()),
            "{:?}",
            v.iter().filter(|d| d.kind.is_error()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn crown_raising_respects_the_exemption_ladder() {
        // J1's rim is 4 ft = 1.2192 m but C1's crown is offset 3 ft +
        // diameter 1.5 ft = 4.5 ft: raised, and warned by name.
        let inp = BASE.replace(
            "C1  J1  J2  400  0.013  0  0",
            "C1  J1  J2  400  0.013  3  0",
        );
        let (net, v) = validated(&inp);
        let j1 = &net.vertices[0];
        let VertexKind::Junction { max_depth, .. } = j1.kind else {
            panic!()
        };
        assert!((max_depth - 4.5 * 0.3048).abs() < 1e-12);
        assert!(has(&v, "J1", |k| matches!(
            k,
            ValidationKind::MaxDepthRaised { .. }
        )));
        // A zero-depth vertex is sized silently: O1 has no user depth.
        assert!(!has(&v, "O1", |k| matches!(
            k,
            ValidationKind::MaxDepthRaised { .. }
        )));
    }

    #[test]
    fn bottom_orifices_are_exempt_from_crown_raising() {
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  1

[OUTFALLS]
O1  95  FREE

[ORIFICES]
R1  J1  O1  BOTTOM  0  0.65  NO  0

[XSECTIONS]
R1  CIRCULAR  4  0  0  0
";
        let (net, v) = validated(inp);
        let VertexKind::Junction { max_depth, .. } = net.vertices[0].kind else {
            panic!()
        };
        // A 4 ft opening would out-top the 1 ft rim, but bottom orifices
        // are exempt.
        assert!((max_depth - 1.0 * 0.3048).abs() < 1e-12);
        assert!(v.iter().all(|d| !d.kind.is_error()), "{v:?}");
        // A side orifice raises it.
        let side = inp.replace("BOTTOM", "SIDE");
        let (net, v) = validated(&side);
        let VertexKind::Junction { max_depth, .. } = net.vertices[0].kind else {
            panic!()
        };
        assert!((max_depth - 4.0 * 0.3048).abs() < 1e-12);
        assert!(has(&v, "J1", |k| matches!(
            k,
            ValidationKind::MaxDepthRaised { .. }
        )));
    }

    #[test]
    fn regulator_crest_below_downstream_invert_is_raised() {
        // O1's invert (95) is above J2's crest at J2 invert 98 + 0 offset?
        // Use a weir from J2 (98) down to a junction at 99: crest below
        // downstream invert by 1 ft.
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  98  6
J2  99  6

[OUTFALLS]
O1  95  FREE

[CONDUITS]
C2  J2  O1  400  0.013  0  0

[XSECTIONS]
C2  CIRCULAR  1.5  0  0  0
W1  RECT_OPEN  2  4  0  0

[WEIRS]
W1  J1  J2  TRANSVERSE  0  3.3  NO  0  0  YES
";
        let (net, v) = validated(inp);
        let w1 = net.links.iter().find(|l| l.id == "W1").unwrap();
        let LinkKind::Weir { offset, .. } = &w1.kind else {
            panic!()
        };
        let Offset::Depth(h) = offset else { panic!() };
        assert!((h - 1.0 * 0.3048).abs() < 1e-12, "{h}");
        assert!(has(&v, "W1", |k| matches!(
            k,
            ValidationKind::CrestRaised { .. }
        )));
    }

    #[test]
    fn slopes_floor_fall_back_and_reverse() {
        // C1 runs uphill (J1 98 → J2 100): reversed, with the swap.
        let inp = BASE
            .replace("J1  100  4", "J1  98  4")
            .replace("J2  98   4", "J2  100  4");
        let (net, v) = validated(&inp);
        let c1 = net.links.iter().find(|l| l.id == "C1").unwrap();
        assert_eq!(net.vertices[c1.from].id, "J2");
        assert_eq!(net.vertices[c1.to].id, "J1");
        let LinkKind::Channel { reversed, .. } = c1.kind else {
            panic!()
        };
        assert!(reversed);
        assert!(has(&v, "C1", |k| matches!(
            k,
            ValidationKind::ChannelReversed
        )));

        // A flat channel gets the negligible-drop notice.
        let flat = BASE.replace("J2  98   4", "J2  100  4");
        let (_, v) = validated(&flat);
        assert!(has(&v, "C1", |k| matches!(
            k,
            ValidationKind::NegligibleDrop
        )));

        // A drop beyond the length falls back to Δz/L.
        let steep = BASE.replace(
            "C1  J1  J2  400  0.013  0  0",
            "C1  J1  J2  1  0.013  50  0",
        );
        let (_, v) = validated(&steep);
        assert!(has(&v, "C1", |k| matches!(
            k,
            ValidationKind::DropExceedsLength
        )));

        // MIN_SLOPE floors a mild grade, by name.
        let mild = BASE.replace(
            "[OPTIONS]\nFLOW_UNITS  CFS",
            "[OPTIONS]\nFLOW_UNITS  CFS\nMIN_SLOPE  2",
        );
        let (_, v) = validated(&mild);
        assert!(
            has(&v, "C1", |k| matches!(
                k,
                ValidationKind::SlopeFloored { .. }
            )),
            "{v:?}"
        );
    }

    #[test]
    fn negative_offsets_zero_with_a_notice() {
        let inp = BASE.replace(
            "C1  J1  J2  400  0.013  0  0",
            "C1  J1  J2  400  0.013  -1  0",
        );
        let (net, v) = validated(&inp);
        let c1 = net.links.iter().find(|l| l.id == "C1").unwrap();
        let LinkKind::Channel { offset1, .. } = c1.kind else {
            panic!()
        };
        assert_eq!(offset1, Offset::Depth(0.0));
        assert!(has(&v, "C1", |k| matches!(
            k,
            ValidationKind::NegativeOffsetZeroed
        )));
    }

    #[test]
    fn elevation_offsets_convert_per_the_option() {
        let inp = BASE
            .replace(
                "[OPTIONS]\nFLOW_UNITS  CFS",
                "[OPTIONS]\nFLOW_UNITS  CFS\nLINK_OFFSETS  ELEVATION",
            )
            .replace(
                "C1  J1  J2  400  0.013  0  0",
                "C1  J1  J2  400  0.013  101  *",
            );
        let (net, v) = validated(&inp);
        let c1 = net.links.iter().find(|l| l.id == "C1").unwrap();
        let LinkKind::Channel {
            offset1, offset2, ..
        } = c1.kind
        else {
            panic!()
        };
        // 101 ft elevation over a 100 ft invert = 1 ft height.
        let Offset::Depth(h1) = offset1 else { panic!() };
        assert!((h1 - 0.3048).abs() < 1e-9);
        // '*' resolves to the invert.
        assert_eq!(offset2, Offset::Depth(0.0));
        assert!(v.iter().all(|d| !d.kind.is_error()));
    }

    #[test]
    fn init_depth_above_max_is_fatal_after_raising() {
        let inp = BASE.replace("J1  100  4", "J1  100  4  6");
        let (_, v) = validated(&inp);
        assert!(has(&v, "J1", |k| matches!(
            k,
            ValidationKind::InitDepthAboveMax
        )));
    }

    #[test]
    fn storage_and_divider_rules() {
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS
FLOW_ROUTING KINWAVE

[JUNCTIONS]
J1  100  4

[STORAGE]
SU1  90  10  0  FUNCTIONAL  -100  1  200

[DIVIDERS]
D1  95  C1  WEIR  2  0  3.3  4

[OUTFALLS]
O1  85  FREE

[CONDUITS]
C1  J1  SU1  300  0.013  0  0
C2  SU1  D1  300  0.013  0  0
C3  D1  O1  300  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0
C2  CIRCULAR  1.5  0  0  0
C3  CIRCULAR  1.5  0  0  0
";
        let (_, v) = validated(inp);
        // Storage: ∫(200 − 100·y) dy goes negative by 10 ft — the
        // predecessor refuses a negative constant at read but accepts a
        // negative coefficient, leaving the integral to validation.
        assert!(has(&v, "SU1", |k| matches!(
            k,
            ValidationKind::NegativeStorageVolume
        )));
        // Divider: C1 does not touch D1; weir dhMax = 0.
        assert!(has(&v, "D1", |k| matches!(
            k,
            ValidationKind::DividerLinkDetached
        )));
        assert!(has(&v, "D1", |k| matches!(
            k,
            ValidationKind::BadWeirDivider
        )));
    }

    #[test]
    fn gage_rules_and_wet_step_reduction() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CFS
INFILTRATION  HORTON
WET_STEP      0:05:00

[RAINGAGES]
G1  INTENSITY  0:01  1.0  TIMESERIES  TS1
G2  VOLUME     1:00  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  O1  10  25  500  0.5  0
S2  G2  O1  10  25  500  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET
S2  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0
S2  3.0  0.5  4  7  0

[OUTFALLS]
O1  95  FREE

[TIMESERIES]
TS1  0:00  1.0
TS1  0:10  2.0
TS1  0:20  0.0
";
        let (net, v) = validated(inp);
        // G2 shares TS1 with G1 but records a different form.
        assert!(has(&v, "G2", |k| matches!(
            k,
            ValidationKind::CoGageFormatConflict
        )));
        // G1 records at 1 min against a 10 min series: finer (advisory),
        // and pulls the 5 min wet step down to 60 s.
        assert!(has(&v, "G1", |k| matches!(
            k,
            ValidationKind::GageIntervalFinerThanSeries
        )));
        assert!(has(&v, "G1", |k| matches!(
            k,
            ValidationKind::WetStepReduced { .. }
        )));
        assert!((net.options.wet_step - 60.0).abs() < 1e-9);
        // A gage coarser than its series is fatal.
        let coarse = inp.replace("G1  INTENSITY  0:01", "G1  INTENSITY  1:00");
        let (_, v) = validated(&coarse);
        assert!(has(&v, "G1", |k| matches!(
            k,
            ValidationKind::GageIntervalCoarserThanSeries
        )));
    }

    #[test]
    fn shared_series_ambiguous_outlet_and_ground_water_table() {
        let inp = "\
[OPTIONS]
FLOW_UNITS    CFS
INFILTRATION  HORTON

[RAINGAGES]
G1  INTENSITY  0:10  1.0  TIMESERIES  TS1

[TEMPERATURE]
TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  S2  10  25  500  0.5  0
S2  G1  J1  10  25  500  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET
S2  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0
S2  3.0  0.5  4  7  0

[AQUIFERS]
AQ1  0.5  0.35  0.20  0.10  10  2.0  1.5  14  3.5  80  96  0.30

[GROUNDWATER]
S2  AQ1  J1  95  0.01  1.5  0  0  0  0  0

[JUNCTIONS]
J1  90  4

[OUTFALLS]
O1  88  FREE

[CONDUITS]
C1  J1  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0

[TIMESERIES]
TS1  0:00  1.0
TS1  1:00  0.0
";
        // Rename J1 to S1 upstream? Instead: make an outlet name ambiguous
        // by giving a junction the same name as a parcel.
        let ambiguous = inp.replace(
            "[JUNCTIONS]\nJ1  90  4",
            "[JUNCTIONS]\nJ1  90  4\nS2  89  4",
        );
        let (_, v) = validated(&ambiguous);
        // S1 drains to \"S2\", which is now both a vertex and a parcel.
        assert!(has(&v, "S1", |k| matches!(
            k,
            ValidationKind::AmbiguousParcelOutlet
        )));
        let (_, v) = validated(inp);
        // TS1 feeds both G1 and temperature.
        assert!(has(&v, "G1", |k| matches!(
            k,
            ValidationKind::GageSeriesShared
        )));
        // Ground 95 sits below the water table 96.
        assert!(has(&v, "S2", |k| matches!(
            k,
            ValidationKind::GroundBelowWaterTable
        )));
    }

    #[test]
    fn cyclic_treatment_and_bad_tables_are_fatal() {
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  4

[OUTFALLS]
O1  95  FREE

[CONDUITS]
C1  J1  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0

[POLLUTANTS]
TSS   MG/L  0  0  0  0
LEAD  MG/L  0  0  0  0

[TREATMENT]
J1  TSS   R = 0.2 * R_LEAD
J1  LEAD  R = 0.5 * R_TSS

[CURVES]
BAD1  STORAGE  5  100  5  200

[TIMESERIES]
TS2  1:00  1.0
TS2  0:30  2.0
";
        let (_, v) = validated(inp);
        assert!(has(&v, "J1", |k| matches!(
            k,
            ValidationKind::CyclicTreatment
        )));
        assert!(has(&v, "BAD1", |k| matches!(
            k,
            ValidationKind::NonIncreasingCurve
        )));
        assert!(has(&v, "TS2", |k| matches!(
            k,
            ValidationKind::NonIncreasingSeries
        )));
    }

    #[test]
    fn hydrograph_bounds_are_fatal() {
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  TS1

[JUNCTIONS]
J1  100  4

[OUTFALLS]
O1  95  FREE

[CONDUITS]
C1  J1  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT   0.5  1.0  2.0
UH1  ALL  MEDIUM  0.4  3.0  2.0
UH1  ALL  LONG    0.2  5.0  2.0

[RDII]
J1  UH1  12.5

[TIMESERIES]
TS1  0:00  1.0
TS1  1:00  0.0
";
        let (_, v) = validated(inp);
        // 0.5 + 0.4 + 0.2 = 1.1 > 1.01.
        assert!(has(&v, "UH1", |k| matches!(
            k,
            ValidationKind::ResponseFractionsAboveUnity
        )));
    }

    #[test]
    fn invalid_inlet_placements_are_removed_with_notice() {
        // CB1 rides a street (valid); moving it to a circular sewer is
        // not, and the placement goes away.
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1   100  4
SEW  90   8

[OUTFALLS]
O1  95  FREE
O2  85  FREE

[CONDUITS]
GUT1  J1   O1  300  0.016  0  0
SEW1  SEW  O2  300  0.013  0  0

[XSECTIONS]
GUT1  STREET    ST1
SEW1  CIRCULAR  1.5  0  0  0

[STREETS]
ST1  20  0.5  2  0.016  0  0  1

[INLETS]
CB1  GRATE  2  2  P_BAR-50

[INLET_USAGE]
GUT1  CB1  SEW
";
        let (net, v) = validated(inp);
        assert_eq!(net.inlet_usage.len(), 1);
        assert!(v
            .iter()
            .all(|d| !matches!(d.kind, ValidationKind::InletPlacementRemoved)));
        let moved = inp.replace("GUT1  CB1  SEW", "SEW1  CB1  SEW");
        let (net, v) = validated(&moved);
        assert!(net.inlet_usage.is_empty());
        assert!(has(&v, "SEW1", |k| matches!(
            k,
            ValidationKind::InletPlacementRemoved
        )));
    }

    #[test]
    fn remaining_advisories_fire() {
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS
START_TIME  06:00

[JUNCTIONS]
J1  100  4

[OUTFALLS]
O1  95  TIDAL  TC1

[CONDUITS]
C1  J1  O1  1  0.013  4  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0

[CURVES]
TC1  TIDAL  0  2  6  3  12  2

[PATTERNS]
HR1  HOURLY  1 1 1 1 1 1 1 1 1 1 1 1
HR1          1 1 1 1 1 1 1 1 1 1 1 1

[DWF]
J1  FLOW  0.02  HR1

[CONTROLS]
RULE  MIXED
IF    NODE J1 DEPTH > 2
AND   NODE J1 DEPTH < 4
OR    SIMULATION TIME > 1:00
THEN  CONDUIT C1 STATUS = CLOSED
";
        let (_, v) = validated(inp);
        // A 1 ft channel with a 20 s routing step is a stub.
        assert!(has(&v, "C1", |k| matches!(k, ValidationKind::StubChannel)));
        // The hourly pattern sits in the monthly slot.
        assert!(has(&v, "J1", |k| matches!(
            k,
            ValidationKind::DwfPatternSlotMismatch
        )));
        // AND and OR mix among the premises.
        assert!(has(&v, "MIXED", |k| matches!(
            k,
            ValidationKind::RuleMixesAndOr
        )));
        // A tidal outfall under an 06:00 start.
        assert!(has(&v, "O1", |k| matches!(
            k,
            ValidationKind::TidalCurveClockIndexed
        )));
        // All of these are advisories, none fatal.
        assert!(v.iter().all(|d| !d.kind.is_error()), "{v:?}");
        // A long channel is no stub, and a midnight start quiets the tide.
        let quiet = inp
            .replace(
                "C1  J1  O1  1  0.013  4  0",
                "C1  J1  O1  1000  0.013  4  0",
            )
            .replace("START_TIME  06:00", "START_TIME  00:00");
        let (_, v) = validated(&quiet);
        assert!(!has(&v, "C1", |k| matches!(k, ValidationKind::StubChannel)));
        assert!(!has(&v, "O1", |k| matches!(
            k,
            ValidationKind::TidalCurveClockIndexed
        )));
    }

    #[test]
    fn regulator_shape_rules_are_fatal() {
        let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  6

[OUTFALLS]
O1  95  FREE

[ORIFICES]
R1  J1  O1  SIDE  0.5  0.65  NO  0

[XSECTIONS]
R1  TRAPEZOIDAL  2  3  1  1
";
        let (_, v) = validated(inp);
        assert!(has(&v, "R1", |k| matches!(
            k,
            ValidationKind::RegulatorShape
        )));
    }
}
