# hydra-common

Foundation contracts for the Hydra workspace: engine identity (descriptor +
registry) and the reportable-output contract by which engines describe and
produce report content. Depends on nothing else in the workspace; every
engine and application may depend on it.

Deliberately slim: shared element schemas, units, and simulation contracts
are explicit non-goals until a second engine implementation exists. See
`src/spec.md` for the authoritative contract specification.

This crate is re-exported through [`hydra-sdk`](https://crates.io/crates/hydra-sdk)
as `hydra::common`; depend on the SDK rather than on this crate directly.
