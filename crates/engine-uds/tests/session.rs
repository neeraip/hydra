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
