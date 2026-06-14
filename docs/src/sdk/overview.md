# SDK Overview

`hydra-sdk` is the umbrella crate for Hydra's public API. Add it to your `Cargo.toml` to get a curated, user-facing surface with Hydra's dependency versions already pinned.

```toml
[dependencies]
hydra-sdk = "0.1.1"
```

It re-exports the primary types and modules needed to parse EPANET inputs, construct simulations, run hydraulics and quality, and query results, including `Simulation`, `io`, `NodeQuantity`, and `LinkQuantity`.
