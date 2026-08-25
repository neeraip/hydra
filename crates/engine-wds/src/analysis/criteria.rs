//! The engine's criteria catalog and its consumption (analysis spec §5;
//! hydra-common spec §7): the assessment standard as descriptors, and the
//! derivation of per-block options from a valuation.

use std::collections::HashMap;

use hydra_common::{BandCut, CategorySeverity, CriterionDescriptor, CriterionKind};

use crate::model::Network;

/// The assessment standard (analysis spec §5). Defaults are SI display
/// units of each criterion's quantity.
const CRITERIA: &[CriterionDescriptor] = &[
    CriterionDescriptor {
        key: "minPressure",
        label: "Min service pressure",
        help: "The pressure every junction must hold while serving demand; \
               the compliance figures judge against it.",
        quantity: Some("pressure"),
        kind: CriterionKind::Value { default: 14.0 },
        severities: &[CategorySeverity::Alarm, CategorySeverity::Nominal],
    },
    CriterionDescriptor {
        key: "pressure",
        label: "Pressure",
        help: "The pressure bands junctions are counted into, from low to high.",
        quantity: Some("pressure"),
        kind: CriterionKind::Band {
            cuts: &[
                BandCut {
                    key: "low",
                    label: "Low",
                    default: 24.0,
                },
                BandCut {
                    key: "required",
                    label: "Required",
                    default: 35.0,
                },
                BandCut {
                    key: "high",
                    label: "High",
                    default: 45.0,
                },
            ],
        },
        severities: &[
            CategorySeverity::Alarm,
            CategorySeverity::Caution,
            CategorySeverity::Nominal,
            CategorySeverity::Caution,
        ],
    },
    CriterionDescriptor {
        key: "velocity",
        label: "Velocity",
        help: "The velocity bands pipes are counted into, from settling to scouring.",
        quantity: Some("velocity"),
        kind: CriterionKind::Band {
            cuts: &[
                BandCut {
                    key: "low",
                    label: "Low",
                    default: 0.1,
                },
                BandCut {
                    key: "target",
                    label: "Target",
                    default: 0.5,
                },
                BandCut {
                    key: "high",
                    label: "High",
                    default: 1.5,
                },
            ],
        },
        severities: &[
            CategorySeverity::Caution,
            CategorySeverity::Nominal,
            CategorySeverity::Nominal,
            CategorySeverity::Alarm,
        ],
    },
    CriterionDescriptor {
        key: "minResidual",
        label: "Min residual",
        help: "The disinfectant residual every junction must keep (chemical \
               quality runs).",
        quantity: Some("concentration"),
        kind: CriterionKind::Value { default: 0.2 },
        severities: &[CategorySeverity::Alarm, CategorySeverity::Nominal],
    },
    CriterionDescriptor {
        key: "maxAge",
        label: "Max water age",
        help: "The water age no junction may exceed (age quality runs).",
        quantity: Some("age"),
        kind: CriterionKind::Value { default: 24.0 },
        severities: &[CategorySeverity::Nominal, CategorySeverity::Alarm],
    },
];

/// The engine's criteria catalog (hydra-common spec §7.2).
pub fn criteria_catalog() -> &'static [CriterionDescriptor] {
    CRITERIA
}

/// One criterion's value from a valuation: the supplied number, the
/// default when absent, or a §7.3 refusal.
fn value_of(valuation: &serde_json::Value, key: &str, default: f64) -> Result<f64, String> {
    match valuation.get(key) {
        None => Ok(default),
        Some(v) => match v.as_f64() {
            Some(n) if n.is_finite() => Ok(n),
            _ => Err(format!(
                "criterion {key:?} must be a finite number, got {v}"
            )),
        },
    }
}

