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
        network: std::sync::Arc::new(network),
        dto,
        owner_project_id: Some("test-project".into()),
        owner_scenario_id: None,
    }
}

/// A drainage model carrying **one element of every kind in the
/// catalog** — the fixture the editing-contract sweep tests share.
///
/// Every catalog-driven test that walks `ELEMENT_KINDS` used to skip a
/// kind its fixture lacked, silently; that silence is how a time series
/// could be served as editable for months while its write refused. The
/// sweeps now assert that no kind is absent, which makes this fixture
/// load-bearing: adding a kind to the engine without adding one here
/// fails the suite instead of thinning it.
pub(crate) const UDS_FULL_INP: &str = "\
[OPTIONS]
FLOW_UNITS    CMS

[JUNCTIONS]
J1  10  3  0.5  0  0

[OUTFALLS]
O1  8  FREE  NO

[DIVIDERS]
D1  9.5  C2  CUTOFF  0.3

[STORAGE]
SU1  5  4  0  FUNCTIONAL  1000  0  0

[CONDUITS]
C1  J1  O1  100  0.013  0  0  0  0
C2  D1  O1  80  0.013  0  0  0  0
C3  J1  D1  80  0.013  0  0  0  0

[PUMPS]
PU1  SU1  J1  PC1  ON  0  0

[ORIFICES]
OR1  J1  SU1  SIDE  0  0.6  NO  0

[WEIRS]
W1  J1  SU1  TRANSVERSE  0.5  3.33  NO  0  0

[OUTLETS]
OL1  J1  SU1  0  FUNCTIONAL/DEPTH  0.5  1.5  NO

[XSECTIONS]
C1  CIRCULAR  1  0  0  0  1
C2  CIRCULAR  1  0  0  0  1
C3  CIRCULAR  1  0  0  0  1
OR1  CIRCULAR  0.5  0  0  0
W1  RECT_OPEN  0.5  1  0  0

[SUBCATCHMENTS]
S1  G1  J1  4.5  35  400  1.2  0

[SUBAREAS]
S1  0.015  0.24  0.06  0.2  20  OUTLET

[INFILTRATION]
S1  3.5  0.6  4.14  6

[RAINGAGES]
G1  INTENSITY  0:15  1.0  TIMESERIES  RS1
G2  INTENSITY  0:15  1.0  TIMESERIES  RS1

[AQUIFERS]
AQ1  0.5  0.15  0.30  0.5  10  15  0.35  14  0.002  0  10  0.30

[TRANSECTS]
NC  0.02  0.02  0.016
X1  TR1  3  0  0  0  0  0  0  0
GR  10  0  0  5  10  10

[STREETS]
STRT1  20  0.5  2  0.016  0.1  2  1  10  4  0.02

[INLETS]
CB1  GRATE  2  2  P_BAR-50

[LID_CONTROLS]
GR1  BC

[CURVES]
PC1  PUMP4  0  0.1
PC1  1  0.05

[PATTERNS]
P1  HOURLY  1.0  1.0  1.0  1.0  1.0  1.0

[HYDROGRAPHS]
UH1  G1
UH1  ALL  SHORT  0.033  1.0  2.0

[POLLUTANTS]
TSS  MG/L  0  0  0  0  NO

[LANDUSES]
RES1

[SNOWPACKS]
SN1  PLOWABLE  0.001  0.001  0  0.10  0  0  0.5

[CONTROLS]
RULE R1
IF NODE J1 DEPTH > 2
THEN PUMP PU1 STATUS = ON

[TIMESERIES]
RS1  0:00  0.4
";

pub(crate) fn loaded_sim() -> hydra::Simulation {
    let network = hydra::io::parse(TEST_INP.as_bytes()).unwrap();
    let mut sim = hydra::Simulation::create();
    sim.load(network).unwrap();
    sim
}
