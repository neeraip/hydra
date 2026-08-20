//! End-to-end session tests (§10, §12): models driven from their input
//! text alone — external inflow series, sanitary patterns, tidal stages,
//! event windows, and reporting.

use hydra_engine_uds::simulation::Simulation;
use hydra_engine_uds::transport::MassSource;

#[test]
fn an_inflow_series_drives_the_network_from_file_alone() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1  100.4  3
J2  100.2  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  J2  200  0.013  0  0
C2  J2  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
C2  RECT_OPEN  2  2  0  0

[INFLOWS]
J1  FLOW  QIN  FLOW  1.0  1.0  0.05

[TIMESERIES]
QIN  0:00  0.25
QIN  2:00  0.25
QIN  2:01  0.0
QIN  9:00  0.0
";
    let (mut sim, _, findings) = Simulation::open(inp).expect("open");
    assert!(findings.iter().all(|f| !f.kind.is_error()));
    sim.run();
    assert!((sim.time() - 4.0 * 3600.0).abs() < 1e-6);

    // Reporting boundaries every 15 minutes over 4 hours.
    assert_eq!(sim.snapshots.len(), 16);
    // During the plateau the outfall carries series + baseline = 0.3.
    let mid = &sim.snapshots[7]; // t = 2 h
    let c2 = mid.flows[1];
    assert!((c2 - 0.3).abs() < 0.01, "plateau outflow {c2}");
    // After the series steps to zero only the 0.05 baseline remains.
    let tail = sim.snapshots.last().unwrap();
    assert!(
        (tail.flows[1] - 0.05).abs() < 0.005,
        "tail {}",
        tail.flows[1]
    );
    // The ledger accounts the full inflow volume: 0.3 for 2 h, then
    // 0.05 for the rest (the one-minute ramp is a sliver).
    let expect = 0.3 * 7200.0 + 0.05 * 7200.0;
    let led = sim.report();
    assert!(
        (led.inflow - expect).abs() < 0.02 * expect,
        "inflow {} vs {expect}",
        led.inflow
    );
}

#[test]
fn sanitary_patterns_modulate_by_the_calendar() {
    // June 1st 2024 is a Saturday: the weekend hourly pattern applies its
    // hour-zero factor at the start.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      01:00
ROUTING_STEP  10

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[DWF]
J1  FLOW  0.05  MON  WKND

[PATTERNS]
MON   MONTHLY  1 1 1 1 1 2.0 1 1 1 1 1 1
WKND  WEEKEND  1.5 1 1 1 1 1 1 1 1 1 1 1
WKND           1 1 1 1 1 1 1 1 1 1 1 1
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    // June (factor 2) on a Saturday midnight hour (factor 1.5):
    // 0.05 × 2 × 1.5 = 0.15.
    let q = sim.flow("C1").unwrap();
    assert!((q - 0.15).abs() < 0.01, "dwf flow {q}");
}

#[test]
fn a_tidal_outfall_follows_clock_time() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    06:00
END_DATE      06/01/2024
END_TIME      12:00
ROUTING_STEP  10

[JUNCTIONS]
J1  102.0  3

[OUTFALLS]
O1  100.0  TIDAL  TC

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[CURVES]
TC  TIDAL  0  100.2  6  100.8  12  100.2  18  100.8  24  100.2
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    // A 06:00 start: the tide is at its 06:00 high, not its curve-origin
    // low — clock-indexed, per §14.7.
    sim.step();
    let d0 = sim.depth("O1").unwrap();
    assert!((d0 - 0.8).abs() < 0.02, "start-of-run tide {d0}");
    sim.run();
    // Six hours later the clock reads 12:00: back at the low.
    let d1 = sim.depth("O1").unwrap();
    assert!((d1 - 0.2).abs() < 0.02, "end-of-run tide {d1}");
}

#[test]
fn event_windows_freeze_the_network_between_events() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      03:00
ROUTING_STEP  10

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[INFLOWS]
J1  FLOW  QIN

[TIMESERIES]
QIN  0:00  0.2
QIN  9:00  0.2

[EVENTS]
06/01/2024  00:00  06/01/2024  01:00
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    // Routing ran only inside the one-hour window: the ledger holds one
    // hour of inflow, and the clock still reached the end.
    let led = sim.report();
    assert!(
        (led.inflow - 0.2 * 3600.0).abs() < 0.02 * 0.2 * 3600.0,
        "inflow {}",
        led.inflow
    );
    assert!((sim.time() - 3.0 * 3600.0).abs() < 1e-6);
}

#[test]
fn an_exhausted_series_warns_once_and_reads_zero() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      02:00
ROUTING_STEP  10

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[INFLOWS]
J1  FLOW  SHORT

[TIMESERIES]
SHORT  0:00  0.2
SHORT  0:30  0.2
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let warnings: Vec<_> = sim
        .notices
        .iter()
        .filter(|n| n.message.contains("SHORT"))
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", sim.notices);
    // The tail runs dry.
    assert!(sim.flow("C1").unwrap() < 0.01);
}

// ── §3 rainfall–runoff ──────────────────────────────────────────────────

/// A one-parcel model: `imperv` percent impervious, Horton parameters
/// hot enough to matter, raining `intensity` mm/h for two hours.
fn runoff_model(imperv: f64, rain_mm_h: f64, infil: &str) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  {infil}
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      08:00
ROUTING_STEP  10
WET_STEP      0:05:00
REPORT_STEP   0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
S1  G1  J1  2  {imperv}  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[TIMESERIES]
RAIN  0:00  {rain_mm_h}
RAIN  1:00  {rain_mm_h}
RAIN  2:00  0
"
    )
}

#[test]
fn an_impervious_parcel_converts_rain_to_runoff() {
    // Fully impervious: everything that falls beyond depression storage
    // reaches the outfall.
    let inp = runoff_model(100.0, 25.0, "HORTON");
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    // 25 mm/h over 2 h on 2 ha = 1000 m³; depression storage holds
    // 0.05 mm × ~15000 m² ≈ 0.75 m³ plus what still ponds.
    let led = sim.report();
    let rain_vol = 0.025 * 2.0 * 20_000.0;
    assert!(
        led.inflow > 0.9 * rain_vol && led.inflow <= rain_vol * 1.01,
        "runoff into network {} vs rain {rain_vol}",
        led.inflow
    );
    // And it drains to the outfall by the end.
    assert!(
        led.outflow > 0.95 * led.inflow,
        "in {} out {}",
        led.inflow,
        led.outflow
    );
}

#[test]
fn horton_infiltration_swallows_light_rain_on_pervious_ground() {
    // Fully pervious with Horton f0 = 20 mm/h: 10 mm/h rain infiltrates
    // whole while capacity lasts.
    let inp = runoff_model(0.0, 10.0, "HORTON");
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let led = sim.report();
    let rain_vol = 0.010 * 2.0 * 20_000.0;
    assert!(
        led.inflow < 0.15 * rain_vol,
        "runoff {} of rain {rain_vol}",
        led.inflow
    );
}

#[test]
fn heavier_rain_exceeds_capacity_and_runs_off() {
    // 40 mm/h against a capacity decaying from 20 to 5 mm/h: runoff is
    // substantial but well below the fully-impervious volume.
    let inp = runoff_model(0.0, 40.0, "HORTON");
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let led = sim.report();
    let rain_vol = 0.040 * 2.0 * 20_000.0;
    assert!(
        led.inflow > 0.3 * rain_vol && led.inflow < 0.95 * rain_vol,
        "runoff {} of rain {rain_vol}",
        led.inflow
    );
}

#[test]
fn green_ampt_and_curve_number_also_close_their_balances() {
    for infil in ["GREEN_AMPT", "CURVE_NUMBER"] {
        let mut inp = runoff_model(0.0, 40.0, infil);
        if infil == "GREEN_AMPT" {
            inp = inp.replace("S1  20  5  4  7  0", "S1  90  10  0.25");
        } else {
            inp = inp.replace("S1  20  5  4  7  0", "S1  80  0  7");
        }
        let (mut sim, _, _) = Simulation::open(&inp).expect(infil);
        sim.run();
        let led = sim.report();
        let rain_vol = 0.040 * 2.0 * 20_000.0;
        // Both relations must remove a real share of an 80 mm storm on
        // pervious ground — neither trickle nor total capture.
        let ratio = led.inflow / rain_vol;
        assert!(
            (0.15..=0.85).contains(&ratio),
            "{infil}: runoff ratio {ratio} of rain {rain_vol}"
        );
        assert!(
            led.outflow > 0.9 * led.inflow,
            "{infil}: in {} out {}",
            led.inflow,
            led.outflow
        );
    }
}

#[test]
fn parcel_cascades_arrive_one_step_delayed() {
    // S2 drains onto S1, which drains to the junction: the cascade's
    // whole volume still arrives.
    let inp = runoff_model(100.0, 25.0, "HORTON").replace(
        "[SUBCATCHMENTS]
S1  G1  J1  2  100  100  0.5  0",
        "[SUBCATCHMENTS]
S1  G1  J1  2  100  100  0.5  0
S2  G1  S1  1  100  100  0.5  0",
    ) + "
[SUBAREAS]
S2  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S2  20  5  4  7  0
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let led = sim.report();
    // Three hectares' worth of rain reaches the network.
    let rain_vol = 0.025 * 2.0 * 30_000.0;
    assert!(
        led.inflow > 0.88 * rain_vol && led.inflow <= rain_vol * 1.01,
        "cascade inflow {} vs rain {rain_vol}",
        led.inflow
    );
}

// ── §4.1 groundwater ────────────────────────────────────────────────────

fn gw_model(a1: f64, b1: f64, water_table: f64) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  GREEN_AMPT
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/02/2024
END_TIME      00:00
ROUTING_STEP  30
WET_STEP      0:05:00
DRY_STEP      0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
S1  G1  J1  2  0  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  90  20  0.3

[AQUIFERS]
AQ1  0.45  0.10  0.20  20  10  1.0  0.35  1.0  0  95  {water_table}  0.30

[GROUNDWATER]
S1  AQ1  J1  100  {a1}  {b1}  0  0  0  0  *

[JUNCTIONS]
J1  98.0  3

[OUTFALLS]
O1  97.8  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[TIMESERIES]
RAIN  0:00  0
"
    )
}

#[test]
fn a_custom_lateral_relation_adds_to_the_power_relation() {
    // No built-in coefficients (a1 = 0) and the table exactly at the
    // threshold: only the §9.3 custom relation discharges — a constant
    // 0.02 m³/s per hectare over 2 ha is 0.04 m³/s at the vertex.
    let inp = gw_model(0.0, 1.0, 98.0)
        + "
[GWF]
S1  LATERAL  0.02
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let q = sim.flow("C1").expect("flow");
    assert!((q - 0.04).abs() < 0.004, "custom lateral flow {q}");
    let led = sim.report();
    let expect = 0.04 * 86_400.0;
    assert!(
        (led.inflow - expect).abs() < 0.05 * expect,
        "volume {} vs {expect}",
        led.inflow
    );
}

#[test]
fn a_domain_guarded_expression_warns_once_and_reads_zero() {
    // sqrt of a negative argument is guarded to zero (§9.3): deep
    // percolation reads zero, and exactly one notice announces it.
    let inp = gw_model(0.0, 1.0, 98.0)
        + "
[GWF]
S1  DEEP  sqrt ( 0 - 1 )
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let warnings: Vec<_> = sim
        .notices
        .iter()
        .filter(|n| n.message.contains("deep-percolation"))
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", sim.notices);
    // Guarded to zero means no deep loss and no discharge: dry network.
    assert!(sim.report().inflow.abs() < 1.0);
}

#[test]
fn an_unknown_expression_name_refuses_the_model() {
    let inp = gw_model(0.0, 1.0, 98.0)
        + "
[GWF]
S1  LATERAL  0.001 * BOGUS
";
    assert!(
        Simulation::open(&inp).is_err(),
        "unknown vocabulary name must refuse the model"
    );
}

#[test]
fn a_charged_aquifer_discharges_and_recedes() {
    // Water table starts 1 m above the threshold (J1's invert at 98,
    // aquifer bottom 95 → h* = 3 m; table at 99 → d_L = 4 m).
    let inp = gw_model(0.01, 1.0, 99.0);
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    // The vertex received baseflow: 0.01 cms/ha × 0.2 ha-scaled head
    // decays as the table falls; the ledger carries real volume.
    let led = sim.report();
    assert!(
        led.inflow > 50.0,
        "groundwater discharge too small: {}",
        led.inflow
    );
    assert!(
        led.outflow > 0.9 * led.inflow,
        "in {} out {}",
        led.inflow,
        led.outflow
    );
    // And it recedes: the peak arrives in the first half of the run and
    // the final rate sits below it as the table falls.
    let flows: Vec<f64> = sim.snapshots.iter().map(|s| s.flows[0]).collect();
    let (peak_i, peak) =
        flows.iter().enumerate().fold(
            (0, 0.0_f64),
            |(bi, bv), (i, &v)| {
                if v > bv {
                    (i, v)
                } else {
                    (bi, bv)
                }
            },
        );
    let late = *flows.last().expect("snapshots");
    // Upper-zone percolation recharges the table, so the recession is
    // gentle — but a non-decaying constant would hold the peak.
    assert!(
        peak_i < flows.len() / 2 && late < 0.95 * peak,
        "no recession: peak {peak} at {peak_i}/{} vs late {late}",
        flows.len()
    );
}

#[test]
fn a_table_below_the_threshold_yields_no_baseflow() {
    // d_L = 1 m < h* = 3 m, and the upper zone at field capacity so no
    // percolation lifts the table.
    let inp = gw_model(0.01, 1.0, 96.0).replace("95  96  0.30", "95  96  0.20");
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    assert!(
        sim.report().inflow < 1.0,
        "unexpected baseflow {}",
        sim.report().inflow
    );
}

#[test]
fn rain_recharges_the_aquifer_through_infiltration() {
    // Start below the threshold; a day of steady rain infiltrates
    // through Green–Ampt and lifts the table into discharge.
    let mut inp = gw_model(0.05, 1.0, 97.5);
    inp = inp.replace(
        "[TIMESERIES]\nRAIN  0:00  0\n",
        "[TIMESERIES]\nRAIN  0:00  15\nRAIN  12:00  15\nRAIN  23:00  15\n",
    );
    // Hourly gage: make every hour rain.
    let series: String = (0..24).map(|h| format!("RAIN  {h}:00  15\n")).collect();
    inp = inp.replace(
        "[TIMESERIES]\nRAIN  0:00  15\nRAIN  12:00  15\nRAIN  23:00  15\n",
        &format!("[TIMESERIES]\n{series}"),
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    // Surface runoff is small on fully pervious ground under 15 mm/h
    // against a 20 mm/h conductivity, but the aquifer fills and
    // discharges: the network sees meaningful inflow by day's end.
    assert!(
        sim.report().inflow > 20.0,
        "no recharge-driven baseflow: {}",
        sim.report().inflow
    );
}

// ── §4.2 snow ───────────────────────────────────────────────────────────

fn snow_model(temps: &str) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
START_DATE    01/15/2024
START_TIME    00:00
END_DATE      01/17/2024
END_TIME      00:00
ROUTING_STEP  30
WET_STEP      0:05:00
DRY_STEP      0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  PRECIP

[SUBCATCHMENTS]
S1  G1  J1  2  100  100  0.5  0  SP1

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[SNOWPACKS]
SP1  PLOWABLE  2  4  0  0.10  0  0  0.0
SP1  IMPERV    2  4  0  0.10  0  0  1.0
SP1  PERV      2  4  0  0.10  0  0  1.0

[TEMPERATURE]
TIMESERIES  TEMP
SNOWMELT    0.5  0.5  0.6  100  45  -75
ADC         IMPERV  1 1 1 1 1 1 1 1 1 1
ADC         PERV    1 1 1 1 1 1 1 1 1 1

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[TIMESERIES]
{temps}
PRECIP  0:00  5
PRECIP  1:00  5
PRECIP  2:00  5
PRECIP  3:00  5
PRECIP  4:00  5
PRECIP  5:00  5
"
    )
}

#[test]
fn snow_accumulates_cold_then_melts_warm() {
    // Six hours of 5 mm/h precipitation at −5 °C: it snows and nothing
    // runs off. The second day warms to +8 °C and the pack melts out.
    let temps = "\
TEMP  0:00   -5
TEMP  20:00  -5
TEMP  24:00  8
TEMP  48:00  8";
    let inp = snow_model(temps);
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    // Run through the cold day: snow holds, the network stays dry.
    while sim.time() < 20.0 * 3600.0 {
        sim.step();
    }
    assert!(
        sim.report().inflow < 1.0,
        "runoff during the cold day: {}",
        sim.report().inflow
    );
    // Run through the warm day: the pack melts and drains.
    sim.run();
    let total_precip = 0.005 * 6.0 * 20_000.0; // 600 m³
    assert!(
        sim.report().inflow > 0.7 * total_precip,
        "melt runoff {} of snowfall {total_precip}",
        sim.report().inflow
    );
}

/// Wind can be declared two ways and must mean the same thing.
///
/// §14.14: a monthly declaration is in the model's own speed unit, and a
/// climate file's wind column is miles per hour whatever the model's
/// units, which is the predecessor's file semantics. Both feed the same
/// rain-melt relation, so equivalent declarations have to melt equally.
/// This engine read the file column as metres per second, making a
/// file-sourced wind 2.24 times too fast and the rain-melt it drives
/// correspondingly too large.
#[test]
fn a_files_wind_and_a_monthly_wind_mean_the_same_speed() {
    // Snow falls cold, then rain arrives just above freezing on the pack,
    // which is where the wind function does its work.
    let temps = "\
TEMP  0:00   -5
TEMP  6:00   -5
TEMP  8:00   1
TEMP  48:00  1";
    let with_wind = |declaration: &str| {
        snow_model(temps).replace(
            "SNOWMELT    0.5  0.5  0.6  100  45  -75",
            &format!("SNOWMELT    0.5  0.5  0.6  100  45  -75\n{declaration}"),
        )
    };
    // Rain on the pack once it exists.
    let rain_on_snow = |inp: String| {
        inp.replace(
            "PRECIP  5:00  5\n",
            "PRECIP  5:00  5\nPRECIP  6:00  0\nPRECIP  9:00  4\nPRECIP  14:00  4\nPRECIP  15:00  0\n",
        )
    };
    let day = |wind: Option<f64>| {
        vec![hydra_engine_uds::model::DailyClimate {
            date: hydra_engine_uds::io::options::Date {
                year: 2024,
                month: 1,
                day: 15,
            },
            tmax: None,
            tmin: None,
            evap: None,
            wind,
        }]
    };
    let melt = |inp: String, records| {
        let (mut sim, _, _) = Simulation::open_with_climate(&inp, records).expect("open");
        sim.run();
        sim.report().inflow
    };

    // Ten miles an hour, declared as a monthly average in the model's own
    // unit (km/h, this model being metric) and as a file column (mph).
    let monthly = format!(
        "WINDSPEED   MONTHLY  {0} {0} {0} {0} {0} {0} {0} {0} {0} {0} {0} {0}",
        10.0 * 1.609_344
    );
    let from_monthly = melt(rain_on_snow(with_wind(&monthly)), Vec::new());
    let from_file = melt(rain_on_snow(with_wind("WINDSPEED   FILE")), day(Some(10.0)));
    assert!(
        (from_monthly - from_file).abs() < 1e-6 * from_monthly.max(1.0),
        "ten miles an hour melted {from_monthly} declared monthly and \
         {from_file} read from a file"
    );

    // And the comparison means something only if wind changes the answer
    // at all: a still day must melt less than a windy one.
    let still = melt(rain_on_snow(with_wind("WINDSPEED   FILE")), day(Some(0.0)));
    assert!(
        still < from_file - 1e-9,
        "a still day melted {still} and a ten-mile-an-hour day {from_file}: \
         the wind function is not reaching this run, so the comparison \
         above proves nothing"
    );
}

