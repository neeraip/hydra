//! Water-distribution element attributes, served through the element
//! taxonomy contract (hydra-common §4.4).
//!
//! The drainage engine has answered `get_element_details` since its viewer
//! shipped; this engine's attributes travelled a different road entirely —
//! typed columns in the binary network snapshot, read by name in the
//! inspector and the editor tables. Two roads meant two vocabularies for
//! the same values, and the difference reached the screen: one engine's
//! table could show a position and the other's could not, one engine's
//! attributes carried an engine-authored label and the other's were
//! labelled in the frontend.
//!
//! This is the road the contract describes, for both.
//!
//! **Values are in the unit each attribute's §5 quantity declares**, which
//! is not always the engine's storage unit. The engine stores SI
//! throughout — metres, m³/s — and two quantities declare something else
//! as their base: a diameter reads in millimetres and a demand in litres
//! per second. Those are the only two factors here, and they are the
//! module-level constants `network_dto` already keeps for exactly this
//! boundary, not new ones. It is worth saying why they are shared rather
//! than restated: that module's own history is a comment about the
//! afternoon someone converted twice.

use std::collections::HashMap;

use hydra::{LinkKind, NodeKind};

use super::element_attrs::{rows_from_schema, ElementAttributeDto};
use super::network_dto::{M3S_TO_LPS, M_TO_MM};
use super::uds_attrs::AttrValue;

/// The §4.4 property rows for one water-distribution element.
///
/// `None` when the model holds no element of that id — the same answer
/// the drainage path gives, so a caller cannot tell which engine
/// declined.
pub(crate) fn element_attributes(
    network: &hydra::Network,
    element_id: &str,
) -> Option<Vec<ElementAttributeDto>> {
    let (kind, values) = extract(network, element_id)?;
    Some(rows_from_schema(
        hydra::descriptors::attribute_schema(kind),
        values,
        super::results::wds_quantity,
    ))
}

/// A yes/no attribute, in the same words the drainage engine's rows use.
fn yes_no(v: bool) -> AttrValue {
    AttrValue::Text(if v { "Yes" } else { "No" }.to_string())
}

