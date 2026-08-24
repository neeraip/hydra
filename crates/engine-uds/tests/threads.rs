//! §6.4's width contract, held from outside: the same model, run serial
//! and run across the worker pool, writes byte-identical results. Only
//! compiled with the `threads` feature; the contract it checks is exactly
//! why a serial (wasm) build and a threaded one can share every other
//! test between them.
#![cfg(feature = "threads")]

use hydra_engine_uds::simulation::engine::Simulation;
use std::fmt::Write as _;

/// An inflow surge down a long junction chain. The chain is long because
/// engagement is: the pool only wakes when every worker gets on the order
/// of a thousand channels (`CHANNELS_PER_WORKER`), so a model this size
/// is what actually exercises the split path — a small one would pass
/// trivially by never leaving the serial one.
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

[JUNCTIONS]
{junctions}
[OUTFALLS]
O1  9  FREE  NO

[CONDUITS]
{conduits}
[XSECTIONS]
{xsections}
[INFLOWS]
J0  FLOW  TS1

[TIMESERIES]
TS1  0:00  0.0
TS1  0:05  1.4
TS1  0:20  1.4
TS1  0:25  0.0

[REPORT]
"
    )
}

fn results_bytes(model: &str) -> Vec<u8> {
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
    out
}

#[test]
fn a_threaded_run_is_byte_identical_to_the_serial_one() {
    let serial = results_bytes(&model(1));
    assert!(!serial.is_empty(), "the serial run wrote nothing");
    for width in [2, 8] {
        let wide = results_bytes(&model(width));
        assert!(
            serial == wide,
            "width {width} moved the results; the §6.4 join must not depend on it"
        );
    }
}
