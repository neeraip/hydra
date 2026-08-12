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

/// What an element kind does in the network, as distinct from what it is
/// geometrically (spec §4.3).
///
/// This exists because it is the distinction an application must draw to
/// present an *unsimulated* model at all: before any results exist there is
/// nothing to colour by, and a network drawn in one uniform tone tells a
/// reader nothing. `ElementClass` cannot answer it — a pump and a pipe are
/// both `Polyline`, a reservoir and a junction both `Point` — and kind
/// cannot either without the application naming kinds it should not know.
///
/// Carries no presentation. An application decides what a boundary looks
/// like; this layer decides only which kinds are boundaries.
///
/// Optional on `ElementKind`: some kinds have no role in the flow network,
/// and the absence is information rather than a gap to be defaulted away.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ElementRole {
    /// Carries flow without imposing a boundary or a control on it — a
    /// junction, a pipe, a conduit. The bulk of any model.
    Conveyance,
    /// Where the model meets what it does not simulate: a fixed head or
    /// stage, a storage volume, an outfall. Flow enters or leaves here.
    Boundary,
    /// Acts on the flow rather than merely passing it — a pump, a valve, a
    /// weir, an orifice, a flow divider.
    Control,
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
    /// What the kind does in the network (spec §4.3), or `None` for a kind
    /// that is not in the flow network at all — a rain gage conveys
    /// nothing, and a curve or a control rule is not in it to begin with.
    pub role: Option<ElementRole>,
    /// One- or two-character glyph for dense UI (markers, chips).
    pub badge: &'static str,
    /// Whether elements of this kind may be created (spec §4.5.3).
    ///
    /// Advisory, like [`AttributeDescriptor::editable`]: creation is the
    /// authority. `false` is the default a kind gets by saying nothing,
    /// so a catalog written before the editing contract offers nothing
    /// rather than everything.
    pub creatable: bool,
    /// What a new element of this kind would need that cannot be
    /// defaulted — a relation curve, a rating, an opening geometry.
    ///
    /// Present only when `creatable` is false, and required then: a
    /// refusal without a reason is a dead end, and the application shows
    /// this rather than inventing its own explanation. Plain text,
    /// engine-authored.
    pub not_creatable_because: Option<&'static str>,
}

/// Description of one attribute an application may display for elements of
/// a kind (spec §4.4).
///
/// Reuses the option-descriptor value vocabulary ([`OptionKind`], spec
/// §3.2.1) and is advisory in exactly that sense: it tells a generic UI
/// what to show; it is not the validation authority, and an engine remains
/// free to hold data no schema advertises.
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
    /// Whether a write to this attribute may be offered (spec §4.5.1).
    ///
    /// Advisory: the write is the authority, and an engine refuses one it
    /// will not accept whether or not this said so. It exists so a
    /// surface can offer an input rather than offer one and be refused,
    /// which teaches the user the same thing one interaction later.
    ///
    /// This says the *attribute* can be written, not that a particular
    /// element can be: an element that carries no value for the key has
    /// nothing to change, and offering an input there would invite
    /// creating a value the model never held.
    ///
    /// Defaulted on the wire so a schema written before the editing
    /// contract deserialises, offering nothing rather than everything.
    #[serde(default)]
    pub editable: bool,
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
            role: Some(ElementRole::Boundary),
            badge: "K",
            creatable: false,
            not_creatable_because: Some("a kind needs something only its engine knows"),
        };
        let json = serde_json::to_value(kind).unwrap();
        assert_eq!(json["labelPlural"], "Kinds");
        assert_eq!(json["class"], "point");
        assert_eq!(json["role"], "boundary");
        assert_eq!(json["creatable"], false);
        assert_eq!(
            json["notCreatableBecause"],
            "a kind needs something only its engine knows"
        );
    }

    /// A schema written before the editing contract has no `editable`
    /// field, and must deserialise as offering nothing rather than
    /// failing outright (spec §4.5.1).
    #[test]
    fn an_attribute_without_the_flag_is_not_editable() {
        let json = serde_json::json!({
            "key": "elevation",
            "label": "Elevation",
            "kind": { "type": "number" },
            "quantity": "elevation",
        });
        let attr: AttributeDescriptor =
            serde_json::from_value(json).expect("a pre-contract schema still reads");
        assert!(!attr.editable);
    }
}
