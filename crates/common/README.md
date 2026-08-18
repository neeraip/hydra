# hydra-common

Foundation contracts for the Hydra workspace: engine identity (descriptor +
registry), the reportable-output contract by which engines describe and
produce report content, and, since a second engine exists to validate them,
the element-taxonomy, quantity, result-variable and criteria contracts
(spec §4 to §7). Depends on nothing else in the workspace; every engine and
application may depend on it.

Deliberately slim: engine meaning travels only through opaque ids and
engine-authored text, and a cross-engine simulation session contract remains
an explicit non-goal. See `src/spec.md` for the authoritative contract
specification.

This crate is re-exported through [`hydra-sdk`](https://crates.io/crates/hydra-sdk)
as `hydra::common`; depend on the SDK rather than on this crate directly.
