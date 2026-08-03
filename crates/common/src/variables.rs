//! Result-variable contract: the per-element time-series variables a
//! completed simulation carries (spec §6).
//!
//! Catalogs are static; which variables are *present* in a given run is
//! resolved per-run by the engine (spec §6.2), the way block options are
//! resolved against a model. Consumers address results by (element class,
//! variable id, reporting period); wire encodings are the consumer's own
//! concern but must be derived from the catalog rather than fixing a
//! variable list (spec §6.3).

use serde::{Deserialize, Serialize};

/// One discrete state of a [`RampHint::Categorical`] variable.
///
/// `value` is the number the engine stores in the result series for this
/// state; `label` is engine-authored display text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryItem {
    /// The stored series value representing this state.
    pub value: i64,
    /// Human-facing label for this state.
    pub label: String,
}

/// How a variable's values are meaningfully mapped to a colour scale
/// (spec §6.1).
///
/// The only presentation vocabulary this layer contributes, and it is a
/// shape statement, never a colour: an application chooses palettes, band
/// edges, and legend styling; the engine says only which shape is truthful
/// for the data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RampHint {
    /// Magnitude on a continuous low→high scale.
    Sequential,
    /// Signed values around a meaningful zero (e.g. flow direction).
    Diverging,
    /// Values classed into user-configurable threshold bands.
    Banded,
    /// A closed set of discrete states, with engine-authored items.
    Categorical { items: Vec<CategoryItem> },
}

/// Descriptor of one result variable in an engine's per-class catalog
/// (spec §6.1).
///
/// `id` follows the block-id stability rule — application preferences and
/// saved views may reference it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariableDescriptor {
    /// Stable variable identifier, opaque to this layer.
    pub id: &'static str,
    /// Human-facing name.
    pub label: &'static str,
    /// Key of the quantity the values carry (spec §5), or `None` for
    /// dimensionless variables.
    pub quantity: Option<&'static str>,
    /// How values map to a colour scale.
    pub ramp: RampHint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_hint_serialises_tagged() {
        let ramp = RampHint::Categorical {
            items: vec![CategoryItem {
                value: 3,
                label: "Open".into(),
            }],
        };
        let json = serde_json::to_value(&ramp).unwrap();
        assert_eq!(json["type"], "categorical");
        assert_eq!(json["items"][0]["value"], 3);
    }
}
