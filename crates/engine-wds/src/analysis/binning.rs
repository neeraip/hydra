// binning — shared threshold-band counting for distribution analytics.

/// Count `values` into the `n + 1` bands defined by ascending `edges`
/// (analysis spec §4.1.2).
///
/// Bands are half-open `[e_i, e_{i+1})`, and **the outer two are unbounded**:
/// everything below `edges[0]` lands in band 0 and everything at or above the
/// last edge lands in the final band. No finite value is ever dropped, so the
/// returned counts sum to `values.len()`.
///
/// This is the shared binning used by the `*-thresholds` report blocks and by
/// any interface presenting the same threshold view, so both count identically.
pub fn threshold_bands(values: &[f64], edges: &[f64]) -> Vec<u64> {
    let mut counts = vec![0u64; edges.len() + 1];
    for &v in values {
        let idx = edges.iter().position(|&e| v < e).unwrap_or(edges.len());
        counts[idx] += 1;
    }
    counts
}
