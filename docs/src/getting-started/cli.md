# CLI

```sh
# Run a simulation — writes report to stdout, no binary output
cargo run --bin hydra -- network.inp

# With explicit output paths (EPANET-style positional convention)
cargo run --bin hydra -- network.inp report.rpt output.out

# Or using named flags
cargo run --bin hydra -- --input network.inp --report report.rpt --output output.out

# JSON report
cargo run --bin hydra -- network.inp report.json

# Install the binary locally
cargo install --path crates/cli
hydra network.inp
```
