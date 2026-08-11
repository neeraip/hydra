//! Model input and output: the predecessor file formats (specification §14).
//!
//! This module is the Tier 1 boundary: everything here binds syntax and
//! interpretation, and nothing here constrains how results are computed.
//! Model bytes are supplied in memory by callers; this crate performs no
//! filesystem or network I/O.

pub mod admin;
pub mod climate;
pub mod hydrology;
pub mod iface;
pub mod keywords;
pub mod lex;
pub mod lid;
pub mod objects;
pub mod options;
pub mod out_reader;
pub mod out_writer;
pub mod quality;
pub mod rain;
pub mod rpt_writer;
pub mod snow_rdii;
pub mod streets;
pub mod survey;
pub mod tables;
pub mod transects;
pub mod validate;

use hydra_common::Recognition;

/// Sections SWMM defines that EPANET's input format does not (spec §14.11).
///
/// Presence of any one of these is positive evidence that the file is a
/// SWMM model. The list is deliberately narrower than "every section §14.5
/// supports": `[TITLE]`, `[OPTIONS]`, `[JUNCTIONS]`, `[PUMPS]`, `[CURVES]`,
/// `[PATTERNS]`, `[CONTROLS]`, `[REPORT]`, `[TAGS]`, `[COORDINATES]`,
/// `[VERTICES]`, `[LABELS]` and `[BACKDROP]` are omitted because EPANET
/// declares them too, so they carry no evidence either way.
///
/// Kept sorted so a reader can scan it; membership is by exact match on the
/// upper-cased section name. This list and `EPANET_ONLY_SECTIONS` are the
/// mirror image of the water-distribution engine's pair (its model spec
/// §4.1.3), so any INP file both engines see gets complementary verdicts.
const SWMM_ONLY_SECTIONS: &[&str] = &[
    "ADJUSTMENTS",
    "AQUIFERS",
    "CONDUITS",
    "COVERAGES",
    "DIVIDERS",
    "DWF",
    "EVAPORATION",
    "GWF",
    "HYDROGRAPHS",
    "INFILTRATION",
    "INFLOWS",
    "LANDUSES",
    "LID_CONTROLS",
    "LID_USAGE",
    "LOADINGS",
    "LOSSES",
    "ORIFICES",
    "OUTFALLS",
    "OUTLETS",
    "POLLUTANTS",
    "POLYGONS",
    "PROFILES",
    "RAINGAGES",
    "SNOWPACKS",
    "STORAGE",
    "SUBAREAS",
    "SUBCATCHMENTS",
    "TEMPERATURE",
    "TRANSECTS",
    "TREATMENT",
    "WEIRS",
    "XSECTIONS",
];

/// Sections EPANET defines that SWMM's input format does not (spec §14.11):
/// foreign markers that settle recognition against this engine, outranking
/// any shared section.
const EPANET_ONLY_SECTIONS: &[&str] = &[
    "DEMANDS",
    "EMITTERS",
    "ENERGY",
    "LEAKAGE",
    "MIXING",
    "PIPES",
    "QUALITY",
    "REACTIONS",
    "RESERVOIRS",
    "ROUGHNESS",
    "SOURCES",
    "STATUS",
    "TANKS",
    "TIMES",
    "VALVES",
];

/// Judge whether these bytes are a SWMM model (spec §14.11).
///
/// This engine's answer to the foundation layer's recognition question
/// (hydra-common spec §2.5). Section names only — no field is parsed, so
/// this stays cheap enough to run against every registered engine before
/// any model is read.
pub fn recognize(bytes: &[u8]) -> Recognition {
    // Shape test first: an INP file opens with a section header or a
    // comment. Anything else is no INP dialect at all.
    let first = bytes
        .iter()
        .find(|&&b| !b.is_ascii_whitespace())
        .copied()
        .unwrap_or(0);
    if !matches!(first, b'[' | b';') {
        return Recognition::no();
    }

    let text = String::from_utf8_lossy(bytes);
    let mut saw_exclusive = false;
    for line in text.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix('[') else {
            continue;
        };
        let Some(name) = rest.split(']').next() else {
            continue;
        };
        let name = name.trim().to_ascii_uppercase();
        // A foreign marker settles it, and outranks any SWMM-exclusive
        // section that might otherwise look like evidence for us. Name what
        // we think it is: §2.5's optional reason turns a bare refusal into
        // something an application can actually report.
        if EPANET_ONLY_SECTIONS.iter().any(|s| *s == name) {
            return Recognition::No {
                reason: Some(format!(
                    "this looks like an EPANET model, not a SWMM one \
                     (it declares a [{name}] section, which SWMM has no concept of)"
                )),
            };
        }
        if SWMM_ONLY_SECTIONS.iter().any(|s| *s == name) {
            saw_exclusive = true;
        }
    }
    if saw_exclusive {
        Recognition::Definite
    } else {
        // INP-shaped, nothing foreign, nothing exclusive: genuinely
        // indistinguishable from a water-distribution model by section
        // vocabulary.
        Recognition::Plausible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_a_swmm_model_definitely() {
        let inp = "[JUNCTIONS]\nJ1 100 3\n\n[SUBCATCHMENTS]\nS1 RG1 J1 10 50 500 0.5 0\n";
        assert_eq!(recognize(inp.as_bytes()), Recognition::Definite);
    }

    #[test]
    fn refuses_an_epanet_model_and_says_why() {
        let inp = "[JUNCTIONS]\nJ1 100\n\n[PIPES]\nP1 J1 J2 100 300 100 0 Open\n";
        let Recognition::No { reason: Some(r) } = recognize(inp.as_bytes()) else {
            panic!("an EPANET marker must refuse with a reason");
        };
        assert!(r.contains("EPANET"), "{r}");
        assert!(r.contains("[PIPES]"), "{r}");
    }

    #[test]
    fn a_foreign_marker_outranks_our_own_evidence() {
        // Both markers present: a malformed hybrid. Refusing is the safe
        // verdict — the wds engine refuses it symmetrically, so it routes
        // nowhere instead of to whichever engine answered first.
        let inp = "[SUBCATCHMENTS]\nS1 RG1 J1 10 50 500 0.5 0\n\n[PIPES]\nP1\n";
        assert!(!recognize(inp.as_bytes()).claims());
    }

    #[test]
    fn shared_sections_alone_are_only_plausible() {
        let inp = "[TITLE]\nA network\n\n[JUNCTIONS]\nJ1 100\n";
        assert_eq!(recognize(inp.as_bytes()), Recognition::Plausible);
    }

    #[test]
    fn refuses_bytes_with_no_inp_shape() {
        assert!(!recognize(b"PK\x03\x04not a model").claims());
        assert!(!recognize(b"").claims());
    }
}
