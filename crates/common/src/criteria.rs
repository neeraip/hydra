//! Criteria contract: engine-published descriptors of the assessment
//! standard a user asserts over simulated behaviour (spec §7).
//!
//! Everything here is *description*. The foundation knows no criterion
//! vocabulary: keys are opaque, meaning travels through engine-authored
//! text, and how a valuation shapes block production is the engine's own
//! (spec §7.4). Valuations themselves are plain JSON objects (spec §7.3)
//! and have no type here — they are caller-held data this layer never
//! interprets.

use serde::Serialize;

/// Descriptor of one criterion in an engine's criteria catalog (spec §7.2).
///
/// `key` is stable per engine: applications persist valuations against it,
/// so renaming one is a compatibility break.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CriterionDescriptor {
    /// Stable criterion identifier, unique within the engine.
    pub key: &'static str,
    /// Human-facing name.
    pub label: &'static str,
    /// One or two sentences on what the criterion judges.
    pub help: &'static str,
    /// §5 quantity key — values are in the quantity's SI display unit —
    /// or `None` for a dimensionless criterion.
    pub quantity: Option<&'static str>,
    /// Shape of the criterion's value.
    pub kind: CriterionKind,
}

/// Shape of one criterion's value (spec §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CriterionKind {
    /// A single number.
    Value {
        /// The engine's conventional standard.
        default: f64,
    },
    /// An ordered list of named cut points; a valuation supplies a
    /// same-length ascending list of numbers.
    Band {
        /// Cut points, defaults strictly ascending.
        cuts: &'static [BandCut],
    },
}

/// One named cut point of a band criterion (spec §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BandCut {
    /// Stable name of this cut within the band.
    pub key: &'static str,
    /// Human-facing label.
    pub label: &'static str,
    /// The engine's conventional standard for this cut.
    pub default: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire shape editors consume: camelCase, kind tagged by `type` —
    /// the same conventions as every other descriptor in this crate.
    #[test]
    fn descriptors_serialize_in_wire_shape() {
        let d = CriterionDescriptor {
            key: "freeboard",
            label: "Freeboard",
            help: "Clearance kept below the rim.",
            quantity: Some("depth"),
            kind: CriterionKind::Value { default: 0.3 },
        };
        let json = serde_json::to_value(d).unwrap();
        assert_eq!(json["quantity"], "depth");
        assert_eq!(json["kind"]["type"], "value");
        assert_eq!(json["kind"]["default"], 0.3);

        let band = CriterionDescriptor {
            key: "velocity",
            label: "Velocity",
            help: "Self-cleansing to erosive.",
            quantity: Some("velocity"),
            kind: CriterionKind::Band {
                cuts: &[
                    BandCut {
                        key: "selfCleansing",
                        label: "Self-cleansing",
                        default: 0.6,
                    },
                    BandCut {
                        key: "erosive",
                        label: "Erosive",
                        default: 3.0,
                    },
                ],
            },
        };
        let json = serde_json::to_value(band).unwrap();
        assert_eq!(json["kind"]["type"], "band");
        assert_eq!(json["kind"]["cuts"][1]["key"], "erosive");
    }
}
