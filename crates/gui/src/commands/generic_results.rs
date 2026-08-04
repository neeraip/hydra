//! The engine-neutral shape of a served result catalog.
//!
//! Every engine publishes a §6 result-variable catalog; this module is how
//! one reaches the frontend. It holds no engine knowledge — no variable
//! ids, no file layouts, no unit factors — only the wire shape and the
//! mechanical descriptor→DTO conversion. Each engine's results provider
//! resolves which of its declared variables a given run actually carries
//! and what their ranges are, then hands the descriptors here.
//!
//! Keeping this separate from any one engine's provider is what lets the
//! frontend render both engines' legends with a single component: the
//! catalog is the contract, and neither side names an engine.

use serde::Serialize;

use hydra::common::{QuantityDescriptor, RampHint, VariableDescriptor};

/// One catalog variable with its per-run value range, ready for the legend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericVariableDto {
    pub id: String,
    pub label: String,
    /// Engine-authored compact notation (§6.1) for space-starved surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// The §5 quantity descriptor for the variable's SI values — the
    /// frontend converts to the active display system with it. `None` for
    /// dimensionless variables.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantity: Option<QuantityDescriptor>,
    /// How values map to a colour scale (§6.1), serialised in the
    /// contract's own tagged form.
    ///
    /// This carries the hint whole rather than reducing it to a name:
    /// `Categorical` owns the engine's state labels, and a variable whose
    /// states are dropped in transit cannot be drawn as anything but a
    /// meaningless gradient over status codes.
    pub ramp: RampHint,
    /// Per-run range, in SI. Both zero when the run carried no finite
    /// value for this variable.
    pub min: f64,
    pub max: f64,
}

impl GenericVariableDto {
    /// Convert one declared variable, given the range this run produced for
    /// it and a lookup for the engine's §5 quantity catalog.
    pub fn from_descriptor(
        v: &VariableDescriptor,
        min: f64,
        max: f64,
        quantity: impl FnOnce(&str) -> Option<QuantityDescriptor>,
    ) -> Self {
        // A run in which nothing varied leaves the accumulator at its
        // infinities; a legend cannot label those, so collapse to an
        // explicit empty range and let the frontend decide how to present
        // it.
        let (min, max) = if min.is_finite() && max.is_finite() {
            (min, max)
        } else {
            (0.0, 0.0)
        };
        Self {
            id: v.id.to_string(),
            label: v.label.to_string(),
            symbol: v.symbol.map(str::to_string),
            quantity: v.quantity.and_then(quantity),
            ramp: v.ramp.clone(),
            min,
            max,
        }
    }
}

/// The engine-described result catalog for one run, per element class.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericResultMetaDto {
    pub point_vars: Vec<GenericVariableDto>,
    pub polyline_vars: Vec<GenericVariableDto>,
    pub region_vars: Vec<GenericVariableDto>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hydra::common::CategoryItem;

    fn descriptor(ramp: RampHint) -> VariableDescriptor {
        VariableDescriptor {
            id: "status",
            label: "Status",
            symbol: Some("St"),
            quantity: None,
            ramp,
        }
    }

    /// The engine's state labels must survive the wire. They were once
    /// dropped by re-encoding the hint as a bare name, which left the
    /// frontend drawing a gradient across status codes.
    #[test]
    fn categorical_items_reach_the_wire() {
        let dto = GenericVariableDto::from_descriptor(
            &descriptor(RampHint::Categorical {
                items: vec![CategoryItem {
                    value: 3,
                    label: "Open".into(),
                    severity: Some(hydra::common::CategorySeverity::Nominal),
                }],
            }),
            0.0,
            1.0,
            |_| None,
        );
        let json = serde_json::to_value(&dto).expect("serialisable");
        assert_eq!(json["ramp"]["type"], "categorical");
        assert_eq!(json["ramp"]["items"][0]["value"], 3);
        assert_eq!(json["ramp"]["items"][0]["label"], "Open");
        // The engine's judgement travels with the state; without it an
        // application can order states but not rank them.
        assert_eq!(json["ramp"]["items"][0]["severity"], "nominal");
    }

    #[test]
    fn continuous_hints_carry_their_shape_name() {
        let dto = GenericVariableDto::from_descriptor(
            &descriptor(RampHint::Diverging),
            -1.0,
            1.0,
            |_| None,
        );
        let json = serde_json::to_value(&dto).expect("serialisable");
        assert_eq!(json["ramp"]["type"], "diverging");
    }

    /// A variable no element carried leaves the accumulator at its
    /// infinities, which serialise to JSON `null` and would blank the
    /// legend's labels.
    #[test]
    fn an_unpopulated_range_collapses_to_zero() {
        let dto = GenericVariableDto::from_descriptor(
            &descriptor(RampHint::Sequential),
            f64::INFINITY,
            f64::NEG_INFINITY,
            |_| None,
        );
        assert_eq!((dto.min, dto.max), (0.0, 0.0));
        let json = serde_json::to_value(&dto).expect("serialisable");
        assert!(json["min"].is_number(), "min must not serialise as null");
    }
}
