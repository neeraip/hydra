//! What this crate has to get right, tested on the host.
//!
//! Every one of these runs the *same* code the browser runs — the
//! `wasm_bindgen` layer holds no judgement, so covering the decisions here
//! covers them there. What it cannot cover is the two things only a browser
//! can answer: that the module loads, and that `chrono` finds a clock. Both
//! are exercised by building and opening the demo page.

use super::*;
use crate::diagnostic::CLI_ERROR_CODES;

/// Read a workspace fixture, or skip the test where the fixtures are absent
/// (the CLI's `e2e_four_node_loop_runs_without_error` does the same).
fn fixture(engine: &str, name: &str) -> Option<Vec<u8>> {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?;
    std::fs::read(workspace.join("tests/fixtures").join(engine).join(name)).ok()
}

fn request<'a>(model: &'a [u8], aux: &'a AuxFiles) -> OpenRequest<'a> {
    OpenRequest {
        model,
        model_name: "model.inp",
        engine: None,
        aux,
        capture_results: false,
    }
}

// ── The whole point ───────────────────────────────────────────────────────────

/// A model runs, and the report it produces is the report the CLI prints.
///
/// Asserted against the engine's own writer rather than a stored string:
/// the claim is that this crate adds nothing and drops nothing, and a
/// golden file would only prove the report has not changed.
#[test]
fn a_wds_model_runs_and_reports_what_the_cli_would_print() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let (run, _) = run_to_completion(request(&bytes, &aux)).expect("run four_node_loop");

    assert_eq!(run.engine().key, "wds");
    let report = run.report_text().expect("report");

    let expected = {
        let network = hydra::io::parse(&bytes).expect("parse");
        let mut sim = hydra::Simulation::create();
        sim.load(network).expect("load");
        let units = sim.flow_units().expect("units");
        let mut es = hydra::engines::EngineSession::from_wds(sim, units);
        while !es.advance().expect("advance").done {}
        let mut buf: Vec<u8> = Vec::new();
        es.write_summary_text(&mut buf).expect("summary");
        String::from_utf8(buf).expect("utf-8")
    };

    // The date stamp on line one is the wall clock, which moves between the
    // two runs; everything below it is the simulation.
    let body = |s: &str| s.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(body(&report), body(&expected));
}

#[test]
fn a_uds_model_runs_and_reports() {
    let Some(bytes) = fixture("uds", "single_conduit.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let (run, _) = run_to_completion(request(&bytes, &aux)).expect("run single_conduit");

    assert_eq!(run.engine().key, "uds");
    assert!(
        !run.report_text().expect("report").is_empty(),
        "a completed uds run should produce a summary"
    );
}

/// A wds run streams its `.out` between steps and backfills the prolog at
/// the end, so a captured file is only correct if the shared sink survived
/// both. Checked by handing the bytes back to the engine's own reader.
#[test]
fn captured_results_are_a_readable_out_file() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let (run, _) = run_to_completion(OpenRequest {
        capture_results: true,
        ..request(&bytes, &aux)
    })
    .expect("run with results");

    let out = run.results_bytes().expect("captured results");
    assert!(out.len() > 1024, "an .out file should not be nearly empty");

    let dir = std::env::temp_dir().join(format!("hydra-demo-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("captured.out");
    std::fs::write(&path, &out).expect("write");
    let meta =
        hydra::io::out_reader::read_metadata_checked(&path).expect("engine reads its own .out");
    assert!(!meta.snapshot_times().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Not capturing results is the default, and has to mean *nothing was
/// held* — the whole reason it is opt-in.
#[test]
fn results_are_absent_unless_asked_for() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let (run, _) = run_to_completion(request(&bytes, &aux)).expect("run");
    assert!(run.results_bytes().is_none());
}

// ── Pacing ────────────────────────────────────────────────────────────────────

/// The browser's drive loop is small budgets in a rAF callback, and it must
/// reach the same end as one unbounded call. A budget of one step is the
/// extreme case of that, and the one most likely to expose an off-by-one in
/// the phase handling.
#[test]
fn a_one_step_budget_reaches_the_same_end_as_an_unbounded_one() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();

    let mut paced = Run::open(request(&bytes, &aux)).expect("open");
    let mut guard = 0;
    while !paced.progress().done {
        paced.advance(1).expect("advance");
        guard += 1;
        assert!(guard < 100_000, "a one-step run should still terminate");
    }

    let (whole, _) = run_to_completion(request(&bytes, &aux)).expect("run");
    let body = |s: String| s.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(
        body(paced.report_text().expect("paced report")),
        body(whole.report_text().expect("whole report"))
    );
}

/// Advancing a finished run is what a drive loop does on its last frame,
/// and it must not re-finalise the results or trip an error.
#[test]
fn advancing_past_the_end_is_harmless() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let mut run = Run::open(OpenRequest {
        capture_results: true,
        ..request(&bytes, &aux)
    })
    .expect("open");
    while !run.advance(u32::MAX).expect("advance").done {}
    let after_first = run.results_bytes().expect("results");

    for _ in 0..3 {
        let p = run.advance(64).expect("advance past the end");
        assert!(p.done);
    }
    assert_eq!(
        run.results_bytes().expect("results"),
        after_first,
        "finishing twice would rewrite the prolog"
    );
}

/// At a phase boundary the run has already moved on, so the phase that
/// *ended* has to be reported separately — labelling a finished phase with
/// its successor's name is the whole reason `completed_phase` exists.
#[test]
fn a_phase_boundary_names_the_phase_that_ended_not_the_next_one() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let mut run = Run::open(request(&bytes, &aux)).expect("open");

    // Only mid-run boundaries: the final one ends the phase the run is
    // still in, where naming both the same is correct.
    let mut boundaries = Vec::new();
    loop {
        let p = run.advance(1).expect("advance");
        match p.completed_phase {
            Some(ended) if !p.done => boundaries.push((ended, p.phase)),
            _ => {}
        }
        if p.done {
            break;
        }
    }

    assert!(
        !boundaries.is_empty(),
        "a wds run has at least one phase boundary"
    );
    for (ended, next) in &boundaries {
        assert_ne!(
            ended, next,
            "a boundary reported the same phase as ended and current"
        );
    }
    assert_eq!(
        boundaries.first().map(|(ended, _)| *ended),
        Some("Hydraulics"),
        "the first phase to end in a wds run is hydraulics"
    );
}