#[test]
fn warm_rain_passes_straight_through_a_snow_parcel() {
    // The same model at +10 °C throughout: plain rain on the impervious
    // parcel, most of it arriving as runoff during the storm.
    let temps = "\
TEMP  0:00   10
TEMP  48:00  10";
    let inp = snow_model(temps);
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let total_precip = 0.005 * 6.0 * 20_000.0;
    assert!(
        sim.report().inflow > 0.85 * total_precip,
        "rain runoff {} of {total_precip}",
        sim.report().inflow
    );
}

// ── §4.3 RDII ───────────────────────────────────────────────────────────

#[test]
fn rdii_convolves_rainfall_into_the_sewer() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      12:00
ROUTING_STEP  30
WET_STEP      0:05:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT   0.10  1.0  2.0
UH1  ALL  MEDIUM  0.20  3.0  2.0

[RDII]
J1  UH1  10

[TIMESERIES]
RAIN  0:00  20
RAIN  1:00  20
RAIN  2:00  0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    // Two hours of 20 mm/h over a 10 ha sewershed with R = 0.30 total:
    // volume in = 0.04 m × 100 000 m² × 0.30 = 1200 m³, all delivered
    // once the slowest 9 h triangle has recessed.
    let led = sim.report();
    let expect = 0.04 * 100_000.0 * 0.30;
    assert!(
        (led.inflow - expect).abs() < 0.05 * expect,
        "rdii volume {} vs {expect}",
        led.inflow
    );
    assert!(led.outflow > 0.9 * led.inflow);
    // The convolution delays the response: the first reporting period
    // carries only a small share of the eventual peak.
    let peak = sim
        .snapshots
        .iter()
        .map(|s| s.flows[0])
        .fold(0.0_f64, f64::max);
    let early: f64 = sim.snapshots.first().map_or(0.0, |s| s.flows[0]);
    assert!(
        early < 0.6 * peak,
        "no convolution delay: early {early} vs peak {peak}"
    );
}

// ── §9 operational control ──────────────────────────────────────────────

/// A constant-inflow model with one conduit, ready for rule text.
fn control_model(controls: &str) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[INFLOWS]
J1  FLOW  QIN

[TIMESERIES]
QIN  0:00  0.25
QIN  9:00  0.25

[CONTROLS]
{controls}
"
    )
}

#[test]
fn a_clock_rule_closes_the_conduit_and_water_backs_up() {
    let inp = control_model(
        "RULE R1
IF SIMULATION CLOCKTIME >= 2:00
THEN CONDUIT C1 STATUS = CLOSED",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    while sim.time() < 1.9 * 3600.0 {
        sim.step();
    }
    assert!(sim.flow("C1").unwrap() > 0.2, "flowing before the rule");
    let d_before = sim.depth("J1").unwrap();
    sim.run();
    // Closed: no flow, and the junction stores the continuing inflow.
    assert!(sim.flow("C1").unwrap().abs() < 1e-6, "conduit still flows");
    assert!(
        sim.depth("J1").unwrap() > d_before + 0.5,
        "no backup: {} vs {d_before}",
        sim.depth("J1").unwrap()
    );
    // The action log recorded the fired constant action once.
    let log = sim.control_actions();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].1, "C1");
    assert_eq!(log[0].3, "R1");
}

#[test]
fn priority_resolves_conflicting_rules_per_link() {
    // Both rules always fire on the same conduit; the higher priority
    // wins the pending slot (§9.1).
    let inp = control_model(
        "RULE R1
IF SIMULATION TIME >= 0
THEN CONDUIT C1 STATUS = CLOSED
PRIORITY 1

RULE R2
IF SIMULATION TIME >= 0
THEN CONDUIT C1 STATUS = OPEN
PRIORITY 5",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    assert!(
        sim.flow("C1").unwrap() > 0.2,
        "the higher-priority OPEN lost: {}",
        sim.flow("C1").unwrap()
    );
}

#[test]
fn a_depth_premise_throttles_an_orifice_with_else() {
    // The orifice half-closes while the junction is deep, reopens via
    // the ELSE branch once it drains — with conventional AND/OR
    // precedence in the premises.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1  100.0  4

[OUTFALLS]
O1  99.0  FREE

[ORIFICES]
OR1  J1  O1  SIDE  0  0.65  NO

[XSECTIONS]
OR1  CIRCULAR  0.5  0  0  0

[INFLOWS]
J1  FLOW  QIN

[TIMESERIES]
QIN  0:00  0.30
QIN  2:00  0.30
QIN  2:01  0.02
QIN  9:00  0.02

[CONTROLS]
RULE R1
IF NODE J1 DEPTH > 0.2
THEN ORIFICE OR1 SETTING = 0.25
ELSE ORIFICE OR1 SETTING = 1.0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    while sim.time() < 2.0 * 3600.0 {
        sim.step();
    }
    // Deep phase: the throttled orifice passes less than the inflow and
    // the junction keeps filling.
    let mid_flow = sim.flow("OR1").unwrap();
    assert!(
        mid_flow < 0.30,
        "throttled orifice passes the whole inflow: {mid_flow}"
    );
    assert!(sim.depth("J1").unwrap() > 1.5, "never got deep");
    sim.run();
    // Drained phase: the ELSE branch reopened the orifice, so the small
    // tail inflow passes at a shallow depth.
    assert!(
        sim.depth("J1").unwrap() < 0.2,
        "never drained: {}",
        sim.depth("J1").unwrap()
    );
}

#[test]
fn a_named_expression_premise_drives_an_action() {
    // The clock reaches the expression through a named variable: E > 0
    // exactly when the clock passes 02:00.
    let inp = control_model(
        "VARIABLE T = SIMULATION CLOCKTIME
EXPRESSION E = T * 24 - 2

RULE R1
IF E > 0
THEN CONDUIT C1 STATUS = CLOSED",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    while sim.time() < 1.9 * 3600.0 {
        sim.step();
    }
    assert!(sim.flow("C1").unwrap() > 0.2, "closed too early");
    sim.run();
    assert!(
        sim.flow("C1").unwrap().abs() < 1e-6,
        "expression premise never fired: {}",
        sim.flow("C1").unwrap()
    );
}

#[test]
fn a_pid_controller_regulates_depth_toward_the_set_point() {
    // A negative-gain PID throttles the orifice to hold the junction at
    // the 1 m set-point named by the rule's premise (§9.2).
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      06:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1  100.0  4

[OUTFALLS]
O1  99.0  FREE

[ORIFICES]
OR1  J1  O1  SIDE  0  0.65  NO

[XSECTIONS]
OR1  CIRCULAR  0.5  0  0  0

[INFLOWS]
J1  FLOW  QIN

[TIMESERIES]
QIN  0:00  0.20
QIN  9:00  0.20

[CONTROLS]
RULE R1
IF NODE J1 DEPTH <> 1.0
THEN ORIFICE OR1 SETTING = PID -0.5 0.1 0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let d = sim.depth("J1").unwrap();
    assert!(
        (d - 1.0).abs() < 0.15,
        "PID settled at {d} instead of the 1.0 set-point"
    );
    // And the orifice is genuinely throttled, not saturated.
    let q = sim.flow("OR1").unwrap();
    assert!((q - 0.20).abs() < 0.02, "not at steady throughflow: {q}");
}

// ── §8 constituent transport ────────────────────────────────────────────

/// One junction, one conduit, constant inflow at a declared
/// concentration, with the given decay (per day).
fn quality_model(decay_per_day: f64) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[POLLUTANTS]
TSS  MG/L  0  0  0  {decay_per_day}  NO

[INFLOWS]
J1  FLOW  QIN
J1  TSS   \"\"  CONCEN  1.0  1.0  100

[TIMESERIES]
QIN  0:00  0.25
QIN  9:00  0.25
"
    )
}

#[test]
fn a_conservative_constituent_arrives_at_its_inflow_concentration() {
    let inp = quality_model(0.0);
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    // Steady state: the whole path carries the 100 mg/L inflow.
    let c_link = sim.link_concentration("C1", "TSS").expect("conc");
    assert!((c_link - 100.0).abs() < 2.0, "link conc {c_link}");
    let c_out = sim.node_concentration("O1", "TSS").expect("conc");
    assert!((c_out - 100.0).abs() < 2.0, "outfall conc {c_out}");
    // The ledger conserves: admitted = discharged + still stored, with
    // nothing reacted.
    let (m_in, m_out, m_react, m_final) = sim.quality_ledger("TSS").expect("ledger");
    assert!(m_react.abs() < 1e-9 && m_final.abs() < 1e-6);
    assert!(
        (m_in - m_out) < 0.05 * m_in && m_in > m_out,
        "ledger: in {m_in} out {m_out}"
    );
}

#[test]
fn first_order_decay_attenuates_along_the_channel() {
    // 100 per day on a ~450 s residence: a visible, partial loss.
    let inp = quality_model(100.0);
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let c_out = sim.node_concentration("O1", "TSS").expect("conc");
    assert!(
        c_out > 30.0 && c_out < 90.0,
        "decayed concentration {c_out} out of range"
    );
    // The ledger closes: what came in either left, reacted, or is still
    // in the water.
    let (m_in, m_out, m_react, m_final) = sim.quality_ledger("TSS").expect("ledger");
    assert!(m_react > 0.0, "nothing reacted");
    let gap = m_in - m_out - m_react - m_final;
    assert!(
        gap.abs() < 0.05 * m_in,
        "ledger gap {gap} of {m_in} admitted"
    );
}

#[test]
fn sanitary_flow_carries_its_declared_concentration() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      02:00
ROUTING_STEP  10

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[POLLUTANTS]
BOD  MG/L  0  0  0  0  NO

[DWF]
J1  FLOW  0.1
J1  BOD   40

[TIMESERIES]
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let c = sim.link_concentration("C1", "BOD").expect("conc");
    assert!((c - 40.0).abs() < 1.5, "sanitary concentration {c}");
}

/// A one-parcel storm model with a land use, ready for quality sections.
fn washoff_model(extra: &str) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      08:00
ROUTING_STEP  10
WET_STEP      0:05:00
REPORT_STEP   0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
S1  G1  J1  2  100  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[POLLUTANTS]
TSS  MG/L  0  0  0  0  NO

[LANDUSES]
RES

[COVERAGES]
S1  RES  100

{extra}
[TIMESERIES]
RAIN  0:00  25
RAIN  1:00  25
RAIN  2:00  0
"
    )
}

#[test]
fn emc_washoff_carries_the_event_mean_concentration() {
    let inp = washoff_model(
        "[WASHOFF]
RES  TSS  EMC  50  0  0  0
",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    while sim.time() < 1.5 * 3600.0 {
        sim.step();
    }
    // Mid-storm the channel carries the event-mean concentration.
    let c = sim.link_concentration("C1", "TSS").expect("conc");
    assert!((c - 50.0).abs() < 5.0, "EMC concentration {c}");
    sim.run();
    // The ledger books the EMC load as a simultaneous accumulation
    // input, so admitted mass ≈ 50 mg/L × the runoff volume.
    let (m_in, _, _, _) = sim.quality_ledger("TSS").expect("ledger");
    let expect = 50.0 * 0.025 * 2.0 * 20_000.0;
    assert!(
        (m_in - expect).abs() < 0.15 * expect,
        "admitted {m_in} vs {expect}"
    );
}