/// One band's values from a valuation: the supplied ascending list, the
/// cut defaults when absent, or a §7.3 refusal.
fn band_of(valuation: &serde_json::Value, key: &str, cuts: &[BandCut]) -> Result<Vec<f64>, String> {
    let values = match valuation.get(key) {
        None => cuts.iter().map(|c| c.default).collect::<Vec<_>>(),
        Some(v) => {
            let arr = v
                .as_array()
                .ok_or_else(|| format!("criterion {key:?} must be a list, got {v}"))?;
            if arr.len() != cuts.len() {
                return Err(format!(
                    "criterion {key:?} must supply {} values, got {}",
                    cuts.len(),
                    arr.len()
                ));
            }
            arr.iter()
                .map(|v| {
                    v.as_f64()
                        .filter(|n| n.is_finite())
                        .ok_or_else(|| format!("criterion {key:?} holds a non-number: {v}"))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    // A band out of ascending order is degenerate, not malformed
    // (hydra-common §7.3): consumption omits what it cannot shape.
    Ok(values)
}

/// Per-block options from a valuation (analysis spec §5; hydra-common
/// spec §7.4). Options are file-display-unit inputs (§4.1.1), so each SI
/// value converts with the engine's own factors — the model's specific
/// gravity included, because an option participates in computation
/// against file values. A band that stops ascending after conversion
/// omits its block, which then runs on its documented defaults.
pub fn criteria_block_options(
    valuation: &serde_json::Value,
    network: &Network,
) -> Result<HashMap<&'static str, serde_json::Value>, String> {
    let ucf =
        crate::model::units::make_ucf(network.options.flow_units, network.options.specific_gravity);
    let si = crate::model::units::is_si(network.options.flow_units);
    let pressure = |m: f64| if si { m } else { m * ucf.pressure };
    let velocity = |ms: f64| if si { ms } else { ms * ucf.elev };
    let ascending = |edges: &[f64]| edges.windows(2).all(|w| w[1] > w[0]);

    let mut options = HashMap::new();

    let min_pressure = value_of(valuation, "minPressure", 14.0)?;
    options.insert(
        "wds.service-compliance",
        serde_json::json!({ "minPressure": pressure(min_pressure) }),
    );

    let pressure_cuts = band_cuts("pressure");
    let pressure_edges: Vec<f64> = band_of(valuation, "pressure", pressure_cuts)?
        .into_iter()
        .map(pressure)
        .collect();
    if ascending(&pressure_edges) {
        options.insert(
            "wds.pressure-thresholds",
            serde_json::json!({ "edges": pressure_edges }),
        );
    }

    let velocity_cuts = band_cuts("velocity");
    let velocity_edges: Vec<f64> = band_of(valuation, "velocity", velocity_cuts)?
        .into_iter()
        .map(velocity)
        .collect();
    if ascending(&velocity_edges) {
        options.insert(
            "wds.velocity-thresholds",
            serde_json::json!({ "edges": velocity_edges }),
        );
    }

    // Both quality criteria always travel (spec §5): which one applies
    // is the run's quality mode, which the block judges — and their
    // quantities read identically in both display systems, so no
    // conversion applies.
    options.insert(
        "wds.quality-compliance",
        serde_json::json!({
            "minResidual": value_of(valuation, "minResidual", 0.2)?,
            "maxAge": value_of(valuation, "maxAge", 24.0)?,
        }),
    );

    Ok(options)
}

/// The catalog's cut list for a band criterion. Panics on a non-band key —
/// a programming error the catalog tests rule out.
fn band_cuts(key: &str) -> &'static [BandCut] {
    match CRITERIA.iter().find(|c| c.key == key).map(|c| c.kind) {
        Some(CriterionKind::Band { cuts }) => cuts,
        _ => unreachable!("{key} is a cataloged band criterion"),
    }
}

#[cfg(test)]
mod tests {

    /// Every banded variable names a criterion this catalog declares, and
    /// every criterion it names says what its regions mean.
    ///
    /// The pair is what lets an application colour a threshold scale
    /// without recognising a variable by name — the contract's whole point
    /// (hydra-common spec §6.1, §7.2). A variable pointing at a criterion
    /// that is missing, or at one with no severities, would leave the map
    /// with thresholds it cannot interpret.
    #[test]
    fn every_banded_variable_resolves_to_a_criterion_with_severities() {
        let catalog = criteria_catalog();
        for class in [
            hydra_common::ElementClass::Point,
            hydra_common::ElementClass::Polyline,
            hydra_common::ElementClass::Region,
        ] {
            for v in crate::descriptors::result_variables(class) {
                let hydra_common::RampHint::Banded { criterion } = v.ramp else {
                    continue;
                };
                let found = catalog
                    .iter()
                    .find(|c| c.key == criterion)
                    .unwrap_or_else(|| {
                        panic!(
                            "variable {:?} bands by unknown criterion {criterion:?}",
                            v.id
                        )
                    });
                assert!(
                    !found.severities.is_empty(),
                    "variable {:?} bands by criterion {criterion:?}, which states no severities",
                    v.id
                );
            }
        }
    }

    /// One region more than there are cuts, or the top or bottom band has
    /// no meaning and the map has to invent one.
    #[test]
    fn severities_describe_one_region_more_than_there_are_cuts() {
        for d in criteria_catalog() {
            if d.severities.is_empty() {
                continue;
            }
            let cuts = match d.kind {
                hydra_common::CriterionKind::Value { .. } => 1,
                hydra_common::CriterionKind::Band { cuts } => cuts.len(),
            };
            assert_eq!(
                d.severities.len(),
                cuts + 1,
                "criterion {:?} has {cuts} cut(s) and {} severities",
                d.key,
                d.severities.len()
            );
        }
    }
    use super::*;

    fn network(units_line: &str) -> Network {
        let inp = format!(
            "[JUNCTIONS]\nJ1  0  10\n\n[RESERVOIRS]\nR1  100\n\n\
             [PIPES]\nP1  R1  J1  1000  300  100  0  Open\n\n\
             [OPTIONS]\nUnits  {units_line}\nHeadloss  H-W\n\n[END]\n"
        );
        crate::dialect::parse(inp.as_bytes()).expect("parse")
    }

    fn valuation() -> serde_json::Value {
        serde_json::json!({
            "minPressure": 14.0,
            "pressure": [24.0, 35.0, 45.0],
            "velocity": [0.1, 0.5, 1.5],
            // A key the catalog no longer declares, kept here on purpose:
            // §7.3 says an unknown key is ignored, and a project saved
            // before the flow criterion was retired still holds one.
            "flow": [0.1, 1.0, 10.0],
        })
    }

    /// Catalog integrity: unique keys, quantities the engine catalogs,
    /// band defaults strictly ascending.
    #[test]
    fn the_catalog_is_well_formed() {
        let mut seen = std::collections::HashSet::new();
        for c in CRITERIA {
            assert!(seen.insert(c.key), "duplicate criterion {}", c.key);
            assert!(!c.label.is_empty() && !c.help.is_empty());
            if let Some(q) = c.quantity {
                assert!(
                    crate::descriptors::QUANTITIES.iter().any(|d| d.key == q),
                    "{}: quantity {q:?} is not cataloged",
                    c.key
                );
            }
            if let CriterionKind::Band { cuts } = c.kind {
                assert!(
                    cuts.windows(2).all(|w| w[1].default > w[0].default),
                    "{}: band defaults must ascend",
                    c.key
                );
            }
        }
    }

    /// Criteria are SI; an SI model's options pass through untouched.
    #[test]
    fn si_valuation_feeds_si_options_unchanged() {
        let options = criteria_block_options(&valuation(), &network("LPS")).expect("options");
        assert_eq!(
            options["wds.service-compliance"]["minPressure"]
                .as_f64()
                .expect("number"),
            14.0
        );
        let edges: Vec<f64> = options["wds.pressure-thresholds"]["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .map(|v| v.as_f64().expect("number"))
            .collect();
        assert_eq!(edges, vec![24.0, 35.0, 45.0]);
    }

    /// Block options are file display units (§4.1.1), so a US model's
    /// valuation converts with the engine's own factors — 14 m of service
    /// pressure is ~20 psi.
    #[test]
    fn us_valuation_converts_to_file_units() {
        let options = criteria_block_options(&valuation(), &network("GPM")).expect("options");
        let psi = options["wds.service-compliance"]["minPressure"]
            .as_f64()
            .expect("number");
        assert!(
            (psi - 19.9).abs() < 0.2,
            "14 m should be ~20 psi, got {psi}"
        );
        let edges: Vec<f64> = options["wds.velocity-thresholds"]["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .map(|v| v.as_f64().expect("number"))
            .collect();
        assert!((edges[2] - 1.5 * 3.2808).abs() < 1e-6, "{edges:?}");
    }

    /// A degenerate band cannot make strictly-ascending edges; its block
    /// is omitted rather than the valuation failing (§7.4) — an editor
    /// mid-drag produces one transiently.
    #[test]
    fn a_degenerate_band_omits_its_block() {
        let mut v = valuation();
        v["pressure"] = serde_json::json!([24.0, 24.0, 45.0]);
        let options = criteria_block_options(&v, &network("LPS")).expect("options");
        assert!(!options.contains_key("wds.pressure-thresholds"));
        assert!(options.contains_key("wds.velocity-thresholds"));
        assert!(options.contains_key("wds.service-compliance"));
    }

    /// Absent keys take catalog defaults; an empty valuation is the whole
    /// default standard (§7.3).
    #[test]
    fn an_empty_valuation_is_the_default_standard() {
        let options =
            criteria_block_options(&serde_json::json!({}), &network("LPS")).expect("options");
        assert_eq!(
            options["wds.service-compliance"]["minPressure"]
                .as_f64()
                .expect("number"),
            14.0
        );
        assert!(options.contains_key("wds.pressure-thresholds"));
        assert!(options.contains_key("wds.velocity-thresholds"));
    }

    /// §7.3 malformation is a refusal naming the criterion.
    #[test]
    fn malformed_valuations_are_refused_by_name() {
        let mut v = valuation();
        v["velocity"] = serde_json::json!([0.5, "fast", 1.5]);
        let err = criteria_block_options(&v, &network("LPS")).expect_err("must refuse");
        assert!(err.contains("velocity"), "{err}");

        let mut v = valuation();
        v["minPressure"] = serde_json::json!("plenty");
        let err = criteria_block_options(&v, &network("LPS")).expect_err("must refuse");
        assert!(err.contains("minPressure"), "{err}");
    }
}