/// Every phase a wds run goes through has to be reported as ending, even
/// when the last step both ends one and finishes the run.
///
/// A steady-state model is where this bites: hydraulics finishes in one
/// step and quality in the next, so the step carrying `done` is also the
/// step that crosses the boundary. Handling completion first collapsed the
/// two, and the quality phase never appeared at all — the CLI shows it.
#[test]
fn a_steady_state_run_still_reports_both_phases() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let mut run = Run::open(request(&bytes, &aux)).expect("open");

    let mut ended = Vec::new();
    loop {
        let p = run.advance(1).expect("advance");
        if let Some(phase) = p.completed_phase {
            ended.push(phase);
        }
        if p.done {
            break;
        }
    }
    assert_eq!(ended, vec!["Hydraulics", "Water quality"]);
}

/// Every call that does not end a phase must say so, or a caller would
/// close its progress line on every frame.
#[test]
fn an_ordinary_step_completes_no_phase() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let mut run = Run::open(request(&bytes, &aux)).expect("open");
    assert_eq!(run.progress().completed_phase, None);
    let p = run.advance(1).expect("advance");
    if !p.done {
        assert_eq!(p.completed_phase, None, "the first step ends no phase");
    }
}

/// Progress must reach the model's own duration, not stop short of it —
/// a bar that ends at 97% reads as a run that failed quietly.
#[test]
fn progress_ends_at_the_full_duration() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let (run, _) = run_to_completion(request(&bytes, &aux)).expect("run");
    let p = run.progress();
    assert!(p.done);
    assert_eq!(p.t, p.duration);
    assert!(p.steps > 0);
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

/// Draining, not accumulating: a caller printing each batch as it arrives
/// must never be handed the same warning twice.
#[test]
fn diagnostics_are_handed_over_once() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let mut run = Run::open(request(&bytes, &aux)).expect("open");
    let mut seen = Vec::new();
    seen.extend(run.take_diagnostics());
    while !run.advance(8).expect("advance").done {
        seen.extend(run.take_diagnostics());
    }
    seen.extend(run.take_diagnostics());
    assert!(
        run.take_diagnostics().is_empty(),
        "a second collection should be empty"
    );

    let mut lines: Vec<String> = seen.iter().map(|d| d.to_line()).collect();
    let before = lines.len();
    lines.sort();
    lines.dedup();
    assert_eq!(before, lines.len(), "a diagnostic was reported twice");
}

