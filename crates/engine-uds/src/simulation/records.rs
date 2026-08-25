//! Run-boundary records (§14.8): the data the session exchanges with
//! interface files, as pure structures.
//!
//! These are engine types — what the session holds and integrates —
//! not parsing. The §14.8 grammars that read and write the
//! predecessor's interface files into them live with the dialect
//! tooling (format-blind extraction, phase 3a).

/// Encoding tolerance for record instants (s): interface files carry
/// dates as text, and a millisecond is far above the encoding's noise
/// and far below any step a model would use.
const DATE_TOL: f64 = 1e-3;

/// How far outside its own span (s) a routing interface file is still
/// served (§14.8). The instants compared are absolute epoch seconds, near
/// 1.6e9 by now, where consecutive representable values are already a
/// quarter of a microsecond apart: a tolerance finer than that rounds away
/// to an exact comparison, and a clock that reaches the file's last instant
/// by accumulating steps rather than by the same arithmetic then loses that
/// period's inflow. A millisecond is far above the spacing and far below
/// any routing step.
const ROUTING_SPAN_TOLERANCE: f64 = 1.0e-3;

/// §14.8: the interface files' declared flow-unit words and their
/// m³/s factors — shared with the dialect tooling that parses them.
pub const FLOW_WORDS: [(&str, f64); 6] = [
    ("CFS", 0.028_316_846_592),
    ("GPM", 6.309_019_64e-5),
    ("MGD", 0.043_812_636_4),
    ("CMS", 1.0),
    ("LPS", 1.0e-3),
    ("MLD", 1.0 / 86.4),
];

/// A parsed routing interface file, resolved against a model.
#[derive(Debug)]
pub struct RoutingInterface {
    /// The file's reporting step (s).
    pub step: f64,
    /// Receiving vertex per file node column (`None` = unknown vertex,
    /// carried but unused).
    pub vertices: Vec<Option<usize>>,
    /// Model constituent per file constituent column (§14.8: unmatched
    /// pollutants read as zero).
    pub constituents: Vec<Option<usize>>,
    /// m³/s per file flow unit.
    pub flow_cv: f64,
    /// Dated records: (epoch s, per-file-node rows of `flow, qual…` in
    /// file units).
    pub records: Vec<(f64, Vec<Vec<f64>>)>,
}

impl RoutingInterface {
    /// The interpolated (flow m³/s, per-model-constituent concentration)
    /// additions at epoch `t`, per resolved vertex.
    pub fn inflows_at(&self, epoch: f64, np: usize) -> Vec<(usize, f64, Vec<f64>)> {
        let mut out = Vec::new();
        if self.records.is_empty() {
            return out;
        }
        // Bracketing records: the series' own span is served inclusive
        // of both end instants, nothing beyond (§14.8).
        let last = self.records.len() - 1;
        let first_t = self.records[0].0;
        let last_t = self.records[last].0;
        if epoch < first_t - ROUTING_SPAN_TOLERANCE || epoch > last_t + ROUTING_SPAN_TOLERANCE {
            return out;
        }
        if self.records.len() == 1 {
            let (_, rows) = &self.records[0];
            for (col, v) in self.vertices.iter().enumerate() {
                let Some(vi) = v else { continue };
                let a = &rows[col];
                let q = a.first().copied().unwrap_or(0.0) * self.flow_cv;
                let mut conc = vec![0.0; np];
                for (fc, m) in self.constituents.iter().enumerate() {
                    if let Some(p) = m {
                        conc[*p] = a.get(fc + 1).copied().unwrap_or(0.0);
                    }
                }
                out.push((*vi, q, conc));
            }
            return out;
        }
        let e = epoch.clamp(first_t, last_t);
        // `e` is clamped into the span and the last record's instant is its
        // end, so the search always finds an upper bracket; the first record
        // is never one, so the index is at least one. Both bounds are the
        // `last` computed above rather than the arithmetic repeated, so a
        // mistake in it has one place to be made and shows up in the span.
        let i = self
            .records
            .iter()
            .position(|(t, _)| *t >= e)
            .unwrap_or(last)
            .clamp(1, last);
        let (t0, r0) = &self.records[i - 1];
        let (t1, r1) = &self.records[i];
        let f = if t1 > t0 {
            (epoch - t0) / (t1 - t0)
        } else {
            1.0
        };
        for (col, v) in self.vertices.iter().enumerate() {
            let Some(vi) = v else { continue };
            let (a, b) = (&r0[col], &r1[col]);
            let at = |r: &Vec<f64>, i: usize| r.get(i).copied().unwrap_or(0.0);
            let q = ((1.0 - f) * at(a, 0) + f * at(b, 0)) * self.flow_cv;
            let mut conc = vec![0.0; np];
            for (fc, m) in self.constituents.iter().enumerate() {
                if let Some(p) = m {
                    conc[*p] = (1.0 - f) * at(a, fc + 1) + f * at(b, fc + 1);
                }
            }
            out.push((*vi, q, conc));
        }
        out
    }
}