#[test]
fn exponential_washoff_depletes_the_initial_loading() {
    // 40 kg/ha over 2 ha = 80 kg on the surface; hot washoff strips
    // essentially all of it into the storm.
    let inp = washoff_model(
        "[WASHOFF]
RES  TSS  EXP  0.5  1.2  0  0

[LOADINGS]
S1  TSS  40
",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    // 80 kg = 80 000 g admitted to the network (U = g for mg/L).
    let (m_in, m_out, _, _) = sim.quality_ledger("TSS").expect("ledger");
    assert!(
        (m_in - 80_000.0).abs() < 0.1 * 80_000.0,
        "admitted {m_in} of 80000"
    );
    // And it reaches the outfall.
    assert!(m_out > 0.8 * m_in, "discharged {m_out} of {m_in}");
}

#[test]
fn a_removal_treatment_halves_the_influent() {
    let inp = quality_model(0.0)
        + "
[TREATMENT]
J1  TSS  R = 0.5
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    // Half the 100 mg/L influent survives treatment at J1.
    let c = sim.link_concentration("C1", "TSS").expect("conc");
    assert!((c - 50.0).abs() < 3.0, "treated concentration {c}");
    // The removed half books as reacted mass, closing the ledger.
    let (m_in, m_out, m_react, _) = sim.quality_ledger("TSS").expect("ledger");
    assert!(
        (m_react - 0.5 * m_in).abs() < 0.1 * m_in,
        "reacted {m_react} of {m_in}"
    );
    assert!(
        (m_in - m_out - m_react).abs() < 0.1 * m_in,
        "ledger gap: in {m_in} out {m_out} reacted {m_react}"
    );
}

#[test]
fn a_concentration_treatment_caps_the_effluent() {
    let inp = quality_model(0.0)
        + "
[TREATMENT]
J1  TSS  C = 20
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let c = sim.link_concentration("C1", "TSS").expect("conc");
    assert!((c - 20.0).abs() < 2.0, "effluent concentration {c}");
}

#[test]
fn an_external_loading_series_accumulates_then_washes_off() {
    // 12 kg/ha/day lands on the surface through the loading series over
    // four dry hours (2 kg/ha over 2 ha = 4 kg), then the storm strips
    // it: the network admits ~4000 g.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      10:00
ROUTING_STEP  10
WET_STEP      0:05:00
DRY_STEP      0:30:00
REPORT_STEP   0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
S1  G1  J1  2  100  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[POLLUTANTS]
TSS  MG/L  0  0  0  0  NO

[LANDUSES]
RES

[COVERAGES]
S1  RES  100

[BUILDUP]
RES  TSS  EXT  100  1  LOAD  AREA

[WASHOFF]
RES  TSS  EXP  2.0  1.2  0  0

[TIMESERIES]
LOAD  0:00  12
LOAD  9:00  12
RAIN  0:00  0
RAIN  4:00  25
RAIN  6:00  0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let (m_in, m_out, _, _) = sim.quality_ledger("TSS").expect("ledger");
    assert!(
        (m_in - 4000.0).abs() < 0.25 * 4000.0,
        "admitted {m_in} of ~4000"
    );
    assert!(m_out > 0.7 * m_in, "discharged {m_out} of {m_in}");
}

// ── §14.9 binary output ─────────────────────────────────────────────────

#[test]
fn the_binary_output_writes_the_predecessor_layout() {
    let inp = quality_model(0.0)
        + "
[REPORT]
NODES  ALL
LINKS  ALL
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let mut buf = Vec::new();
    sim.write_out(&mut buf).expect("write");

    let i32_at = |o: usize| i32::from_le_bytes(buf[o..o + 4].try_into().unwrap());
    let f32_at = |o: usize| f32::from_le_bytes(buf[o..o + 4].try_into().unwrap());

    // Magic and version open the file; the epilog closes with the magic.
    assert_eq!(i32_at(0), 516_114_522);
    assert_eq!(i32_at(4), 52_004);
    assert_eq!(i32_at(8), 3, "CMS flow-units code");
    let n = buf.len();
    assert_eq!(i32_at(n - 4), 516_114_522);
    assert_eq!(i32_at(n - 8), 0, "error code");
    let n_periods = i32_at(n - 12) as usize;
    assert_eq!(n_periods, sim.snapshots.len());

    // Counts: 0 subcatchments, 2 nodes, 1 link, 1 pollutant.
    assert_eq!(i32_at(12), 0);
    assert_eq!(i32_at(16), 2);
    assert_eq!(i32_at(20), 1);
    assert_eq!(i32_at(24), 1);

    // The epilog's output offset locates the first period; its record is
    // a date followed by node then link then system floats.
    let out_start = i32_at(n - 16) as usize;
    let node_vars = 6 + 1;
    let link_vars = 5 + 1;
    let period_bytes = 8 + 4 * (2 * node_vars + link_vars + 15);
    assert_eq!(n - 24 - out_start, n_periods * period_bytes);

    // First period, first node's depth matches the first snapshot (CMS
    // files write SI values verbatim).
    let d = f32_at(out_start + 8);
    assert!(
        (f64::from(d) - sim.snapshots[0].depths[0]).abs() < 1e-5,
        "depth {d} vs {}",
        sim.snapshots[0].depths[0]
    );
    // The link's flow leads its block after both node blocks.
    let q = f32_at(out_start + 8 + 4 * 2 * node_vars);
    assert!(
        (f64::from(q) - sim.snapshots[0].flows[0]).abs() < 1e-4,
        "flow {q} vs {}",
        sim.snapshots[0].flows[0]
    );
}

// ── §11 conservation ────────────────────────────────────────────────────

#[test]
fn the_ledgers_close_over_a_storm() {
    // Rain-driven runoff through the network: the surface and network
    // balances close within a few percent, judged by their own §11.1
    // definitions.
    let inp = runoff_model(50.0, 25.0, "HORTON");
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let led = sim.ledgers();
    let surf = led.surface.expect("surface ledger");
    assert!(
        surf.error_percent.abs() < 5.0,
        "surface error {}% (in {} out {})",
        surf.error_percent,
        surf.inflow,
        surf.outflow
    );
    assert!(
        led.network.error_percent.abs() < 2.0,
        "network error {}% (in {} out {})",
        led.network.error_percent,
        led.network.inflow,
        led.network.outflow
    );
}

#[test]
fn the_constituent_and_loading_ledgers_close() {
    let inp = washoff_model(
        "[WASHOFF]
RES  TSS  EMC  50  0  0  0
",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let led = sim.ledgers();
    let (_, tss) = &led.constituents[0];
    assert!(
        tss.error_percent.abs() < 5.0,
        "constituent error {}% (in {} out {})",
        tss.error_percent,
        tss.inflow,
        tss.outflow
    );
    let (_, load) = &led.loading[0];
    assert!(
        load.error_percent.abs() < 5.0,
        "loading error {}% (in {} out {})",
        load.error_percent,
        load.inflow,
        load.outflow
    );
}

#[test]
fn the_admitted_load_splits_into_its_origins_without_loss() {
    // §11.2 splits the admitted load five ways. The split is only worth
    // printing if it partitions the total exactly — a report whose
    // inflow rows do not add up to what entered is worse than one that
    // never claimed to break it down. Asserted against a model whose
    // load arrives by two different routes, so more than one bucket is
    // non-zero and a mistake cannot hide in a single term.
    let inp = washoff_model(
        "[WASHOFF]
RES  TSS  EMC  50  0  0  0
",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();

    let (admitted, ..) = sim.quality_ledger("TSS").expect("ledger");
    let by_source = sim.quality_inflow_by_source("TSS").expect("split");
    let summed: f64 = by_source.iter().sum();
    assert!(
        (summed - admitted).abs() <= 1e-9 * admitted.abs().max(1.0),
        "origin split sums to {summed}, admitted {admitted}: {by_source:?}"
    );
    assert!(admitted > 0.0, "fixture admitted no load at all");
    // Wash-off is wet-weather load, so that bucket carries it — a split
    // that summed correctly while booking everything to one wrong
    // origin would pass the check above.
    let wet = by_source[MassSource::WetWeather.index()];
    assert!(
        wet > 0.9 * admitted,
        "wash-off booked as {wet} of {admitted} wet-weather load: {by_source:?}"
    );
}

#[test]
fn sanitary_and_declared_loads_book_to_their_own_origins() {
    // The corpus only ever admits wash-off, so wet weather is the one
    // bucket a real model exercises. A split that quietly booked
    // everything there would pass every other check in this file. This
    // model admits by two named routes at once — sanitary base flow and
    // a declared concentration — and asserts each lands in its own
    // bucket and nowhere else.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1  100.4  3
J2  100.2  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  J2  200  0.013  0  0
C2  J2  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
C2  RECT_OPEN  2  2  0  0

[TIMESERIES]
QIN  0:00  0.05
QIN  9:00  0.05
CIN  0:00  100.0
CIN  9:00  100.0

[POLLUTANTS]
TSS  MG/L  0  0  0  0  NO

[DWF]
J1  FLOW  0.05

[INFLOWS]
J2  FLOW  QIN
J2  TSS   CIN  CONCEN
";
    let (mut sim, _, findings) = Simulation::open(inp).expect("open");
    assert!(findings.iter().all(|f| !f.kind.is_error()), "{findings:?}");
    sim.run();

    let (admitted, ..) = sim.quality_ledger("TSS").expect("ledger");
    let by = sim.quality_inflow_by_source("TSS").expect("split");
    assert!(
        (by.iter().sum::<f64>() - admitted).abs() <= 1e-9 * admitted.abs().max(1.0),
        "origin split sums to {} against admitted {admitted}: {by:?}",
        by.iter().sum::<f64>()
    );
    // The declared concentration rides the external inflow, so that is
    // the bucket it lands in — and the only one.
    let ext = by[MassSource::External.index()];
    assert!(ext > 0.0, "declared load booked nothing external: {by:?}");
    for (i, v) in by.iter().enumerate() {
        if i != MassSource::External.index() {
            assert!(*v == 0.0, "declared load leaked into source {i}: {by:?}");
        }
    }
}

#[test]
fn the_subsurface_ledger_closes() {
    let inp = gw_model(0.01, 1.0, 99.0);
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let led = sim.ledgers();
    let gw = led.subsurface.expect("subsurface ledger");
    assert!(
        gw.error_percent.abs() < 3.0,
        "subsurface error {}% (in {} out {})",
        gw.error_percent,
        gw.inflow,
        gw.outflow
    );
}

#[test]
fn a_series_evaporation_holds_each_rate_stepwise() {
    // 240 mm/day of evaporation out-competes most of a 25 mm/h storm;
    // the series is a step function per §3.1.
    let inp = runoff_model(100.0, 25.0, "HORTON")
        + "
[EVAPORATION]
TIMESERIES  EVP
DRY_ONLY    NO
";
    let inp = inp
        + "
[TIMESERIES]
EVP  0:00  240
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let rain_vol = 0.025 * 2.0 * 20_000.0;
    let led = sim.report();
    assert!(
        led.inflow < 0.75 * rain_vol,
        "evaporation removed nothing: {} of {rain_vol}",
        led.inflow
    );
    // And the surface ledger books the evaporated share.
    let surf = sim.ledgers().surface.expect("ledger");
    assert!(
        surf.error_percent.abs() < 5.0,
        "surface error {}%",
        surf.error_percent
    );
}

// ── §6.7 initial conditions ─────────────────────────────────────────────

/// A routing-only model whose links differ in exactly the ways §6.7
/// distinguishes: two carry an initial flow into one junction with
/// different offsets there, one is dry but offset, one reaches a staged
/// outfall, and one reaches storage.
///
/// The two flow-carrying links are given the same slope — J5 sits 0.6 m
/// higher to pay for its own outlet offset — so they imply the same
/// normal depth and the averaging at J2 is a claim about offsets alone.
fn seeding_model(init_flow: f64) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    01/01/2024
START_TIME    00:00
END_DATE      01/01/2024
END_TIME      01:00
ROUTING_STEP  10

[JUNCTIONS]
J1  101    5
J2  100    5
J3  100    5  1.5
J4  100    5
J5  101.6  5

[STORAGE]
ST1  100  5  0  FUNCTIONAL  100  0  0

[OUTFALLS]
O1  99  FIXED  100.5

[CONDUITS]
C1  J1  J2   100  0.013  0    0    {init_flow}
C5  J5  J2   100  0.013  0    0.6  {init_flow}
C2  J2  J3   100  0.013  0.4  0    0
C3  J3  O1   100  0.013  0    0    0
C4  J4  ST1  100  0.013  0.7  0    0

[XSECTIONS]
C1  RECT_OPEN  3  2  0  0  2
C5  RECT_OPEN  3  2  0  0  2
C2  CIRCULAR   1.5  0  0  0
C3  CIRCULAR   1.5  0  0  0
C4  CIRCULAR   1.5  0  0  0

[REPORT]
"
    )
}

/// Manning normal depth in an open rectangle, solved here rather than
/// asked of the engine: $\psi = A R^{2/3}$ with $A = by$ and
/// $R = A/(b + 2y)$, bisected for the depth matching the section factor
/// the flow implies.
fn rect_normal_depth(psi: f64, width: f64) -> f64 {
    let (mut lo, mut hi) = (0.0_f64, 10.0_f64);
    for _ in 0..200 {
        let y = 0.5 * (lo + hi);
        let a = width * y;
        let r = a / (width + 2.0 * y);
        if a * r.powf(2.0 / 3.0) < psi {
            lo = y;
        } else {
            hi = y;
        }
    }
    0.5 * (lo + hi)
}

/// §6.7: a user-supplied initial channel flow implies Manning normal
/// depth, per barrel, and the vertex it comes from takes that depth.
#[test]
fn an_initial_flow_implies_the_normal_depth_it_carries() {
    let (sim, _, _) = Simulation::open(&seeding_model(4.0)).expect("open");
    assert!(
        (sim.flow("C1").expect("C1") - 4.0).abs() < 1e-12,
        "the channel carries the flow it was given"
    );
    // Four cumecs down two barrels is two each, on a slope of one in a
    // hundred, through a rectangle two metres wide.
    let psi = 0.013 * 2.0 / (0.01_f64).sqrt();
    let expected = rect_normal_depth(psi, 2.0);
    let j1 = sim.depth("J1").expect("J1");
    assert!(
        (j1 - expected).abs() < 1e-5,
        "J1 was seeded at {j1}, and the normal depth for {psi} is {expected}"
    );
    // The barrels matter: all four cumecs down one barrel would be deeper.
    assert!(
        rect_normal_depth(0.013 * 4.0 / (0.01_f64).sqrt(), 2.0) > expected + 1e-3,
        "the per-barrel split is doing something"
    );
}

/// §6.7: a vertex without a supplied depth takes the *average*, over the
/// links that carry an initial flow, of end depth plus that link's own
/// offset. J2 is reached by two such links whose outlet offsets differ.
#[test]
fn a_junction_averages_its_flowing_links_end_depths_and_offsets() {
    let (sim, _, _) = Simulation::open(&seeding_model(4.0)).expect("open");
    let psi = 0.013 * 2.0 / (0.01_f64).sqrt();
    let y = rect_normal_depth(psi, 2.0);

    // Both links imply the same depth; one lands 0.6 m up, so the mean
    // of the two offsets is 0.3.
    let j2 = sim.depth("J2").expect("J2");
    assert!(
        (j2 - (y + 0.3)).abs() < 1e-5,
        "J2 was seeded at {j2}, the mean of {y} and {} being {}",
        y + 0.6,
        y + 0.3
    );
    // Their upstream ends have one link each and no offset there.
    for v in ["J1", "J5"] {
        let d = sim.depth(v).expect(v);
        assert!(
            (d - y).abs() < 1e-5,
            "{v} was seeded at {d} rather than {y}"
        );
    }
}

/// §6.7: a vertex whose connecting links all start dry starts dry itself,
/// because an offset alone is geometry rather than water. J4's only link
/// is dry and offset 0.7 m; averaging that offset in would pour phantom
/// depth into the junction.
#[test]
fn a_vertex_reached_only_by_dry_links_starts_dry() {
    let (sim, _, _) = Simulation::open(&seeding_model(4.0)).expect("open");
    assert_eq!(
        0.0,
        sim.depth("J4").expect("J4"),
        "a dry link's offset is not water"
    );
    // And with no initial flow anywhere, nothing is seeded at all.
    let (dry, _, _) = Simulation::open(&seeding_model(0.0)).expect("open");
    for v in ["J1", "J2", "J5"] {
        assert_eq!(0.0, dry.depth(v).expect(v), "{v}");
    }
    assert_eq!(0.0, dry.flow("C1").expect("C1"));
}

/// §6.7: a supplied depth is used as given rather than averaged with what
/// the links imply.
#[test]
fn a_supplied_depth_is_not_averaged_away() {
    let (sim, _, _) = Simulation::open(&seeding_model(4.0)).expect("open");
    assert!(
        (sim.depth("J3").expect("J3") - 1.5).abs() < 1e-12,
        "J3 was given 1.5 m: {}",
        sim.depth("J3").unwrap()
    );
}

/// §6.7: an outfall is seeded from its own boundary condition, not from
/// its neighbours. A staged boundary is water standing against the outlet
/// before the run begins, and it belongs to the opening storage of §11.1
/// rather than arriving as volume created on the first step.
#[test]
fn a_staged_outfall_starts_holding_the_water_its_stage_implies() {
    let (sim, _, _) = Simulation::open(&seeding_model(4.0)).expect("open");
    assert!(
        (sim.depth("O1").expect("O1") - 1.5).abs() < 1e-9,
        "the outfall holds {} rather than its stage's 1.5 m",
        sim.depth("O1").unwrap()
    );
}

// ── §7.1 pump characteristics ───────────────────────────────────────────

/// A wet well two metres deep over a thousand square metres, pumping to a
/// stage three metres above its water surface. Every pump type reads a
/// different argument out of that one state: volume 2000, depth 2, head
/// difference 3.
fn pump_model(kind: &str, points: &str) -> String {
    pump_model_at_speed(kind, points, "")
}

fn pump_model_at_speed(kind: &str, points: &str, controls: &str) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    01/01/2024
START_TIME    00:00
END_DATE      01/01/2024
END_TIME      00:10
ROUTING_STEP  1
REPORT_STEP   00:01:00

[STORAGE]
ST1  100  6  2  FUNCTIONAL  0  0  1000

[OUTFALLS]
O1  99  FIXED  105

[JUNCTIONS]
J9  100  5

[CONDUITS]
C9  J9  O1  100  0.013  0  0  0

[XSECTIONS]
C9  CIRCULAR  1  0  0  0

[PUMPS]
P1  ST1  O1  PC1  ON  0  0

[CURVES]
PC1  {kind}  {points}

[CONTROLS]
{controls}

[REPORT]
"
    )
}

/// The pump's flow after one second, before the well has drawn down
/// enough to matter.
fn pump_flow(kind: &str, points: &str) -> f64 {
    let (mut sim, _, _) = Simulation::open(&pump_model(kind, points)).expect("open");
    sim.step();
    sim.flow("P1").expect("P1")
}

/// §7.1 type 3: the centrifugal characteristic, $q(H_2 - H_1)$,
/// interpolated linearly. The stage stands three metres above the well,
/// which on this curve is 0.38 m³/s.
#[test]
fn a_type_three_pump_reads_its_curve_at_the_head_difference() {
    let q = pump_flow("PUMP3", "0 0.5   5 0.3   10 0.1");
    // Between (0, 0.5) and (5, 0.3) at a head of 3.
    assert!(
        (q - 0.38).abs() < 1e-3,
        "{q} is not the curve at three metres"
    );
}

/// §7.1 type 5 is a variable-speed type 3, and at unit speed the affinity
/// scaling is the identity: the head is divided by one and the flow
/// multiplied by one.
#[test]
fn a_type_five_pump_matches_type_three_at_unit_speed() {
    let pts = "0 0.5   5 0.3   10 0.1";
    let (three, five) = (pump_flow("PUMP3", pts), pump_flow("PUMP5", pts));
    assert!(
        (three - five).abs() < 1e-12,
        "type 3 gave {three} and type 5 {five}"
    );
}

/// §7.1 types 1 and 2 are *stepwise*: the curve's value at the first
/// point whose abscissa exceeds the argument, not an interpolation. Type
/// 1 reads the wet well's volume and type 2 its depth, which is why the
/// two curves here carry the same flows against different abscissae.
#[test]
fn the_stepwise_pump_types_read_volume_and_depth_without_interpolating() {
    // Volume is 2000: the first abscissa beyond it is 3000.
    let q = pump_flow("PUMP1", "0 0.2   1500 0.4   3000 0.6");
    assert!(
        (q - 0.6).abs() < 1e-9,
        "type 1 gave {q}, not the step's 0.6"
    );

    // Depth is 2: the first abscissa beyond it is 3.
    let q = pump_flow("PUMP2", "0 0.2   1 0.4   3 0.7");
    assert!(
        (q - 0.7).abs() < 1e-9,
        "type 2 gave {q}, not the step's 0.7"
    );

    // Interpolating either would have given something between the
    // bracketing values instead.
    assert!(
        (pump_flow("PUMP1", "0 0.2   1500 0.4   3000 0.6") - 0.5).abs() > 0.05,
        "a stepwise curve is not interpolated"
    );
}

/// §7.1 type 4 is an in-line depth profile and *does* interpolate, which
/// is what separates it from type 2 on the same argument.
#[test]
fn a_type_four_pump_interpolates_on_depth() {
    let pts = "0 0.2   1 0.4   3 0.6";
    let q = pump_flow("PUMP4", pts);
    // Halfway from (1, 0.4) to (3, 0.6) at a depth of 2.
    assert!((q - 0.5).abs() < 1e-3, "{q} is not the interpolated 0.5");
    // The stepwise type on the identical curve reads the next point up.
    let stepwise = pump_flow("PUMP2", pts);
    assert!(
        (stepwise - 0.6).abs() < 1e-9 && (q - stepwise).abs() > 0.05,
        "type 2 gave {stepwise} and type 4 {q}: they read the same curve \
         differently"
    );
}

/// §7.1: type 5's affinity scaling is the whole of its difference from
/// type 3 — the head is divided by $\omega^2$ before the curve is read and
/// the flow multiplied by $\omega$ after. At unit speed that is the
/// identity, which is why the two agree above and why telling them apart
/// needs a pump running at anything else.
#[test]
fn the_affinity_scaling_separates_type_five_from_type_three() {
    let pts = "0 0.5   5 0.3   10 0.1";
    let half = "RULE R1\nIF SIMULATION TIME > 0\nTHEN PUMP P1 SETTING = 0.5";
    let flow = |kind: &str| {
        let (mut sim, _, _) =
            Simulation::open(&pump_model_at_speed(kind, pts, half)).expect("open");
        for _ in 0..3 {
            sim.step();
        }
        sim.flow("P1").expect("P1")
    };

    // Type 3 reads the curve at the real head of three metres and halves
    // the result: 0.38 becomes 0.19.
    let three = flow("PUMP3");
    assert!(
        (three - 0.19).abs() < 2e-3,
        "type 3 at half speed gave {three}"
    );

    // Type 5 reads it at 3/0.25 = 12 metres, past the curve's end, where
    // it clamps to 0.1 — and then halves that.
    let five = flow("PUMP5");
    assert!(
        (five - 0.05).abs() < 2e-3,
        "type 5 at half speed gave {five}"
    );
    assert!(
        three > 2.0 * five,
        "the affinity law makes a real difference: {three} against {five}"
    );
}

// ── §14.7 channel slope ─────────────────────────────────────────────────

/// One conduit of a stated length and drop, carrying an initial flow so
/// §6.7 seeds its upstream vertex at Manning normal depth. That depth is
/// the probe: it is a function of the section factor
/// $\psi = n q/\sqrt{S}$, so reading it back tells us which slope the
/// router computed.
fn slope_probe_model(length: f64, upper: f64, min_slope_percent: f64) -> String {
    slope_probe_model_at(length, upper, min_slope_percent, 2.0)
}

fn slope_probe_model_at(length: f64, upper: f64, min_slope_percent: f64, flow: f64) -> String {
    slope_probe_offset(length, upper, min_slope_percent, flow, 0.0, 0.0)
}

fn slope_probe_offset(
    length: f64,
    upper: f64,
    min_slope_percent: f64,
    flow: f64,
    off1: f64,
    off2: f64,
) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    01/01/2024
START_TIME    00:00
END_DATE      01/01/2024
END_TIME      01:00
ROUTING_STEP  10
MIN_SLOPE     {min_slope_percent}

[JUNCTIONS]
J1  {upper}  9
J2  100      9

[OUTFALLS]
O1  99  FREE

[CONDUITS]
C1  J1  J2  {length}  0.013  {off1}  {off2}  {flow}
C2  J2  O1  100       0.013  0  0  0

[XSECTIONS]
C1  RECT_OPEN  3  2  0  0  1
C2  RECT_OPEN  3  2  0  0  1

[REPORT]
"
    )
}

/// The slope the probe implies, recovered from the seeded depth.
fn slope_from_seed(depth: f64, flow: f64, width: f64, n: f64) -> f64 {
    let a = width * depth;
    let r = a / (width + 2.0 * depth);
    let psi = a * r.powf(2.0 / 3.0);
    let root = n * flow / psi;
    root * root
}

/// §14.7: a channel's slope is its drop over its *horizontal* run, not
/// over its length along the bed. The two agree for the long shallow
/// conduits of ordinary drainage and diverge sharply for a steep one:
/// six metres of drop over ten metres of pipe is a slope of 0.75, not
/// 0.6.
#[test]
fn a_channels_slope_is_its_drop_over_its_horizontal_run() {
    let (sim, _, _) = Simulation::open(&slope_probe_model(10.0, 106.0, 0.0)).expect("open");
    let seeded = sim.depth("J1").expect("J1");
    let got = slope_from_seed(seeded, 2.0, 2.0, 0.013);
    let dz: f64 = 6.0;
    let expected = dz / (10.0_f64 * 10.0 - dz * dz).sqrt();
    assert!(
        (got / expected - 1.0).abs() < 1e-3,
        "the router used a slope of {got}, and the horizontal run gives \
         {expected} where the bed length would give {}",
        dz / 10.0
    );
}

/// §14.7: the degenerate geometry whose drop exceeds its length has no
/// horizontal run to speak of, and falls back to the drop over the
/// length. Reading it the other way asks for the square root of a
/// negative number.
#[test]
fn a_drop_exceeding_the_length_falls_back_to_the_bed_slope() {
    let (sim, _, _) = Simulation::open(&slope_probe_model(5.0, 106.0, 0.0)).expect("open");
    let seeded = sim.depth("J1").expect("J1");
    assert!(seeded.is_finite() && seeded > 0.0, "seeded at {seeded}");
    let got = slope_from_seed(seeded, 2.0, 2.0, 0.013);
    assert!(
        (got / (6.0 / 5.0) - 1.0).abs() < 1e-3,
        "the router used {got} rather than the drop over the length, 1.2"
    );
}

/// §14.7: the slope is floored at the minimum-slope option, so a flat
/// channel routes at the floor rather than at nothing.
#[test]
fn a_flat_channel_takes_the_minimum_slope() {
    // Level inverts, and a floor of one percent.
    let (sim, _, _) = Simulation::open(&slope_probe_model(100.0, 100.0, 1.0)).expect("open");
    let seeded = sim.depth("J1").expect("J1");
    let got = slope_from_seed(seeded, 2.0, 2.0, 0.013);
    assert!(
        (got / 0.01 - 1.0).abs() < 1e-3,
        "a level channel routed at {got} rather than the one percent floor"
    );

    // And the floor does not hold a steeper channel back.
    let (steep, _, _) = Simulation::open(&slope_probe_model(100.0, 110.0, 1.0)).expect("open");
    let got = slope_from_seed(steep.depth("J1").expect("J1"), 2.0, 2.0, 0.013);
    assert!(
        got > 0.05,
        "a ten-metre drop is steeper than the floor: {got}"
    );
}

/// §14.7: a level channel still routes. The drop is floored at the
/// smallest the engine will represent — a thousandth of a foot — so the
/// slope is tiny rather than zero, and a zero would divide the section
/// factor by nothing.
#[test]
fn a_level_channel_takes_the_smallest_drop_rather_than_none() {
    let flow = 0.3;
    let (sim, _, _) =
        Simulation::open(&slope_probe_model_at(100.0, 100.0, 0.0, flow)).expect("open");
    let seeded = sim.depth("J1").expect("J1");
    assert!(seeded.is_finite() && seeded > 0.0, "seeded at {seeded}");
    let got = slope_from_seed(seeded, flow, 2.0, 0.013);
    let floor: f64 = 0.001 * 0.3048;
    let expected = floor / (100.0_f64 * 100.0 - floor * floor).sqrt();
    assert!(
        (got / expected - 1.0).abs() < 1e-2,
        "a level channel routed at {got} rather than the floored {expected}"
    );
}