/// The codes are hand-mirrored from the CLI, so nothing may escape with one
/// that is not in the list — a typo reads as plausible and would only be
/// caught by someone diffing two runs by eye.
#[test]
fn every_diagnostic_uses_a_known_code() {
    for (engine, name) in [
        ("wds", "four_node_loop.inp"),
        ("wds", "unbalanced_network.inp"),
        ("uds", "single_conduit.inp"),
        ("uds", "rdii_sanitary_inflow.inp"),
    ] {
        let Some(bytes) = fixture(engine, name) else {
            continue;
        };
        let aux = AuxFiles::new();
        let diagnostics = match run_to_completion(request(&bytes, &aux)) {
            Ok((_, d)) => d,
            Err(f) => f.diagnostics,
        };
        for d in &diagnostics {
            assert!(
                CLI_ERROR_CODES.contains(&d.code.as_str()) || d.code.starts_with("warning/"),
                "{engine}/{name} produced an unknown diagnostic code {:?}",
                d.code
            );
        }
    }
}

#[test]
fn diagnostics_are_written_one_json_line_each() {
    let diagnostics = vec![
        Diagnostic::error("input/parse", "first"),
        Diagnostic::warning("input/notice", "second"),
    ];
    let mut buf: Vec<u8> = Vec::new();
    write_diagnostics(&mut buf, &diagnostics).expect("write");
    let text = String::from_utf8(buf).expect("utf-8");
    assert_eq!(text.lines().count(), 2);
    assert!(text.lines().all(|l| l.starts_with('{') && l.ends_with('}')));
}

// ── Engine resolution ─────────────────────────────────────────────────────────

#[test]
fn an_unknown_engine_key_is_an_input_error() {
    let f = resolve_engine(Some("nope"), b"").expect_err("unknown key");
    assert_eq!(f.exit, EXIT_INPUT);
    assert_eq!(f.diagnostics[0].code, "input/engine");
    assert!(f.diagnostics[0].message.contains("unknown engine"));
}

/// A planned engine resolves but cannot run, and saying "unknown" would be
/// wrong — the key is reserved, not absent (common spec §2.3).
#[test]
fn a_planned_engine_is_refused_as_unimplemented_not_unknown() {
    let planned = hydra::common::ENGINES.iter().find(|e| !e.is_available());
    let Some(planned) = planned else {
        return; // every registered engine ships; nothing to assert
    };
    let f = resolve_engine(Some(planned.key), b"").expect_err("planned engine");
    assert_eq!(f.exit, EXIT_INPUT);
    let message = &f.diagnostics[0].message;
    assert!(message.contains("not yet implemented"), "{message}");
    assert!(!message.contains("unknown"), "{message}");
}

/// There is no default engine: bytes nobody claims are refused rather than
/// guessed at.
#[test]
fn bytes_no_engine_claims_are_refused() {
    let f = resolve_engine(None, b"this is not a model of anything").expect_err("no engine");
    assert_eq!(f.exit, EXIT_INPUT);
    assert_eq!(f.diagnostics[0].code, "input/engine");
}

/// An empty string from the page's "detect" option must mean *detect*, not
/// "an engine named nothing" — the two go down different paths and only one
/// of them works.
#[test]
fn an_empty_engine_key_means_detect() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let engine = resolve_engine(Some(""), &bytes).expect("empty key should detect");
    assert_eq!(engine.key, "wds");
}

#[test]
fn a_named_engine_overrides_detection() {
    let Some(bytes) = fixture("wds", "four_node_loop.inp") else {
        return;
    };
    let engine = resolve_engine(Some("wds"), &bytes).expect("named engine");
    assert_eq!(engine.key, "wds");
}

// ── Opening failures ──────────────────────────────────────────────────────────

#[test]
fn unreadable_bytes_fail_as_input_not_as_a_panic() {
    let aux = AuxFiles::new();
    let Err(f) = Run::open(OpenRequest {
        engine: Some("wds"),
        ..request(b"\x00\x01\x02 not an inp", &aux)
    }) else {
        panic!("garbage should not open");
    };
    assert_eq!(f.exit, EXIT_INPUT);
    assert!(!f.diagnostics.is_empty());
}

/// A model belonging to the other engine is a routing mistake, not a
/// damaged file, and the message has to say which tool it belongs to.
#[test]
fn a_uds_model_forced_through_wds_says_whose_model_it_is() {
    let Some(bytes) = fixture("uds", "single_conduit.inp") else {
        return;
    };
    let aux = AuxFiles::new();
    let Err(f) = Run::open(OpenRequest {
        engine: Some("wds"),
        ..request(&bytes, &aux)
    }) else {
        panic!("a SWMM model is not an EPANET one");
    };
    assert_eq!(f.exit, EXIT_INPUT);
    assert_eq!(f.diagnostics[0].code, "input/engine");
}

