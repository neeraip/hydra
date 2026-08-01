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
