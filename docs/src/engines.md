# Engines

Hydra is a platform, not a single solver. Each modelling domain gets its own
**engine**, with its own data model, source-model format, and numerical
methods. They all sit behind one shared toolchain: the desktop GUI, the
`hydra` CLI, and the `hydra-sdk` Rust library.

| Engine | Key | Domain | Source model |
|---|---|---|---|
| Water Distribution | `wds` | Pressurised supply networks: hydraulics, water quality, energy | EPANET `.inp` (2.x) |
| Urban Drainage | `uds` | Stormwater and wastewater collection: runoff, routing, 2D overland flow, water quality | SWMM `.inp` |

Both engines run from all three surfaces, and both are edited in the GUI.
The one difference is how a project starts: a water distribution project
can begin from a blank network, while a drainage project begins by
importing a SWMM model (there is no smallest-valid drainage model to start
from). Once imported, a drainage model is edited like any other.

## The engine registry

Every engine is registered in Hydra's engine registry with an immutable
descriptor: a stable key, a display label, a badge, an accent colour, a
one-line summary, an availability status, and the file formats it imports.
The registry is the single source of truth; applications resolve a
project's stored engine key against it rather than hardcoding names or
file filters. `hydra engines` prints it.

A key this build has never heard of (for example a project created by a
newer Hydra) is an error, and it is always surfaced as an explicit
unsupported state, never silently substituted with a default engine.

## Why the split matters

Two engines can claim the same file extension with wholly incompatible
contents: `wds` and `uds` both read `.inp`, one an EPANET model and one a SWMM
model. An extension is therefore a file-picker filter, never a validity test:
only the owning engine's parser can decide whether a file really is one of its
models. Hydra rejects a mismatched model explicitly rather than loading a
stormwater network as a plausible-looking pressurised one. See
[Foreign `.inp` dialects](reference/inp-format.md#foreign-inp-dialects) for what
that rejection looks like in the CLI, the GUI, and the SDK.

The same separation runs through the codebase. `hydra-common` holds only what
every engine shares (engine identity, the reportable-output contract, and the
element, quantity, result-variable and criteria contracts) and carries no
engine vocabulary. Engines emit neutral content fragments;
`hydra-report` renders them without knowing which engine produced them. See
[Crate Layout](architecture/crates.md) for how the workspace is arranged.

## Working with the registry from code

```rust
use hydra_sdk::common::{engine_by_key, ENGINES};

for engine in ENGINES {
    println!("{} ({}) — {:?}", engine.label, engine.key, engine.status);
}

let wds = engine_by_key("wds")?;
assert!(wds.is_available());
```

See the [SDK Overview](sdk/overview.md) for the full engine-identity API.
