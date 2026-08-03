//! Element taxonomy contract: element classes, kind descriptors, and
//! attribute schemas (spec §4).
//!
//! Everything here is *data*. The only structural vocabulary this layer
//! owns is [`ElementClass`] — the geometric and referential nature of an
//! element, which an application must know to render it. Everything else
//! (what a junction or a subcatchment *is*) travels as opaque ids and
//! engine-authored text, exactly as the recognition and reportable-output
//! contracts do.

use serde::{Deserialize, Serialize};

use crate::OptionKind;

/// The geometric and referential nature of an element kind (spec §4.1).
///
/// The class list is closed in this revision; extending it is an additive
/// spec change in this layer, never an engine decision. A subcatchment is
/// the proof case for [`ElementClass::Region`]: it is neither a node nor a
/// link, and a taxonomy offering only those two classes would have baked
/// one engine family's shape into the foundation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementClass {
    /// A located element: one coordinate. May anchor `Polyline` ends and
    /// `Region` discharge references.
    Point,
    /// A connecting element: references a from-point and a to-point, with
    /// optional intermediate vertices.
    Polyline,
    /// An areal element: a polygon boundary, with an optional reference to
    /// a `Point` element it discharges to.
    Region,
    /// A non-spatial named object (a curve, a pattern, a time series, a
    /// control). Enumerable and countable; presentation is
    /// application-defined and may be engine-specific.
    Collection,
}

/// Descriptor of one element kind in an engine's catalog (spec §4.2).
///
/// The catalog is static and model-free, like the block catalog: an
/// application must be able to build its chrome — tables, filters, layer
/// toggles, legends — before any model is loaded. `id` follows the block-id
/// stability rule: removing one, or changing the *meaning* of one, is a
/// break on the order of a file-format break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementKind {
    /// Stable kind identifier, opaque to this layer.
    pub id: &'static str,
    /// Human-facing singular name.
    pub label: &'static str,
    /// Human-facing plural name.
    pub label_plural: &'static str,
    /// The kind's element class.
    pub class: ElementClass,
    /// One- or two-character glyph for dense UI (markers, chips).
    pub badge: &'static str,
}

/// Description of one attribute an application may display for elements of
/// a kind (spec §4.3).
///
/// Reuses the option-descriptor value vocabulary ([`OptionKind`], spec
/// §3.2.1) and is advisory in exactly that sense: it tells a generic UI
/// what to show; it is not the validation authority, and an engine remains
/// free to hold data no schema advertises. This revision describes
/// attributes for **display**; editability, defaults, and creation flows
/// are a later additive revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeDescriptor {
    /// Field name in the element's attribute data. Stable per kind;
    /// renaming one is a break, like a block id.
    pub key: String,
    /// Human-facing name.
    pub label: String,
    /// Value shape and bounds.
    pub kind: OptionKind,
    /// Key of the physical quantity the value carries (spec §5), or `None`
    /// for dimensionless or textual attributes.
    pub quantity: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_class_serialises_lowercase() {
        assert_eq!(
            serde_json::to_string(&ElementClass::Region).unwrap(),
            "\"region\""
        );
        assert_eq!(
            serde_json::to_string(&ElementClass::Collection).unwrap(),
            "\"collection\""
        );
    }

    #[test]
    fn kind_descriptor_serialises_camel_case() {
        let kind = ElementKind {
            id: "k",
            label: "Kind",
            label_plural: "Kinds",
            class: ElementClass::Point,
            badge: "K",
        };
        let json = serde_json::to_value(kind).unwrap();
        assert_eq!(json["labelPlural"], "Kinds");
        assert_eq!(json["class"], "point");
    }
}
