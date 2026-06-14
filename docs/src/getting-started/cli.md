# CLI

```sh
# Install from crates.io
cargo install hydra-cli

# Run a simulation (report goes to stdout)
hydra network.inp

# With explicit output paths (EPANET-style positional convention)
hydra network.inp report.rpt output.out

# Or using named flags
hydra --input network.inp --report report.rpt --output output.out

# JSON report
hydra network.inp --report report.json

# Suppress progress output
hydra -q network.inp

# Print version
hydra -v
```
