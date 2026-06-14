/// Errors returned by analysis computation functions.
#[derive(Debug)]
pub enum AnalysisComputeError {
    /// The simulation (or `.out` file) contains no reporting periods.
    NoSnapshots,
    /// Reading results from an in-memory [`crate::simulation::Simulation`] failed.
    Session(crate::simulation::SessionError),
    /// Reading or parsing the `.out` binary file failed.
    OutRead(String),
    /// The supplied input parameters are inconsistent or out of range.
    InvalidInput(String),
}

impl std::fmt::Display for AnalysisComputeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSnapshots => write!(f, "simulation has no snapshots"),
            Self::Session(e) => write!(f, "failed to read simulation results: {e}"),
            Self::OutRead(e) => write!(f, "failed to read output file: {e}"),
            Self::InvalidInput(e) => write!(f, "invalid analysis input: {e}"),
        }
    }
}

impl std::error::Error for AnalysisComputeError {}

impl From<crate::simulation::SessionError> for AnalysisComputeError {
    fn from(value: crate::simulation::SessionError) -> Self {
        Self::Session(value)
    }
}
