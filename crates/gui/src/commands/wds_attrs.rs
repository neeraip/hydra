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
use super::uds_attrs::{AttrValue, KindColumnDto, KindElementsDto};

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
    if let Some(node) = network.nodes.iter().find(|n| n.base.id == element_id) {
        return Some(node_values(node));
    }
    network
        .links
        .iter()
        .find(|l| l.base.id == element_id)
        .map(link_values)
}

fn node_values(node: &hydra::Node) -> (&'static str, HashMap<&'static str, AttrValue>) {
    use AttrValue::{Number, Text};
    let mut m: HashMap<&'static str, AttrValue> = HashMap::new();
    match &node.kind {
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
    }
}

fn link_values(link: &hydra::Link) -> (&'static str, HashMap<&'static str, AttrValue>) {
    use AttrValue::{Number, Text};
    let mut m: HashMap<&'static str, AttrValue> = HashMap::new();
    match &link.kind {
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
    }
}

/// Every element of one kind, with its §4.4 attribute columns and — for
/// a class that is somewhere — its positions.
///
/// The same shape the drainage engine serves, so one table renders
/// either. Its own values come from the same per-element builders the
/// inspector reads, so a column and a property row cannot disagree.
pub(crate) fn kind_elements(network: &hydra::Network, kind: &str) -> KindElementsDto {
    let mut rows: Vec<(String, HashMap<&'static str, AttrValue>)> = Vec::new();
    for node in &network.nodes {
        let (k, values) = node_values(node);
        if k == kind {
            rows.push((node.base.id.clone(), values));
        }
    }
    for link in &network.links {
        let (k, values) = link_values(link);
        if k == kind {
            rows.push((link.base.id.clone(), values));
        }
    }

    let ids: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
    let columns = hydra::descriptors::attribute_schema(kind)
        .into_iter()
        .map(|attr| {
            let values = rows
                .iter()
                .map(|(_, m)| match m.get(attr.key.as_str()) {
                    Some(AttrValue::Number(n)) => serde_json::json!(n),
                    Some(AttrValue::Text(t)) => serde_json::json!(t),
                    None => serde_json::Value::Null,
                })
                .collect();
            KindColumnDto {
                editable: attr.editable,
                key: attr.key,
                label: attr.label,
                quantity: attr
                    .quantity
                    .as_deref()
                    .and_then(super::results::wds_quantity),
                values,
            }
        })
        .collect();

    // Only the classes that are somewhere. A link's position is its two
    // ends, which the table shows as its own columns.
    let spatial = hydra::descriptors::ELEMENT_KINDS
        .iter()
        .find(|k| k.id == kind)
        .is_some_and(|k| {
            matches!(
                k.class,
                hydra::common::ElementClass::Point | hydra::common::ElementClass::Region
            )
        });
    let positions = if spatial {
        ids.iter()
            .map(|id| network.coordinates.get(id).map(|&(x, y)| [x, y]))
            .collect()
    } else {
        Vec::new()
    };
    KindElementsDto {
        ids,
        columns,
        positions,
    }
}

/// The inverses of the two factors the read applies. Named here rather
/// than inlined so a reader can see that there are exactly two, and that
/// each is the reciprocal of the one above.
const LPS_TO_M3S: f64 = 1.0 / M3S_TO_LPS;
const MM_TO_M: f64 = 1.0 / M_TO_MM;

/// Write one attribute, addressed by the schema key the read served.
///
/// The unit conventions are the read's, in reverse — a diameter arrives
/// in millimetres, a demand in litres per second, a tank's elevation as
/// its bottom. Getting one backwards writes a wrong value into the model
/// rather than merely showing one, and a round-trip test cannot see it,
/// so the tests below assert absolute values.
///
/// **The legacy patch path spells three of these differently**, because
/// it grew before the schema was published: `initialLevel` for
/// `initLevel`, `curve` for `headCurve`, and `powerKw` in kilowatts for
/// `power` in watts. The schema keys are the published contract and this
/// takes those; the editor still speaks its own until it moves over.
pub(crate) fn set_attribute(
    network: &mut hydra::Network,
    element_id: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    let number = || -> Result<f64, String> {
        value
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| format!("'{key}' takes a number"))
    };
    // An empty string clears an optional reference; a missing pattern is
    // how a junction says it has no pattern, not an error to report.
    let reference = || -> Option<String> {
        let s = value.as_str().unwrap_or("").trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let flag = || -> Result<bool, String> {
        match value.as_str().unwrap_or("").to_ascii_lowercase().as_str() {
            "yes" | "true" => Ok(true),
            "no" | "false" => Ok(false),
            other => Err(format!("'{key}' takes Yes or No, not '{other}'")),
        }
    };

    if let Some(node) = network.nodes.iter_mut().find(|n| n.base.id == element_id) {
        return match (&mut node.kind, key) {
            (hydra::NodeKind::Junction(_), "elevation")
            | (hydra::NodeKind::Reservoir(_), "head") => {
                node.base.elevation = number()?;
                Ok(())
            }
            (hydra::NodeKind::Junction(j), "baseDemand") => {
                let m3s = number()? * LPS_TO_M3S;
                // The read sums every category, so writing one total to a
                // junction that has several would silently drop the rest.
                // Refuse instead, naming the surface that can do it.
                match j.demands.len() {
                    0 => j.demands.push(hydra::DemandCategory {
                        base_demand: m3s,
                        pattern: None,
                        name: None,
                    }),
                    1 => j.demands[0].base_demand = m3s,
                    n => {
                        return Err(format!(
                            "'{element_id}' has {n} demand categories; \
                             their total cannot be set as one number"
                        ));
                    }
                }
                Ok(())
            }
            (hydra::NodeKind::Junction(j), "demandPattern") => {
                let p = reference();
                match j.demands.first_mut() {
                    Some(first) => first.pattern = p,
                    None => j.demands.push(hydra::DemandCategory {
                        base_demand: 0.0,
                        pattern: p,
                        name: None,
                    }),
                }
                Ok(())
            }
            (hydra::NodeKind::Reservoir(r), "headPattern") => {
                r.head_pattern = reference();
                Ok(())
            }
            (hydra::NodeKind::Tank(t), key) => {
                let stored = node.base.elevation;
                match key {
                    // Bottom in, bottom-plus-minimum stored. Both of
                    // these move the stored elevation, and forgetting
                    // either moves the tank instead of resizing it.
                    "elevation" => node.base.elevation = number()? + t.min_level,
                    "minLevel" => {
                        let bottom = stored - t.min_level;
                        t.min_level = number()?;
                        node.base.elevation = bottom + t.min_level;
                    }
                    "initLevel" => t.initial_level = number()?,
                    "maxLevel" => t.max_level = number()?,
                    "diameter" => t.diameter = number()? * MM_TO_M,
                    "minVolume" => t.min_volume = number()?,
                    "volumeCurve" => t.volume_curve = reference(),
                    "overflow" => t.overflow = flag()?,
                    other => return Err(unwritable(other)),
                }
                Ok(())
            }
            (_, other) => Err(unwritable(other)),
        };
    }

    let link = network
        .links
        .iter_mut()
        .find(|l| l.base.id == element_id)
        .ok_or_else(|| format!("element '{element_id}' not found"))?;
    match (&mut link.kind, key) {
        (hydra::LinkKind::Pipe(p), key) => match key {
            "length" => p.length = number()?,
            "diameter" => p.diameter = number()? * MM_TO_M,
            "roughness" => p.roughness = number()?,
            "minorLoss" => p.minor_loss = number()?,
            "checkValve" => p.check_valve = flag()?,
            other => return Err(unwritable(other)),
        },
        (hydra::LinkKind::Pump(p), "headCurve") => {
            p.head_curve = reference();
            // A curve and a constant power are mutually exclusive; the
            // one that was just set wins.
            if p.head_curve.is_some() {
                p.power = None;
            }
        }
        (hydra::LinkKind::Pump(p), "power") => {
            p.power = Some(number()?);
            p.head_curve = None;
        }
        (hydra::LinkKind::Pump(p), "speedPattern") => p.speed_pattern = reference(),
        (hydra::LinkKind::Pump(_), "speed") => link.base.initial_setting = Some(number()?),
        (hydra::LinkKind::Valve(v), key) => match key {
            "diameter" => v.diameter = number()? * MM_TO_M,
            "minorLoss" => v.minor_loss = number()?,
            "valveType" => {
                v.valve_type = match value.as_str().unwrap_or("").to_ascii_uppercase().as_str() {
                    "PRV" => hydra::ValveType::Prv,
                    "PSV" => hydra::ValveType::Psv,
                    "PBV" => hydra::ValveType::Pbv,
                    "FCV" => hydra::ValveType::Fcv,
                    "TCV" => hydra::ValveType::Tcv,
                    "GPV" => hydra::ValveType::Gpv,
                    "PCV" => hydra::ValveType::Pcv,
                    other => return Err(format!("unknown valve type '{other}'")),
                };
            }
            "setting" => {
                let raw = number()?;
                link.base.initial_setting =
                    Some(if matches!(v.valve_type, hydra::ValveType::Fcv) {
                        raw * LPS_TO_M3S
                    } else {
                        raw
                    });
            }
            other => return Err(unwritable(other)),
        },
        (_, other) => return Err(unwritable(other)),
    }
    Ok(())
}

fn unwritable(key: &str) -> String {
    format!("'{key}' cannot be edited here")
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

#[cfg(test)]
mod write_tests {
    use super::*;
    use crate::commands::test_fixtures::TEST_INP;

    fn model() -> hydra::Network {
        hydra::io::parse(TEST_INP.as_bytes()).expect("the fixture parses")
    }

    fn read(net: &hydra::Network, id: &str, key: &str) -> Option<f64> {
        element_attributes(net, id)?
            .into_iter()
            .find(|r| r.key == key)?
            .number
    }

    fn set(net: &mut hydra::Network, id: &str, key: &str, v: serde_json::Value) {
        set_attribute(net, id, key, &v).unwrap_or_else(|e| panic!("{id}.{key}: {e}"));
    }

    /// The write applies the read's factors in reverse, and a round trip
    /// cannot tell whether it applied them at all — the error cancels.
    /// So this reaches past the contract and asserts what the *model*
    /// now holds.
    #[test]
    fn a_diameter_arrives_in_millimetres_and_is_stored_in_metres() {
        let mut net = model();
        set(&mut net, "P1", "diameter", serde_json::json!(500.0));
        let stored = net
            .links
            .iter()
            .find_map(|l| match &l.kind {
                hydra::LinkKind::Pipe(p) if l.base.id == "P1" => Some(p.diameter),
                _ => None,
            })
            .expect("P1");
        assert!(
            (stored - 0.5).abs() < 1e-12,
            "500 mm stored as {stored} m, expected 0.5"
        );
        assert!((read(&net, "P1", "diameter").expect("a number") - 500.0).abs() < 1e-9);
    }

    #[test]
    fn a_demand_arrives_in_litres_per_second_and_is_stored_in_cubic_metres() {
        let mut net = model();
        set(&mut net, "J1", "baseDemand", serde_json::json!(20.0));
        let stored = net
            .nodes
            .iter()
            .find_map(|n| match &n.kind {
                hydra::NodeKind::Junction(j) if n.base.id == "J1" => {
                    Some(j.demands.iter().map(|d| d.base_demand).sum::<f64>())
                }
                _ => None,
            })
            .expect("J1");
        assert!(
            (stored - 0.02).abs() < 1e-12,
            "20 L/s stored as {stored} m³/s, expected 0.02"
        );
    }

    /// A tank's bottom and its minimum level both move the stored
    /// elevation, and forgetting either moves the tank instead of
    /// resizing it.
    #[test]
    fn a_tanks_bottom_and_minimum_level_keep_their_relationship() {
        let mut net = model();
        set(&mut net, "T1", "elevation", serde_json::json!(60.0));
        assert!((read(&net, "T1", "elevation").expect("a number") - 60.0).abs() < 1e-9);

        let min_before = read(&net, "T1", "minLevel").expect("a number");
        set(
            &mut net,
            "T1",
            "minLevel",
            serde_json::json!(min_before + 3.0),
        );
        // The bottom is where it was put: raising the minimum level
        // deepens the tank rather than lifting it.
        assert!(
            (read(&net, "T1", "elevation").expect("a number") - 60.0).abs() < 1e-9,
            "changing the minimum level moved the bottom"
        );
        let stored = net
            .nodes
            .iter()
            .find(|n| n.base.id == "T1")
            .map(|n| n.base.elevation)
            .expect("T1");
        assert!((stored - (60.0 + min_before + 3.0)).abs() < 1e-9);
    }

    /// Every key the read serves as a number must be writable by the
    /// same key, or a surface that read a row cannot write it back.
    #[test]
    fn every_numeric_row_can_be_written_by_the_key_it_was_read_by() {
        let mut checked = 0;
        for id in ["J1", "R1", "T1", "P1"] {
            let rows = element_attributes(&model(), id).expect("attributes");
            for r in rows
                .into_iter()
                .filter(|r| r.number.is_some() && r.editable)
            {
                let mut net = model();
                let before = r.number.expect("a number");
                let value = if before.abs() < 1e-9 {
                    1.0
                } else {
                    before * 1.5
                };
                set_attribute(&mut net, id, &r.key, &serde_json::json!(value))
                    .unwrap_or_else(|e| panic!("{id}.{}: {e}", r.key));
                let after = read(&net, id, &r.key).expect("a number");
                assert!(
                    (after - value).abs() < 1e-6,
                    "{id}.{} set {value}, read back {after}",
                    r.key
                );
                checked += 1;
            }
        }
        assert!(checked >= 8, "only {checked} attributes were exercised");
    }

    #[test]
    fn a_reference_is_cleared_by_an_empty_string() {
        let mut net = model();
        set(&mut net, "J1", "demandPattern", serde_json::json!("P7"));
        assert_eq!(
            element_attributes(&net, "J1")
                .expect("rows")
                .into_iter()
                .find(|r| r.key == "demandPattern")
                .and_then(|r| r.text),
            Some("P7".to_string())
        );
        set(&mut net, "J1", "demandPattern", serde_json::json!(""));
        // Cleared, so the row is gone rather than blank — an element
        // with no value for a key produces none (§4.5.1).
        assert!(element_attributes(&net, "J1")
            .expect("rows")
            .iter()
            .all(|r| r.key != "demandPattern"));
    }

    #[test]
    fn a_key_this_kind_does_not_carry_is_refused() {
        let mut net = model();
        let err = set_attribute(&mut net, "J1", "diameter", &serde_json::json!(100.0))
            .expect_err("a junction has no diameter");
        assert!(err.contains("diameter"), "unhelpful: {err}");
    }

    #[test]
    fn a_value_of_the_wrong_shape_is_refused() {
        let mut net = model();
        assert!(set_attribute(&mut net, "P1", "length", &serde_json::json!("wide")).is_err());
        assert!(set_attribute(&mut net, "P1", "checkValve", &serde_json::json!(3)).is_err());
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;
    use crate::commands::test_fixtures::TEST_INP;

    fn model() -> hydra::Network {
        hydra::io::parse(TEST_INP.as_bytes()).expect("the fixture parses")
    }

    /// A column and a property row are two views of one value, built by
    /// the same per-element function — so a table and an inspector
    /// showing different numbers for the same element is not something
    /// this can do. That was never true while the table read a binary
    /// snapshot and the inspector read nothing at all.
    #[test]
    fn a_column_agrees_with_the_property_row_beside_it() {
        let net = model();
        for kind in ["junction", "reservoir", "tank", "pipe"] {
            let table = kind_elements(&net, kind);
            for (row, id) in table.ids.iter().enumerate() {
                let rows = element_attributes(&net, id).expect("attributes");
                for column in &table.columns {
                    let Some(attr) = rows.iter().find(|r| r.key == column.key) else {
                        // The element carries no value for this key, so
                        // the table serves a null cell — the §4.5.1
                        // distinction, and nothing to compare.
                        assert!(table.columns.iter().any(|c| c.key == column.key));
                        continue;
                    };
                    if let Some(n) = attr.number {
                        let cell = column.values[row].as_f64().unwrap_or(f64::NAN);
                        assert!(
                            (cell - n).abs() < 1e-9,
                            "{kind}.{} reads {n} as a row and {cell} as a cell",
                            column.key
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_located_kind_carries_its_positions() {
        let net = model();
        let junctions = kind_elements(&net, "junction");
        assert_eq!(junctions.ids, vec!["J1"]);
        assert_eq!(junctions.positions, vec![Some([1.0, 2.0])]);
        // A pipe is somewhere only in the sense that its ends are.
        assert!(kind_elements(&net, "pipe").positions.is_empty());
    }

    #[test]
    fn a_kind_the_model_has_none_of_is_empty_rather_than_absent() {
        let net = model();
        let pumps = kind_elements(&net, "pump");
        assert!(pumps.ids.is_empty());
        // The columns are still published, so a table can draw its
        // headings before any element exists — the same reason the
        // catalog is model-free.
        assert!(!pumps.columns.is_empty(), "a pump kind still has columns");
    }

    #[test]
    fn the_columns_are_the_schemas_in_its_order() {
        let net = model();
        assert_eq!(
            kind_elements(&net, "pipe")
                .columns
                .iter()
                .map(|c| c.key.as_str())
                .collect::<Vec<_>>(),
            hydra::descriptors::attribute_schema("pipe")
                .iter()
                .map(|a| a.key.as_str())
                .collect::<Vec<_>>(),
        );
    }
}
