//! Writing a collection element's contents, for whichever engine holds
//! the model.
//!
//! The §4.5.2.2 operation. A curve is its points, a pattern its
//! multipliers, a time series its values — a table whose *length* is part
//! of what the modeller is authoring, which is why none of it is an
//! attribute and why it took a section of its own.
//!
//! **The whole table goes at once.** Not a row inserted, a row removed, a
//! cell set. The rows are ordered and interdependent — a curve's abscissae
//! must ascend, a pattern's multipliers are a cycle whose length is its
//! period — so half the useful edits are illegal midway through: adding a
//! point before sorting it into place makes a curve that is briefly wrong.
//! One write is one validation, and its inverse is the table that was
//! there, which is what undo needs and what a sequence of refused row
//! operations cannot give.
//!
//! Numbers arrive in the unit each column declared on the way out, the
//! same rule an attribute follows. For a curve that unit depends on what
//! the curve is *for*, and the engines answer that themselves — this
//! module never names one.

use super::network_dto::NetworkState;
use super::projects::{app_data_dir, project_engine_key, validate_target_ids};

/// Replace the contents of one collection element.
///
/// Refuses, changing nothing, for a kind whose contents this path cannot
/// take — a control rule is language, and taking it back means parsing it
/// with the engine's own model reader.
#[tauri::command(async)]
pub fn set_collection_contents(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    kind: String,
    element_id: String,
    rows: Vec<Vec<f64>>,
) -> Result<(), String> {
    validate_target_ids(&project_id, None)?;
    let app_data = app_data_dir(&app)?;
    if rows.iter().flatten().any(|v| !v.is_finite()) {
        return Err("every value has to be a number".into());
    }
    match project_engine_key(&app_data, &project_id).as_str() {
        "uds" => super::mutations::mutate_uds(&app, &state, &project_id, |network| {
            set_uds_contents(network, &kind, &element_id, &rows)
        }),
        "wds" => super::mutations::mutate_wds(&app, &state, &project_id, |network| {
            set_wds_contents(network, &kind, &element_id, &rows)
        }),
        other => Err(super::projects::unknown_engine(other)),
    }
}

/// The refusal for a shape this path does not take.
fn not_tabular(kind: &str) -> String {
    format!("a {kind}'s contents cannot be edited here")
}

/// The two-column check every tabular container shares, and the message
/// it gives — worded once so the engines cannot come to disagree.
fn pairs(rows: &[Vec<f64>], least: usize) -> Result<Vec<(f64, f64)>, String> {
    if rows.len() < least {
        return Err(format!("that needs at least {least} rows"));
    }
    rows.iter()
        .map(|r| match r.as_slice() {
            [x, y] => Ok((*x, *y)),
            other => Err(format!("every row takes 2 values, got {}", other.len())),
        })
        .collect()
}

/// The second column alone, for a container whose first is a row number
/// the reader supplied so the table would be countable.
fn factors(rows: &[Vec<f64>]) -> Result<Vec<f64>, String> {
    Ok(pairs(rows, 1)?.into_iter().map(|(_, f)| f).collect())
}