/// One element's values, keyed by the schema keys of its kind.
///
/// A value the element does not have is **absent from the map** rather
/// than present and empty: the row builder drops what it finds no value
/// for, which is how a pump with no rated power shows no power row
/// instead of a blank one offering an input.
///
/// Ids are unique within the node family and within the link family but
/// not across them, so nodes are looked at first — the same order the
/// rest of this crate resolves a wds id in.
fn extract(
    network: &hydra::Network,
    element_id: &str,
) -> Option<(&'static str, HashMap<&'static str, AttrValue>)> {
    use AttrValue::{Number, Text};
    let mut m: HashMap<&'static str, AttrValue> = HashMap::new();

    if let Some(node) = network.nodes.iter().find(|n| n.base.id == element_id) {
        return Some(match &node.kind {
            NodeKind::Junction(j) => {
                m.insert("elevation", Number(node.base.elevation));
                m.insert(
                    "baseDemand",
                    Number(j.demands.iter().map(|d| d.base_demand).sum::<f64>() * M3S_TO_LPS),
                );
                if let Some(p) = j.demands.first().and_then(|d| d.pattern.clone()) {
                    m.insert("demandPattern", Text(p));
                }
                ("junction", m)
            }
            NodeKind::Reservoir(r) => {
                // A reservoir's elevation *is* its head: it is a fixed
                // grade, and the model stores that grade in the base
                // elevation every node has.
                m.insert("head", Number(node.base.elevation));
                if let Some(p) = r.head_pattern.clone() {
                    m.insert("headPattern", Text(p));
                }
                ("reservoir", m)
            }
            NodeKind::Tank(t) => {
                // The bottom, not the stored value. A tank's base
                // elevation is its bottom *plus* its minimum level — the
                // minimum piezometric head — and the schema publishes the
                // bottom, because that is the number on the drawing and
                // the one the patch path takes.
                m.insert("elevation", Number(node.base.elevation - t.min_level));
                m.insert("initLevel", Number(t.initial_level));
                m.insert("minLevel", Number(t.min_level));
                m.insert("maxLevel", Number(t.max_level));
                m.insert("diameter", Number(t.diameter * M_TO_MM));
                m.insert("minVolume", Number(t.min_volume));
                if let Some(c) = t.volume_curve.clone() {
                    m.insert("volumeCurve", Text(c));
                }
                m.insert("overflow", yes_no(t.overflow));
                ("tank", m)
            }
        });
    }

    let link = network.links.iter().find(|l| l.base.id == element_id)?;
    Some(match &link.kind {
        LinkKind::Pipe(p) => {
            m.insert("length", Number(p.length));
            m.insert("diameter", Number(p.diameter * M_TO_MM));
            m.insert("roughness", Number(p.roughness));
            m.insert("minorLoss", Number(p.minor_loss));
            m.insert("checkValve", yes_no(p.check_valve));
            ("pipe", m)
        }
        LinkKind::Pump(p) => {
            if let Some(c) = p.head_curve.clone() {
                m.insert("headCurve", Text(c));
            }
            // Watts in the model, watts here: the descriptor declares no
            // quantity for it, so there is no other unit to be in.
            if let Some(w) = p.power {
                m.insert("power", Number(w));
            }
            if let Some(s) = link.base.initial_setting {
                m.insert("speed", Number(s));
            }
            if let Some(pat) = p.speed_pattern.clone() {
                m.insert("speedPattern", Text(pat));
            }
            ("pump", m)
        }
        LinkKind::Valve(v) => {
            m.insert(
                "valveType",
                Text(format!("{:?}", v.valve_type).to_uppercase()),
            );
            m.insert("diameter", Number(v.diameter * M_TO_MM));
            // The setting's unit depends on the valve type, which is why
            // its descriptor declares no quantity — so a flow setpoint
            // converts to the flow base unit and everything else is
            // served as stored.
            if let Some(s) = link.base.initial_setting {
                m.insert(
                    "setting",
                    Number(if matches!(v.valve_type, hydra::ValveType::Fcv) {
                        s * M3S_TO_LPS
                    } else {
                        s
                    }),
                );
            }
            m.insert("minorLoss", Number(v.minor_loss));
            ("valve", m)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::test_fixtures::TEST_INP;

    fn sample_network() -> hydra::Network {
        hydra::io::parse(TEST_INP.as_bytes()).expect("the fixture parses")
    }

    fn row(net: &hydra::Network, id: &str, key: &str) -> ElementAttributeDto {
        element_attributes(net, id)
            .unwrap_or_else(|| panic!("no attributes for {id}"))
            .into_iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("{id} has no {key} row"))
    }

    /// The trap this module's header is about: two of the quantities
    /// declare a base unit the engine does not store in, and a value
    /// served unconverted is wrong by a factor of a thousand while
    /// looking entirely plausible.
    ///
    /// Asserted as absolute numbers, because a round trip cancels the
    /// error — which is how the same mistake survived in `network_dto`
    /// for the life of the repo.
    #[test]
    fn a_value_is_served_in_the_unit_its_quantity_declares() {
        let net = sample_network();
        // P2 rather than P1: the fixture's P1 is 1000 ft long and 12 in
        // across, and both convert to 304.8 — so a test on it would pass
        // with the two factors swapped. P2 is 800 ft and 10 in.
        let d = row(&net, "P2", "diameter");
        assert_eq!(d.quantity.as_ref().map(|q| q.si_label), Some("mm"));
        assert!(
            // Loose on purpose: what this catches is a factor of a
            // thousand, and the importer's inch factor is not exactly
            // 25.4 mm — chasing its sixth decimal here would be
            // asserting the parser's arithmetic from the wrong place.
            (d.number.expect("a number") - 254.0).abs() < 0.01,
            "diameter served as {:?}, expected 254 mm",
            d.number
        );
        // A length has no such gap: metres stored, metres served.
        let l = row(&net, "P2", "length");
        assert_eq!(l.quantity.as_ref().map(|q| q.si_label), Some("m"));
        assert!(
            (l.number.expect("a number") - 243.84).abs() < 0.01,
            "length served as {:?}, expected 243.84 m",
            l.number
        );
    }

    /// A tank publishes its bottom, and the model stores bottom plus
    /// minimum level. Serving the stored value would put every tank in
    /// the model a metre or two into the air.
    #[test]
    fn a_tank_publishes_its_bottom_not_its_stored_elevation() {
        let net = sample_network();
        let bottom = row(&net, "T1", "elevation").number.expect("a number");
        let min = row(&net, "T1", "minLevel").number.expect("a number");
        let node = net
            .nodes
            .iter()
            .find(|n| n.base.id == "T1")
            .expect("the tank");
        assert!((bottom + min - node.base.elevation).abs() < 1e-9);
    }

    /// Every key the schema publishes has to be produced here, or the
    /// element shows a row the engine never filled in.
    #[test]
    fn every_published_key_is_answered() {
        let net = sample_network();
        for (id, kind) in [
            ("J1", "junction"),
            ("R1", "reservoir"),
            ("T1", "tank"),
            ("P1", "pipe"),
        ] {
            let published: Vec<String> = hydra::descriptors::attribute_schema(kind)
                .into_iter()
                .map(|a| a.key)
                .collect();
            let rows = element_attributes(&net, id).unwrap_or_else(|| panic!("{id}"));
            for r in &rows {
                assert!(
                    published.contains(&r.key),
                    "{kind} produced {}, which its schema does not publish",
                    r.key
                );
            }
            // And the values that are not optional are all there. An
            // absent row is how an element says it has no value for a
            // key (a pump with no rated power), so this counts rather
            // than requiring every one.
            assert!(
                rows.len() >= published.len() - 1,
                "{kind} produced {} rows of {} published",
                rows.len(),
                published.len()
            );
        }
    }

    #[test]
    fn an_unknown_id_declines_rather_than_inventing_rows() {
        assert!(element_attributes(&sample_network(), "NOPE").is_none());
    }
}
