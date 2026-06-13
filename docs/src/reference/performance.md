# Performance

Benchmarked on real-world networks from the [KIOS-Research/EPANET-Benchmarks](https://github.com/KIOS-Research/EPANET-Benchmarks) collection. Hydra is compiled with `lto = "fat"` and `codegen-units = 1`. Times are the minimum of 3 wall-clock runs.

| Network | Nodes | Links | Steps | EPANET | Hydra | Ratio |
|---|---|---|---|---|---|---|
| Balerma | 447 | 454 | 3 | 7 ms | 5 ms | 0.78× |
| KY 8 | 2,432 | 2,823 | 289 | 118 ms | 94 ms | 0.80× |
| KY 9 | 2,650 | 3,042 | 289 | 120 ms | 76 ms | 0.63× |
| KY 10 | 3,211 | 4,528 | 289 | 318 ms | 222 ms | 0.70× |
| Richmond | 872 | 958 | 289 | 26 ms | 31 ms | 1.19× |
| D-Town | 407 | 459 | 1,441 | 137 ms | 120 ms | 0.88× |
| L-TOWN | 785 | 909 | 2,035 | 207 ms | 209 ms | 1.01× |
| BWSN2 | 12,523 | 14,822 | 97 | 598 ms | 731 ms | 1.22× |

Values < 1.0× mean Hydra is faster. Hydra matches or outperforms EPANET on most networks. The remaining gap on control-heavy networks (Richmond, BWSN2) is due to per-iteration overhead in Rust's safe indexing model versus C pointer arithmetic.

For maximum local performance, build with native CPU target features:

```sh
just release-native
```
