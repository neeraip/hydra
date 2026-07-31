# hydra-engines

Engine dispatch for the Hydra workspace: given the bytes of a model of
unknown provenance, decides which Hydra engine owns it.

`hydra-common` holds the engine registry but depends on nothing, so it can
describe engines and never invoke them; each engine's recognition lives in
that engine. This crate is the one layer that sees both, and implements the
routing policy of the foundation contract (`hydra-common` spec §2.5.1) once
rather than duplicating it into every interface.

Routing never falls back to a default engine. An ambiguous or unrecognised
model is a terminal error, because handing a model to a solver that models
different physics returns a confident, wrong answer.

This crate is re-exported through [`hydra-sdk`](https://crates.io/crates/hydra-sdk)
as `hydra::engines`; depend on the SDK rather than on this crate directly.
