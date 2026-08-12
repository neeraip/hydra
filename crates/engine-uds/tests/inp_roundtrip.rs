//! §14.13.2 across whatever models the caller has to hand.
//!
//! The unit fixture beside the writer is hand-built to exercise one
//! feature at a time; this drives real files, which is where a section
//! nobody thought about turns up. It reads from a directory named by
//! `HYDRA_UDS_CORPUS` and is inert without it — the repository vendors no
//! models, so the corpus is whatever the person running it points at.

use std::path::PathBuf;

use hydra_engine_uds::io::inp_writer::write_inp;
use hydra_engine_uds::io::objects::parse_network;

fn corpus() -> Vec<PathBuf> {
    let Ok(dir) = std::env::var("HYDRA_UDS_CORPUS") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("inp")))
        .collect();
    out.sort();
    out
}

#[test]
fn every_corpus_model_survives_export_and_re_import() {
    let models = corpus();
    if models.is_empty() {
        return;
    }
    let mut failures = Vec::new();
    for path in &models {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let (a, da) = parse_network(&text);
        // A model this engine already refuses is not this test's
        // business: export is defined on models that imported.
        if da.iter().any(|d| d.kind.is_error()) {
            continue;
        }
        let written = match write_inp(&a) {
            Ok(w) => w,
            Err(e) => {
                failures.push(format!("{name}: export refused: {e}"));
                continue;
            }
        };
        let (b, db) = parse_network(&written);
        let errors: Vec<_> = db.iter().filter(|d| d.kind.is_error()).collect();
        if !errors.is_empty() {
            failures.push(format!("{name}: re-import failed: {errors:?}"));
            continue;
        }
        // Counts first: a section the writer forgets shows up here as a
        // population that shrank, and says which one without a diff of
        // two whole models.
        for (what, x, y) in [
            ("vertices", a.vertices.len(), b.vertices.len()),
            ("links", a.links.len(), b.links.len()),
            ("parcels", a.parcels.len(), b.parcels.len()),
            ("curves", a.curves.len(), b.curves.len()),
            ("series", a.timeseries.len(), b.timeseries.len()),
            ("patterns", a.patterns.len(), b.patterns.len()),
            ("gages", a.gages.len(), b.gages.len()),
            ("constituents", a.constituents.len(), b.constituents.len()),
            ("land uses", a.land_uses.len(), b.land_uses.len()),
            ("transects", a.transects.len(), b.transects.len()),
            ("streets", a.streets.len(), b.streets.len()),
            ("aquifers", a.aquifers.len(), b.aquifers.len()),
            ("snowpacks", a.snowpacks.len(), b.snowpacks.len()),
            (
                "control measures",
                a.lid_controls.len(),
                b.lid_controls.len(),
            ),
            ("inflows", a.inflows.len(), b.inflows.len()),
            ("sanitary inflows", a.dry_weather.len(), b.dry_weather.len()),
            ("sewer inflows", a.rdii.len(), b.rdii.len()),
            ("treatments", a.treatments.len(), b.treatments.len()),
        ] {
            if x != y {
                failures.push(format!("{name}: {what} {x} -> {y}"));
            }
        }
        // Then idempotence, which catches an ordering that is stable in
        // the model and not in the file.
        if let Ok(again) = write_inp(&b) {
            if again != written {
                failures.push(format!("{name}: second export differs"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} models did not survive:\n{}",
        failures.len(),
        models.len(),
        failures.join("\n")
    );
}