/// A parsed RDII interface file, resolved against a model.
#[derive(Debug)]
pub struct RdiiInterface {
    /// The file's step (s): how long each record's flows apply.
    pub step: f64,
    /// Model vertex per file column, in the file's own column order.
    pub vertices: Vec<usize>,
    /// Dated records: (epoch s, flow per column in m³/s).
    pub records: Vec<(f64, Vec<f64>)>,
}

impl RdiiInterface {
    /// The (vertex, flow m³/s) additions at epoch `t`.
    ///
    /// Piecewise constant, never interpolated: a record's flows apply from
    /// its own instant until `step` later, and the hydrograph is zero
    /// before the first record, after the last, and in any gap between
    /// them. An RDII hydrograph is a volume already apportioned to a step,
    /// so interpolating would move water between the steps the unit
    /// hydrographs put it in.
    pub fn inflows_at(&self, epoch: f64) -> Vec<(usize, f64)> {
        // The last record at or before `epoch`, within the encoding's own
        // precision. Records are written in time order and read in it.
        let target = epoch + DATE_TOL;
        let idx = match self
            .records
            .binary_search_by(|(t, _)| t.partial_cmp(&target).unwrap_or(std::cmp::Ordering::Less))
        {
            Ok(i) => i,
            Err(0) => return Vec::new(),
            Err(i) => i - 1,
        };
        let (t, flows) = &self.records[idx];
        if epoch >= *t + self.step - DATE_TOL {
            return Vec::new();
        }
        self.vertices
            .iter()
            .zip(flows)
            .map(|(v, q)| (*v, *q))
            .collect()
    }
}

/// m³/s per unit of a model's declared flow unit.
///
/// Matched by name rather than by position: the two lists happen to be in
/// the same order today, and a reordering of either would otherwise
/// silently rescale every flow read from a binary file.
pub fn flow_cv_of(units: crate::model::options::FlowUnits) -> f64 {
    use crate::model::options::FlowUnits::*;
    let word = match units {
        Cfs => "CFS",
        Gpm => "GPM",
        Mgd => "MGD",
        Cms => "CMS",
        Lps => "LPS",
        Mld => "MLD",
    };
    FLOW_WORDS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, cv)| *cv)
        .unwrap_or(1.0)
}

/// One parcel's replayed results for one step, in the file's own order.
///
/// Values are as the file holds them, in the writing model's user units,
/// and are not converted here: the flow unit is checked on parse, so the
/// units are this model's, but which physical unit each field carries
/// depends on the quantity and is the caller's to apply (§14.8.2).
#[derive(Debug, Clone)]
pub struct ParcelReplay {
    /// Rainfall intensity.
    pub rainfall: f64,
    /// Snow depth.
    pub snow_depth: f64,
    /// Evaporation loss.
    pub evap: f64,
    /// Infiltration loss.
    pub infil: f64,
    /// Runoff flow, in the file's flow unit.
    pub runoff: f64,
    /// Groundwater flow to the vertex, in the file's flow unit.
    pub gw_flow: f64,
    /// Saturated groundwater elevation.
    pub gw_elev: f64,
    /// Soil moisture (dimensionless).
    pub soil_moisture: f64,
    /// Washoff concentration per constituent.
    pub washoff: Vec<f64>,
}

