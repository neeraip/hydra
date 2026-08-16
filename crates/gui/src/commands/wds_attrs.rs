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
use super::network_dto::{LPS_TO_M3S, M3S_TO_LPS, MM_TO_M, M_TO_MM};
use super::uds_attrs::{AttrValue, CollectionDetailDto, KindColumnDto, KindElementsDto};

/// The §4.4 property rows for one water-distribution element.
///
/// `None` when the model holds no element of that id — the same answer
/// the drainage path gives, so a caller cannot tell which engine
/// declined.
///
/// `kind` says which family to look in. It matters because **this engine
/// keeps two namespaces**: a node and a link may share an id, and with
/// numeric ids they often do. Without it, a pipe `10` beside a junction
/// `10` served the junction's rows under the pipe's heading, and a tag
/// typed on the pipe went to the junction and reported success. `None`
/// is accepted for a caller that has only an id, and refuses rather than
/// guesses when the id names two things.
pub(crate) fn element_attributes(
    network: &hydra::Network,
    kind: Option<&str>,
    element_id: &str,
) -> Option<Vec<ElementAttributeDto>> {
    let (kind, values) = extract(network, kind, element_id)?;
    Some(rows_from_schema(
        hydra::descriptors::attribute_schema(kind),
        values,
        super::results::wds_quantity,
    ))
}

/// Which family a kind id belongs to, or `None` for a kind this engine
/// does not have.
fn family(kind: &str) -> Option<Family> {
    match kind {
        "junction" | "reservoir" | "tank" => Some(Family::Node),
        "pipe" | "pump" | "valve" => Some(Family::Link),
        _ => None,
    }
}

/// The two namespaces an id may live in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Node,
    Link,
}

/// A yes/no attribute, in the same words the drainage engine's rows use.
fn yes_no(v: bool) -> AttrValue {
    AttrValue::Text(if v { "Yes" } else { "No" }.to_string())
}

/// What a curve is for, in words rather than in the enum's spelling.
///
/// Written out rather than derived from `Debug`, because these reach the
/// screen: `PcvLossRatio` is a name for programmers and "Valve loss
/// ratio" is one for the person reading a table of fourteen curves.
///
/// `Generic` is the honest answer for a curve nothing references — it
/// has points and no declared meaning, which is a state a model may hold
/// rather than a gap.
///
/// `None` for a purpose this list has no word for. The enum is
/// `non_exhaustive`, so a catch-all naming one of the words above would
/// eventually call a new kind of curve by the wrong name — and an empty
/// cell says "nothing to report here", which is true, where a wrong word
/// is not.
fn curve_type_label(kind: hydra::CurveKind) -> Option<String> {
    use hydra::CurveKind as K;
    let label = match kind {
        K::Generic => "Unassigned",
        K::PumpHead => "Pump head",
        K::PumpEfficiency => "Pump efficiency",
        K::TankVolume => "Tank volume",
        K::GpvHeadloss => "Valve headloss",
        K::PcvLossRatio => "Valve loss ratio",
        _ => return None,
    };
    Some(label.to_string())
}

/// One element's values, keyed by the schema keys of its kind.
///
/// A value the element does not have is **absent from the map** rather
/// than present and empty: the row builder drops what it finds no value
/// for, which is how a pump with no rated power shows no power row
/// instead of a blank one offering an input.
///
/// Ids are unique within the node family and within the link family but
/// **not across them** — EPANET keeps the two namespaces apart and
/// numeric ids collide often, which the rename path has always said in
/// as many words.
///
/// So the caller's kind decides which family to look in. Without one,
/// an id that names exactly one element still resolves, and an id that
/// names two resolves to neither: looking at nodes first and taking what
/// it found is what served junction `10`'s rows for pipe `10`, and let a
/// tag typed on the pipe land on the junction and report success.
fn extract(
    network: &hydra::Network,
    kind: Option<&str>,
    element_id: &str,
) -> Option<(&'static str, HashMap<&'static str, AttrValue>)> {
    let want = kind.map(family);
    // A kind this engine does not have names nothing, rather than being
    // ignored down to a guess.
    if want == Some(None) {
        return None;
    }
    let want = want.flatten();
    let node = (want != Some(Family::Link))
        .then(|| network.nodes.iter().find(|n| n.base.id == element_id))
        .flatten();
    let link = (want != Some(Family::Node))
        .then(|| network.links.iter().find(|l| l.base.id == element_id))
        .flatten();
    match (node, link) {
        (Some(node), None) => {
            let (kind, mut m) = node_values(node);
            m.insert("tag", tag_value(&network.node_tags, element_id));
            Some((kind, m))
        }
        (None, Some(link)) => {
            let (kind, mut m) = link_values(link);
            m.insert("tag", tag_value(&network.link_tags, element_id));
            Some((kind, m))
        }
        // Both, and nothing said which. Refused rather than guessed —
        // the answer would be right half the time and look right always.
        (Some(_), Some(_)) | (None, None) => None,
    }
}

