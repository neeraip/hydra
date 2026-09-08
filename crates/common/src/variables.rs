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

/// How remarkable one categorical state is (spec §6.1).
///
/// A statement about the domain, not about presentation: whether a state is
/// an abnormal condition is something only the engine knows. Applications
/// decide what — if anything — each level looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CategorySeverity {
    /// The ordinary condition; nothing to notice.
    Nominal,
    /// Worth attention, but not a fault.
    Caution,
    /// An abnormal condition.
    Alarm,
}

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
    /// Whether this state is unremarkable, worth attention, or wrong.
    ///
    /// `None` where the states carry no such judgement — a partition like a
    /// land-use class or a material has no abnormal member, and must not be
    /// made to invent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<CategorySeverity>,
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
    /// Values classed against a criterion's threshold bands (spec §7).
    ///
    /// The criterion is named rather than left to the application to
    /// work out: a valuation holds several criteria, and matching them to
    /// variables by quantity is a guess — two criteria can share one, and
    /// two engines can publish a variable of the same name meaning
    /// different things. Guessing produced a drainage map offered a
    /// threshold scale annotated with water-distribution numbers.
    Banded {
        /// Key of the criterion (spec §7.1) whose valuation supplies the
        /// thresholds. Must exist in the same engine's criteria catalog
        /// and carry severities.
        criterion: &'static str,
    },
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
    /// Compact engine-authored notation (≤3 chars) for space-starved
    /// surfaces — ideally the domain's standard symbol (spec §6.1) — or
    /// `None` for the application's own fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<&'static str>,
    /// Key of the quantity the values carry (spec §5), or `None` for
    /// dimensionless variables.
    pub quantity: Option<&'static str>,
    /// How values map to a colour scale.
    pub ramp: RampHint,
}

/// A variable published for a particular model (spec §6.3).
///
/// [`VariableDescriptor`] is a catalog entry, fixed in an engine's own
/// code, so its text is `'static`. Some variables instead take their
/// identity from the model — one concentration series per pollutant a
/// model declares — and an engine cannot name those ahead of time. This
/// is the same descriptor with owned text, so one list can carry both.
///
/// The fields mean exactly what §6.1 says they mean. In particular `id`
/// stays opaque: an application never parses it to recover the model
/// object it was composed from.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelVariable {
    /// Stable variable identifier, opaque to this layer.
    pub id: String,
    /// Human-facing name.
    pub label: String,
    /// Compact engine-authored notation (≤3 chars), or `None` for the
    /// application's own fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Key of the quantity the values carry (spec §5), or `None` for
    /// dimensionless variables.
    pub quantity: Option<String>,
    /// How values map to a colour scale.
    pub ramp: RampHint,
}

impl From<&VariableDescriptor> for ModelVariable {
    fn from(v: &VariableDescriptor) -> Self {
        ModelVariable {
            id: v.id.to_string(),
            label: v.label.to_string(),
            symbol: v.symbol.map(str::to_string),
            quantity: v.quantity.map(str::to_string),
            ramp: v.ramp.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §6.3: a catalog entry converts to a model variable whole. A field
    /// dropped here would silently un-publish a symbol or a quantity, and
    /// a `Categorical` ramp reduced to its name would leave the states out
    /// of a legend that has nothing else to draw them from.
    #[test]
    fn a_catalog_entry_converts_without_losing_a_field() {
        let ramp = RampHint::Categorical {
            items: vec![CategoryItem {
                value: 1,
                label: "Open".into(),
                severity: None,
            }],
        };
        let d = VariableDescriptor {
            id: "depth",
            label: "Depth",
            symbol: Some("y"),
            quantity: Some("depth"),
            ramp: ramp.clone(),
        };
        let m = ModelVariable::from(&d);
        assert_eq!(m.id, "depth");
        assert_eq!(m.label, "Depth");
        assert_eq!(m.symbol.as_deref(), Some("y"));
        assert_eq!(m.quantity.as_deref(), Some("depth"));
        assert_eq!(m.ramp, ramp);

        // The optional fields survive being absent, rather than becoming
        // empty strings a legend would render as a blank chip.
        let bare = VariableDescriptor {
            id: "x",
            label: "X",
            symbol: None,
            quantity: None,
            ramp: RampHint::Sequential,
        };
        let m = ModelVariable::from(&bare);
        assert_eq!((m.symbol, m.quantity), (None, None));
    }

    #[test]
    fn ramp_hint_serialises_tagged() {
        let ramp = RampHint::Categorical {
            items: vec![CategoryItem {
                value: 3,
                label: "Open".into(),
                severity: Some(CategorySeverity::Nominal),
            }],
        };
        let json = serde_json::to_value(&ramp).unwrap();
        assert_eq!(json["type"], "categorical");
        assert_eq!(json["items"][0]["value"], 3);
        assert_eq!(json["items"][0]["severity"], "nominal");
    }

    /// Severity is a real claim about the domain, so a state set that is
    /// merely a partition must be able to decline it — and declining must
    /// be distinguishable from claiming "nominal", not collapsed into it.
    #[test]
    fn a_state_without_a_judgement_omits_severity() {
        let ramp = RampHint::Categorical {
            items: vec![CategoryItem {
                value: 1,
                label: "Concrete".into(),
                severity: None,
            }],
        };
        let json = serde_json::to_value(&ramp).unwrap();
        assert!(
            json["items"][0].get("severity").is_none(),
            "absent severity must not serialise, got {}",
            json["items"][0]
        );
    }

    /// Catalogs are persisted and exchanged; a severity that round-trips to
    /// a different level would silently re-rank an engine's states.
    #[test]
    fn severity_round_trips() {
        for level in [
            CategorySeverity::Nominal,
            CategorySeverity::Caution,
            CategorySeverity::Alarm,
        ] {
            let item = CategoryItem {
                value: 0,
                label: "s".into(),
                severity: Some(level),
            };
            let json = serde_json::to_string(&item).unwrap();
            let back: CategoryItem = serde_json::from_str(&json).unwrap();
            assert_eq!(back.severity, Some(level));
        }
    }
}
