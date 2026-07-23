//! Shared fixtures for the command submodules' unit tests.

use super::network_dto::{network_to_dto, NetworkStateInner};

/// Minimal parseable network: 1 junction, 1 reservoir, 1 tank, 2 pipes.
pub(crate) const TEST_INP: &str = "\
[JUNCTIONS]
J1  10  5

[RESERVOIRS]
R1  100

[TANKS]
T1  50  10  5  20  40  0

[PIPES]
P1  R1  J1  1000  12  100  0  Open
P2  J1  T1  800   10  100  0  Open

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

pub(crate) fn loaded_state() -> NetworkStateInner {
    let raw = TEST_INP.as_bytes().to_vec();
    let network = hydra::io::parse(&raw).expect("test INP must parse");
    let dto = network_to_dto(&network);
    NetworkStateInner::Loaded {
        raw_bytes: raw,
        dirty: false,
        network,
        dto,
        owner_project_id: Some("test-project".into()),
        owner_scenario_id: None,
    }
}

pub(crate) fn loaded_sim() -> hydra::Simulation {
    let network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
    let mut sim = hydra::Simulation::create();
    sim.load(network).unwrap();
    sim
}