// ── Auxiliary files ───────────────────────────────────────────────────────────

/// The smallest routable drainage model, declaring an external climate
/// record by name.
const UDS_WITH_CLIMATE: &[u8] = b"[OPTIONS]\n\
    FLOW_UNITS       CFS\n\
    FLOW_ROUTING     DYNWAVE\n\n\
    [TEMPERATURE]\n\
    FILE climate.dat\n\n\
    [JUNCTIONS]\n\
    J1  100  4\n\n\
    [OUTFALLS]\n\
    O1  98  FREE\n\n\
    [CONDUITS]\n\
    C1  J1  O1  400  0.013  0  0\n\n\
    [XSECTIONS]\n\
    C1  CIRCULAR  1.5  0  0  0\n";

/// One daily record, in the user format the engine's parser accepts:
/// `station year month day tmax tmin`.
const CLIMATE_RECORD: &[u8] = b"STN 2026 1 1 50 30\nSTN 2026 1 2 52 31\n";

/// A declared climate file that nobody supplied changes the answer, so it
/// is said out loud rather than run past.
///
/// This is the browser's version of "the file was not beside the model",
/// and silence here would produce a run that looks complete and is not.
#[test]
fn a_missing_climate_file_is_reported_not_ignored() {
    let aux = AuxFiles::new();
    let mut run =
        Run::open(request(UDS_WITH_CLIMATE, &aux)).expect("open without the climate file");
    let notices = run.take_diagnostics();
    assert!(
        notices
            .iter()
            .any(|d| d.code == "input/notice" && d.message.contains("climate.dat")),
        "expected a notice naming the missing file, got {notices:?}"
    );
}

/// Supplying it silences the notice — the notice has to mean something, and
/// one that fires either way means nothing.
#[test]
fn a_supplied_climate_file_is_used_without_complaint() {
    let mut aux = AuxFiles::new();
    aux.insert("climate.dat", CLIMATE_RECORD.to_vec());
    let mut run = Run::open(request(UDS_WITH_CLIMATE, &aux)).expect("open with the climate file");
    let notices = run.take_diagnostics();
    assert!(
        !notices
            .iter()
            .any(|d| d.message.contains("was not supplied")),
        "expected no missing-file notice, got {notices:?}"
    );
}

/// A supplied file the engine cannot read is fatal, not a notice: running
/// on silently-absent climate data the user believes they provided is the
/// worst of the three outcomes.
#[test]
fn an_unreadable_climate_file_fails_the_open() {
    let mut aux = AuxFiles::new();
    aux.insert(
        "climate.dat",
        b"GHCND this is an archival format\n".to_vec(),
    );
    let Err(f) = Run::open(request(UDS_WITH_CLIMATE, &aux)) else {
        panic!("an unparseable climate file should not open");
    };
    assert_eq!(f.exit, EXIT_INPUT);
    assert_eq!(f.diagnostics[0].code, "input/parse");
    assert!(f.diagnostics[0].message.contains("climate.dat"));
}

/// A model that parses but cannot run yields one diagnostic per problem,
/// not one summarising them.
///
/// The CLI prints them all, and a reader fixing a model wants the list
/// rather than to rerun once per fault. Both tanks here start below their
/// minimum level, which is two faults of the same class — the case a
/// `for` loop that breaks early would silently reduce to one.
#[test]
fn a_failure_carries_every_error_not_just_the_first() {
    let model = b"[TITLE]\nbroken\n\n\
                  [JUNCTIONS]\n J1 100\n J2 100\n\n\
                  [TANKS]\n T1 100 5 20 1 50 0\n T2 100 5 20 1 50 0\n\n\
                  [PIPES]\n P1 T1 J1 100 300 100 0 OPEN\n \
                  P2 T2 J2 100 300 100 0 OPEN\n\n[END]\n";
    let aux = AuxFiles::new();
    let Err(f) = Run::open(OpenRequest {
        engine: Some("wds"),
        ..request(model, &aux)
    }) else {
        panic!("a tank starting outside its own range should not open");
    };
    assert_eq!(f.exit, EXIT_INPUT);
    assert_eq!(
        f.diagnostics.len(),
        2,
        "expected one diagnostic per faulty tank, got {:?}",
        f.diagnostics
    );
    assert!(f.diagnostics.iter().all(|d| d.code == "validation/network"));
}