pub(crate) fn set_uds_contents(
    net: &mut hydra::uds::model::Network,
    kind: &str,
    id: &str,
    rows: &[Vec<f64>],
) -> Result<(), String> {
    match kind {
        "curve" => {
            // Two points at least: a curve of one is a value, and every
            // evaluation of it would be an extrapolation.
            let points = pairs(rows, 2)?;
            let curve = net
                .curves
                .iter_mut()
                .find(|c| c.id.eq_ignore_ascii_case(id))
                .ok_or_else(|| format!("no curve '{id}'"))?;
            ascending(&points)?;
            curve.points = points;
            Ok(())
        }
        "transect" => {
            // Two points at least: a section of one station has no width
            // and conveys nothing.
            let points = pairs(rows, 2)?;
            let transect = net
                .transects
                .iter_mut()
                .find(|t| t.id.eq_ignore_ascii_case(id))
                .ok_or_else(|| format!("no transect '{id}'"))?;
            // Stations must advance across the section, which is the
            // same rule a curve's abscissae follow — except the station
            // is the *second* value here, not the first.
            for w in points.windows(2) {
                if w[1].1 <= w[0].1 {
                    return Err(format!(
                        "a transect's stations have to increase: {} does not follow {}",
                        w[1].1, w[0].1
                    ));
                }
            }
            transect.stations = points;
            Ok(())
        }
        "pattern" => {
            let factors = factors(rows)?;
            let pattern = net
                .patterns
                .iter_mut()
                .find(|p| p.id.eq_ignore_ascii_case(id))
                .ok_or_else(|| format!("no pattern '{id}'"))?;
            // The period bounds the table, and this is the same rule the
            // type write enforces from the other side — a pattern may
            // carry fewer multipliers than its period has slots, never
            // more, because a multiplier past the end of the period stops
            // meaning anything without anyone being told. Enforced on
            // both doors or on neither: one door alone is a model that
            // reaches a state its own rule forbids.
            let want = super::uds_attrs::pattern_period(pattern.kind);
            if factors.len() > want {
                return Err(format!(
                    "'{id}' repeats every {want} multipliers and this is {}",
                    factors.len()
                ));
            }
            pattern.factors = factors;
            Ok(())
        }
        "timeseries" => {
            // One reading at least: the file writer emits a line per
            // point, so a series written empty would vanish at the next
            // save — the same reason a new one is created with two.
            let readings = pairs(rows, 1)?;
            let series = net
                .timeseries
                .iter_mut()
                .find(|t| t.id.eq_ignore_ascii_case(id))
                .ok_or_else(|| format!("no time series '{id}'"))?;
            // The two shapes the read serves as something other than this
            // table stay unwritable from this side too. Dated readings
            // render as text because a date cannot go in the numeric time
            // column, and an external series' values live in a file this
            // crate never reads — accepting rows for either would quietly
            // replace what the reader was just shown with something else.
            match &series.source {
                hydra::uds::model::TimeSeriesSource::External { file } => {
                    return Err(format!("'{id}' is read from '{file}', not from the model"));
                }
                hydra::uds::model::TimeSeriesSource::Points(pts)
                    if pts.iter().any(|p| {
                        matches!(p.time, hydra::uds::model::SeriesTime::Absolute { .. })
                    }) =>
                {
                    return Err(format!("'{id}' carries dated readings, which this table of elapsed hours cannot hold"));
                }
                hydra::uds::model::TimeSeriesSource::Points(_) => {}
            }
            // Interpolation brackets a run time between neighbouring
            // points, so the times must advance — the same rule a curve's
            // abscissae follow.
            for w in readings.windows(2) {
                if w[1].0 <= w[0].0 {
                    return Err(format!(
                        "a series' times have to increase: {} does not follow {}",
                        w[1].0, w[0].0
                    ));
                }
            }
            series.source = hydra::uds::model::TimeSeriesSource::Points(
                readings
                    .into_iter()
                    // Back through the same unit the read served: hours
                    // in the table, seconds in the model.
                    .map(|(hours, value)| hydra::uds::model::TimeSeriesPoint {
                        time: hydra::uds::model::SeriesTime::Elapsed(hours * 3600.0),
                        value,
                    })
                    .collect(),
            );
            Ok(())
        }
        other => Err(not_tabular(other)),
    }
}

pub(crate) fn set_wds_contents(
    network: &mut hydra::Network,
    kind: &str,
    id: &str,
    rows: &[Vec<f64>],
) -> Result<(), String> {
    match kind {
        "curve" => {
            let points = pairs(rows, 2)?;
            ascending(&points)?;
            let index = network
                .curves
                .iter()
                .position(|c| c.id == id)
                .ok_or_else(|| format!("no curve '{id}'"))?;
            // Back through the same axis scales the read applied, asked
            // of the one table that owns them — a curve's units depend on
            // its purpose, and dividing by the wrong pair stores a value
            // that looks plausible and is out by a thousand.
            let axes = super::network_dto::curve_axes(network.curves[index].kind);
            network.curves[index].points = points
                .into_iter()
                .map(|(x, y)| hydra::CurvePoint {
                    x: x / axes[0].scale(),
                    y: y / axes[1].scale(),
                })
                .collect();
            Ok(())
        }
        "pattern" => {
            let factors = factors(rows)?;
            if factors.is_empty() {
                return Err("a pattern needs at least one multiplier".into());
            }
            let pattern = network
                .patterns
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("no pattern '{id}'"))?;
            pattern.factors = factors;
            Ok(())
        }
        other => Err(not_tabular(other)),
    }
}

