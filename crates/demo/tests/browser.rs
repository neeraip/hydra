//! The engines, run in a real browser.
//!
//! Everything else in this crate is tested on the host, which is fast and
//! covers every decision — and misses the entire class of bug that made
//! this crate hard to bring up. Both failures during the port compiled
//! cleanly, passed every host test, and only appeared when a browser ran
//! them:
//!
//! * `SystemTime::now()` in the engine's session lifecycle. There is no
//!   clock behind it on `wasm32-unknown-unknown`, so it panicked, and a
//!   panic compiled to wasm is an `unreachable` trap carrying no message.
//! * `chrono` without its `wasmbind` feature. Same shape: the report's date
//!   stamp asks the host what time it is, and chrono answered through a
//!   `SystemTime` that is not there.
//!
//! Neither is visible to `cargo check --target wasm32-unknown-unknown` —
//! both compile. Only executing the code finds them, and only a browser can
//! execute it.
//!
//! Run with `just test-wasm`.
//!
//! # Why so few tests
//!
//! This layer is expensive (a browser, a driver, a full wasm build) and
//! everything it could assert about *behaviour* is already asserted on the
//! host, where a failure names a line instead of a trap. So it asks one
//! question only: does the engine survive a real run here? Adding coverage
//! to this file is almost always the wrong place to add it.

#![cfg(target_arch = "wasm32")]

use hydra_demo::aux_files::AuxFiles;
use hydra_demo::run::{run_to_completion, OpenRequest};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// Compiled in rather than read: there is no filesystem here, which is the
/// whole reason this crate exists.
const WDS_MODEL: &[u8] = include_bytes!("../../../tests/fixtures/wds/four_node_loop.inp");
const UDS_MODEL: &[u8] = include_bytes!("../../../tests/fixtures/uds/single_conduit.inp");

fn report_for(model: &[u8]) -> String {
    let aux = AuxFiles::new();
    let (run, _) = run_to_completion(OpenRequest {
        model,
        model_name: "model.inp",
        engine: None,
        aux: &aux,
        capture_results: false,
    })
    .unwrap_or_else(|f| panic!("run failed: {:?}", f.diagnostics));
    run.report_text().expect("report")
}

/// The whole point: a model opens, solves and reports without trapping.
///
/// This is what `SystemTime::now()` broke. It failed on the *first* solver
/// step, so any run at all is enough to catch it.
#[wasm_bindgen_test]
fn a_wds_model_runs_to_completion_in_a_browser() {
    let report = report_for(WDS_MODEL);
    assert!(
        report.contains("Analysis begun"),
        "report is missing its analysis stamps:\n{report}"
    );
}

#[wasm_bindgen_test]
fn a_uds_model_runs_to_completion_in_a_browser() {
    let report = report_for(UDS_MODEL);
    assert!(!report.is_empty(), "a completed uds run reported nothing");
}

/// The date stamp is a real date, not the epoch.
///
/// Separate from the test above because the two fail differently. Without
/// `wasmbind`, chrono panics and the run above catches it. But the obvious
/// wrong "fix" — falling back to `UNIX_EPOCH` so nothing panics — passes
/// that test while stamping every report in a browser with 1970. This is
/// the assertion that refuses it.
///
/// Checked as "after 2020" rather than against a known date because the
/// host clock is whatever the machine running the test says it is; the
/// claim is only that a clock was found.
#[wasm_bindgen_test]
fn the_report_is_stamped_with_a_real_date() {
    let report = report_for(WDS_MODEL);
    let first = report.lines().next().unwrap_or_default().trim().to_string();
    let year: i32 = first
        .rsplit(' ')
        .next()
        .and_then(|y| y.parse().ok())
        .unwrap_or_else(|| panic!("no year at the end of the date stamp: {first:?}"));
    assert!(
        year > 2020,
        "the report's date stamp reads {year}, so no host clock was found: {first:?}"
    );
}

/// And so are the analysis stamps, which come from the engine's own clock
/// rather than the report writer's. Two clocks, two ways to lose one.
#[wasm_bindgen_test]
fn the_analysis_stamps_come_from_a_real_clock() {
    let report = report_for(WDS_MODEL);
    let begun = report
        .lines()
        .find(|l| l.contains("Analysis begun"))
        .unwrap_or_else(|| panic!("no analysis stamp:\n{report}"))
        .to_string();
    assert!(
        !begun.contains("1970"),
        "analysis stamp fell back to the epoch: {begun:?}"
    );
}

/// A failure has to arrive as a diagnostic rather than a trap. A panicking
/// error path is indistinguishable from a crash in a browser, and the page
/// would show "unreachable" where it should show what is wrong with the
/// model.
#[wasm_bindgen_test]
fn a_bad_model_fails_with_diagnostics_rather_than_trapping() {
    let aux = AuxFiles::new();
    let result = run_to_completion(OpenRequest {
        model: b"this is not a model of anything",
        model_name: "junk.inp",
        engine: None,
        aux: &aux,
        capture_results: false,
    });
    let Err(failure) = result else {
        panic!("junk bytes should not run");
    };
    assert_eq!(failure.exit, 1);
    assert_eq!(failure.diagnostics[0].code, "input/engine");
}
