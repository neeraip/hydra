//! §6.4's width contract, held from outside: the same model, run serial
//! and run across the worker pool, writes byte-identical results. Only
//! compiled with the `threads` feature; the contract it checks is exactly
//! why a serial (wasm) build and a threaded one can share every other
//! test between them.
#![cfg(feature = "threads")]

use hydra_engine_uds::simulation::engine::Simulation;
use std::fmt::Write as _;

/// An inflow surge down a long junction chain, with a regulator pocket
/// grafted near the tail — a weir and an orifice spilling to a storage
/// unit, a pump and a rated outlet draining it — plus constant
/// evaporation, so the pooled trial's every phase does real work: the
/// channel map, the vertex gather (laterals, §7.7 loss shares, positive
/// arrivals), the frozen-state structure phase, and the storage-loss
/// setup. The chain is long because engagement is: the pool only wakes
/// when every worker gets on the order of a hundred channels
/// (`CHANNELS_PER_WORKER`), so a model this size is what actually
/// exercises the pooled path — a small one would pass trivially by
/// never leaving the serial one.
const LINKS: usize = 2500;

fn model(threads: u32) -> String {
    let mut junctions = String::new();
    let mut conduits = String::new();
    let mut xsections = String::new();
    for i in 0..=LINKS {
        // A gentle fall toward the outfall keeps every channel wet and
        // working once the surge arrives.
        let invert = 50.0 - 40.0 * (i as f64) / (LINKS as f64);
        if i < LINKS {
            writeln!(junctions, "J{i}  {invert:.4}  4  0  0  0").unwrap();
            let a = i;
            let b = i + 1;
            writeln!(
                conduits,
                "C{a}  J{a}  {}  120  0.013  0  0  0  0",
                if b == LINKS {
                    "O1".to_string()
                } else {
                    format!("J{b}")
                }
            )
            .unwrap();
            writeln!(xsections, "C{a}  CIRCULAR  1.2  0  0  0  1").unwrap();
        }
    }
    format!(
        "[OPTIONS]
FLOW_UNITS           CMS
FLOW_ROUTING         DYNWAVE
THREADS              {threads}
START_DATE           01/01/2020
START_TIME           00:00:00
END_DATE             01/01/2020
END_TIME             00:30:00
ROUTING_STEP         00:00:05
REPORT_STEP          00:05:00

[EVAPORATION]
CONSTANT  4.0

[JUNCTIONS]
{junctions}
[OUTFALLS]
O1  9  FREE  NO

[STORAGE]
SU1  10  6  0.5  FUNCTIONAL  0  0  500

[CONDUITS]
{conduits}
[PUMPS]
P1  SU1  J2450  PC1  ON  0  0

[ORIFICES]
OR1  J2300  SU1  SIDE  0  0.65  NO  0

[WEIRS]
W1  J2400  SU1  TRANSVERSE  0.3  1.84  NO  0  0

[OUTLETS]
OL1  SU1  J2490  0  FUNCTIONAL  0.05  0.5  NO

[XSECTIONS]
{xsections}OR1  CIRCULAR  0.25  0  0  0
W1  RECT_OPEN  0.6  3  0  0

[INFLOWS]
J0  FLOW  TS1
J1200  FLOW  TS2

[CURVES]
PC1  PUMP2  0.5  0.02
PC1        2.0  0.05
PC1        4.0  0.08

[TIMESERIES]
TS1  0:00  0.0
TS1  0:05  1.4
TS1  0:20  1.4
TS1  0:25  0.0
TS2  0:00  0.05
TS2  0:30  0.05

[REPORT]
"
    )
}

/// A run's two byte surfaces: the results stream and the report, which
/// carries the §11.2 statistics the results file does not.
/// The report, minus the one line that echoes the model's own `THREADS`
/// option — the models under comparison differ in exactly that input,
/// and an input echo is not a result.
fn comparable_report(report: Vec<u8>) -> Vec<u8> {
    let text = String::from_utf8(report).expect("report is utf-8");
    text.lines()
        .filter(|l| !l.contains("Number of Threads"))
        .flat_map(|l| l.bytes().chain(std::iter::once(b'\n')))
        .collect()
}

fn results_bytes(model: &str) -> (Vec<u8>, Vec<u8>) {
    let (mut sim, _, _) = Simulation::open(model).expect("open");
    let held = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    struct Shared(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Shared {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("lock").extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    sim.begin_results(Box::new(Shared(held.clone())), false)
        .expect("attach");
    sim.run();
    sim.finish_results().expect("finish");
    let out = held.lock().expect("lock").clone();
    let mut report = Vec::new();
    sim.write_report(&mut report).expect("report");
    (out, comparable_report(report))
}

#[test]
fn a_threaded_run_is_byte_identical_to_the_serial_one() {
    let (serial, serial_rpt) = results_bytes(&model(1));
    assert!(!serial.is_empty(), "the serial run wrote nothing");
    assert!(!serial_rpt.is_empty(), "the serial run reported nothing");
    for width in [2, 8] {
        let (wide, wide_rpt) = results_bytes(&model(width));
        assert!(
            serial == wide,
            "width {width} moved the results; §6.4 must not depend on it"
        );
        assert!(
            serial_rpt == wide_rpt,
            "width {width} moved the report; the §11.2 statistics must not depend on it"
        );
    }
}
