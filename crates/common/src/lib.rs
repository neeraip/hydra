#![doc = include_str!("spec.md")]

/// The crate version, taken from `Cargo.toml` at compile time.
pub const HYDRA_COMMON_VERSION: &str = env!("CARGO_PKG_VERSION");

mod elements;
mod identity;
mod quantity;
mod report;
mod variables;

pub use elements::{AttributeDescriptor, ElementClass, ElementKind};
pub use identity::{
    engine_by_key, EngineDescriptor, EngineStatus, ImportFormat, Recognition, UnknownEngineError,
    ENGINES,
};
pub use quantity::QuantityDescriptor;
pub use report::{
    BlockDescriptor, BlockError, Chart, ChartData, ChoiceItem, Column, Fragment, FragmentItem,
    KeyValue, LineSeries, OptionDescriptor, OptionKind, Table, Value, ValueKind,
};
pub use variables::{CategoryItem, RampHint, VariableDescriptor};