impl ParcelReplay {
    /// This record in the engine's own units.
    ///
    /// The file holds the writing model's user units, and which unit each
    /// field carries differs by quantity: rainfall and infiltration are
    /// depth per *hour* where evaporation is depth per *day*, and the
    /// depth itself is inches or millimetres by the model's system. These
    /// invert `out_writer`'s conversions exactly, since the predecessor
    /// writes one result vector to both files.
    pub fn to_si(&self, us: bool, flow_cv: f64) -> ParcelReplay {
        let depth = if us { 0.0254 } else { 1.0e-3 };
        let length = if us { 0.3048 } else { 1.0 };
        ParcelReplay {
            rainfall: self.rainfall * depth / 3600.0,
            snow_depth: self.snow_depth * depth,
            evap: self.evap * depth / 86_400.0,
            infil: self.infil * depth / 3600.0,
            runoff: self.runoff * flow_cv,
            gw_flow: self.gw_flow * flow_cv,
            gw_elev: self.gw_elev * length,
            soil_moisture: self.soil_moisture,
            washoff: self.washoff.clone(),
        }
    }

    /// This record in the writing model's user units, the inverse of
    /// [`ParcelReplay::to_si`].
    ///
    /// A file is written in the units of the model that wrote it, so this
    /// is what a writer applies on the way out. The two directions are
    /// stated separately rather than one deriving from the other: a single
    /// shared table would round-trip perfectly while being wrong in both
    /// directions, and the units are pinned against the predecessor's own
    /// conversion factors in the tests.
    pub fn from_si(&self, us: bool, flow_cv: f64) -> ParcelReplay {
        let depth = if us { 0.0254 } else { 1.0e-3 };
        let length = if us { 0.3048 } else { 1.0 };
        ParcelReplay {
            rainfall: self.rainfall * 3600.0 / depth,
            snow_depth: self.snow_depth / depth,
            evap: self.evap * 86_400.0 / depth,
            infil: self.infil * 3600.0 / depth,
            runoff: self.runoff / flow_cv,
            gw_flow: self.gw_flow / flow_cv,
            gw_elev: self.gw_elev / length,
            soil_moisture: self.soil_moisture,
            washoff: self.washoff.clone(),
        }
    }
}

/// A parsed runoff interface file, checked against a model.
#[derive(Debug)]
pub struct RunoffInterface {
    /// Steps: `(step length s, per-parcel results in model order)`.
    pub steps: Vec<(f64, Vec<ParcelReplay>)>,
}

/// One station's cached record (§14.8.3).
#[derive(Debug, Clone, PartialEq)]
pub struct RainGageRecord {
    /// The recording station's identifier, which is what a gage is
    /// matched by. Not the gage's own name.
    pub station: String,
    /// The gage's recording interval (s).
    pub interval: f64,
    /// Readings as `(decimal day, depth in inches over the interval)`.
    /// Zero depths are present and mean a dry interval, not a gap.
    pub readings: Vec<(f64, f64)>,
}

/// A parsed rainfall interface file.
#[derive(Debug, Clone, PartialEq)]
pub struct RainInterface {
    /// The stations the file caches, in the order it holds them.
    pub gages: Vec<RainGageRecord>,
}

impl RainInterface {
    /// The record for a station, compared without case as every other
    /// identifier in this engine is.
    pub fn station(&self, station: &str) -> Option<&RainGageRecord> {
        self.gages
            .iter()
            .find(|g| g.station.eq_ignore_ascii_case(station))
    }
}

/// One reading: a station's value for the recording interval starting at
/// the stamped minute (§3.1 of the hydrology specification).
#[derive(Debug, Clone, PartialEq)]
pub struct RainReading {
    /// Station identifier as written.
    pub station: String,
    /// Calendar date of the reading.
    pub date: crate::model::options::Date,
    /// Seconds past that date's midnight.
    pub seconds: f64,
    /// The value, in the record's own declared unit, meaning whatever the
    /// gage's form declares (intensity, volume, cumulative).
    pub value: f64,
}

/// A supplied rain file, in whichever form it turned out to be.
///
/// A caller does not declare which: the layouts are recognised from the
/// file's own opening lines, as the predecessor recognises them, so a
/// modeller who swaps a station export for an archive changes nothing but
/// the file.
#[derive(Debug, Clone, PartialEq)]
pub enum RainRecords {
    /// The user-prepared station format (§14.12), read in the record's own
    /// declared unit and meaning whatever the gage's form declares.
    Station(Vec<RainReading>),
    /// An archival station record (§14.12.1), already normalised to depths
    /// in inches over the interval the file declares.
    Archive(RainGageRecord),
}