/// §14.7: an adverse channel is reversed internally, and routes at the
/// same slope as the equivalent falling one. The reversal happens at
/// validation, which compares end elevations *including offsets* and
/// swaps the endpoints and the offsets together, so what this asserts is
/// that the reversal reaches the router intact.
#[test]
fn an_adverse_channel_uses_the_size_of_its_drop() {
    // The downstream invert six metres above the upstream one.
    let (adverse, _, _) = Simulation::open(&slope_probe_model(100.0, 94.0, 0.0)).expect("open");
    let (normal, _, _) = Simulation::open(&slope_probe_model(100.0, 106.0, 0.0)).expect("open");
    let a = slope_from_seed(adverse.depth("J1").expect("J1"), 2.0, 2.0, 0.013);
    let n = slope_from_seed(normal.depth("J1").expect("J1"), 2.0, 2.0, 0.013);
    assert!(
        (a / n - 1.0).abs() < 1e-3,
        "the adverse channel routed at {a} against the falling one's {n}"
    );
    assert!(
        a > 0.05,
        "and it is the real slope, not the flat floor: {a}"
    );
}

/// §14.7: the slope is measured between the *inverts the link actually
/// sits on*, which is each vertex's invert plus that end's offset. Two
/// level inverts with two metres of offset at one end are a bed that
/// falls two metres, and dropping either offset would leave it level and
/// route at the flat-channel floor instead.
#[test]
fn the_offsets_are_part_of_the_bed_the_slope_is_measured_along() {
    // Level inverts, and two metres of outlet offset: the bed falls
    // backwards by two metres over a hundred.
    let (sim, _, _) =
        Simulation::open(&slope_probe_offset(100.0, 100.0, 0.0, 2.0, 0.0, 2.0)).expect("open");
    let got = slope_from_seed(sim.depth("J1").expect("J1"), 2.0, 2.0, 0.013);
    let dz: f64 = 2.0;
    let expected = dz / (100.0_f64 * 100.0 - dz * dz).sqrt();
    assert!(
        (got / expected - 1.0).abs() < 1e-2,
        "the router used {got}; two metres of offset over a hundred is \
         {expected}, and ignoring the offsets would leave a level bed"
    );
    assert!(
        got > 0.01,
        "and it is a real slope, not the flat-channel floor: {got}"
    );
}

// ── §14.8 hotstart ──────────────────────────────────────────────────────

#[test]
fn a_hotstart_file_round_trips_the_running_state() {
    let inp = quality_model(0.0);
    let (mut a, _, _) = Simulation::open(&inp).expect("open");
    while a.time() < 2.0 * 3600.0 {
        a.step();
    }
    let mut buf = Vec::new();
    a.save_hotstart(&mut buf).expect("save");
    assert_eq!(&buf[..15], b"SWMM5-HOTSTART4");

    let (mut b, _, _) = Simulation::open(&inp).expect("open");
    b.load_hotstart(&buf).expect("load");
    // The restored session resumes at the running state: same depth,
    // flow, and concentration.
    let (da, db) = (a.depth("J1").unwrap(), b.depth("J1").unwrap());
    assert!((da - db).abs() < 1e-4, "depth {da} vs {db}");
    let (qa, qb) = (a.flow("C1").unwrap(), b.flow("C1").unwrap());
    assert!((qa - qb).abs() < 1e-4, "flow {qa} vs {qb}");
    let (ca, cb) = (
        a.link_concentration("C1", "TSS").unwrap(),
        b.link_concentration("C1", "TSS").unwrap(),
    );
    assert!((ca - cb).abs() < 0.5, "conc {ca} vs {cb}");
    // And it keeps routing from there rather than re-filling.
    b.step();
    let q = b.flow("C1").unwrap();
    assert!((q - qa).abs() < 0.05 * qa.max(0.01), "post-resume flow {q}");
}

/// A hotstart carries how wet the ground already is.
///
/// The round-trip test above watches the network — depth, flow,
/// concentration — and the surface's infiltration state reaches none of
/// those in the step it checks. So both halves of the infiltration
/// hotstart could be replaced by nothing at all and the whole workspace
/// stayed green, which would mean a resumed run infiltrating as if the
/// storm had not happened.
#[test]
fn a_hotstart_carries_how_wet_the_ground_already_is() {
    // Fully pervious, so infiltration is the whole story, and Horton,
    // whose capacity decays with wetting: f0 20 mm/h down to 5 mm/h.
    let inp = runoff_model(0.0, 25.0, "HORTON");
    let (mut a, _, _) = Simulation::open(&inp).expect("open");
    while a.time() < 3600.0 {
        a.step();
    }
    let mut saved = Vec::new();
    a.save_hotstart(&mut saved).expect("save");

    // The same first quarter hour of the same storm, once on ground the
    // hotstart says is already wet and once on dry ground.
    let quarter = |sim: &mut Simulation| {
        while sim.time() < 900.0 {
            sim.step();
        }
        sim.snapshots.last().expect("a reporting boundary").subcatch[0].infil
    };
    let (mut wet, _, _) = Simulation::open(&inp).expect("open");
    wet.load_hotstart(&saved).expect("load");
    let wet_rate = quarter(&mut wet);
    let (mut dry, _, _) = Simulation::open(&inp).expect("open");
    let dry_rate = quarter(&mut dry);

    // An hour of rain has taken the capacity most of the way from 20 mm/h
    // to its 5 mm/h floor, while dry ground still averages well above it.
    assert!(
        wet_rate < 0.7 * dry_rate,
        "restored ground infiltrates {} m/s, dry ground {} m/s: the \
         hotstart did not carry the wetting",
        wet_rate,
        dry_rate
    );
    assert!(dry_rate > 0.0, "dry ground infiltrates something");
}

/// A hotstart carries the water table.
///
/// The round-trip test above watches the network. Groundwater state
/// reaches none of what it checks, so `hotstart_set` on the aquifer could
/// be replaced by nothing at all and the whole workspace stayed green,
/// which would mean a resumed run starting from the model's initial water
/// table however long the previous run had been draining it.
#[test]
fn a_hotstart_carries_the_water_table() {
    let inp = gw_model(0.01, 1.5, 99.0);
    let (mut a, _, _) = Simulation::open(&inp).expect("open");
    while a.time() < 6.0 * 3600.0 {
        a.step();
    }
    let drained = a.snapshots.last().expect("a reporting boundary").subcatch[0].gw_elev;
    let mut saved = Vec::new();
    a.save_hotstart(&mut saved).expect("save");

    let first_elevation = |sim: &mut Simulation| {
        while sim.snapshots.is_empty() {
            sim.step();
        }
        sim.snapshots[0].subcatch[0].gw_elev
    };
    let (mut resumed, _, _) = Simulation::open(&inp).expect("open");
    resumed.load_hotstart(&saved).expect("load");
    let carried = first_elevation(&mut resumed);
    let (mut fresh, _, _) = Simulation::open(&inp).expect("open");
    let initial = first_elevation(&mut fresh);

    assert!(
        (carried - drained).abs() < (initial - drained).abs() * 0.25,
        "the resumed run started at {carried}, the saved table was \
         {drained} and the model's own is {initial}"
    );
}

/// A hotstart carries the snow pack, for the same reason and with the
/// same consequence: a resumed winter run would otherwise start bare.
#[test]
fn a_hotstart_carries_the_snow_pack() {
    let temps = "\
TEMP  0:00   -5
TEMP  48:00  -5";
    let inp = snow_model(temps);
    let (mut a, _, _) = Simulation::open(&inp).expect("open");
    while a.time() < 6.0 * 3600.0 {
        a.step();
    }
    let lying = a.snapshots.last().expect("a reporting boundary").subcatch[0].snow_depth;
    assert!(lying > 0.0, "six cold hours built a pack: {lying}");
    let mut saved = Vec::new();
    a.save_hotstart(&mut saved).expect("save");

    let first_depth = |sim: &mut Simulation| {
        while sim.snapshots.is_empty() {
            sim.step();
        }
        sim.snapshots[0].subcatch[0].snow_depth
    };
    let (mut resumed, _, _) = Simulation::open(&inp).expect("open");
    resumed.load_hotstart(&saved).expect("load");
    let carried = first_depth(&mut resumed);
    let (mut fresh, _, _) = Simulation::open(&inp).expect("open");
    let bare = first_depth(&mut fresh);

    assert!(
        carried > bare + 0.5 * lying,
        "the resumed run began with {carried} of snow against a fresh \
         run's {bare}, the saved pack being {lying}"
    );
}

#[test]
fn a_mismatched_hotstart_is_refused() {
    let (mut a, _, _) = Simulation::open(&quality_model(0.0)).expect("open");
    a.run();
    let mut buf = Vec::new();
    a.save_hotstart(&mut buf).expect("save");
    // A model with different object counts refuses the file.
    let (mut b, _, _) = Simulation::open(&runoff_model(100.0, 25.0, "HORTON")).expect("open");
    assert!(b.load_hotstart(&buf).is_err());
}

