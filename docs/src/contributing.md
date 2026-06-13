# Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](https://github.com/neeraip/hydra/blob/main/.github/CONTRIBUTING.md) before opening a pull request, in particular the **Spec First** workflow, which requires spec changes to land before implementation changes for any solver, model, or analytics work.

Hydra uses conventional commit messages, expects one logical change per pull request, and requires all CI checks to pass before merge.

## Testing Strategy

Hydra's integration strategy is Hydra-native and fixture-driven:

- Purpose-built fixture networks validate control, timestep, quality, reaction, and tank behaviour through physics/behaviour invariants.
- Large real-world networks are exercised with deterministic regression checks by comparing repeated Hydra runs section-by-section.

Correctness is established by physics/behaviour invariants and deterministic Hydra-vs-Hydra regression checks, not by agreement with any external tool's output.
