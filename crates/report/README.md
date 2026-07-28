# hydra-report

Report generation for the Hydra workspace: JSON report templates,
document assembly from engine-neutral content fragments, and
deterministic txt / csv / html renderers. Knows nothing about engines or
the results they produce — applications obtain fragments from an engine
and hand them to this layer. See `src/spec.md` for the authoritative
specification.

This crate is re-exported through [`hydra-sdk`](https://crates.io/crates/hydra-sdk)
as `hydra::report`; depend on the SDK rather than on this crate directly.