/// An element's tag, empty when it has none.
///
/// Always a value, never absent — which is the one place a tag departs
/// from every other attribute here. §4.5.1 makes a missing value mean
/// "this element has nothing to change", and for a tag that is the wrong
/// statement: an untagged element is one whose tag is empty, and a cell
/// that vanished for it could never be typed into.
fn tag_value(tags: &HashMap<String, String>, id: &str) -> AttrValue {
    AttrValue::Text(tags.get(id).cloned().unwrap_or_default())
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

/// Which kind an element is, without building its values.
///
/// Counting the elements of every kind is a page-load question — the
/// Editor's rail asks it before anything is shown — and answering it
/// through `kind_elements` built a value map per element per kind: ten
/// passes over ninety thousand elements to produce ten numbers.
fn node_kind_id(node: &hydra::Node) -> &'static str {
    match &node.kind {
        NodeKind::Junction(_) => "junction",
        NodeKind::Reservoir(_) => "reservoir",
        NodeKind::Tank(_) => "tank",
    }
}

fn link_kind_id(link: &hydra::Link) -> &'static str {
    match &link.kind {
        LinkKind::Pipe(_) => "pipe",
        LinkKind::Pump(_) => "pump",
        LinkKind::Valve(_) => "valve",
    }
}

/// How many elements of each kind the model holds.
pub(crate) fn kind_counts(network: &hydra::Network) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = hydra::descriptors::ELEMENT_KINDS
        .iter()
        .map(|k| (k.id.to_string(), 0))
        .collect();
    for node in &network.nodes {
        *counts.entry(node_kind_id(node).to_string()).or_default() += 1;
    }
    for link in &network.links {
        *counts.entry(link_kind_id(link).to_string()).or_default() += 1;
    }
    // The collection kinds, counted the same way the table lists them —
    // `the_counts_agree_with_the_tables_they_count` is what holds the
    // two together, and it walks every kind in the catalog rather than
    // the spatial ones, so a kind added to one and not the other fails
    // there rather than in the rail.
    counts.insert("curve".to_string(), network.curves.len());
    counts.insert("pattern".to_string(), network.patterns.len());
    counts.insert("control".to_string(), network.controls.len());
    counts.insert("rule".to_string(), network.rules.len());
    counts
}