/// A curve's abscissae must strictly ascend.
///
/// Checked here rather than left to the solver because the whole table
/// arrives at once: this is the validation the one write exists to make
/// possible, and refusing here means the model never holds a curve that
/// cannot be interpolated.
fn ascending(points: &[(f64, f64)]) -> Result<(), String> {
    for pair in points.windows(2) {
        if pair[1].0 <= pair[0].0 {
            return Err(format!(
                "a curve's first column has to increase: {} does not follow {}",
                pair[1].0, pair[0].0
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UDS: &str = "\
[OPTIONS]
FLOW_UNITS CMS
[JUNCTIONS]
J1 10 3 0 0 0
[OUTFALLS]
O1 8 FREE NO
[CONDUITS]
C1 J1 O1 100 0.013 0 0
[XSECTIONS]
C1 CIRCULAR 1 0 0 0
[CURVES]
ST1 STORAGE 0 100
ST1 1 150
[PATTERNS]
P1 HOURLY 1 1.2 0.8 1 1 1
[TIMESERIES]
TS1 0:00 0.0
TS1 1:00 0.5
";

    fn uds() -> hydra::uds::model::Network {
        hydra::swmm::objects::parse_network(UDS).0
    }

    fn wds() -> hydra::Network {
        hydra::io::parse(
            "\
[JUNCTIONS]
 J1 100
[RESERVOIRS]
 R1 120
[PIPES]
 P1 J1 R1 100 300 0.1 0 Open
[CURVES]
 C1 0 50
 C1 5 0
[PATTERNS]
 PA1 1.0 1.2 0.8
[END]
"
            .as_bytes(),
        )
        .expect("parse")
    }

    /// The whole point of the section: a curve's points reach the model
    /// and come back, in both engines, through one call.
    #[test]
    fn a_curve_takes_a_new_table_of_points() {
        let mut net = uds();
        set_uds_contents(
            &mut net,
            "curve",
            "ST1",
            &[vec![0.0, 90.0], vec![2.0, 200.0]],
        )
        .expect("set");
        assert_eq!(
            net.curves[0].points,
            vec![(0.0, 90.0), (2.0, 200.0)],
            "the table did not reach the model"
        );

        let mut network = wds();
        set_wds_contents(
            &mut network,
            "curve",
            "C1",
            &[vec![0.0, 60.0], vec![3.0, 30.0], vec![6.0, 0.0]],
        )
        .expect("set");
        assert_eq!(network.curves[0].points.len(), 3);
    }

    /// A pump curve's flows are litres per second on the way out, so they
    /// have to be litres per second on the way back. The round trip
    /// cancels the error, which is how this class of bug survives — so it
    /// is asserted against the model's own SI value.
    #[test]
    fn a_wds_curve_converts_back_through_the_axis_it_came_out_by() {
        let mut network = wds();
        // C1 is referenced by nothing, so it is a generic curve whose
        // axes carry no scale — the case that must *not* convert.
        set_wds_contents(
            &mut network,
            "curve",
            "C1",
            &[vec![0.0, 50.0], vec![5.0, 0.0]],
        )
        .expect("set");
        assert_eq!(network.curves[0].points[1].x, 5.0);

        // And the read agrees with the write, which is the only thing
        // that makes the pair safe as the axis table grows.
        let served = super::super::wds_attrs::collection_detail(&network, "curve", "C1");
        assert_eq!(served.rows, vec![vec![0.0, 50.0], vec![5.0, 0.0]]);
        assert!(served.editable);
    }

    #[test]
    fn a_pattern_takes_its_multipliers_and_may_change_length() {
        // The length is the period, so a shorter table is a different
        // pattern rather than a truncated one — which is exactly why the
        // whole table travels together.
        let mut net = uds();
        set_uds_contents(&mut net, "pattern", "P1", &[vec![1.0, 2.0], vec![2.0, 0.5]])
            .expect("set");
        assert_eq!(net.patterns[0].factors, vec![2.0, 0.5]);

        let mut network = wds();
        set_wds_contents(&mut network, "pattern", "PA1", &[vec![1.0, 1.5]]).expect("set");
        assert_eq!(network.patterns[0].factors, vec![1.5]);
    }

    /// The period bounds the table, from this door as well as from the
    /// type write's.
    ///
    /// A drainage pattern repeats, so a multiplier past the end of the
    /// period is one the engine never reads — it would sit in the file
    /// and in this table looking like it did something. The type write
    /// already refused a period too short for the multipliers on hand;
    /// enforcing it there alone left this door open to reach the same
    /// state from the other side.
    ///
    /// Fewer is not more, and stays allowed: an absent multiplier reads
    /// as 1.0, and the fixture's own hourly pattern carries six.
    #[test]
    fn a_drainage_pattern_takes_no_more_multipliers_than_its_period() {
        let mut net = uds();
        let rows: Vec<Vec<f64>> = (0..25).map(|i| vec![f64::from(i), 1.0]).collect();
        let err = set_uds_contents(&mut net, "pattern", "P1", &rows).expect_err("past the period");
        assert!(err.contains("every 24 multipliers"), "{err}");
        assert_eq!(net.patterns[0].factors.len(), 6);

        set_uds_contents(&mut net, "pattern", "P1", &rows[..24]).expect("a full day");
        assert_eq!(net.patterns[0].factors.len(), 24);

        // And a water-distribution pattern has no period to bound it.
        let mut network = wds();
        let long: Vec<Vec<f64>> = (0..30).map(|i| vec![f64::from(i), 1.0]).collect();
        set_wds_contents(&mut network, "pattern", "PA1", &long).expect("no period");
        assert_eq!(network.patterns[0].factors.len(), 30);
    }

    /// A time series' readings reach the model in the unit the table
    /// shows: hours in the column, seconds in the model. Asserted against
    /// the model's own value *and* back through the read, because a
    /// conversion error cancels itself over a round trip — which is how
    /// it survives.
    #[test]
    fn a_time_series_takes_a_new_table_of_readings() {
        let mut net = uds();
        set_uds_contents(
            &mut net,
            "timeseries",
            "TS1",
            &[vec![0.0, 0.0], vec![0.5, 2.0], vec![2.0, 0.0]],
        )
        .expect("set");
        let hydra::uds::model::TimeSeriesSource::Points(pts) = &net.timeseries[0].source else {
            panic!("not points");
        };
        assert_eq!(pts.len(), 3);
        assert_eq!(
            pts[1].time,
            hydra::uds::model::SeriesTime::Elapsed(1800.0),
            "half an hour is 1800 seconds"
        );
        assert_eq!(pts[1].value, 2.0);

        // And the read agrees with the write.
        let served = super::super::uds_attrs::collection_detail(&net, "timeseries", "TS1");
        assert_eq!(
            served.rows,
            vec![vec![0.0, 0.0], vec![0.5, 2.0], vec![2.0, 0.0]]
        );
        assert!(served.editable);
        // And names its time column as the one that must advance, which
        // is what the panel seeds a new row by.
        assert_eq!(served.advances, Some(0));
    }

    /// Interpolation brackets a run time between neighbouring points, so
    /// a series whose times do not advance cannot be evaluated — refused,
    /// changing nothing, like a curve that does not ascend.
    #[test]
    fn a_series_whose_times_do_not_advance_is_refused() {
        let mut net = uds();
        let err = set_uds_contents(
            &mut net,
            "timeseries",
            "TS1",
            &[vec![1.0, 0.0], vec![1.0, 2.0]],
        )
        .expect_err("equal times");
        assert!(err.contains("increase"), "{err}");
        let hydra::uds::model::TimeSeriesSource::Points(pts) = &net.timeseries[0].source else {
            panic!("not points");
        };
        assert_eq!(pts.len(), 2, "a refusal changed the model");

        assert!(
            set_uds_contents(&mut net, "timeseries", "TS1", &[]).is_err(),
            "a series written empty would vanish at the next save"
        );
    }

    /// The two shapes the read serves as something other than an elapsed
    /// table are the two this write refuses: dated readings and an
    /// external file. The read already marks both uneditable; the write
    /// refusing too is what keeps the rule enforced on both doors.
    #[test]
    fn a_dated_or_external_series_is_refused_by_name() {
        let mut net = uds();
        net.timeseries.push(hydra::uds::model::TimeSeries {
            id: "DATED".into(),
            source: hydra::uds::model::TimeSeriesSource::Points(vec![
                hydra::uds::model::TimeSeriesPoint {
                    time: hydra::uds::model::SeriesTime::Absolute {
                        date: hydra::swmm::options::Date {
                            year: 2026,
                            month: 8,
                            day: 1,
                        },
                        seconds: 0.0,
                    },
                    value: 1.0,
                },
            ]),
        });
        net.timeseries.push(hydra::uds::model::TimeSeries {
            id: "EXT".into(),
            source: hydra::uds::model::TimeSeriesSource::External {
                file: "rain.dat".into(),
            },
        });

        let err = set_uds_contents(&mut net, "timeseries", "DATED", &[vec![0.0, 1.0]])
            .expect_err("dated");
        assert!(err.contains("dated"), "{err}");
        assert!(
            !super::super::uds_attrs::collection_detail(&net, "timeseries", "DATED").editable,
            "the read offers what the write just refused"
        );

        let err =
            set_uds_contents(&mut net, "timeseries", "EXT", &[vec![0.0, 1.0]]).expect_err("file");
        assert!(err.contains("rain.dat"), "{err}");
    }

    /// Whatever the read serves as editable, the write takes back — and
    /// takes one more row of, seeded the way the panel seeds one — for
    /// every collection kind, in both engines.
    ///
    /// This is the invariant that broke, twice over: a time series was
    /// served as an editable table while the write had no arm for it, so
    /// the panel offered an edit that could only ever refuse — and the
    /// panel's add button seeded a row of zeros, which a table whose
    /// advancing column has passed zero can also only refuse. The catalog
    /// is the loop's spine so the next collection kind is enrolled by
    /// existing, and the exercised list is asserted so a fixture missing
    /// its elements cannot quietly hollow the test out.
    #[test]
    fn every_container_served_editable_is_one_the_write_takes() {
        let mut net = uds();
        net.transects.push(hydra::uds::model::Transect {
            id: "TR1".into(),
            n_left: 0.03,
            n_right: 0.03,
            n_channel: 0.02,
            x_left: 0.0,
            x_right: 0.0,
            meander_factor: 1.0,
            stations: vec![(2.0, 0.0), (0.0, 5.0), (2.0, 10.0)],
        });
        let wds_net = wds();

        let mut exercised = Vec::new();
        for (engine, catalog) in [
            ("uds", hydra::uds::descriptors::ELEMENT_KINDS),
            ("wds", hydra::descriptors::ELEMENT_KINDS),
        ] {
            for kind in catalog
                .iter()
                .filter(|k| k.class == hydra::common::ElementClass::Collection)
            {
                let ids = if engine == "uds" {
                    super::super::uds_attrs::kind_elements(&net, kind.id).ids
                } else {
                    super::super::wds_attrs::kind_elements(&wds_net, kind.id).ids
                };
                for id in ids {
                    let detail = if engine == "uds" {
                        super::super::uds_attrs::collection_detail(&net, kind.id, &id)
                    } else {
                        super::super::wds_attrs::collection_detail(&wds_net, kind.id, &id)
                    };
                    if !detail.editable {
                        continue;
                    }
                    let write = |rows: &[Vec<f64>]| {
                        if engine == "uds" {
                            set_uds_contents(&mut net.clone(), kind.id, &id, rows)
                        } else {
                            set_wds_contents(&mut wds_net.clone(), kind.id, &id, rows)
                        }
                    };
                    write(&detail.rows).unwrap_or_else(|e| {
                        panic!("{engine}.{} '{id}' is served editable but: {e}", kind.id)
                    });
                    // And one more row, seeded the way the panel seeds
                    // one: the last row copied, its advancing column
                    // moved on.
                    let mut seeded = detail.rows.last().cloned().unwrap_or_default();
                    if let Some(col) = detail.advances {
                        seeded[col] += 1.0;
                    }
                    let mut added = detail.rows.clone();
                    added.push(seeded);
                    write(&added).unwrap_or_else(|e| {
                        panic!(
                            "{engine}.{} '{id}' refuses the panel's added row: {e}",
                            kind.id
                        )
                    });
                    exercised.push(format!("{engine}.{}", kind.id));
                }
            }
        }
        for want in [
            "uds.curve",
            "uds.pattern",
            "uds.timeseries",
            "uds.transect",
            "wds.curve",
            "wds.pattern",
        ] {
            assert!(
                exercised.iter().any(|e| e == want),
                "the fixture no longer exercises {want}"
            );
        }
    }

    /// A transect's survey points, which are the same table shape as a
    /// curve's with one difference that matters: the model holds them as
    /// (elevation, station), so it is the *second* value that has to
    /// advance across the section.
    #[test]
    fn a_transect_takes_survey_points_and_checks_the_station_not_the_elevation() {
        let mut net = uds();
        net.transects.push(hydra::uds::model::Transect {
            id: "TR1".into(),
            n_left: 0.03,
            n_right: 0.03,
            n_channel: 0.02,
            x_left: 0.0,
            x_right: 0.0,
            meander_factor: 1.0,
            stations: vec![(0.0, 0.0), (0.0, 1.0)],
        });
        set_uds_contents(
            &mut net,
            "transect",
            "TR1",
            &[vec![2.0, 0.0], vec![0.0, 5.0], vec![2.0, 10.0]],
        )
        .expect("a surveyed section");
        assert_eq!(net.transects[0].stations.len(), 3);

        // Elevations legitimately repeat and fall — a section rises to
        // both banks — so the check is on the station alone.
        let err = set_uds_contents(
            &mut net,
            "transect",
            "TR1",
            &[vec![2.0, 5.0], vec![0.0, 5.0]],
        )
        .expect_err("two points at one station");
        assert!(err.contains("stations"), "{err}");
        assert_eq!(net.transects[0].stations.len(), 3, "a refusal changed it");
    }

    #[test]
    fn a_curve_that_does_not_ascend_is_refused() {
        let mut net = uds();
        let err = set_uds_contents(
            &mut net,
            "curve",
            "ST1",
            &[vec![0.0, 90.0], vec![0.0, 200.0]],
        )
        .expect_err("equal abscissae");
        assert!(err.contains("increase"), "{err}");
        // And nothing moved.
        assert_eq!(net.curves[0].points, vec![(0.0, 100.0), (1.0, 150.0)]);

        assert!(
            set_uds_contents(&mut net, "curve", "ST1", &[vec![0.0, 1.0]]).is_err(),
            "one point is a value, not a curve"
        );
    }

    #[test]
    fn a_row_of_the_wrong_width_is_refused() {
        let mut net = uds();
        let err = set_uds_contents(&mut net, "curve", "ST1", &[vec![0.0], vec![1.0, 2.0]])
            .expect_err("short row");
        assert!(err.contains("2 values"), "{err}");
    }

    #[test]
    fn a_kind_whose_contents_are_language_refuses_by_name() {
        let mut net = uds();
        let err = set_uds_contents(&mut net, "rule", "R1", &[vec![1.0, 2.0]]).expect_err("refused");
        assert!(err.contains("rule"), "{err}");
        let mut network = wds();
        assert!(set_wds_contents(&mut network, "control", "1", &[vec![1.0, 2.0]]).is_err());
    }

    #[test]
    fn an_unknown_container_is_refused() {
        let mut net = uds();
        assert!(
            set_uds_contents(&mut net, "curve", "NOPE", &[vec![0.0, 1.0], vec![1.0, 2.0]]).is_err()
        );
    }
}
