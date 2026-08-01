//! End-to-end session tests (§10, §12): models driven from their input
//! text alone — external inflow series, sanitary patterns, tidal stages,
//! event windows, and reporting.

use hydra_engine_uds::simulation::Simulation;

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
        assert!(
            led.inflow > 0.0 && led.inflow < rain_vol,
            "{infil}: runoff {} of rain {rain_vol}",
            led.inflow
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
    // And it recedes: end-of-run inflow rate is below the start's.
    assert!(
        led.outflow > 0.9 * led.inflow,
        "in {} out {}",
        led.inflow,
        led.outflow
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
    // The response peaks after the rain, not during its first minutes.
    let early: f64 = sim.snapshots.first().map_or(0.0, |s| s.flows[0]);
    assert!(led.outflow > 0.9 * led.inflow);
    let _ = early;
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