#[test]
fn the_text_report_carries_the_continuity_blocks() {
    let inp = washoff_model(
        "[WASHOFF]
RES  TSS  EMC  50  0  0  0
",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let mut buf = Vec::new();
    sim.write_report(&mut buf).expect("report");
    let rpt = String::from_utf8(buf).expect("utf8");
    for needle in [
        "Analysis Options",
        "Runoff Quantity Continuity",
        "Flow Routing Continuity",
        "Runoff Quality Continuity",
        "Quality Routing Continuity",
        "Routing Time Step Summary",
        "Continuity Error (%)",
        "Total Precipitation",
        "Wet Weather Inflow",
        "Subcatchment Runoff Summary",
        "Subcatchment Washoff Summary",
        "Node Depth Summary",
        "Node Inflow Summary",
        "Node Surcharge Summary",
        "Node Flooding Summary",
        "Outfall Loading Summary",
        "Link Flow Summary",
        "Flow Classification Summary",
        "Conduit Surcharge Summary",
        "Link Pollutant Load Summary",
    ] {
        assert!(rpt.contains(needle), "report missing '{needle}':\n{rpt}");
    }
    // The wet-weather inflow line carries a real volume: 1000 m³ of rain
    // mostly delivered, printed in hectare-metres for a CMS file.
    let line = rpt
        .lines()
        .find(|l| l.contains("Wet Weather Inflow"))
        .expect("line");
    let v: f64 = line
        .split_whitespace()
        .rev()
        .nth(1)
        .unwrap()
        .parse()
        .expect("volume");
    assert!((v - 0.1).abs() < 0.02, "wet-weather volume {v} ha-m");
}

#[test]
fn the_report_holds_the_predecessors_column_geometry() {
    // §14.9 makes the report a compatibility surface: tools parse it by
    // column position, so a heading rule or a field width that drifts
    // breaks readers even though every number is still right. The
    // geometry is asserted here because nothing else would notice.
    let inp = washoff_model(
        "[WASHOFF]
RES  TSS  EMC  50  0  0  0
",
    );
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let mut buf = Vec::new();
    sim.write_report(&mut buf).expect("report");
    let rpt = String::from_utf8(buf).expect("utf8");

    // A continuity block's asterisk rule is a fixed 26 wide whatever the
    // title's length, because the column headings sit on the same line
    // and a rule that tracked the title would shift them per block.
    let mut continuity_rules = 0;
    for l in rpt.lines() {
        let stars = l.chars().skip(2).take_while(|c| *c == '*').count();
        if stars > 0 && l.starts_with("  *") && l.len() > stars + 2 {
            assert_eq!(stars, 26, "continuity rule is not 26 wide: {l:?}");
            continuity_rules += 1;
        }
    }
    assert!(
        continuity_rules >= 6,
        "expected the continuity blocks' ruled headings, saw {continuity_rules}"
    );

    // Every continuity row's dot leader runs to column 28, so the first
    // value column always starts there.
    let mut leaders = 0;
    for l in rpt.lines() {
        if l.starts_with("  ") && l.contains(" ....") {
            assert!(
                l.len() > 28 && l[..28].ends_with('.'),
                "continuity leader does not reach column 28: {l:?}"
            );
            leaders += 1;
        }
    }
    assert!(leaders >= 10, "expected continuity rows, saw {leaders}");

    // Each table is bracketed by dashed rules of one width, so no rule
    // width should appear only once.
    // Scanned from the options block on, because the banner's rule is
    // not a table's and legitimately stands alone.
    let body = rpt.split("Analysis Options").nth(1).expect("options block");
    let mut widths: Vec<usize> = body
        .lines()
        .filter(|l| l.starts_with("  --") && l[2..].chars().all(|c| c == '-'))
        .map(str::len)
        .collect();
    assert!(
        widths.len() >= 8,
        "expected a rule per table, got {widths:?}"
    );
    widths.sort_unstable();
    for w in &widths {
        assert!(
            widths.iter().filter(|x| *x == w).count() >= 2,
            "rule width {w} appears once, so a table is missing a rule"
        );
    }
}

// ── §7.8 street inlets ──────────────────────────────────────────────────

#[test]
fn an_on_grade_inlet_splits_street_flow_to_the_sewer() {
    // A combination inlet on the street gutter captures part of a 2 cfs
    // street flow into the sewer; the rest bypasses. HEC-22 on-grade
    // relations, evaluated in their published units.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CFS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
J1   100  4
J2   99   4
SEW  90   8

[OUTFALLS]
O1  95  FREE
O2  85  FREE

[CONDUITS]
GUT1  J1   J2  300  0.016  0  0
C2    J2   O1  300  0.016  0  0
SEW1  SEW  O2  300  0.013  0  0

[XSECTIONS]
GUT1  STREET    ST1
C2    STREET    ST1
SEW1  CIRCULAR  1.5  0  0  0

[STREETS]
ST1  20  0.5  2  0.016  0.1  2  1  10  4  0.02

[INLETS]
CB1  GRATE  2  2  P_BAR-50
CB1  CURB   2  0.5  HORIZONTAL

[INLET_USAGE]
GUT1  CB1  SEW  1  0  0  0  0  ON_GRADE

[INFLOWS]
J1  FLOW  QIN

[TIMESERIES]
QIN  0:00  2.0
QIN  9:00  2.0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let q_in = 2.0 * 0.028_316_846_592;
    let q_sewer = sim.flow("SEW1").expect("sewer flow");
    let q_bypass = sim.flow("C2").expect("bypass flow");
    assert!(
        q_sewer > 0.2 * q_in,
        "inlet captured almost nothing: {q_sewer} of {q_in}"
    );
    assert!(
        q_bypass > 0.01 * q_in,
        "everything captured, nothing bypassed: {q_bypass}"
    );
    // Steady state conserves: sewer + bypass carries the inflow.
    assert!(
        (q_sewer + q_bypass - q_in).abs() < 0.05 * q_in,
        "split does not conserve: {q_sewer} + {q_bypass} vs {q_in}"
    );
}

// ── §3.1 climate records ────────────────────────────────────────────────

#[test]
fn hargreaves_evaporation_runs_from_supplied_climate_records() {
    // Constant 30/20 °C days at the equator: Hargreaves settles at
    // ≈ 4.3 mm/day. The records arrive through the caller-owned climate
    // channel; the parser reads the user format.
    let climate_text = "\
STA  2024  5  25  30  20
STA  2024  6  2   30  20
";
    let records = hydra_engine_uds::io::climate::parse_climate_file(climate_text).expect("parse");
    let inp = runoff_model(100.0, 10.0, "HORTON")
        + "
[EVAPORATION]
TEMPERATURE
DRY_ONLY  NO

[TEMPERATURE]
FILE  climate.txt
";
    let (mut sim, _, _) =
        hydra_engine_uds::simulation::Simulation::open_with_climate(&inp, records).expect("open");
    sim.run();
    // The surface ledger's evaporation side carries a real volume: the
    // ponded tail evaporates at the Hargreaves rate.
    let led = sim.ledgers();
    let surf = led.surface.expect("ledger");
    assert!(
        surf.error_percent.abs() < 5.0,
        "surface error {}%",
        surf.error_percent
    );
    let mut rpt = Vec::new();
    sim.write_report(&mut rpt).expect("report");
    let rpt = String::from_utf8(rpt).unwrap();
    let line = rpt
        .lines()
        .find(|l| l.contains("Evaporation Loss"))
        .expect("evap line");
    let v: f64 = line
        .split_whitespace()
        .rev()
        .nth(1)
        .unwrap()
        .parse()
        .expect("volume");
    // Hargreaves at the equator for constant 30/20 °C days ≈ 4.3 mm/day;
    // the wet surfaces can only sustain a fraction of the run at that
    // rate, so the booked volume is bounded, not merely positive.
    assert!(
        v > 0.0 && v < 0.05,
        "evaporation volume {v} ha-m implausible for 4.3 mm/day"
    );
}

#[test]
fn series_evaporation_is_a_step_function_not_interpolated() {
    // The series jumps 0 → 240 mm/day at 04:00. A step function keeps
    // evaporation at zero through the storm (00:00–02:00), so the full
    // rain volume reaches the network; linear interpolation would have
    // evaporated a visible share during the storm.
    let inp = runoff_model(100.0, 25.0, "HORTON")
        + "
[EVAPORATION]
TIMESERIES  EVS
DRY_ONLY    NO
";
    let inp = inp
        + "
[TIMESERIES]
EVS  0:00  0
EVS  4:00  240
EVS  9:00  240
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let rain_vol = 0.025 * 2.0 * 20_000.0;
    let led = sim.report();
    assert!(
        led.inflow > 0.9 * rain_vol,
        "storm-period evaporation should be zero under step semantics: {} of {rain_vol}",
        led.inflow
    );
}

#[test]
fn missing_climate_records_refuse_hargreaves_at_open() {
    let inp = runoff_model(100.0, 10.0, "HORTON")
        + "
[EVAPORATION]
TEMPERATURE
";
    assert!(Simulation::open(&inp).is_err());
}

// ── §14.8 routing interface files ───────────────────────────────────────

#[test]
fn routing_interface_files_chain_two_models() {
    // Model A discharges 0.25 m³/s at 100 mg/L; its outflow file drives
    // model B's boundary inflow, interpolated between periods.
    let (mut a, _, _) = Simulation::open(&quality_model(0.0)).expect("open A");
    a.run();
    let mut iface = Vec::new();
    a.write_routing_outflows(&mut iface).expect("write");
    let text = String::from_utf8(iface).expect("utf8");
    assert!(text.starts_with("SWMM5 Interface File"), "{text}");

    // Model B: same clock, an inflow-less junction fed by the file.
    let b_inp = "\
[OPTIONS]
FLOW_UNITS    CMS
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      04:00
ROUTING_STEP  10
REPORT_STEP   0:15:00

[JUNCTIONS]
O1  100.4  3

[OUTFALLS]
OB  100.0  FREE

[CONDUITS]
CB  O1  OB  200  0.013  0  0

[XSECTIONS]
CB  RECT_OPEN  2  2  0  0

[POLLUTANTS]
TSS  MG/L  0  0  0  0  NO
";
    let (mut b, _, _) = Simulation::open(b_inp).expect("open B");
    b.supply_routing_inflows(&text).expect("supply");
    b.run();
    // B's outfall carries A's discharge at A's concentration.
    let q = b.flow("CB").expect("flow");
    assert!((q - 0.25).abs() < 0.03, "chained flow {q}");
    let c = b.link_concentration("CB", "TSS").expect("conc");
    assert!((c - 100.0).abs() < 8.0, "chained concentration {c}");
}

// ── §3.4 control measures ───────────────────────────────────────────────

#[test]
fn a_bioretention_cell_detains_and_sheds_runoff() {
    let base = runoff_model(100.0, 25.0, "HORTON");
    // Deploy a 2000 m² bio-retention cell capturing 60 % of the
    // impervious runoff.
    let inp = base
        + "
[LID_CONTROLS]
BC1  BC
BC1  SURFACE  150  0.1  0.1  1.0  5
BC1  SOIL     600  0.5  0.2  0.1  50  10  60
BC1  STORAGE  300  0.6  20  0
BC1  DRAIN    1.0  0.5  50  0  0  0

[LID_USAGE]
S1  BC1  1  2000  20  0  60  0
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    sim.run();
    let led = sim.report();
    // The plain model delivers ≈ rain volume; the cell holds back and
    // exfiltrates a meaningful share.
    let rain_vol = 0.025 * 2.0 * 20_000.0;
    assert!(
        led.inflow < 0.85 * rain_vol,
        "cell captured nothing: {} of {rain_vol}",
        led.inflow
    );
    assert!(
        led.inflow > 0.2 * rain_vol,
        "cell swallowed everything: {}",
        led.inflow
    );
}

#[test]
fn a_swale_attenuates_and_infiltrates_captured_runoff() {
    // A fully impervious 2 ha parcel routes all runoff through a
    // 1000 m² vegetative swale on a Green–Ampt parcel, so the swale
    // clones the parcel's infiltration parameters.
    let swale = "
[LID_CONTROLS]
SW1  VS
SW1  SURFACE  500  0  0.24  0.1  3

[LID_USAGE]
S1  SW1  1  1000  10  0  100  0
";
    let base = "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  GREEN_AMPT
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      08:00
ROUTING_STEP  10
WET_STEP      0:05:00
REPORT_STEP   0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
S1  G1  J1  2  100  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  100  10  0.3

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[TIMESERIES]
RAIN  0:00  25
RAIN  1:00  25
RAIN  2:00  0
";
    let (mut plain, _, _) = Simulation::open(base).expect("open plain");
    while plain.time() < 2.5 * 3600.0 {
        plain.step();
    }
    let plain_early = plain.report().inflow;
    plain.run();
    let plain_vol = plain.report().inflow;

    let (mut sim, _, _) = Simulation::open(&format!("{base}{swale}")).expect("open swale");
    // Shortly after the storm the swale is still holding water back —
    // its delivered fraction lags the plain model's at the same clock.
    while sim.time() < 2.5 * 3600.0 {
        sim.step();
    }
    let early = sim.report().inflow;
    sim.run();
    let led = sim.report();
    assert!(
        early / led.inflow < 0.95 * (plain_early / plain_vol),
        "no attenuation: swale {early}/{} vs plain {plain_early}/{plain_vol}",
        led.inflow
    );
    // Green–Ampt exfiltration through the swale bed trims the total, but
    // the swale is a conveyance, not a sink.
    assert!(
        led.inflow < 0.98 * plain_vol,
        "swale infiltrated nothing: {} vs plain {plain_vol}",
        led.inflow
    );
    assert!(
        led.inflow > 0.5 * plain_vol,
        "swale swallowed the storm: {} vs plain {plain_vol}",
        led.inflow
    );
}

#[test]
fn runon_reaches_a_full_footprint_swale_parcel() {
    // S1 (1 ha, impervious) drains onto S2, whose 1000 m² footprint is
    // entirely one vegetative swale — the §3.4 gate must hand S1's
    // run-on to the unit instead of dropping it with the vanished
    // ordinary sub-areas.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  GREEN_AMPT
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      08:00
ROUTING_STEP  10
WET_STEP      0:05:00
REPORT_STEP   0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
S1  G1  S2  1    100  100  0.5  0
S2  G1  J1  0.1  100  10   0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET
S2  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  100  10  0.3
S2  100  10  0.3

[LID_CONTROLS]
SW1  VS
SW1  SURFACE  500  0  0.24  0.1  3

[LID_USAGE]
S2  SW1  1  1000  10  0  0  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[TIMESERIES]
RAIN  0:00  25
RAIN  1:00  25
RAIN  2:00  0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let led = sim.report();
    // 500 m³ falls on S1 and 50 m³ directly on the swale. If the gate
    // dropped run-on, at most the direct 50 m³ could ever arrive.
    let rain_vol = 0.025 * 2.0 * 11_000.0;
    assert!(
        led.inflow > 0.5 * rain_vol,
        "run-on never traversed the swale: {} of {rain_vol}",
        led.inflow
    );
    assert!(
        led.inflow < 0.99 * rain_vol,
        "the swale infiltrated nothing: {} of {rain_vol}",
        led.inflow
    );
}

#[test]
fn a_rain_barrel_holds_the_storm_then_drains_after_the_delay() {
    let base = runoff_model(100.0, 10.0, "HORTON");
    // Barrels big enough for the whole storm: 1 m deep, covering 400 m²
    // effective, catching all impervious runoff; drains open an hour
    // after rain ends.
    let inp = base
        + "
[LID_CONTROLS]
RB1  RB
RB1  STORAGE  1000  1000  0  0  YES
RB1  DRAIN    20   0.5   0  1  0  0

[LID_USAGE]
S1  RB1  400  1  1  0  100  0
";
    let (mut sim, _, _) = Simulation::open(&inp).expect("open");
    // Through the storm and just past — but before the 1 h drain delay
    // elapses at t = 3 h: barrels holding, little at the outfall.
    while sim.time() < 2.5 * 3600.0 {
        sim.step();
    }
    let held = sim.report().inflow;
    let rain_vol = 0.010 * 2.0 * 20_000.0;
    assert!(
        held < 0.25 * rain_vol,
        "barrels leaked during the storm: {held} of {rain_vol}"
    );
    // By run end (8 h) the delayed drains have released the store.
    sim.run();
    assert!(
        sim.report().inflow > 0.7 * rain_vol,
        "barrels never drained: {} of {rain_vol}",
        sim.report().inflow
    );
}

#[test]
fn treatment_booking_closes_the_mass_ledger() {
    // A rain-fed treated junction whose stored water dilutes the influent.
    // The removal booked must be the concentration drop over the mixed
    // pool (§8.5), not influent-plus-store — the distinction is worth ~9%
    // of this model's ledger, since (c_in − c_mix)·Q·dt is positive all
    // through the rising limb.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CFS
START_DATE    01/01/2004
START_TIME    00:00:00
END_TIME      06:00:00
REPORT_STEP   00:15:00
WET_STEP      00:05:00
ROUTING_STEP  20
INFILTRATION  HORTON

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  RAIN1

[SUBCATCHMENTS]
S1  G1  J1  10  25  500  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  100  4

[OUTFALLS]
O1  98  FREE

[CONDUITS]
C1  J1  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0

[POLLUTANTS]
TSS  MG/L  10  0  0  0

[TREATMENT]
J1  TSS  R = 0.5 * FLOW

[TIMESERIES]
RAIN1  0:00  1.0
RAIN1  1:00  0.0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let (m_in, m_out, m_react, m_final) = sim.quality_ledger("TSS").expect("ledger");
    assert!(m_react > 0.0, "treatment removed nothing");
    assert!(m_out > 0.0, "nothing discharged");
    let gap = m_in - m_out - m_react - m_final;
    assert!(
        gap.abs() < 0.01 * m_in,
        "ledger gap {gap} of {m_in} admitted (out {m_out}, reacted {m_react})"
    );
}

#[test]
fn a_storage_unit_evaporates_and_seeps_per_its_declaration() {
    // §7.7: evaporation at the potential rate times the realisation
    // fraction, seepage at the declared conductivity, both on the
    // start-of-step surface area. 4.8 in/day evap + 0.5 in/hr seepage
    // over two hours ≈ 1.4 in off a constant-area tank.
    let inp = "\
[OPTIONS]
FLOW_UNITS    CFS
START_DATE    01/01/2004
START_TIME    00:00:00
END_TIME      02:00:00
REPORT_STEP   00:15:00
ROUTING_STEP  20

[EVAPORATION]
CONSTANT  4.8

[STORAGE]
S1  100  10  4  FUNCTIONAL  0  0  1000  0  1.0  0  0.5  0

[OUTFALLS]
O1  90  FREE

[WEIRS]
W1  S1  O1  TRANSVERSE  8  3.3

[XSECTIONS]
W1  RECT_OPEN  1  4  0  0
";
    let (mut sim, _, _) = Simulation::open(inp).expect("open");
    sim.run();
    let d = sim.depth("S1").expect("depth");
    let expected = (4.0 - 1.4 / 12.0) * 0.3048;
    assert!(
        (d - expected).abs() < 0.01,
        "storage depth {d} m, expected ≈ {expected}"
    );
}

#[test]
fn identifiers_match_case_insensitively() {
    // §14.2: the predecessor's hash table ignores case — a reference in a
    // different case resolves, and a case-only redeclaration is refused
    // as the duplicate it is.
    let inp = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
Node1  100  4

[OUTFALLS]
O1  98  FREE

[CONDUITS]
C1  NODE1  o1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0
";
    let (sim, _, _) = Simulation::open(inp).expect("mixed-case references must resolve");
    assert!(sim.depth("Node1").is_some());

    let dup = "\
[OPTIONS]
FLOW_UNITS  CFS

[JUNCTIONS]
J1  100  4
j1  100  4

[OUTFALLS]
O1  98  FREE

[CONDUITS]
C1  J1  O1  400  0.013  0  0

[XSECTIONS]
C1  CIRCULAR  1.5  0  0  0
";
    assert!(
        Simulation::open(dup).is_err(),
        "a case-only redeclaration is a duplicate"
    );
}

/// §6.4's continuity allowance must not collapse with the flow.
///
/// The dry-weather regression: after a storm, a network spends hours
/// draining its runoff recession tail — a trickle of ~1 L/s spread over
/// every junction, re-perturbed by each hydrology interval so the routing
/// iterates never settle to machine noise. Under a purely relative mass
/// allowance that regime is unconvergeable: the allowance shrinks with the
/// flow while the residual's floor (the settled iterates' own noise across
/// the vertices) does not, so every trial fails criterion 2, every step
/// halves to the 0.5 s floor, and every floor step carries the
/// degraded-accuracy warning. This 12-hour model took 30 629 steps with
/// 16 644 degraded before the ε_H term entered the allowance; it takes
/// ~900 clean steps after.
///
/// Constant-inflow miniatures do NOT reproduce this — with a fixed
/// boundary the iterates settle to machine noise and the old gate passes.
/// The rainfall-driven hydrology is load-bearing.
#[test]
fn a_runoff_recession_tail_converges_at_full_steps() {
    let inp = "\
[TITLE]
Runoff recession tail through a small circular chain

[OPTIONS]
FLOW_UNITS           CFS
INFILTRATION         HORTON
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/1998
START_TIME           00:00:00
END_DATE             01/01/1998
END_TIME             12:00:00
REPORT_STEP          01:00:00
WET_STEP             00:15:00
DRY_STEP             01:00:00
ROUTING_STEP         0:01:00
VARIABLE_STEP        0.75

[RAINGAGES]
RG1  INTENSITY  1:00  1.0  TIMESERIES TS1

[SUBCATCHMENTS]
S1  RG1  J1  10  50  500  0.01  0
S2  RG1  J2  5   50  500  0.01  0
S3  RG1  J3  15  10  500  0.01  0

[SUBAREAS]
S1  0.001  0.10  0.05  0.05  25  OUTLET
S2  0.001  0.10  0.05  0.05  25  OUTLET
S3  0.001  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  0.7  0.3  4.14  0.50  0
S2  0.7  0.3  4.14  0.50  0
S3  0.7  0.3  4.14  0.50  0

[JUNCTIONS]
J1  1000  3  0  0  0
J2  995   3  0  0  0
J3  990   3  0  0  0

[OUTFALLS]
O1  985  FREE

[CONDUITS]
C1  J1  J2  400  0.01  0  0  0  0
C2  J2  J3  400  0.01  0  0  0  0
C3  J3  O1  400  0.01  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  1    0  0  0  1
C2  CIRCULAR  1.5  0  0  0  1
C3  CIRCULAR  1.5  0  0  0  1

[TIMESERIES]
TS1  0:00  0.4
TS1  1:00  0.4
TS1  2:00  0.0
TS1  12:00 0.0

[REPORT]
";
    let (mut sim, diags, findings) = Simulation::open(inp).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    assert!(!findings.iter().any(|f| f.kind.is_error()), "{findings:?}");
    sim.run();
    let led = sim.report();

    // No degraded-accuracy steps anywhere: the tail converges, it does not
    // get force-accepted at the floor.
    assert!(
        led.degraded.is_empty(),
        "recession tail degraded {} steps (first {:?})",
        led.degraded.len(),
        led.degraded.first()
    );
    // Steps stay near the user step; the pinned run is ~30 000.
    assert!(
        led.accepted < 5_000,
        "accepted {} steps — the run pinned at the floor",
        led.accepted
    );
    assert!(
        led.rejected < 200,
        "rejected {} trials — criterion 2 is fighting the noise floor",
        led.rejected
    );
    // It rained and the water left: this is a real run, not a dry no-op.
    assert!(led.outflow > 0.0, "nothing reached the outfall");
}

/// A file-sourced gage with its record supplied at load is the equivalent
/// series (§3.1) — held to the same model with the record inlined, run for
/// run, ledger for ledger.
#[test]
fn a_supplied_rain_record_equals_the_inline_series() {
    // Volume form at a 5-minute interval, absolute dates, a wet hour with
    // a dry gap — enough structure that an interval mix-up would show.
    let base = |gage_line: &str, series: &str| {
        format!(
            "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
START_DATE    06/29/2012
START_TIME    00:00
END_DATE      06/29/2012
END_TIME      06:00
ROUTING_STEP  10
WET_STEP      0:05:00
REPORT_STEP   0:15:00

[RAINGAGES]
{gage_line}

[SUBCATCHMENTS]
S1  G1  J1  2  60  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
{series}
"
        )
    };

    let wet = [
        ("00:05", 1.2),
        ("00:10", 0.8),
        ("00:15", 2.4),
        // A dry gap, then a second burst.
        ("01:30", 3.0),
        ("01:35", 1.0),
    ];
    let inline_series: String = std::iter::once("\n[TIMESERIES]\n".to_string())
        .chain(
            wet.iter()
                .map(|(t, v)| format!("RAIN  06/29/2012  {t}  {v}\n")),
        )
        .collect();
    let inline = base("G1  VOLUME  0:05  1.0  TIMESERIES  RAIN", &inline_series);

    let record: String = wet
        .iter()
        .map(|(t, v)| {
            let (h, m) = t.split_once(':').unwrap();
            format!("sta7 2012 6 29 {h} {m} {v}\n")
        })
        .chain(std::iter::once(
            // Another station's readings, to be ignored.
            "other 2012 6 29 0 5 99.0\n".to_string(),
        ))
        .collect();
    let filed = base("G1  VOLUME  0:05  1.0  FILE  \"rain.dat\"  sta7  MM", "");

    let readings = hydra_engine_uds::io::rain::parse_rain_file(&record).expect("record parses");
    let (mut sim_inline, _, _) = Simulation::open(&inline).expect("inline opens");
    let (mut sim_filed, _, _) =
        Simulation::open_with_files(&filed, Vec::new(), vec![("rain.dat".to_string(), readings)])
            .expect("filed opens");
    sim_inline.run();
    sim_filed.run();

    let a = sim_inline.ledgers();
    let b = sim_filed.ledgers();
    let (sa, sb) = (a.surface.expect("surface"), b.surface.expect("surface"));
    assert_eq!(sa.inflow, sb.inflow, "rain volumes differ");
    assert_eq!(sa.outflow, sb.outflow);
    assert_eq!(a.network.inflow, b.network.inflow);
    assert_eq!(a.network.outflow, b.network.outflow);
    assert!(sa.inflow > 0.0, "the storm actually rained");
}

/// A gage naming a record nobody supplied refuses the load, naming the
/// file — absent rain data is a missing input, never a dry model.
#[test]
fn an_unsupplied_rain_record_refuses_the_load() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
ROUTING_STEP  10

[RAINGAGES]
G1  VOLUME  0:05  1.0  FILE  \"missing.dat\"  sta1  MM

[SUBCATCHMENTS]
S1  G1  J1  2  60  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
";
    let err = Simulation::open(inp).err().expect("refuses");
    let msg = format!("{err:?}");
    assert!(msg.contains("missing.dat"), "{msg}");
    assert!(msg.contains("was not supplied"), "{msg}");
}

/// Multi-pollutant hotstart files restore completely (§14.8): the reader
/// consumes the layout the writer actually emits — P doubles per buildup
/// slot, the leading one the value — so nothing after the first land-use
/// block misaligns. Save → load → save is byte-identical, which no reader
/// with the wrong stride could manage; the predecessor's own reader
/// cannot read these files.
#[test]
fn a_multi_pollutant_hotstart_round_trips_buildup() {
    let inp = "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
START_DATE    06/01/2024
START_TIME    00:00
END_DATE      06/01/2024
END_TIME      08:00
DRY_DAYS      5
ROUTING_STEP  10
WET_STEP      0:05:00
REPORT_STEP   0:15:00

[RAINGAGES]
G1  INTENSITY  1:00  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
S1  G1  J1  2  100  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0

[POLLUTANTS]
TSS   MG/L  0  0  0  0  NO
LEAD  UG/L  0  0  0  0  NO  TSS  0.2

[LANDUSES]
RES

[COVERAGES]
S1  RES  100

[BUILDUP]
RES  TSS  POW  50  2  1  AREA

[WASHOFF]
RES  TSS  EXP  0.1  1  0  0

[TIMESERIES]
RAIN  0:00  25
RAIN  1:00  25
RAIN  2:00  0
";
    let (mut a, _, _) = Simulation::open(inp).expect("open");
    while a.time() < 2.0 * 3600.0 {
        a.step();
    }
    let mut saved = Vec::new();
    a.save_hotstart(&mut saved).expect("save");

    let (mut b, _, _) = Simulation::open(inp).expect("open");
    b.load_hotstart(&saved).expect("multi-pollutant restore");
    let mut resaved = Vec::new();
    b.save_hotstart(&mut resaved).expect("resave");
    // The routing block's lateral-inflow floats are runtime forcing the
    // restore deliberately discards, so equality is asserted on the
    // runoff block — everything before routing, buildup included. Its
    // length: the whole file minus the computable routing block (2
    // non-storage vertices and 1 link, np = 2 as f32s).
    let routing_len = 2 * (8 + 2 * 4) + (12 + 2 * 4);
    let runoff_a = &saved[..saved.len() - routing_len];
    let runoff_b = &resaved[..resaved.len() - routing_len];
    assert_eq!(saved.len(), resaved.len());
    assert_eq!(
        runoff_a, runoff_b,
        "restore lost or misaligned runoff state"
    );

    // And the restored session resumes, not restarts.
    let (da, db) = (a.depth("J1").unwrap(), b.depth("J1").unwrap());
    assert!((da - db).abs() < 1e-4, "depth {da} vs {db}");

    // §11.1: restored storage is the run's starting storage — the surface
    // ledger closes over a hotstarted run instead of booking the restored
    // ponded water as volume from nowhere.
    b.run();
    let surf = b.ledgers().surface.expect("surface ledger");
    assert!(
        surf.error_percent.abs() < 2.0,
        "surface error {}% after restore (in {} out {})",
        surf.error_percent,
        surf.inflow,
        surf.outflow
    );
}

