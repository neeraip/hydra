//! Models bundled with the demo, for someone who does not have one to hand.
//!
//! Most people arriving at a page like this have no `.inp` file within
//! reach, and a drop target with nothing to drop on it demonstrates
//! nothing. Two real models ship with the page instead — one per engine, so
//! the engine-detection story is visible rather than described.
//!
//! # Why they are compiled in
//!
//! The demo ships two ways, served from a directory and as one portable
//! HTML file, and the portable one cannot fetch anything. Compiling the
//! text into the module means one mechanism instead of one per delivery,
//! and it costs about 17 kB before compression on a bundle already over a
//! megabyte.
//!
//! # Why the text is fetched separately from the list
//!
//! [`catalog`] describes the examples and [`model`] returns one. Returning
//! both together would push every model across the boundary on page load to
//! populate a picker that names them, when at most one is ever run.
//!
//! Both files are unmodified upstream examples; `models/NOTICE.md` records
//! where each came from and its licence. They are deliberately not test
//! fixtures — see that file.

use serde::Serialize;

/// One bundled model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Example {
    /// Stable id, used to ask for the model text.
    pub id: &'static str,
    /// The file name it is offered under, which is also what the `.out`
    /// prolog records if results are captured.
    pub file_name: &'static str,
    /// The engine that owns it. Shown in the picker, and not passed to the
    /// run: the model is routed by its own contents like any other, which
    /// is worth demonstrating rather than short-circuiting.
    pub engine: &'static str,
    /// One line about what the model is.
    pub description: &'static str,
    /// What a reader should expect that would otherwise look like a fault.
    /// `None` when there is nothing to warn about.
    pub note: Option<&'static str>,
}

const NET1: &str = include_str!("../models/Net1.inp");
const SIMULATION1: &str = include_str!("../models/Simulation1.inp");

/// Every bundled model, in the order a picker should offer them.
pub const EXAMPLES: &[Example] = &[
    Example {
        id: "net1",
        file_name: "Net1.inp",
        engine: "wds",
        description: "A small distribution network modelling chlorine decay \
                      over 24 hours, with both bulk and wall reactions.",
        note: None,
    },
    Example {
        id: "simulation1",
        file_name: "Simulation1.inp",
        engine: "uds",
        // Said up front because the run prints a dozen of these, and a
        // reader who has not been told reads a wall of warnings on a
        // bundled example as a broken example.
        note: Some(
            "Several short channels in this model would Courant-limit the run, \
             which the engine reports as it goes. The warnings are expected. The \
             same ones appear at a terminal.",
        ),
        description: "A drainage network routed by dynamic wave with Horton \
                      infiltration, over a day and a half.",
    },
];

/// The examples, as JSON, without their model text.
pub fn catalog() -> String {
    serde_json::to_string(EXAMPLES).unwrap_or_else(|_| String::from("[]"))
}

/// One example's model text, by id.
pub fn model(id: &str) -> Option<&'static str> {
    match id {
        "net1" => Some(NET1),
        "simulation1" => Some(SIMULATION1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aux_files::AuxFiles;
    use crate::run::{run_to_completion, OpenRequest};

    /// Every listed example has text behind it.
    ///
    /// The list and the lookup are two separate matches on the same ids, so
    /// adding an example is two edits and forgetting the second one gives a
    /// picker entry that does nothing when clicked.
    #[test]
    fn every_listed_example_has_a_model() {
        for e in EXAMPLES {
            assert!(model(e.id).is_some(), "{} has no model text", e.id);
        }
    }

    #[test]
    fn an_unknown_id_has_none() {
        assert!(model("net99").is_none());
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = EXAMPLES.iter().map(|e| e.id).collect();
        let before = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), before, "two examples share an id");
    }

    /// The catalog is what the picker renders, so it has to be JSON and it
    /// has to carry no model text — the whole reason the two are separate.
    #[test]
    fn the_catalog_is_json_without_the_models() {
        let json = catalog();
        assert!(json.starts_with('['));
        assert!(json.contains("Net1.inp"));
        assert!(
            !json.contains("[JUNCTIONS]"),
            "the catalog is carrying model text"
        );
    }

    /// A bundled example that does not run is worse than no example: it is
    /// the first thing a visitor clicks.
    #[test]
    fn every_example_runs() {
        for e in EXAMPLES {
            let text = model(e.id).expect("model");
            let aux = AuxFiles::new();
            let (run, _) = run_to_completion(OpenRequest {
                model: text.as_bytes(),
                model_name: e.file_name,
                engine: None,
                aux: &aux,
                capture_results: false,
            })
            .unwrap_or_else(|f| panic!("{} failed to run: {:?}", e.id, f.diagnostics));
            assert!(
                !run.report_text().expect("report").is_empty(),
                "{} reported nothing",
                e.id
            );
        }
    }

    /// Each example claims an engine, and the claim is checked by routing
    /// rather than trusted — the picker shows it as a label, and a label
    /// that disagrees with what the run says would be a lie on the page.
    #[test]
    fn each_example_is_owned_by_the_engine_it_claims() {
        for e in EXAMPLES {
            let text = model(e.id).expect("model");
            let engine = hydra::engines::route(text.as_bytes())
                .unwrap_or_else(|err| panic!("{} routes nowhere: {err}", e.id));
            assert_eq!(engine.key, e.engine, "{} claims the wrong engine", e.id);
        }
    }

    /// Both engines are represented. One of the things the page shows is
    /// that the engine comes from the model rather than from a menu, and a
    /// single-engine example set cannot show it.
    #[test]
    fn both_available_engines_have_an_example() {
        for engine in hydra::common::ENGINES.iter().filter(|e| e.is_available()) {
            assert!(
                EXAMPLES.iter().any(|e| e.engine == engine.key),
                "no bundled example for the {} engine",
                engine.key
            );
        }
    }
}
