# Engines

Hydra is a platform, not a single solver. Each modelling domain gets its own
**engine** — its own data model, source-model format, and numerical methods —
and they all sit behind one shared toolchain: the desktop GUI, the `hydra` CLI,
and the `hydra-sdk` Rust library.

| Engine | Key | Domain | Source model | Status |
|---|---|---|---|---|
| Water Distribution | `wds` | Pressurised supply networks — hydraulics, water quality, energy | EPANET 2.3 `.inp` | **Available** |
| Urban Drainage | `uds` | Stormwater and wastewater collection — runoff, routing, water quality | SWMM `.inp` | Planned |
| Open Channel | `och` | Rivers and open channels — steady and unsteady flow | HEC-RAS project archive | Planned |

## Available vs. planned

Every engine — shipping or not — is registered in Hydra's engine registry with
an immutable descriptor: a stable key, a display label, a badge, an accent
colour, a one-line summary, and the file formats it imports. The registry is
the single source of truth; applications resolve a project's stored engine key
against it rather than hardcoding names or file filters.

The descriptor also carries a **status**:

- **Available** — implemented in this distribution and usable.
- **Planned** — registered and reserved, with no implementation behind it.
  Applications present it so the full modelling scope is visible, but refuse to
  create projects, import models, or run simulations for it.

"Planned" and "unknown" are deliberately distinct states. A planned key
resolves successfully; only a key this build has never heard of — for example a
project created by a newer Hydra — is an error, and it is always surfaced as an
explicit unsupported state, never silently substituted with a default engine.

The crates for the planned engines (`hydra-engine-uds`, `hydra-engine-och`) are
already published as empty scaffolds, so their names and version history track
the workspace from the start rather than being introduced mid-life.

## Why the split matters

Two engines can claim the same file extension with wholly incompatible
contents — `wds` and `uds` both read `.inp`, one an EPANET model and one a SWMM
model. An extension is therefore a file-picker filter, never a validity test:
only the owning engine's parser can decide whether a file really is one of its
models. Hydra rejects a mismatched model explicitly rather than loading a
stormwater network as a plausible-looking pressurised one.

The same separation runs through the codebase. `hydra-common` holds only what
every engine shares — engine identity and the reportable-output contract — and
carries no engine vocabulary. Engines emit neutral content fragments;
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