/// A restore carries the water *quality* state, not just the water.
///
/// Two defects this pins, both invisible to a byte-identity check on the
/// saved file (§14.8 restores the bytes correctly; what follows was
/// wrong):
///
/// The §8.4 mixing form reads the previous step's volume to tell a vertex
/// holding water from an empty one. Left at the volumes the session was
/// *built* with — cold and dry — every restored vertex looked empty on
/// the first step, and its concentration was replaced by the inflow
/// mixture: a conduit restored at 1.30 mg/L read 0.006 one step later.
///
/// And the loading ledger opened from the buildup the model was built
/// with rather than the buildup restored over it, reporting ~99% error
/// on a run whose cold twin closes.
#[test]
fn a_hotstart_restores_quality_state_not_just_water() {
    // Five antecedent dry days so there is buildup to wash off — the
    // state whose ledger baseline the restore has to re-take.
    let inp = washoff_model(
        "[BUILDUP]
RES  TSS  POW  50  2  1  AREA

[WASHOFF]
RES  TSS  EXP  0.1  1  0  0
",
    )
    .replace(
        "INFILTRATION  HORTON",
        "INFILTRATION  HORTON\nDRY_DAYS      5",
    );
    // Mid-storm: the network is wet and carrying load, which is the
    // state a restore has to preserve.
    let (mut a, _, _) = Simulation::open(&inp).expect("open");
    while a.time() < 1.5 * 3600.0 {
        a.step();
    }
    let mut saved = Vec::new();
    a.save_hotstart(&mut saved).expect("save");
    let restored_conc = a.link_concentration("C1", "TSS").expect("conc");
    assert!(restored_conc > 0.0, "the storm actually carried load");

    let (mut b, _, _) = Simulation::open(&inp).expect("open");
    b.load_hotstart(&saved).expect("load");

    // One step must not wipe what the restore just loaded: the two
    // sessions stay together rather than the restored one collapsing to
    // its inflow mixture.
    a.step();
    b.step();
    let (ca, cb) = (
        a.link_concentration("C1", "TSS").unwrap(),
        b.link_concentration("C1", "TSS").unwrap(),
    );
    assert!(
        (ca - cb).abs() < 0.05 * ca.max(1e-9),
        "restored concentration diverged after one step: {ca} vs {cb}",
    );

    // And the ledgers close over the hotstarted run — the loading ledger
    // opening from the restored buildup, not the discarded one.
    b.run();
    let led = b.ledgers();
    let (_, load) = &led.loading[0];
    assert!(
        load.error_percent.abs() < 5.0,
        "loading error {}% after restore (in {} out {})",
        load.error_percent,
        load.inflow,
        load.outflow
    );
    let (_, tss) = &led.constituents[0];
    assert!(
        tss.error_percent.abs() < 5.0,
        "constituent error {}% after restore",
        tss.error_percent
    );
}

/// A realised record is the model's own series in every respect (§3.1) —
/// including the §5 mutations and findings validation applies to one.
///
/// The wet-step reduction is the observable case: a gage recording finer
/// than `WET_STEP` reduces it, and says so. Realising after validation
/// let a file-sourced gage skip that, running a coarser hydrology step
/// than the identical model with its record inlined — the equivalence
/// this engine claims, quietly untrue.
#[test]
fn a_realised_record_gets_the_same_validation_as_an_inline_series() {
    let model = |gage_line: &str, series: &str| {
        format!(
            "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
START_DATE    06/29/2012
START_TIME    00:00
END_DATE      06/29/2012
END_TIME      02:00
ROUTING_STEP  10
WET_STEP      0:15:00
REPORT_STEP   0:15:00

[RAINGAGES]
{gage_line}

[SUBCATCHMENTS]
S1  G1  J1  2  60  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
{series}
"
        )
    };
    let wet = [("00:05", 1.2), ("00:10", 0.8), ("00:15", 2.4)];
    let inline_series: String = std::iter::once("\n[TIMESERIES]\n".to_string())
        .chain(
            wet.iter()
                .map(|(t, v)| format!("RAIN  06/29/2012  {t}  {v}\n")),
        )
        .collect();
    let (_, _, inline_findings) = Simulation::open(&model(
        "G1  VOLUME  0:05  1.0  TIMESERIES  RAIN",
        &inline_series,
    ))
    .expect("inline opens");

    let record: String = wet
        .iter()
        .map(|(t, v)| {
            let (h, m) = t.split_once(':').unwrap();
            format!("sta7 2012 6 29 {h} {m} {v}\n")
        })
        .collect();
    let readings = hydra_engine_uds::io::rain::parse_rain_file(&record).expect("record parses");
    let (_, _, filed_findings) = Simulation::open_with_files(
        &model("G1  VOLUME  0:05  1.0  FILE  \"rain.dat\"  sta7  MM", ""),
        Vec::new(),
        vec![("rain.dat".to_string(), readings)],
    )
    .expect("filed opens");

    // The same finding, for the same reason, on both paths.
    let mentions_wet_step = |fs: &[hydra_engine_uds::io::validate::ValidationDiagnostic]| {
        fs.iter()
            .filter(|f| f.to_string().contains("wet-weather step"))
            .count()
    };
    assert_eq!(
        mentions_wet_step(&inline_findings),
        1,
        "the inline model reduces its wet step: {inline_findings:?}"
    );
    assert_eq!(
        mentions_wet_step(&filed_findings),
        mentions_wet_step(&inline_findings),
        "a realised record must reduce it too: {filed_findings:?}"
    );
}

/// A record supplied for the wrong station is a missing input, not a dry
/// model — and station ids compare without case, as every other id does.
#[test]
fn a_record_matching_no_station_refuses_and_case_does_not_matter() {
    let model = |station: &str| {
        format!(
            "\
[OPTIONS]
FLOW_UNITS    CMS
INFILTRATION  HORTON
ROUTING_STEP  10

[RAINGAGES]
G1  VOLUME  0:05  1.0  FILE  \"rain.dat\"  {station}  MM

[SUBCATCHMENTS]
S1  G1  J1  2  60  100  0.5  0

[SUBAREAS]
S1  0.012  0.1  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  20  5  4  7  0

[JUNCTIONS]
J1  100.4  3

[OUTFALLS]
O1  100.0  FREE

[CONDUITS]
C1  J1  O1  200  0.013  0  0

[XSECTIONS]
C1  RECT_OPEN  2  2  0  0
"
        )
    };
    let readings =
        hydra_engine_uds::io::rain::parse_rain_file("STA7 2012 6 29 0 5 1.2\n").expect("parses");

    // Case apart, this is the same station: it opens.
    Simulation::open_with_files(
        &model("sta7"),
        Vec::new(),
        vec![("rain.dat".to_string(), readings.clone())],
    )
    .expect("case-insensitive station match");

    // A station the record does not hold refuses, naming both.
    let err = match Simulation::open_with_files(
        &model("elsewhere"),
        Vec::new(),
        vec![("rain.dat".to_string(), readings)],
    ) {
        Err(e) => format!("{e:?}"),
        Ok(_) => panic!("opened dry on a record holding nothing for this gage"),
    };
    assert!(err.contains("elsewhere"), "{err}");
    assert!(err.contains("rain.dat"), "{err}");
}

/// `MINIMUM_STEP` is honoured, and it is the lever the spec says it is.
///
/// The engine has always floored the adaptive step at 0.5 s, which is the
/// predecessor's default, so a model that never set the option already
/// routed identically. A model that did set it was run at 0.5 anyway: the
/// value was parsed, written back out, and ignored.
///
/// The network below is Courant-limited by a five-metre conduit, so the
/// seed sits under any floor worth testing and the floor is what decides
/// the step. Raising it therefore has to cost steps, and the run has to
/// stay a real run rather than becoming a no-op that trivially takes few.
#[test]
fn the_minimum_step_option_sets_the_step_floor() {
    fn model(minimum_step: Option<&str>) -> String {
        format!(
            "[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/1998
START_TIME           00:00:00
END_DATE             01/01/1998
END_TIME             00:20:00
REPORT_STEP          00:05:00
ROUTING_STEP         0:00:30
VARIABLE_STEP        0.75
{}

[JUNCTIONS]
J1  10  3  0  0  0
J2   9  3  0  0  0

[OUTFALLS]
O1   8  FREE

[CONDUITS]
C1  J1  J2  5    0.01  0  0  0  0
C2  J2  O1  200  0.01  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  1  0  0  0  1
C2  CIRCULAR  1  0  0  0  1

[INFLOWS]
J1  FLOW  TS1

[TIMESERIES]
TS1  0:00  0.5
TS1  0:10  0.5
TS1  0:20  0.5

[REPORT]
",
            minimum_step.map_or(String::new(), |v| format!("MINIMUM_STEP         {v}"))
        )
    }

    fn run(inp: &str) -> (u64, f64) {
        let (mut sim, diags, findings) = Simulation::open(inp).expect("open");
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        assert!(!findings.iter().any(|f| f.kind.is_error()), "{findings:?}");
        sim.run();
        let led = sim.report();
        (led.accepted, led.outflow)
    }

    let (default_steps, default_out) = run(&model(None));
    let (raised_steps, raised_out) = run(&model(Some("5")));
    let (lowered_steps, lowered_out) = run(&model(Some("0.1")));

    // Both are real runs: water arrived at the outfall in each.
    for (name, out) in [
        ("default", default_out),
        ("raised", raised_out),
        ("lowered", lowered_out),
    ] {
        assert!(out > 0.0, "{name} run moved no water, so it proves nothing");
    }

    // A floor of 5 s cannot take more steps than a floor of 0.5 s, and on
    // a run this short it takes materially fewer.
    assert!(
        raised_steps < default_steps,
        "raising the floor changed nothing: {raised_steps} steps against {default_steps}"
    );
    // And lowering it costs steps, which is the other half of the lever.
    assert!(
        lowered_steps > default_steps,
        "lowering the floor changed nothing: {lowered_steps} steps against {default_steps}"
    );
}

/// An RDII interface file replaces the convolution rather than adding to it.
///
/// The file exists so a model whose unit hydrographs and rainfall have not
/// changed need not recompute them (§14.8.1), so a run that both convolved
/// *and* applied the file would double the hydrograph it was given. Three
/// runs of one model separate the two: no file, a file of zeros, and a file
/// of large flows.
#[test]
fn an_rdii_interface_file_replaces_the_convolution() {
    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/1998
START_TIME           00:00:00
END_DATE             01/01/1998
END_TIME             06:00:00
REPORT_STEP          00:15:00
WET_STEP             00:05:00
DRY_STEP             00:15:00
ROUTING_STEP         0:00:30

[FILES]
USE RDII \"rdii.txt\"

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT  0.5  1.0  2.0

[RDII]
J1  UH1  100

[TIMESERIES]
TS1  0:00  25.0
TS1  1:00  0.0
TS1  6:00  0.0

[REPORT]
";

    // A text file covering the run at a quarter-hour step.
    fn rdii_text(flow: f64) -> String {
        let mut s = String::from(
            "SWMM5\nRDII for the test\n900\n1\nFLOW CMS\n1\nJ1\nNode Year Mon Day Hr Min Sec Flow\n",
        );
        for q in 0..24 {
            let (hr, min) = (q / 4, (q % 4) * 15);
            s.push_str(&format!("J1 1998 1 1 {hr} {min} 0 {flow}\n"));
        }
        s
    }

    fn inflow(file: Option<&str>) -> f64 {
        // The model declares USE RDII, which used to refuse the model
        // outright; it now opens and waits for the bytes.
        let (mut sim, diags, findings) = Simulation::open(MODEL).expect("open");
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        assert!(!findings.iter().any(|f| f.kind.is_error()), "{findings:?}");
        if let Some(text) = file {
            sim.supply_rdii(text.as_bytes()).expect("supply");
        }
        sim.run();
        sim.ledgers().network.inflow
    }

    let convolved = inflow(None);
    let zeroed = inflow(Some(&rdii_text(0.0)));
    let supplied = inflow(Some(&rdii_text(2.0)));

    assert!(
        convolved > 0.0,
        "the convolution produced nothing, so this proves nothing"
    );
    // A file of zeros is a hydrograph of zero, not an absent one: the
    // convolution must not go on running underneath it.
    assert!(
        zeroed < convolved * 0.01,
        "a zero file left {zeroed} against the convolution's {convolved}: \
         the convolution is still running"
    );
    // And the file's own flows are what the run receives. Nothing else
    // feeds this model, so the whole inflow is the file's: 2 m³/s across
    // the six hours its records cover.
    let expected = 2.0 * 6.0 * 3600.0;
    assert!(
        (supplied - expected).abs() < expected * 0.02,
        "the file declares {expected} m³ and the run received {supplied}"
    );
}

/// A run's own RDII file, read back, reproduces the run.
///
/// This is the whole promise of the format: compute the hydrograph once
/// and reuse it. The writer and the reader are only useful if they agree,
/// and each is easy to get self-consistently wrong — a date origin, a unit
/// conversion, or a step that leaves gaps would all pass their own tests
/// and fail this one.
#[test]
fn an_rdii_file_written_by_a_run_reproduces_that_run() {
    fn model(files: &str) -> String {
        format!(
            "\
[OPTIONS]
FLOW_UNITS           LPS
FLOW_ROUTING         DYNWAVE
START_DATE           03/15/2023
START_TIME           00:00:00
END_DATE             03/15/2023
END_TIME             06:00:00
REPORT_STEP          00:15:00
WET_STEP             00:05:00
DRY_STEP             00:20:00
ROUTING_STEP         0:00:30

[FILES]
{files}

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1

[JUNCTIONS]
J1  10  4  0  0  0
J2  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0
C2  J2  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1
C2  CIRCULAR  2  0  0  0  1

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT   0.4  1.0  2.0
UH1  ALL  MEDIUM  0.3  3.0  2.0

[RDII]
J1  UH1  40
J2  UH1  25

[TIMESERIES]
TS1  0:00  20.0
TS1  1:00  0.0
TS1  6:00  0.0

[REPORT]
"
        )
    }

    // The run that computes the hydrograph and saves it.
    let (mut saver, diags, _) = Simulation::open(&model("SAVE RDII rdii.txt")).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    saver.run();
    let computed = saver.ledgers().network.inflow;
    let mut file = Vec::new();
    assert!(
        saver.write_rdii(&mut file).expect("write"),
        "a SAVE model writes"
    );
    let file = String::from_utf8(file).expect("the text form is text");

    // Two vertices, so a file that mixed their columns would show up.
    assert!(file.contains("J1"), "{file}");
    assert!(file.contains("J2"), "{file}");
    // Written in the model's own units, which the text form declares.
    assert!(
        file.contains("FLOW LPS"),
        "{}",
        &file[..120.min(file.len())]
    );

    // The run that reuses it. Its convolution must not run at all.
    let (mut reader, _, _) = Simulation::open(&model("USE RDII rdii.txt")).expect("open");
    reader.supply_rdii(file.as_bytes()).expect("supply");
    reader.run();
    let replayed = reader.ledgers().network.inflow;

    assert!(computed > 0.0, "the saving run produced no RDII");
    assert!(
        (replayed - computed).abs() < computed * 0.01,
        "replaying the file gave {replayed} against the run's own {computed}"
    );
}

