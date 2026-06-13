# SDK Examples

## Parse an INP file and run a full simulation

```rust
use hydra_sdk::{io, Simulation, NodeQuantity, LinkQuantity};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Read and parse an EPANET .inp file.
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    // 2. Create a simulation and load the network.
    let mut sim = Simulation::create();
    sim.load(network)?;

    // 3. Run hydraulics + quality to completion.
    sim.run()?;

    // 4. Query results at each reporting time step.
    for t in sim.snapshot_times() {
        let head = sim.get_node_result("J1", NodeQuantity::Head, t)?;
        let pressure = sim.get_node_result("J1", NodeQuantity::GaugePressure, t)?;
        let flow = sim.get_link_result("P1", LinkQuantity::Flow, t)?;
        println!("t={t:.0}s  head={head:.3}  pressure={pressure:.3}  flow={flow:.6}");
    }

    // 5. Print warnings (if any).
    for w in sim.warnings() {
        println!("[t={:.0}s] {:?}", w.t, w.kind);
    }

    Ok(())
}
```

## Create a simulation from a parsed network directly

```rust
use hydra_sdk::{io, Simulation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    // Convenience constructor: shorthand for create() + load().
    let mut sim = Simulation::from_network(network)?;
    sim.run()?;

    Ok(())
}
```

## Step through hydraulics manually

```rust
use hydra_sdk::{io, Simulation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    let mut sim = Simulation::create();
    sim.load(network)?;

    // Step one hydraulic period at a time.
    loop {
        let dt = sim.step_hydraulics()?;
        if dt == 0.0 { break; }
        // ... inspect or modify state between steps ...
    }

    Ok(())
}
```
