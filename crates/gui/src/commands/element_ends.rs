//! Reconnecting a line, for whichever engine holds the model.
//!
//! The §4.5.2.1 operation. An end is not an attribute — it is implied by
//! the `polyline` class, the same way a position is implied by `point` —
//! so it does not travel through the attribute write and does not appear
//! in any schema. Both engines store an end as an index into their own
//! element array, and neither index means anything above this line.
//!
//! **Both ends go at once, even to change one.** The caller has both:
//! the table shows them side by side and the inspector reads them
//! together. Taking both makes the operation idempotent, makes its
//! inverse the pair that was there before — which is what undo needs —
//! and puts the "must differ" check on the values actually being stored
//! rather than on one new value and one remembered one.

use super::network_dto::NetworkState;
use super::projects::{app_data_dir, project_engine_key, validate_target_ids};

/// Point the element `id` at `from_id` and `to_id`.
///
/// Refuses, changing nothing, when either name is not in the model or
/// the two are the same element — the two rules §4.5.2.1 puts on this
/// layer rather than on an engine, because they follow from what a line
/// is rather than from any file format.
#[tauri::command(async)]
pub fn set_element_ends(
    app: tauri::AppHandle,
    state: tauri::State<'_, NetworkState>,
    project_id: String,
    element_id: String,
    from_id: String,
    to_id: String,
) -> Result<(), String> {
    validate_target_ids(&project_id, None)?;
    let app_data = app_data_dir(&app)?;
    match project_engine_key(&app_data, &project_id).as_str() {
        "uds" => super::mutations::mutate_uds(&app, &state, |network| {
            set_uds_ends(network, &element_id, &from_id, &to_id)
        }),
        "wds" => super::mutations::mutate_wds(&app, &state, |network| {
            set_wds_ends(network, &element_id, &from_id, &to_id)
        }),
        other => Err(format!("no editing surface for engine '{other}'")),
    }
}

/// The refusal both engines give for an end naming nothing, so the two
/// cannot come to word it differently.
fn no_such_end(name: &str) -> String {
    format!("'{name}' is not a node in this model")
}

/// The refusal for a line from a thing to itself.
const SAME_END: &str = "a link needs two different ends";

pub(crate) fn set_uds_ends(
    net: &mut hydra::uds::model::Network,
    id: &str,
    from_id: &str,
    to_id: &str,
) -> Result<(), String> {
    let find = |name: &str| {
        net.vertices
            .iter()
            .position(|v| v.id.eq_ignore_ascii_case(name))
            .ok_or_else(|| no_such_end(name))
    };
    let from = find(from_id)?;
    let to = find(to_id)?;
    if from == to {
        return Err(SAME_END.into());
    }
    let link = net
        .links
        .iter_mut()
        .find(|l| l.id.eq_ignore_ascii_case(id))
        .ok_or_else(|| format!("'{id}' is not a link in this model"))?;
    link.from = from;
    link.to = to;
    Ok(())
}