/// A model that asks for no RDII file writes none, and says so by
/// answering `false` rather than producing an empty one.
#[test]
fn a_model_without_an_rdii_file_writes_nothing() {
    let (sim, _, _) = Simulation::open(
        "\
[OPTIONS]
FLOW_UNITS  CMS
[JUNCTIONS]
J1  10  4  0  0  0
[OUTFALLS]
O1  8  FREE  NO
[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0
[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1
[REPORT]
",
    )
    .expect("open");
    let mut out = Vec::new();
    assert!(!sim.write_rdii(&mut out).expect("write"));
    assert!(out.is_empty());
}

/// The declared step bounds every gap between records.
///
/// §14.8.1 writes a record at each hydrology step and declares the *longer*
/// of the two, so the written hydrograph leaves no instant uncovered: where
/// the run took the shorter step the windows overlap, which costs nothing,
/// and where it took the longer they abut. Declaring the wet step instead
/// would leave the whole dry-weather recession in gaps, which a reader
/// serves as no flow.
///
/// The round-trip test does not catch that on its own: its rain falls in
/// the first hour, so most of its volume is written while the run is on the
/// wet step and the missing recession moves the total by less than its
/// tolerance.
#[test]
fn a_written_rdii_file_leaves_no_gap_between_its_records() {
    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
START_DATE           03/15/2023
START_TIME           00:00:00
END_DATE             03/15/2023
END_TIME             08:00:00
REPORT_STEP          00:15:00
WET_STEP             00:05:00
DRY_STEP             00:20:00
ROUTING_STEP         0:00:30

[FILES]
SAVE RDII rdii.txt

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT  0.4  1.0  2.0

[RDII]
J1  UH1  40

[TIMESERIES]
TS1  0:00  20.0
TS1  0:30  0.0
TS1  8:00  0.0

[REPORT]
";
    let (mut sim, _, _) = Simulation::open(MODEL).expect("open");
    sim.run();
    let mut file = Vec::new();
    assert!(sim.write_rdii(&mut file).expect("write"));
    let text = String::from_utf8(file).expect("text form");

    let (net, _) = hydra_engine_uds::io::objects::parse_network(MODEL);
    let parsed = hydra_engine_uds::io::iface::parse_rdii_file(text.as_bytes(), &net, 1.0)
        .expect("the file we just wrote must read back");

    assert!(
        parsed.records.len() > 20,
        "only {} records: the run was too short to say anything",
        parsed.records.len()
    );
    // The run must actually have taken the longer step somewhere, or this
    // proves nothing about gaps.
    let widest = parsed
        .records
        .windows(2)
        .map(|w| w[1].0 - w[0].0)
        .fold(0.0f64, f64::max);
    assert!(
        widest > 5.0 * 60.0 + 1.0,
        "every step was the wet step ({widest}s apart at most), so the dry \
         tail never exercised the bound"
    );
    assert!(
        widest <= parsed.step + 1.0,
        "records are {widest}s apart under a declared step of {}s, so the \
         hydrograph has a hole a reader serves as no flow",
        parsed.step
    );
}

/// A runoff interface file replaces the surface entirely (§14.8.2).
///
/// The whole point is to route again without recomputing hydrology, so a
/// replayed run must take its laterals from the file and not from a
/// surface it also ran. The clock comes from the file too: each record
/// carries the length of the step that produced it.
#[test]
fn a_runoff_interface_file_replaces_the_surface() {
    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/1998
START_TIME           00:00:00
END_DATE             01/01/1998
END_TIME             01:00:00
REPORT_STEP          00:15:00
WET_STEP             00:05:00
DRY_STEP             00:05:00
ROUTING_STEP         0:00:30

[FILES]
USE RUNOFF runoff.bin

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  J1  10  50  500  0.01  0

[SUBAREAS]
S1  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[TIMESERIES]
TS1  0:00  30.0
TS1  1:00  0.0

[REPORT]
";

    /// One parcel, no constituents, CMS: eight floats per parcel per step.
    fn runoff_file(steps: usize, dt: f32, runoff: f32) -> Vec<u8> {
        let mut b = b"SWMM5-RUNOFF".to_vec();
        for v in [1i32, 0, 3, steps as i32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..steps {
            b.extend_from_slice(&dt.to_le_bytes());
            let mut row = [0.0f32; 8];
            row[4] = runoff;
            for x in row {
                b.extend_from_slice(&x.to_le_bytes());
            }
        }
        b
    }

    fn run(file: Option<Vec<u8>>) -> (f64, bool) {
        let (mut sim, diags, _) = Simulation::open(MODEL).expect("open");
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        if let Some(bytes) = file {
            sim.supply_runoff(&bytes).expect("supply");
        }
        sim.run();
        let led = sim.ledgers();
        (led.network.inflow, led.surface.is_some())
    }

    // 12 steps of 300 s covers the hour; 0.5 m³/s throughout.
    let (replayed, surface_ledger) = run(Some(runoff_file(12, 300.0, 0.5)));
    let (computed, _) = run(None);

    assert!(computed > 0.0, "the computed run produced no runoff");
    // The file's own volume, and nothing of the surface's. Hydrology
    // laterals reach the routing interpolated between their steps, as they
    // do for a computed run, so the first interval rises from zero rather
    // than starting at the file's value: half a step's worth less.
    let expected = 0.5 * 3600.0 - 0.5 * 300.0 / 2.0;
    assert!(
        (replayed - expected).abs() < expected * 0.01,
        "the file declares {expected} m³ after the opening ramp and the run \
         received {replayed} (the computed run makes {computed})"
    );
    // §14.8.2: a replayed run states no surface balance, because the file
    // carries flows and a balance is made of volumes and storages.
    assert!(
        !surface_ledger,
        "a replayed run reported a surface balance built from accumulators \
         that never ran"
    );
}

/// A file shorter than the run says so once and stops contributing.
#[test]
fn a_runoff_file_that_ends_early_is_reported() {
    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/1998
START_TIME           00:00:00
END_DATE             01/01/1998
END_TIME             01:00:00
REPORT_STEP          00:15:00
WET_STEP             00:05:00
DRY_STEP             00:05:00
ROUTING_STEP         0:00:30

[FILES]
USE RUNOFF runoff.bin

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  J1  10  50  500  0.01  0

[SUBAREAS]
S1  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[TIMESERIES]
TS1  0:00  30.0
TS1  1:00  0.0

[REPORT]
";
    let mut b = b"SWMM5-RUNOFF".to_vec();
    for v in [1i32, 0, 3, 2] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    // Two records of 300 s against an hour-long run.
    for _ in 0..2 {
        b.extend_from_slice(&300.0f32.to_le_bytes());
        let mut row = [0.0f32; 8];
        row[4] = 1.0;
        for x in row {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }

    let (mut sim, _, _) = Simulation::open(MODEL).expect("open");
    sim.supply_runoff(&b).expect("supply");
    sim.run();

    let notices = &sim.notices;
    let ended = notices
        .iter()
        .filter(|n| n.message.contains("runoff interface file ended"))
        .count();
    assert_eq!(1, ended, "said {ended} times: {notices:?}");
    // Only the file's two records contribute: 1 m³/s for 600 s.
    let inflow = sim.ledgers().network.inflow;
    assert!(
        (inflow - 600.0).abs() < 60.0,
        "received {inflow} m³ where the file carries 600"
    );
}

/// A replayed run reports the file's parcel results, not the surface's.
///
/// The reported per-parcel series are built from the live surface, which a
/// replayed run never steps. Without taking them from the file instead, a
/// replay routed the file's flows correctly and reported every parcel as
/// producing nothing: the results file contradicted the routing, and the
/// first version of this feature shipped exactly that, because its tests
/// only ever looked at the network total.
#[test]
fn a_replayed_run_reports_the_files_parcel_results() {
    const MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/1998
START_TIME           00:00:00
END_DATE             01/01/1998
END_TIME             00:30:00
REPORT_STEP          00:05:00
WET_STEP             00:05:00
DRY_STEP             00:05:00
ROUTING_STEP         0:00:30

[FILES]
USE RUNOFF runoff.bin

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  J1  10  50  500  0.01  0

[SUBAREAS]
S1  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[TIMESERIES]
TS1  0:00  30.0
TS1  1:00  0.0

[REPORT]
";
    // SI, so depths are millimetres: 12 mm/hr of rain, 3 mm/hr infiltration,
    // 24 mm/day evaporation, 50 mm of snow, 0.75 m³/s of runoff.
    let mut b = b"SWMM5-RUNOFF".to_vec();
    for v in [1i32, 0, 3, 8] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    for _ in 0..8 {
        b.extend_from_slice(&300.0f32.to_le_bytes());
        for x in [12.0f32, 50.0, 24.0, 3.0, 0.75, 0.0, 0.0, 0.3] {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }

    let (mut sim, _, _) = Simulation::open(MODEL).expect("open");
    sim.supply_runoff(&b).expect("supply");
    sim.run();

    let snaps = &sim.snapshots;
    let last = snaps.last().expect("a run produces snapshots");
    let p = &last.subcatch[0];

    // Runoff is the flow the file carries, not the zero a surface that
    // never ran would report.
    assert!(
        (p.runoff - 0.75).abs() < 1e-6,
        "parcel runoff {} where the file carries 0.75 m³/s",
        p.runoff
    );
    // The depth quantities convert from the file's units, and each has its
    // own: rain and infiltration per hour, evaporation per day.
    assert!(
        (p.rain - 12.0e-3 / 3600.0).abs() < 1e-12,
        "rain {} m/s from 12 mm/hr",
        p.rain
    );
    assert!(
        (p.infil - 3.0e-3 / 3600.0).abs() < 1e-12,
        "infiltration {} m/s from 3 mm/hr",
        p.infil
    );
    assert!(
        (p.evap - 24.0e-3 / 86_400.0).abs() < 1e-12,
        "evaporation {} m/s from 24 mm/day, which is not 24 mm/hr",
        p.evap
    );
    assert!(
        (p.snow_depth - 50.0e-3).abs() < 1e-12,
        "snow {} m from 50 mm",
        p.snow_depth
    );
    // The format stores 32-bit floats, so equality is to their precision.
    assert!(
        (p.soil_moisture - 0.3).abs() < 1e-6,
        "soil moisture {} is dimensionless and must pass through",
        p.soil_moisture
    );
}

// ── §14.8.2 writing a runoff interface file ─────────────────────────────

/// A model that both computes a hydrology worth recording and routes it.
///
/// The clock is explicit so the wet step and the reporting step can differ:
/// the file must follow the hydrology, not the reporting.
const RUNOFF_SAVE_MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
INFILTRATION         HORTON
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/2020
START_TIME           00:00:00
END_DATE             01/01/2020
END_TIME             01:00:00
REPORT_START_TIME    00:30:00
WET_STEP             00:05:00
DRY_STEP             00:05:00
ROUTING_STEP         00:00:15
REPORT_STEP          00:15:00

[FILES]
SAVE RUNOFF runoff.bin

[RAINGAGES]
G1  INTENSITY  0:05  1.0  TIMESERIES  TS1

[SUBCATCHMENTS]
S1  G1  J1  10  75  500  0.01  0
S2  G1  J1  6   60  400  0.01  0

[SUBAREAS]
S1  0.01  0.10  0.05  0.05  25  OUTLET
S2  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0
S2  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[TIMESERIES]
TS1  0:00  30.0
TS1  0:30  0.0

[REPORT]
";

/// The same model with the file read instead of written.
fn runoff_use_model() -> String {
    RUNOFF_SAVE_MODEL.replace("SAVE RUNOFF runoff.bin", "USE RUNOFF runoff.bin")
}

/// The saved file covers the run at the hydrology step, from time zero.
///
/// Both halves matter and neither follows from the other. Written at the
/// reporting cadence the file would be four records rather than twelve;
/// begun at the reporting start it would omit the first half hour, which
/// is where all the rain falls.
#[test]
fn a_saved_runoff_file_covers_every_hydrology_step() {
    let (mut sim, diags, _) = Simulation::open(RUNOFF_SAVE_MODEL).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    sim.run();
    let mut bytes = Vec::new();
    assert!(sim.write_runoff(&mut bytes).expect("write"), "file written");

    let word = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    assert_eq!(2, word(12), "two parcels");
    assert_eq!(0, word(16), "no constituents");
    // An hour at a five-minute wet step, from zero rather than from the
    // reporting start half an hour in.
    assert_eq!(12, word(24), "records");
    let f32_at = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let record = 4 + 2 * 8 * 4;
    for i in 0..12 {
        assert_eq!(300.0, f32_at(28 + i * record), "record {i} step length");
    }
    // The rain stops at 0:30, so the first six records carry rainfall and
    // the parcels are still draining after. A file that began at the
    // reporting start would show the opposite.
    let rain_of = |i: usize| f32_at(28 + i * record + 4);
    assert!(rain_of(0) > 0.0, "no rain in the first record");
    assert_eq!(0.0, rain_of(11), "rain in the last record");
}

/// Saving one changes nothing else the run reports.
///
/// The writer only observes, so every other surface must be untouched:
/// the ledgers, the report, and the results file the run produces.
#[test]
fn saving_a_runoff_file_changes_nothing_else() {
    let run = |model: &str| {
        let (mut sim, _, _) = Simulation::open(model).expect("open");
        sim.run();
        let mut out = Vec::new();
        sim.write_out(&mut out).expect("out");
        let mut rpt = Vec::new();
        sim.write_report(&mut rpt).expect("report");
        (sim.ledgers().network, out, rpt)
    };
    let plain = RUNOFF_SAVE_MODEL.replace("[FILES]\nSAVE RUNOFF runoff.bin\n\n", "");
    let (led_a, out_a, rpt_a) = run(&plain);
    let (led_b, out_b, rpt_b) = run(RUNOFF_SAVE_MODEL);
    assert_eq!(led_a.inflow, led_b.inflow, "network inflow");
    assert_eq!(led_a.outflow, led_b.outflow, "network outflow");
    assert_eq!(out_a, out_b, "the results file differs");
    assert_eq!(rpt_a, rpt_b, "the report differs");
}

/// A saved file replays as the run that wrote it.
///
/// This is the whole point of the format, and it is the one assertion that
/// exercises writer and reader together against real hydrology rather than
/// against hand-built rows.
#[test]
fn a_saved_runoff_file_replays_as_the_run_that_wrote_it() {
    let (mut a, _, _) = Simulation::open(RUNOFF_SAVE_MODEL).expect("open save");
    a.run();
    let mut bytes = Vec::new();
    assert!(a.write_runoff(&mut bytes).expect("write"), "file written");

    let (mut b, _, _) = Simulation::open(&runoff_use_model()).expect("open use");
    b.supply_runoff(&bytes).expect("supply");
    b.run();

    let (la, lb) = (a.ledgers().network, b.ledgers().network);
    assert!(la.inflow > 0.0, "the computed run produced no inflow");
    assert!(
        (la.inflow - lb.inflow).abs() < la.inflow * 1e-3,
        "replayed inflow {} against the computed {}",
        lb.inflow,
        la.inflow
    );

    // Per parcel and per reporting instant, not just the total: a file
    // that scaled every parcel by the same factor, or swapped two of
    // them, would leave the total untouched.
    assert_eq!(a.snapshots.len(), b.snapshots.len(), "reporting instants");
    for (i, (sa, sb)) in a.snapshots.iter().zip(&b.snapshots).enumerate() {
        for (pi, (pa, pb)) in sa.subcatch.iter().zip(&sb.subcatch).enumerate() {
            for (got, want, what) in [
                (pb.runoff, pa.runoff, "runoff"),
                (pb.rain, pa.rain, "rainfall"),
                (pb.infil, pa.infil, "infiltration"),
                (pb.evap, pa.evap, "evaporation"),
            ] {
                assert!(
                    (got - want).abs() <= want.abs() * 1e-5 + 1e-12,
                    "instant {i} parcel {pi} {what}: replayed {got}, computed {want}"
                );
            }
        }
    }
}

/// A run cannot both record a hydrology and replay one.
#[test]
fn a_run_that_saves_a_runoff_file_cannot_also_replay_one() {
    let (mut a, _, _) = Simulation::open(RUNOFF_SAVE_MODEL).expect("open");
    a.run();
    let mut bytes = Vec::new();
    a.write_runoff(&mut bytes).expect("write");

    let (mut b, _, _) = Simulation::open(RUNOFF_SAVE_MODEL).expect("open");
    let err = b
        .supply_runoff(&bytes)
        .expect_err("both at once is refused");
    assert!(err.contains("cannot also replay"), "{err}");
}

/// A model that saves no file writes none, and says so by returning false
/// rather than by producing an empty one.
#[test]
fn a_model_that_asks_for_no_runoff_file_writes_none() {
    let plain = RUNOFF_SAVE_MODEL.replace("[FILES]\nSAVE RUNOFF runoff.bin\n\n", "");
    let (mut sim, _, _) = Simulation::open(&plain).expect("open");
    sim.run();
    let mut bytes = Vec::new();
    assert!(!sim.write_runoff(&mut bytes).expect("write"), "no file");
    assert!(bytes.is_empty(), "wrote {} bytes", bytes.len());
}

// ── §14.8.3 rainfall interface files ────────────────────────────────────

/// A model whose gage reads an external record, so it has something to
/// cache. The clock is explicit and the record covers the first half hour.
fn rain_model(form: &str, unit: &str, files: &str) -> String {
    format!(
        "\
[OPTIONS]
FLOW_UNITS           CFS
INFILTRATION         HORTON
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/2020
START_TIME           00:00:00
END_DATE             01/01/2020
END_TIME             01:00:00
WET_STEP             00:15:00
DRY_STEP             00:15:00
ROUTING_STEP         00:00:30
REPORT_STEP          00:15:00

{files}

[RAINGAGES]
G1  {form}  0:15  1.0  FILE  \"rain.dat\"  STA01  {unit}

[SUBCATCHMENTS]
S1  G1  J1  10  75  500  0.01  0

[SUBAREAS]
S1  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[REPORT]
"
    )
}

/// The record the gage reads: four quarter-hour readings, one of them dry.
const RAIN_RECORD: &str = "\
STA01  2020  1  1  0   0   0.40
STA01  2020  1  1  0   15  0.80
STA01  2020  1  1  0   30  0.00
STA01  2020  1  1  0   45  0.20
";

fn rain_readings() -> Vec<(String, Vec<hydra_engine_uds::io::rain::RainReading>)> {
    vec![(
        "rain.dat".to_string(),
        hydra_engine_uds::io::rain::parse_rain_file(RAIN_RECORD).expect("record parses"),
    )]
}

/// A run reading the cache matches the run that wrote it, on every
/// surface that observes rainfall.
///
/// This is the assertion that matters: the file exists so the second run
/// need not parse the record, and the two runs are supposed to be the same
/// run. Comparing the results file compares the parcel rainfall series,
/// the runoff it produced, and the system totals in one go.
#[test]
fn a_cached_record_runs_as_the_record_it_cached() {
    let model = rain_model("VOLUME", "IN", "[FILES]\nSAVE RAINFALL rain.rff");
    let (mut a, _, _) = Simulation::open_with_files(&model, Vec::new(), rain_readings())
        .expect("open with the record");
    a.run();
    let mut cache = Vec::new();
    assert!(a.write_rain(&mut cache).expect("write"), "file written");

    let use_model = rain_model("VOLUME", "IN", "[FILES]\nUSE RAINFALL rain.rff");
    let (mut b, _, _) = Simulation::open_with_rain_interface(&use_model, Vec::new(), &cache)
        .expect("open with the cache");
    b.run();

    // The results agree to the precision the cache stores: depths are
    // 32-bit on the file, so 0.4 in returns as 0.40000000596 and the
    // runoff it drives differs in the last bit. Byte equality of the two
    // results files is therefore the wrong assertion, and asserting it
    // was how this limit got noticed.
    assert_eq!(a.snapshots.len(), b.snapshots.len(), "reporting instants");
    for (i, (sa, sb)) in a.snapshots.iter().zip(&b.snapshots).enumerate() {
        for (pi, (pa, pb)) in sa.subcatch.iter().zip(&sb.subcatch).enumerate() {
            for (x, y, what) in [
                (pa.rain, pb.rain, "rainfall"),
                (pa.runoff, pb.runoff, "runoff"),
                (pa.infil, pb.infil, "infiltration"),
            ] {
                assert!(
                    (x - y).abs() <= x.abs() * 1e-6 + 1e-12,
                    "instant {i} parcel {pi} {what}: cached {y}, computed {x}"
                );
            }
        }
    }

    let (la, lb) = (a.ledgers(), b.ledgers());
    let (sa, sb) = (
        la.surface.expect("surface ledger").inflow,
        lb.surface.expect("surface ledger").inflow,
    );
    assert!(sa > 0.0, "the run received no rain to cache");
    assert!(
        (sa - sb).abs() <= sa * 1e-6,
        "precipitation volume: cached {sb}, computed {sa}"
    );
    assert!(
        (la.network.inflow - lb.network.inflow).abs() <= la.network.inflow * 1e-6,
        "network inflow: cached {}, computed {}",
        lb.network.inflow,
        la.network.inflow
    );
}

/// The cache holds interval depths in inches whatever the gage declared.
///
/// An intensity record and a volume record describing the same rain cache
/// identically, which is the normalisation the format is built on. Written
/// as literals rather than by comparing the two runs, so a normalisation
/// that was wrong in the same way for both would still fail.
#[test]
fn a_cached_record_holds_interval_depths_in_inches() {
    let cache_of = |form: &str, unit: &str, record: &str| {
        let model = rain_model(form, unit, "[FILES]\nSAVE RAINFALL rain.rff");
        let readings = vec![(
            "rain.dat".to_string(),
            hydra_engine_uds::io::rain::parse_rain_file(record).expect("record parses"),
        )];
        let (sim, _, _) = Simulation::open_with_files(&model, Vec::new(), readings).expect("open");
        let mut bytes = Vec::new();
        assert!(sim.write_rain(&mut bytes).expect("write"), "written");
        let f = hydra_engine_uds::io::iface::parse_rain_iface(&bytes).expect("parse");
        f.gages[0]
            .readings
            .iter()
            .map(|(_, v)| (v * 1e6).round() / 1e6)
            .collect::<Vec<f64>>()
    };

    // A volume record in inches is already what the file holds.
    assert_eq!(
        vec![0.4, 0.8, 0.0, 0.2],
        cache_of("VOLUME", "IN", RAIN_RECORD),
        "volume in inches"
    );
    // The same rain as an intensity: 1.6 in/hr over a quarter hour is
    // 0.4 in, so the depths must come out identical.
    let intensity = "\
STA01  2020  1  1  0   0   1.6
STA01  2020  1  1  0   15  3.2
STA01  2020  1  1  0   30  0.0
STA01  2020  1  1  0   45  0.8
";
    assert_eq!(
        vec![0.4, 0.8, 0.0, 0.2],
        cache_of("INTENSITY", "IN", intensity),
        "an intensity record caches as a depth per interval"
    );
    // And as a running total, differenced.
    let cumulative = "\
STA01  2020  1  1  0   0   0.4
STA01  2020  1  1  0   15  1.2
STA01  2020  1  1  0   30  1.2
STA01  2020  1  1  0   45  1.4
";
    assert_eq!(
        vec![0.4, 0.8, 0.0, 0.2],
        cache_of("CUMULATIVE", "IN", cumulative),
        "a cumulative record caches as its increments"
    );
    // A record in millimetres holds the same rain: 10.16 mm is 0.4 in.
    let metric = "\
STA01  2020  1  1  0   0   10.16
STA01  2020  1  1  0   15  20.32
STA01  2020  1  1  0   30  0.0
STA01  2020  1  1  0   45  5.08
";
    assert_eq!(
        vec![0.4, 0.8, 0.0, 0.2],
        cache_of("VOLUME", "MM", metric),
        "the file is inches whatever the record declared"
    );
}

/// A gage whose station the cache does not carry is refused by name, not
/// left reading nothing.
#[test]
fn a_gage_missing_from_the_cache_is_refused() {
    let model = rain_model("VOLUME", "IN", "[FILES]\nSAVE RAINFALL rain.rff");
    let (sim, _, _) = Simulation::open_with_files(&model, Vec::new(), rain_readings())
        .expect("open with the record");
    let mut cache = Vec::new();
    sim.write_rain(&mut cache).expect("write");

    let other =
        rain_model("VOLUME", "IN", "[FILES]\nUSE RAINFALL rain.rff").replace("STA01", "STA99");
    let err = Simulation::open_with_rain_interface(&other, Vec::new(), &cache)
        .err()
        .expect("a station that is not there must refuse");
    let text = err.to_string();
    assert!(text.contains("STA99"), "{text}");
}

/// A model that asks for no cache writes none.
#[test]
fn a_model_that_asks_for_no_rainfall_file_writes_none() {
    let model = rain_model("VOLUME", "IN", "");
    let (sim, _, _) =
        Simulation::open_with_files(&model, Vec::new(), rain_readings()).expect("open");
    let mut bytes = Vec::new();
    assert!(!sim.write_rain(&mut bytes).expect("write"), "no file");
    assert!(bytes.is_empty(), "wrote {} bytes", bytes.len());
}

/// A metric model reads the cache as inches and converts.
///
/// Every other test here runs a US model, where the conversion is 1 and a
/// missing one is invisible. Deleting it left the whole suite green: a
/// metric model would have read 0.4 inches as 0.4 mm and run on a
/// twenty-fifth of its rain.
#[test]
fn a_metric_model_reads_the_cache_in_inches() {
    let metric = |files: &str| {
        rain_model("VOLUME", "MM", files)
            .replace("FLOW_UNITS           CFS", "FLOW_UNITS           CMS")
    };
    // 10.16 mm per quarter hour is 0.4 in, so the cache holds 0.4.
    let record = "\
STA01  2020  1  1  0   0   10.16
STA01  2020  1  1  0   15  20.32
STA01  2020  1  1  0   30  0.00
STA01  2020  1  1  0   45  5.08
";
    let readings = vec![(
        "rain.dat".to_string(),
        hydra_engine_uds::io::rain::parse_rain_file(record).expect("record parses"),
    )];
    let (mut a, _, _) = Simulation::open_with_files(
        &metric("[FILES]\nSAVE RAINFALL rain.rff"),
        Vec::new(),
        readings,
    )
    .expect("open with the record");
    a.run();
    let mut cache = Vec::new();
    assert!(a.write_rain(&mut cache).expect("write"), "written");

    let f = hydra_engine_uds::io::iface::parse_rain_iface(&cache).expect("parse");
    let depths: Vec<f64> = f.gages[0]
        .readings
        .iter()
        .map(|(_, v)| (v * 1e6).round() / 1e6)
        .collect();
    assert_eq!(vec![0.4, 0.8, 0.0, 0.2], depths, "the file is in inches");

    let (mut b, _, _) = Simulation::open_with_rain_interface(
        &metric("[FILES]\nUSE RAINFALL rain.rff"),
        Vec::new(),
        &cache,
    )
    .expect("open with the cache");
    b.run();
    let (sa, sb) = (
        a.ledgers().surface.expect("surface").inflow,
        b.ledgers().surface.expect("surface").inflow,
    );
    assert!(sa > 0.0, "the metric run received no rain");
    assert!(
        (sa - sb).abs() <= sa * 1e-6,
        "a metric model read {sb} m³ of the {sa} m³ it cached"
    );
}

// ── §14.12.1 archival station records ───────────────────────────────────

/// A run whose gage reads an archival record matches the same run given
/// the same rainfall as a station record.
///
/// This is the assertion the layout work is for: the archive is a
/// different way of writing the same weather, and a model must not care
/// which it was handed.
#[test]
fn an_archival_record_drives_a_run_as_its_station_record_does() {
    use hydra_engine_uds::io::rain::{parse_any_rain_file, parse_rain_file, RainRecords};

    // 0.25 in in the hour ending 01:00, 0.10 in in the hour ending 02:00.
    let archive = "123456 21 HPCP  HI2020 01 01 0100     25     0200     10    \n";
    // The same rain as a station record: each hour stamped where it began,
    // as a depth over the hour, in inches.
    let station = "\
STA01  2020  1  1  0   0   0.25
STA01  2020  1  1  1   0   0.10
";
    let model = "\
[OPTIONS]
FLOW_UNITS           CFS
INFILTRATION         HORTON
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/2020
START_TIME           00:00:00
END_DATE             01/01/2020
END_TIME             04:00:00
WET_STEP             01:00:00
DRY_STEP             01:00:00
ROUTING_STEP         00:01:00
REPORT_STEP          01:00:00

[RAINGAGES]
G1  VOLUME  1:00  1.0  FILE  \"rain.dat\"  STA01  IN

[SUBCATCHMENTS]
S1  G1  J1  10  75  500  0.01  0

[SUBAREAS]
S1  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
S1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0

[OUTFALLS]
O1  8  FREE  NO

[CONDUITS]
C1  J1  O1  400  0.013  0  0  0  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1

[REPORT]
";

    let (records, notices) = parse_any_rain_file(archive).expect("the archive is recognised");
    assert!(
        matches!(records, RainRecords::Archive(_)),
        "recognised as an archive"
    );
    assert!(notices.is_empty(), "{notices:?}");
    let (mut from_archive, _, _) = Simulation::open_with_rain_records(
        model,
        Vec::new(),
        vec![("rain.dat".to_string(), records)],
    )
    .expect("open with the archive");
    from_archive.run();

    let (mut from_station, _, _) = Simulation::open_with_files(
        model,
        Vec::new(),
        vec![(
            "rain.dat".to_string(),
            parse_rain_file(station).expect("the station record parses"),
        )],
    )
    .expect("open with the station record");
    from_station.run();

    let rain_of = |sim: &Simulation| -> Vec<f64> {
        sim.snapshots
            .iter()
            .map(|s| (s.subcatch[0].rain * 1e12).round() / 1e12)
            .collect()
    };
    assert!(
        rain_of(&from_archive).iter().any(|r| *r > 0.0),
        "the archival run received no rain at all"
    );
    assert_eq!(
        rain_of(&from_station),
        rain_of(&from_archive),
        "the two records describe the same weather"
    );
    let (a, b) = (
        from_archive.ledgers().surface.expect("surface").inflow,
        from_station.ledgers().surface.expect("surface").inflow,
    );
    assert!((a - b).abs() <= a * 1e-9, "precipitation {a} against {b}");
}

/// A station record is still recognised as one, and a file that is
/// neither says so once with both reasons.
#[test]
fn a_file_that_is_neither_layout_names_both_reasons() {
    use hydra_engine_uds::io::rain::{parse_any_rain_file, RainRecords};

    let (records, _) = parse_any_rain_file("STA01  2020  1  1  0  0  0.10\n")
        .expect("the station format is still recognised");
    assert!(matches!(records, RainRecords::Station(_)));

    let err = parse_any_rain_file("this is not a rain record at all\n").unwrap_err();
    assert!(
        err.contains("line 1"),
        "the station reason names its line: {err}"
    );
    assert!(
        err.contains("HPCP"),
        "and the archival reason is kept: {err}"
    );
}

// ── §12.4 mid-run forcing ───────────────────────────────────────────────

/// A model with a gage, a controllable orifice and an outfall, so every
/// injection has something to act on.
const FORCING_MODEL: &str = "\
[OPTIONS]
FLOW_UNITS           CMS
INFILTRATION         HORTON
FLOW_ROUTING         DYNWAVE
START_DATE           01/01/2020
START_TIME           00:00:00
END_DATE             01/01/2020
END_TIME             02:00:00
WET_STEP             00:05:00
DRY_STEP             00:05:00
ROUTING_STEP         00:00:15
REPORT_STEP          00:05:00

[RAINGAGES]
G1  INTENSITY  0:05  1.0  TIMESERIES  RAIN

[SUBCATCHMENTS]
P1  G1  J1  10  75  500  0.01  0

[SUBAREAS]
P1  0.01  0.10  0.05  0.05  25  OUTLET

[INFILTRATION]
P1  3.0  0.5  4  7  0

[JUNCTIONS]
J1  10  4  0  0  0
J2  9   4  0  0  0

[OUTFALLS]
O1  6  FREE  NO

[CONDUITS]
C1  J1  J2  200  0.013  0  0  0  0

[ORIFICES]
R1  J2  O1  SIDE  0  0.65  NO  0

[XSECTIONS]
C1  CIRCULAR  2  0  0  0  1
R1  CIRCULAR  1  0  0  0

[TIMESERIES]
RAIN  0:00  0.0
RAIN  1:00  0.0

[REPORT]
";

/// Run the model, calling `force` once at each step, and return the
/// rainfall and outflow the run produced.
fn forced_run(model: &str, mut force: impl FnMut(&mut Simulation, usize)) -> (f64, Vec<f64>) {
    let (mut sim, diags, _) = Simulation::open(model).expect("open");
    assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
    let mut step = 0;
    while {
        force(&mut sim, step);
        step += 1;
        sim.step()
    } {}
    let rain: f64 = sim.snapshots.iter().map(|s| s.subcatch[0].rain).sum();
    let depths = sim.snapshots.iter().map(|s| s.depths[1]).collect();
    (rain, depths)
}

/// Injected precipitation drives a run whose own record is dry.
#[test]
fn injected_precipitation_supersedes_a_dry_record() {
    let (dry, _) = forced_run(FORCING_MODEL, |_, _| {});
    assert_eq!(0.0, dry, "the model's own record must be dry");

    // 20 mm/h for the first half of the run, then released.
    let (wet, _) = forced_run(FORCING_MODEL, |sim, step| {
        if step == 0 {
            assert!(sim.set_precipitation("G1", Some(20.0e-3 / 3600.0)));
        } else if step == 12 {
            assert!(sim.set_precipitation("G1", None));
        }
    });
    assert!(wet > 0.0, "the injection produced no rain at all");

    // Releasing returns the gage to its record, which is dry: the tail of
    // the run must stop raining.
    //
    // Measured in reporting instants, not in `step` calls: a step is a
    // routing period and there are twenty of them to a reporting one, so
    // counting steps released the injection before the first instant and
    // then compared the whole run against itself.
    let (mut sim, _, _) = Simulation::open(FORCING_MODEL).expect("open");
    assert!(sim.set_precipitation("G1", Some(20.0e-3 / 3600.0)));
    while sim.snapshots.len() < 4 {
        assert!(sim.step(), "the run ended before the release");
    }
    let before = sim.snapshots.len();
    let raining: f64 = sim.snapshots.iter().map(|s| s.subcatch[0].rain).sum();
    assert!(
        raining > 0.0,
        "the injection produced no rain before release"
    );
    assert!(sim.set_precipitation("G1", None));
    while sim.step() {}
    let after: f64 = sim.snapshots[before..]
        .iter()
        .map(|s| s.subcatch[0].rain)
        .sum();
    assert!(
        sim.snapshots.len() > before,
        "no reporting instant followed the release"
    );
    assert_eq!(0.0, after, "the released gage kept raining");
}

/// An injected stage holds an outfall that declares none.
#[test]
fn an_injected_stage_holds_a_free_outfall() {
    // A free outfall discharges at critical depth; held at an elevation
    // above its invert it backs the network up instead.
    let (_, free) = forced_run(FORCING_MODEL, |sim, step| {
        if step == 0 {
            assert!(sim.set_precipitation("G1", Some(40.0e-3 / 3600.0)));
        }
    });
    let (_, held) = forced_run(FORCING_MODEL, |sim, step| {
        if step == 0 {
            assert!(sim.set_precipitation("G1", Some(40.0e-3 / 3600.0)));
            // Above J2's invert of 9: a stage below it holds nothing back,
            // which is what the first version of this test asked for.
            assert!(sim.set_outfall_stage("O1", Some(10.5)));
        }
    });
    let peak = |d: &[f64]| d.iter().cloned().fold(0.0_f64, f64::max);
    assert!(peak(&free) > 0.0, "nothing reached the network at all");
    assert!(
        peak(&held) > peak(&free) + 1e-6,
        "a held outfall must back the network up: {} against {}",
        peak(&held),
        peak(&free)
    );
}

/// A stage is refused where it is not a thing to set.
#[test]
fn a_stage_on_a_junction_is_refused() {
    let (mut sim, _, _) = Simulation::open(FORCING_MODEL).expect("open");
    assert!(!sim.set_outfall_stage("J1", Some(9.0)), "J1 is a junction");
    assert!(
        !sim.set_outfall_stage("nowhere", Some(9.0)),
        "no such vertex"
    );
    assert!(sim.set_outfall_stage("O1", Some(9.0)), "O1 is an outfall");
}

/// An injected setting closes a regulator, and is logged as an injection.
#[test]
fn an_injected_setting_closes_a_regulator_and_says_so() {
    let (_, open) = forced_run(FORCING_MODEL, |sim, step| {
        if step == 0 {
            assert!(sim.set_precipitation("G1", Some(40.0e-3 / 3600.0)));
        }
    });
    let (_, shut) = forced_run(FORCING_MODEL, |sim, step| {
        if step == 0 {
            assert!(sim.set_precipitation("G1", Some(40.0e-3 / 3600.0)));
            assert!(sim.set_link_setting("R1", Some(0.0)));
        }
    });
    let peak = |d: &[f64]| d.iter().cloned().fold(0.0_f64, f64::max);
    assert!(
        peak(&shut) > peak(&open) + 1e-6,
        "a closed orifice must hold water back: {} against {}",
        peak(&shut),
        peak(&open)
    );

    let (mut sim, _, _) = Simulation::open(FORCING_MODEL).expect("open");
    assert!(sim.set_link_setting("R1", Some(0.0)));
    assert!(!sim.set_link_setting("nowhere", Some(0.0)), "no such link");
}

/// A capped channel carries no more than its cap.
#[test]
fn an_injected_flow_limit_caps_a_channel() {
    let peak = |cap: Option<f64>| {
        let (mut sim, _, _) = Simulation::open(FORCING_MODEL).expect("open");
        assert!(sim.set_precipitation("G1", Some(60.0e-3 / 3600.0)));
        if let Some(q) = cap {
            assert!(sim.set_flow_limit("C1", Some(q)));
        }
        sim.run();
        sim.snapshots
            .iter()
            .map(|s| s.flows[0].abs())
            .fold(0.0_f64, f64::max)
    };
    let free = peak(None);
    assert!(free > 0.05, "the uncapped channel carried {free}");
    let capped = peak(Some(0.02));
    assert!(
        capped <= 0.02 + 1e-6,
        "the cap was ignored: {capped} against a cap of 0.02"
    );
    // Zero is no cap, as it is in a model, and must not stop the flow.
    assert!(
        (peak(Some(0.0)) - free).abs() < 1e-9,
        "a zero cap must mean no cap"
    );
}

/// Losses are refused where they are not a thing to set, and accepted
/// where they are.
#[test]
fn losses_are_set_on_channels_and_refused_elsewhere() {
    // The refusal is enforced twice, in the session and again in the
    // router, and this test cannot tell which one acted: deleting the
    // session's guard leaves it green because the router still refuses.
    // The session's guard earns its place on the release path, where it
    // is what supplies the model's own values to return to.
    let (mut sim, _, _) = Simulation::open(FORCING_MODEL).expect("open");
    assert!(
        sim.set_losses("C1", Some((1.5, 1.0, 0.2))),
        "C1 is a channel"
    );
    assert!(
        !sim.set_losses("R1", Some((1.5, 1.0, 0.2))),
        "R1 is an orifice"
    );
    assert!(!sim.set_losses("nowhere", Some((1.5, 1.0, 0.2))));
    // Releasing returns the model's own, which this model leaves at zero.
    assert!(sim.set_losses("C1", None));
    assert!(
        !sim.set_flow_limit("R1", Some(1.0)),
        "an orifice has no cap"
    );
}

/// An injected inflow carries the concentrations it is given, and none
/// when it is given none.
#[test]
fn an_injected_inflow_carries_its_concentrations() {
    let with_quality = FORCING_MODEL.replace(
        "[JUNCTIONS]",
        "[POLLUTANTS]\nTSS  MG/L  0  0  0  0\n\n[JUNCTIONS]",
    );
    let load = |conc: Option<f64>| {
        let (mut sim, diags, _) = Simulation::open(&with_quality).expect("open");
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        assert!(sim.set_lateral_inflow("J1", Some(0.5)));
        if let Some(c) = conc {
            assert!(sim.set_inflow_concentrations("J1", Some(vec![c])));
        }
        sim.run();
        sim.snapshots
            .iter()
            .map(|s| s.node_quality[0][1])
            .fold(0.0_f64, f64::max)
    };
    assert_eq!(
        0.0,
        load(None),
        "an inflow given no concentration carries none"
    );
    let dirty = load(Some(50.0));
    assert!(
        dirty > 1.0,
        "the injected concentration did not arrive: {dirty}"
    );

    // One value per constituent, or the call is refused rather than
    // padded to a shape the caller did not mean.
    let (mut sim, _, _) = Simulation::open(&with_quality).expect("open");
    assert!(!sim.set_inflow_concentrations("J1", Some(vec![1.0, 2.0])));
    assert!(sim.set_inflow_concentrations("J1", Some(vec![1.0])));
    assert!(sim.set_inflow_concentrations("J1", None));
}

/// A control measure's underdrain can be opened mid-run, and released
/// back to the design the model gives it.
#[test]
fn an_injected_drain_empties_a_control_measure() {
    use hydra_engine_uds::hydrology::lid::DrainSetting;

    // A rain barrel whose drain is shut by a long delay: nothing leaves
    // it during a two-hour run unless a caller opens it.
    let base = {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/uds/rain_barrel_delayed_drain.inp");
        let text = std::fs::read_to_string(path).expect("fixture readable");
        // The fixture declares no clock of its own.
        format!(
            "{text}\n[OPTIONS]\nSTART_DATE 01/01/2024\nSTART_TIME 00:00:00\n\
             END_DATE 01/01/2024\nEND_TIME 02:00:00\nREPORT_STEP 00:05:00\n\
             [REPORT]\nSUBCATCHMENTS ALL\nNODES ALL\nLINKS ALL\n"
        )
    };
    let held = |open: bool| {
        let (mut sim, diags, _) = Simulation::open(&base).expect("open");
        assert!(!diags.iter().any(|d| d.kind.is_error()), "{diags:?}");
        if open {
            assert!(
                sim.set_drain(
                    "S1",
                    "RB1",
                    Some(DrainSetting {
                        coeff: 5.0,
                        exponent: 0.5,
                        offset: 0.0,
                        delay: 0.0,
                        h_open: 0.0,
                        h_close: 0.0,
                    }),
                ),
                "the barrel has a drain to set"
            );
        }
        sim.run();
        sim.ledgers().network.inflow
    };
    let shut = held(false);
    let opened = held(true);
    assert!(
        opened > shut + 1e-9,
        "an opened drain must deliver more to the network: {opened} against {shut}"
    );

    // The coefficient has to be applied too, and opening the drain is not
    // evidence of that: with only the delay applied the barrel still
    // drains, at the design's own coefficient. Two coefficients, two
    // answers, or the value was ignored.
    // Zero against a small non-zero, not two large values: above about
    // 0.01 this barrel drains as fast as its storage allows, so 0.5 and
    // 20 deliver identically and comparing them compared nothing.
    let with_coeff = |coeff: f64| {
        let (mut sim, _, _) = Simulation::open(&base).expect("open");
        assert!(sim.set_drain(
            "S1",
            "RB1",
            Some(DrainSetting {
                coeff,
                exponent: 0.5,
                offset: 0.0,
                delay: 0.0,
                h_open: 0.0,
                h_close: 0.0,
            }),
        ));
        sim.run();
        sim.ledgers().network.inflow
    };
    let none = with_coeff(0.0);
    let some = with_coeff(0.01);
    assert!(
        some > none + 1e-9,
        "the coefficient was ignored: {some} against {none}"
    );

    // Addressed as the model addresses a placement, and refused for a
    // pair it does not place.
    let (mut sim, _, _) = Simulation::open(&base).expect("open");
    let from_model = sim.drain("S1", "RB1").expect("the barrel has a drain");
    assert!(
        !sim.set_drain("S1", "nothing", Some(from_model)),
        "no such control"
    );
    assert!(
        !sim.set_drain("nowhere", "RB1", Some(from_model)),
        "no such parcel"
    );

    // Inject something different first, and check it took: releasing a
    // drain that was never changed proves nothing, which is what the
    // first version of this asserted.
    let injected = DrainSetting {
        coeff: from_model.coeff + 7.0,
        exponent: 0.5,
        offset: 0.0,
        delay: 0.0,
        h_open: 0.0,
        h_close: 0.0,
    };
    assert!(sim.set_drain("S1", "RB1", Some(injected)));
    assert_eq!(Some(injected), sim.drain("S1", "RB1"), "the injection took");
    assert!(sim.set_drain("S1", "RB1", None), "releasing is accepted");
    assert_eq!(
        Some(from_model),
        sim.drain("S1", "RB1"),
        "release must restore the model's own drain"
    );
}
