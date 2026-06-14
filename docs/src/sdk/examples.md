# SDK Examples

## Parse an INP file and run a full simulation

```rust
use hydra_sdk::{io, Simulation, NodeQuantity, LinkQuantity};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    let mut sim = Simulation::create();
    sim.load(network)?;
    sim.run()?;

    for t in sim.snapshot_times() {
        let head = sim.get_node_result("J1", NodeQuantity::Head, t)?;
        let pressure = sim.get_node_result("J1", NodeQuantity::GaugePressure, t)?;
        let flow = sim.get_link_result("P1", LinkQuantity::Flow, t)?;
        println!("t={t:.0}s  head={head:.3}  pressure={pressure:.3}  flow={flow:.6}");
    }

    for w in sim.warnings() {
        println!("[t={:.0}s] {:?}", w.t, w.kind);
    }

    Ok(())
}
```

## Shorthand constructor

```rust
use hydra_sdk::{io, Simulation};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    // Convenience: shorthand for Simulation::create() + sim.load(network).
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

    loop {
        let dt = sim.step_hydraulics()?;
        if dt == 0.0 { break; }
        // inspect or modify state between steps
    }

    Ok(())
}
```

## Write output files

```rust
use hydra_sdk::{io, Simulation, WritableSimulation};
use std::fs::File;
use std::io::BufWriter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;
    let mut sim = Simulation::from_network(network)?;
    sim.run()?;

    // Plain-text .rpt report
    let rpt = io::rpt_writer::build_text_report(&sim)?;
    std::fs::write("report.rpt", rpt)?;

    // JSON report
    let json = io::rpt_writer::build_json_report(&sim)?;
    std::fs::write("report.json", json)?;

    // EPANET-compatible binary .out file
    let out_file = BufWriter::new(File::create("output.out")?);
    io::out_writer::write_binary_output(out_file, &sim)?;

    Ok(())
}
```

## Demand reliability analysis

Post-simulation analytics operate on a saved `.out` file and the original `Network`.

```rust
use hydra_sdk::{io, compute_demand_reliability_from_out};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("network.inp")?;
    let network = io::parse(&bytes)?;

    let report = compute_demand_reliability_from_out(
        std::path::Path::new("output.out"),
        &network,
    )?;

    println!("Network reliability: {:.1}%",
        report.summary.network_reliability_ratio * 100.0);

    for node in &report.nodes {
        if node.reliability_ratio() < 0.99 {
            println!(
                "  {} — {:.1}% reliable, {} deficit period(s)",
                node.node_id,
                node.reliability_ratio() * 100.0,
                node.deficit_periods,
            );
        }
    }

    Ok(())
}
```

## Pressure compliance analysis

```rust
use hydra_sdk::{compute_service_compliance_from_out, ServiceComplianceThresholds};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check that all nodes stay above 10 m and below 80 m.
    let thresholds = ServiceComplianceThresholds {
        min_pressure: 10.0,
        max_pressure: Some(80.0),
    };

    let report = compute_service_compliance_from_out(
        std::path::Path::new("output.out"),
        thresholds,
    )?;

    println!("Compliant nodes: {}/{}", 
        report.summary.compliant_node_count,
        report.nodes.len());

    for node in &report.nodes {
        if node.below_min_count > 0 {
            println!(
                "  node {} — {} period(s) below {}m (worst deficit: {:.2}m)",
                node.node_index,
                node.below_min_count,
                thresholds.min_pressure,
                node.worst_below_min,
            );
        }
    }

    Ok(())
}
```