pub(crate) fn set_wds_ends(
    network: &mut hydra::Network,
    id: &str,
    from_id: &str,
    to_id: &str,
) -> Result<(), String> {
    // By id, then by the model's own 1-based index — the arrays are not
    // addressed by position here, and reading one as the other would
    // reconnect the line to its neighbour and still look plausible.
    let find = |name: &str| {
        network
            .nodes
            .iter()
            .find(|n| n.base.id == name)
            .map(|n| n.base.index)
            .ok_or_else(|| no_such_end(name))
    };
    let from = find(from_id)?;
    let to = find(to_id)?;
    if from == to {
        return Err(SAME_END.into());
    }
    let link = network
        .links
        .iter_mut()
        .find(|l| l.base.id == id)
        .ok_or_else(|| format!("'{id}' is not a link in this model"))?;
    link.base.from_node = from;
    link.base.to_node = to;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const UDS: &str = "\
[OPTIONS]
FLOW_UNITS CFS
[JUNCTIONS]
J1 100 4 0 0 0
J2 98 4 0 0 0
[OUTFALLS]
O1 90 FREE NO
[CONDUITS]
C1 J1 O1 400 0.013 0 0 0 0
[XSECTIONS]
C1 CIRCULAR 1.5 0 0 0
[COORDINATES]
J1 0 0
J2 50 0
O1 100 0
";

    fn uds_model() -> hydra::uds::model::Network {
        hydra::uds::io::objects::parse_network(UDS).0
    }

    fn wds_model() -> hydra::Network {
        let inp = "\
[JUNCTIONS]
 J1 100
 J2 98
[RESERVOIRS]
 R1 120
[PIPES]
 P1 J1 R1 100 300 0.1 0 Open
 P2 J2 R1 100 300 0.1 0 Open
[END]
";
        hydra::io::parse(inp.as_bytes()).expect("parse")
    }

    /// The reconnection both engines have to perform identically: the
    /// same call, the same refusals, and a model that still writes.
    #[test]
    fn an_end_moves_to_another_element() {
        let mut net = uds_model();
        set_uds_ends(&mut net, "C1", "J2", "O1").expect("reconnect");
        let link = &net.links[0];
        assert_eq!(net.vertices[link.from].id, "J2");
        assert_eq!(net.vertices[link.to].id, "O1");
        // Through the writer, because an index that moved without the
        // section that names it would produce a file naming the old end.
        let written = hydra::uds::io::inp_writer::write_inp(&net).expect("write");
        // Compared by fields rather than by the spacing between them:
        // the writer pads its columns.
        assert!(
            written
                .lines()
                .any(|l| l.split_whitespace().take(3).eq(["C1", "J2", "O1"])),
            "the written conduit still names its old end:\n{written}"
        );

        let mut network = wds_model();
        set_wds_ends(&mut network, "P1", "J2", "J1").expect("reconnect");
        let link = &network.links[0];
        assert_eq!(link.base.from_node, 2, "J2 is the model's second node");
        assert_eq!(link.base.to_node, 1);
    }

    /// Reversing a line is setting its ends the other way round, and it
    /// has to be allowed — the ends are ordered because that order is
    /// the sign convention for the flow, so reversing one is a thing a
    /// modeller means to do.
    #[test]
    fn the_two_ends_can_be_swapped() {
        let mut net = uds_model();
        set_uds_ends(&mut net, "C1", "O1", "J1").expect("reverse");
        assert_eq!(net.vertices[net.links[0].from].id, "O1");
        assert_eq!(net.vertices[net.links[0].to].id, "J1");
    }

    #[test]
    fn an_end_that_names_nothing_is_refused() {
        let mut net = uds_model();
        assert!(set_uds_ends(&mut net, "C1", "NOPE", "O1").is_err());
        assert!(set_uds_ends(&mut net, "C1", "J1", "NOPE").is_err());
        // And nothing moved: a refusal that half-applied would leave the
        // line attached to one end it was never given.
        assert_eq!(net.vertices[net.links[0].from].id, "J1");
        assert_eq!(net.vertices[net.links[0].to].id, "O1");

        let mut network = wds_model();
        assert!(set_wds_ends(&mut network, "P1", "NOPE", "R1").is_err());
        assert_eq!(network.links[0].base.from_node, 1);
    }

    #[test]
    fn a_line_from_a_thing_to_itself_is_refused() {
        let mut net = uds_model();
        assert!(set_uds_ends(&mut net, "C1", "J1", "J1").is_err());
        let mut network = wds_model();
        assert!(set_wds_ends(&mut network, "P1", "J1", "J1").is_err());
    }

    #[test]
    fn an_unknown_link_is_refused_after_the_ends_check() {
        let mut net = uds_model();
        let err = set_uds_ends(&mut net, "NOPE", "J1", "O1").expect_err("unknown link");
        assert!(err.contains("NOPE"), "{err}");
    }
}