/// Every element of one kind, with its §4.4 attribute columns and — for
/// a class that is somewhere — its positions.
///
/// The same shape the drainage engine serves, so one table renders
/// either. Its own values come from the same per-element builders the
/// inspector reads, so a column and a property row cannot disagree.
pub(crate) fn kind_elements(network: &hydra::Network, kind: &str) -> KindElementsDto {
    // Classified before its values are built, so an element of another
    // kind costs a match rather than a map.
    let mut rows: Vec<(String, HashMap<&'static str, AttrValue>)> = Vec::new();
    for node in &network.nodes {
        if node_kind_id(node) == kind {
            let mut m = node_values(node).1;
            m.insert("tag", tag_value(&network.node_tags, &node.base.id));
            rows.push((node.base.id.clone(), m));
        }
    }
    for link in &network.links {
        if link_kind_id(link) == kind {
            let mut m = link_values(link).1;
            m.insert("tag", tag_value(&network.link_tags, &link.base.id));
            rows.push((link.base.id.clone(), m));
        }
    }
    // The collection kinds, which this walk used to leave out entirely —
    // it visited the nodes and the links and stopped, so a model with
    // fourteen curves showed a Curves tab holding none. Everything below
    // this point already worked: a curve's points are served and
    // editable, it can be renamed, it can be removed. None of it was
    // reachable, because a table with no rows offers nothing to select.
    //
    // They carry no values of their own yet, so each contributes an id
    // and an empty map — a collection's substance is its contents
    // (§4.5.2.2), which the panel below the table serves.
    // Numbered from one: the reader keeps no name for a control or a
    // rule, because the names a file gives them are decoration that
    // nothing resolves through. Position is the whole of their identity,
    // and it is what the writer names them by on the way out.
    let numbered = |i: usize| (i + 1).to_string();
    match kind {
        "curve" => rows.extend(network.curves.iter().map(|c| {
            let mut m = HashMap::new();
            if let Some(label) = curve_type_label(c.kind) {
                m.insert("curveType", AttrValue::Text(label));
            }
            m.insert("points", AttrValue::Number(c.points.len() as f64));
            (c.id.clone(), m)
        })),
        "pattern" => rows.extend(network.patterns.iter().map(|p| {
            let mut m = HashMap::new();
            m.insert("factors", AttrValue::Number(p.factors.len() as f64));
            (p.id.clone(), m)
        })),
        "control" => rows.extend(network.controls.iter().enumerate().map(|(i, c)| {
            let mut m = HashMap::new();
            // The model holds a 1-based link index; the row names the
            // link, because an index is not what a reader is scanning
            // for.
            if let Some(link) = network.links.iter().find(|l| l.base.index == c.link) {
                m.insert("link", AttrValue::Text(link.base.id.clone()));
            }
            m.insert("enabled", yes_no(c.enabled));
            (numbered(i), m)
        })),
        "rule" => rows.extend(network.rules.iter().enumerate().map(|(i, r)| {
            let mut m = HashMap::new();
            m.insert("priority", AttrValue::Number(r.priority));
            // Every line the panel below would show: the premises and
            // both sets of actions, which is what "how big is this rule"
            // means and what the contents panel actually draws.
            m.insert(
                "clauses",
                AttrValue::Number(
                    (r.premises.len() + r.then_actions.len() + r.else_actions.len()) as f64,
                ),
            );
            (numbered(i), m)
        })),
        _ => {}
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
                references: attr.references,
                kind: attr.kind,
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

    let class = hydra::descriptors::ELEMENT_KINDS
        .iter()
        .find(|k| k.id == kind)
        .map(|k| k.class);
    // Only the classes that are at a place. A link is not at one — it
    // runs between two, which is `ends` below rather than a coordinate.
    let spatial = matches!(
        class,
        Some(hydra::common::ElementClass::Point | hydra::common::ElementClass::Region)
    );
    let positions = if spatial {
        ids.iter()
            .map(|id| network.coordinates.get(id).map(|&(x, y)| [x, y]))
            .collect()
    } else {
        Vec::new()
    };
    // Node indices are 1-based in the model, so the lookup goes through
    // the id rather than through the array — an off-by-one here would
    // name the wrong element on every row and still look plausible.
    let ends = if class == Some(hydra::common::ElementClass::Polyline) {
        let name = |index: usize| {
            network
                .nodes
                .iter()
                .find(|n| n.base.index == index)
                .map_or_else(String::new, |n| n.base.id.clone())
        };
        network
            .links
            .iter()
            .filter(|l| link_kind_id(l) == kind)
            .map(|l| [name(l.base.from_node), name(l.base.to_node)])
            .collect()
    } else {
        Vec::new()
    };
    KindElementsDto {
        ids,
        columns,
        positions,
        ends,
    }
}

/// The entry a positional id names, or `None` if it names none.
///
/// A control and a rule have no name — the reader keeps none, because the
/// names a file gives them are decoration nothing resolves through — so
/// they are addressed by their 1-based position, which is what
/// `kind_elements` lists them as and what the writer names them by.
fn at<T>(id: &str, mut entries: Vec<T>) -> Option<T> {
    let index = id.parse::<usize>().ok()?.checked_sub(1)?;
    (index < entries.len()).then(|| entries.swap_remove(index))
}

/// The contents of one collection element (§4.5.2.2).
///
/// This engine had none: the shared Editor lists its curves and patterns
/// from the catalog and opened a blank panel for every one of them,
/// because the only implementation of this was the drainage engine's.
/// The rail said a model had 14 curves and no surface could show one.
///
/// Numbers leave in the unit each column declares, which for a curve is
/// whatever its purpose implies — a volume curve is levels and volumes,
/// a pump curve flows and heads. `curve_axes` is the single authority
/// for that in both directions, so a read and a write cannot drift.
pub(crate) fn collection_detail(
    network: &hydra::Network,
    kind: &str,
    id: &str,
) -> CollectionDetailDto {
    match kind {
        "curve" => network
            .curves
            .iter()
            .find(|c| c.id == id)
            .map(|c| {
                let axes = super::network_dto::curve_axes(c.kind);
                CollectionDetailDto {
                    columns: axes.iter().map(|a| a.label().to_string()).collect(),
                    quantities: axes
                        .iter()
                        .map(|a| a.quantity().and_then(super::results::wds_quantity))
                        .collect(),
                    rows: c
                        .points
                        .iter()
                        .map(|p| vec![p.x * axes[0].scale(), p.y * axes[1].scale()])
                        .collect(),
                    lines: Vec::new(),
                    editable: true,
                    note: None,
                }
            })
            .unwrap_or_default(),
        "pattern" => network
            .patterns
            .iter()
            .find(|p| p.id == id)
            .map(|p| CollectionDetailDto {
                columns: vec!["Interval".to_string(), "Factor".to_string()],
                quantities: vec![None, None],
                rows: p
                    .factors
                    .iter()
                    .enumerate()
                    // 1-based: a modeller counts hour 1, not hour 0. The
                    // same convention the drainage engine's patterns use,
                    // so one table reads the same under either.
                    .map(|(i, f)| vec![(i + 1) as f64, *f])
                    .collect(),
                lines: Vec::new(),
                editable: true,
                note: None,
            })
            .unwrap_or_default(),
        // Controls and rules are language, and this path has no way to
        // parse them back — the reader that can is the model reader.
        // Served to be read, not rewritten (§4.5.2.2).
        //
        // The text is the *writer's*, asked of the engine rather than
        // formatted here. A statement built in this file would be a
        // second implementation of the valve-setting and tank-grade
        // conversions the section needs, and a control shown in one set
        // of units and saved in another is the kind of wrong that reads
        // as right. Both are addressed by position, which is the whole
        // of a control's identity — see `kind_elements`.
        "control" => at(id, hydra::io::control_statements(network))
            .filter(|line| !line.is_empty())
            .map_or_else(CollectionDetailDto::default, |line| CollectionDetailDto {
                lines: vec![line],
                editable: false,
                ..CollectionDetailDto::default()
            }),
        "rule" => at(id, hydra::io::rule_statements(network)).map_or_else(
            CollectionDetailDto::default,
            |lines| CollectionDetailDto {
                lines,
                editable: false,
                ..CollectionDetailDto::default()
            },
        ),
        _ => CollectionDetailDto::default(),
    }
}

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
    kind: Option<&str>,
    element_id: &str,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), String> {
    // Which namespace the id is in, decided before anything is written.
    // A node and a link may share one — EPANET keeps the two apart, and
    // numeric ids collide often — so an id alone is half an address. A
    // tag typed on pipe `10` used to land on junction `10` and report
    // success, which is the worst shape a bug of this kind takes.
    let want = match kind.map(family) {
        Some(None) => return Err(format!("'{}' is not a kind here", kind.unwrap_or(""))),
        other => other.flatten(),
    };
    let is_node =
        want != Some(Family::Link) && network.nodes.iter().any(|n| n.base.id == element_id);
    let is_link =
        want != Some(Family::Node) && network.links.iter().any(|l| l.base.id == element_id);
    if is_node && is_link {
        return Err(format!(
            "'{element_id}' names both a node and a link; say which"
        ));
    }
    if !is_node && !is_link {
        return Err(format!("no element '{element_id}'"));
    }
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

    // Before the per-kind arms, because a tag belongs to the element
    // rather than to its kind, and it lives in a table beside the model
    // rather than on the element — so there is nothing for those arms
    // to match on.
    if key == "tag" {
        let text = value.as_str().unwrap_or("").trim().to_string();
        let tags = if is_node {
            &mut network.node_tags
        } else {
            &mut network.link_tags
        };
        // An emptied tag is removed rather than stored blank: the writer
        // emits a line per entry, and a blank one is a line the file did
        // not have before the edit.
        if text.is_empty() {
            tags.remove(element_id);
        } else {
            tags.insert(element_id.to_string(), text);
        }
        return Ok(());
    }

    if let Some(node) = is_node
        .then(|| network.nodes.iter_mut().find(|n| n.base.id == element_id))
        .flatten()
    {
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
                             edit them in Demand categories, not as one total"
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
        element_attributes(net, None, id)
            .unwrap_or_else(|| panic!("no attributes for {id}"))
            .into_iter()
            .find(|r| r.key == key)
            .unwrap_or_else(|| panic!("{id} has no {key} row"))
    }

    /// The invariant drainage has had since its inspector could offer an
    /// input: an attribute that does not read as editable must be one
    /// the setter refuses, and one that does must be one it takes.
    ///
    /// Water distribution never had it. The flag and the setter live in
    /// two files, and nothing but this pairs them — which is exactly how
    /// an inlet's fields were marked editable with no setter behind them.
    /// Every editable value can be changed, and put back.
    ///
    /// The second half is what an application leans on and nothing was
    /// asserting: undo works by writing the value that was there before,
    /// so an attribute whose write is not invertible is one the history
    /// cannot restore. A write that clamps, converts asymmetrically, or
    /// touches a second field passes "set it and read it back" and fails
    /// here — a tank publishes its *bottom* while the model stores the
    /// minimum piezometric head, which is exactly the shape that would.
    ///
    /// The sample comes from the model rather than from a list written
    /// here, so adding a kind to the fixture extends the check by
    /// itself.
    #[test]
    fn every_editable_value_can_be_changed_and_put_back() {
        let value_of = |net: &hydra::Network, kind: &str, id: &str, key: &str| -> f64 {
            element_attributes(net, Some(kind), id)
                .unwrap_or_else(|| panic!("{id} has no attributes"))
                .into_iter()
                .find(|r| r.key == key)
                .unwrap_or_else(|| panic!("{id} has no {key} row"))
                .number
                .expect("a number")
        };
        let net = sample_network();
        let mut checked = 0;
        for kind in hydra::descriptors::ELEMENT_KINDS {
            let Some(id) = kind_elements(&net, kind.id).ids.first().cloned() else {
                continue;
            };
            let Some(rows) = element_attributes(&net, Some(kind.id), &id) else {
                continue;
            };
            for row in rows.into_iter().filter(|r| r.editable) {
                let Some(before) = row.number else { continue };
                let changed = if before.abs() < 1e-9 {
                    1.0
                } else {
                    before * 1.5
                };

                let mut draft = sample_network();
                set_attribute(
                    &mut draft,
                    Some(kind.id),
                    &id,
                    &row.key,
                    &serde_json::json!(changed),
                )
                .unwrap_or_else(|e| panic!("{}.{} refused the edit: {e}", kind.id, row.key));
                let now = value_of(&draft, kind.id, &id, &row.key);
                assert!(
                    (now - changed).abs() < 1e-6,
                    "{}.{} was set to {changed} and reads {now}",
                    kind.id,
                    row.key
                );

                // The undo: the same write, with what was there before.
                set_attribute(
                    &mut draft,
                    Some(kind.id),
                    &id,
                    &row.key,
                    &serde_json::json!(before),
                )
                .unwrap_or_else(|e| panic!("{}.{} refused the undo: {e}", kind.id, row.key));
                let back = value_of(&draft, kind.id, &id, &row.key);
                assert!(
                    (back - before).abs() < 1e-6,
                    "{}.{} started at {before}, was set to {changed}, and came back {back}",
                    kind.id,
                    row.key
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 8,
            "only {checked} editable values were exercised"
        );
    }

    #[test]
    fn the_editable_flag_and_the_setter_agree() {
        let sample = |kind: &str| -> Option<&'static str> {
            match kind {
                "junction" => Some("J1"),
                "reservoir" => Some("R1"),
                "tank" => Some("T1"),
                "pipe" => Some("P1"),
                _ => None,
            }
        };
        let mut checked = 0;
        for kind in hydra::descriptors::ELEMENT_KINDS {
            let Some(id) = sample(kind.id) else { continue };
            for attr in hydra::descriptors::attribute_schema(kind.id) {
                let mut net = sample_network();
                // A value of the shape the attribute declares, so a
                // refusal cannot come from the value being wrong rather
                // than the key being unwritable.
                let value = match &attr.kind {
                    hydra::common::OptionKind::Number { .. }
                    | hydra::common::OptionKind::Integer { .. } => serde_json::json!(1.0),
                    hydra::common::OptionKind::Boolean { .. } => serde_json::json!("Yes"),
                    hydra::common::OptionKind::Choice { items, .. } => {
                        serde_json::json!(items
                            .first()
                            .map(|i| i.value.clone())
                            .unwrap_or_default())
                    }
                    _ => serde_json::json!(""),
                };
                let took = set_attribute(&mut net, None, id, &attr.key, &value).is_ok();
                assert_eq!(
                    took,
                    attr.editable,
                    "{}.{} reads as editable={} and the setter {}",
                    kind.id,
                    attr.key,
                    attr.editable,
                    if took { "took it" } else { "refused" }
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no attribute was exercised");
    }

    /// A tag is not on the element — it is a table beside the model,
    /// keyed by id, and the node and link tables are separate. So the
    /// write has to find which of the two the id is in before it can
    /// store anything, and a read that looked in one only would show
    /// every link untagged.
    #[test]
    fn a_tag_is_read_and_written_for_a_node_and_for_a_link() {
        let mut net = sample_network();
        for id in ["J1", "P1"] {
            assert_eq!(
                row(&net, id, "tag").text.as_deref(),
                Some(""),
                "{id} offers no tag to type into"
            );
            set_attribute(&mut net, None, id, "tag", &serde_json::json!("Zone A")).expect("tag");
            assert_eq!(row(&net, id, "tag").text.as_deref(), Some("Zone A"));
        }
        assert_eq!(net.node_tags.get("J1").map(String::as_str), Some("Zone A"));
        assert_eq!(net.link_tags.get("P1").map(String::as_str), Some("Zone A"));

        // Emptied is removed, not stored blank: the writer emits a line
        // per entry, and a blank one is a line the file did not have.
        set_attribute(&mut net, None, "J1", "tag", &serde_json::json!("")).expect("clear");
        assert!(!net.node_tags.contains_key("J1"));
        assert_eq!(row(&net, "J1", "tag").text.as_deref(), Some(""));

        assert!(set_attribute(&mut net, None, "NOPE", "tag", &serde_json::json!("x")).is_err());
    }

    /// A line's ends travel beside the columns, never as one of them —
    /// the frontend reads `ends` and `positions` from the same two
    /// fields whichever engine answered, so a kind that publishes the
    /// wrong one of the pair shows a table with no way to reconnect.
    #[test]
    fn a_line_reports_its_ends_and_a_point_reports_a_position() {
        let net = sample_network();

        let pipes = kind_elements(&net, "pipe");
        assert_eq!(pipes.ends.len(), pipes.ids.len());
        assert!(pipes.positions.is_empty(), "a line is not at a place");
        let p = pipes.ids.iter().position(|id| id == "P1").expect("P1");
        let link = net.links.iter().find(|l| l.base.id == "P1").expect("P1");
        let name = |index: usize| {
            net.nodes
                .iter()
                .find(|n| n.base.index == index)
                .map(|n| n.base.id.clone())
                .expect("an end names a node")
        };
        assert_eq!(
            pipes.ends[p],
            [name(link.base.from_node), name(link.base.to_node)]
        );

        let junctions = kind_elements(&net, "junction");
        assert!(junctions.ends.is_empty(), "a point runs between nothing");
        assert_eq!(junctions.positions.len(), junctions.ids.len());
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
            let rows = element_attributes(&net, None, id).unwrap_or_else(|| panic!("{id}"));
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
        assert!(element_attributes(&sample_network(), None, "NOPE").is_none());
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
        element_attributes(net, None, id)?
            .into_iter()
            .find(|r| r.key == key)?
            .number
    }

    fn set(net: &mut hydra::Network, id: &str, key: &str, v: serde_json::Value) {
        set_attribute(net, None, id, key, &v).unwrap_or_else(|e| panic!("{id}.{key}: {e}"));
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
            let rows = element_attributes(&model(), None, id).expect("attributes");
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
                set_attribute(&mut net, None, id, &r.key, &serde_json::json!(value))
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
            element_attributes(&net, None, "J1")
                .expect("rows")
                .into_iter()
                .find(|r| r.key == "demandPattern")
                .and_then(|r| r.text),
            Some("P7".to_string())
        );
        set(&mut net, "J1", "demandPattern", serde_json::json!(""));
        // Cleared, so the row is gone rather than blank — an element
        // with no value for a key produces none (§4.5.1).
        assert!(element_attributes(&net, None, "J1")
            .expect("rows")
            .iter()
            .all(|r| r.key != "demandPattern"));
    }

    #[test]
    fn a_key_this_kind_does_not_carry_is_refused() {
        let mut net = model();
        let err = set_attribute(&mut net, None, "J1", "diameter", &serde_json::json!(100.0))
            .expect_err("a junction has no diameter");
        assert!(err.contains("diameter"), "unhelpful: {err}");
    }

    #[test]
    fn a_value_of_the_wrong_shape_is_refused() {
        let mut net = model();
        assert!(set_attribute(&mut net, None, "P1", "length", &serde_json::json!("wide")).is_err());
        assert!(set_attribute(&mut net, None, "P1", "checkValve", &serde_json::json!(3)).is_err());
    }
}

#[cfg(test)]
mod table_tests {
    use super::*;
    use crate::commands::test_fixtures::TEST_INP;

    fn model() -> hydra::Network {
        hydra::io::parse(TEST_INP.as_bytes()).expect("the fixture parses")
    }

    /// A model holding one of every collection kind this engine has.
    const CONTAINERS_INP: &str = "\
[JUNCTIONS]
J1  10  5
[RESERVOIRS]
R1  100
[TANKS]
T1  50  10  5  20  40  0
[PIPES]
P1  R1  J1  1000  12  100  0  Open
P2  J1  T1  800   10  100  0  Open
[PATTERNS]
PAT1  1.0  1.2  0.8
[CURVES]
CV1  0   100
CV1  50  80
[CONTROLS]
 LINK P1 CLOSED AT TIME 5
[RULES]
 RULE R1
 IF TANK T1 LEVEL ABOVE 30
 THEN LINK P1 STATUS IS CLOSED
[COORDINATES]
J1  1.0  2.0
R1  0.0  0.0
T1  2.0  2.0
[OPTIONS]
Units  GPM
[TIMES]
Duration  0
[END]
";

    /// A model where a node and a link share an id, which EPANET allows
    /// and numeric ids make common.
    const COLLIDING_INP: &str = "\
[JUNCTIONS]
10  10  5
[RESERVOIRS]
R1  100
[PIPES]
10  R1  10  1000  12  100  0  Open
[COORDINATES]
10  1.0  2.0
R1  0.0  0.0
[OPTIONS]
Units  GPM
[TIMES]
Duration  0
[END]
";

    /// An id names an element only together with its kind.
    ///
    /// This engine keeps nodes and links in separate namespaces — the
    /// rename path has said so in as many words for as long as it has
    /// existed — and the attribute path looked up by id alone, taking
    /// whichever it found first. So a pipe `10` beside a junction `10`
    /// showed the junction's rows under the pipe's heading. The write
    /// was worse: `diameter` refused with "cannot be edited here", which
    /// is confusing but safe, while `tag` — the one key both kinds have
    /// — wrote the pipe's tag onto the junction and reported success.
    #[test]
    fn an_id_names_an_element_only_with_its_kind() {
        let mut net = hydra::io::parse(COLLIDING_INP.as_bytes()).expect("a legal EPANET model");
        assert_eq!(net.links.len(), 1, "the fixture really does collide");

        let key = |kind| {
            element_attributes(&net, Some(kind), "10")
                .expect("rows")
                .iter()
                .map(|r| r.key.clone())
                .collect::<Vec<_>>()
        };
        assert!(key("junction").contains(&"elevation".to_string()));
        assert!(!key("junction").contains(&"diameter".to_string()));
        assert!(key("pipe").contains(&"diameter".to_string()));
        assert!(!key("pipe").contains(&"elevation".to_string()));

        // Said nothing, and the id names two things: neither, rather than
        // whichever the walk reached first.
        assert!(element_attributes(&net, None, "10").is_none());

        // The tag goes where the caller said, and nowhere else.
        set_attribute(
            &mut net,
            Some("pipe"),
            "10",
            "tag",
            &serde_json::json!("Main"),
        )
        .expect("the pipe takes its own tag");
        assert_eq!(net.link_tags.get("10").map(String::as_str), Some("Main"));
        assert_eq!(net.node_tags.get("10"), None);

        // And with nothing said, it is refused rather than written to one
        // of the two.
        let err = set_attribute(&mut net, None, "10", "tag", &serde_json::json!("Zone"))
            .expect_err("ambiguous");
        assert!(err.contains("both a node and a link"), "{err}");
        assert_eq!(net.link_tags.get("10").map(String::as_str), Some("Main"));
    }

    /// An id that names one thing still resolves without a kind, because
    /// most ids do and most callers have only an id.
    #[test]
    fn an_unambiguous_id_needs_no_kind() {
        let net = hydra::io::parse(COLLIDING_INP.as_bytes()).expect("fixture");
        assert!(element_attributes(&net, None, "R1").is_some());
        // A kind that disagrees with the element names nothing, rather
        // than being ignored down to the same guess.
        assert!(element_attributes(&net, Some("pipe"), "R1").is_none());
        assert!(element_attributes(&net, Some("curve"), "R1").is_none());
    }

    /// The collection kinds are listed, not only the spatial ones.
    ///
    /// They were not. The walk that built the rows visited the nodes and
    /// the links and stopped, so a model with fourteen curves showed a
    /// Curves tab holding none — and the same for its patterns, its
    /// controls and its rules. Everything below that point already
    /// worked: a curve's points are served and editable, it can be
    /// renamed, it can be deleted. None of it was reachable, because a
    /// table with no rows offers nothing to select.
    #[test]
    fn kind_elements_lists_the_collection_kinds() {
        let net = hydra::io::parse(CONTAINERS_INP.as_bytes()).expect("the fixture parses");
        assert_eq!(kind_elements(&net, "curve").ids, vec!["CV1"]);
        // Two, and the second is the implicit default pattern the reader
        // adds when the options name one the file never defined. It is a
        // pattern the solver reads and the writer emits, so a table that
        // hid it would be showing something other than the model.
        assert_eq!(kind_elements(&net, "pattern").ids, vec!["PAT1", "1"]);
        // A control and a rule have no name in this model: the reader
        // keeps neither, because the file's own names are decoration the
        // engine does not resolve anything through. So they are numbered
        // from one, which is how the writer names them on the way out.
        assert_eq!(kind_elements(&net, "control").ids, vec!["1"]);
        assert_eq!(kind_elements(&net, "rule").ids, vec!["1"]);
    }

    /// A collection row says what the thing is and how big.
    ///
    /// It said nothing at all: these four kinds published an empty
    /// schema, so once they were listed the tabs showed a column of ids.
    /// Fourteen curve ids tell a reader nothing about any of them, and
    /// opening each in turn was how they were meant to find the storage
    /// curve.
    ///
    /// Every one is read-only, because every one is derived — a point
    /// count, a clause count, the purpose a curve acquired by being
    /// referenced. Offering an input for a derived number invites
    /// editing the shadow instead of the thing.
    #[test]
    fn a_collection_row_says_what_it_is_and_how_big() {
        let net = hydra::io::parse(CONTAINERS_INP.as_bytes()).expect("the fixture parses");
        let cell = |kind: &str, key: &str, row: usize| {
            let table = kind_elements(&net, kind);
            let column = table
                .columns
                .iter()
                .find(|c| c.key == key)
                .unwrap_or_else(|| panic!("{kind} publishes no {key}"));
            assert!(!column.editable, "{kind}.{key} is derived, not stored");
            column.values[row].clone()
        };

        // Nothing references CV1, so it has points and no declared
        // meaning — which is a state a model may hold, not a gap.
        assert_eq!(
            cell("curve", "curveType", 0),
            serde_json::json!("Unassigned")
        );
        assert_eq!(cell("curve", "points", 0), serde_json::json!(2.0));
        assert_eq!(cell("pattern", "factors", 0), serde_json::json!(3.0));

        // The link it acts on, by name: the model holds a 1-based index,
        // and an index is not what a reader is scanning for.
        assert_eq!(cell("control", "link", 0), serde_json::json!("P1"));
        assert_eq!(cell("control", "enabled", 0), serde_json::json!("Yes"));

        // One premise and one action — every line the panel below draws.
        assert_eq!(cell("rule", "clauses", 0), serde_json::json!(2.0));
        assert_eq!(cell("rule", "priority", 0), serde_json::json!(0.0));
    }

    /// A control and a rule are shown as what they say.
    ///
    /// They were served as an empty panel, so the Controls tab could
    /// only ever tell you how many there were. Deleting one from that
    /// screen would have been deleting a statement you could not read.
    #[test]
    fn a_control_and_a_rule_are_served_as_their_statements() {
        let net = hydra::io::parse(CONTAINERS_INP.as_bytes()).expect("the fixture parses");

        let control = collection_detail(&net, "control", "1");
        assert_eq!(control.lines, vec!["LINK P1 CLOSED AT TIME 5:00"]);
        // Read, never rewritten: taking a statement back means parsing it
        // with the engine's own reader, which this path does not reach.
        assert!(!control.editable);

        let rule = collection_detail(&net, "rule", "1");
        assert_eq!(
            rule.lines,
            vec![
                // The writer's own operator spelling, not the reader's:
                // `ABOVE` goes in and `>` comes out, and both are the
                // same premise. What is shown is what is saved, which
                // `the_statement_shown_is_the_statement_written` holds.
                "IF NODE T1 LEVEL > 30.0000",
                "THEN LINK P1 STATUS IS CLOSED",
            ]
        );
        assert!(!rule.editable);

        // A position no statement sits at is nothing, rather than the
        // last one — `at` bounds-checks instead of clamping.
        assert!(collection_detail(&net, "control", "2").lines.is_empty());
        assert!(collection_detail(&net, "control", "0").lines.is_empty());
    }

    /// The text a control is shown as is the text it is saved as.
    ///
    /// Asserted against the file the writer produces rather than against
    /// a literal, because the two being the same function is the whole
    /// point: a statement formatted in the GUI would be a second
    /// implementation of the valve-setting and tank-grade conversions,
    /// and a control displayed in one set of units and written in
    /// another is the kind of wrong that reads as right.
    #[test]
    fn the_statement_shown_is_the_statement_written() {
        let net = hydra::io::parse(CONTAINERS_INP.as_bytes()).expect("the fixture parses");
        let inp = String::from_utf8(hydra::io::write_inp(&net)).expect("utf-8");
        for line in collection_detail(&net, "control", "1").lines {
            assert!(inp.contains(&line), "the file does not contain {line:?}");
        }
        for line in collection_detail(&net, "rule", "1").lines {
            assert!(inp.contains(&line), "the file does not contain {line:?}");
        }
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
                let rows = element_attributes(&net, None, id).expect("attributes");
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

    /// The rail's number and the table's rows are two answers to one
    /// question, computed by two different functions since counting
    /// stopped building every element to measure it. Two answers is the
    /// arrangement that drifts.
    #[test]
    fn the_counts_agree_with_the_tables_they_count() {
        let net = model();
        let counts = kind_counts(&net);
        for kind in hydra::descriptors::ELEMENT_KINDS {
            assert_eq!(
                counts.get(kind.id).copied(),
                Some(kind_elements(&net, kind.id).ids.len()),
                "{} disagrees",
                kind.id
            );
        }
        // The fixture has some of each spatial kind, so the check is not
        // three zeroes agreeing with three zeroes.
        assert_eq!(counts.get("junction").copied(), Some(1));
        assert_eq!(counts.get("pipe").copied(), Some(2));
    }
}
