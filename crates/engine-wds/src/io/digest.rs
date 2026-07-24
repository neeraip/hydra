//! Network topology digest (model spec §4.5.7).
//!
//! Computes a deterministic, order-sensitive 64-bit fingerprint of a network's
//! identity and connectivity (node IDs, link IDs, link endpoints). Persisted
//! results files store this digest so consumers can detect that results are
//! stale after the network topology has been edited. Property edits (demands,
//! diameters, options) intentionally do not change the digest.

use crate::Network;

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Field separator inside a link record (US — unit separator).
const FIELD_SEP: u8 = 0x1f;
/// Record terminator after each node ID and each link record (LF).
const RECORD_SEP: u8 = 0x0a;
/// Separator between the node block and the link block (NUL).
const BLOCK_SEP: u8 = 0x00;

/// Compute the FNV-1a 64-bit network topology digest (model spec §4.5.7).
///
/// The hashed byte stream is, in network order:
///
/// 1. for each node: its ID's UTF-8 bytes, then `0x0A`;
/// 2. a single `0x00` byte;
/// 3. for each link: its ID, `0x1F`, the from-node ID, `0x1F`, the to-node
///    ID, then `0x0A`.
///
/// The digest is deterministic and order-sensitive: reordering nodes or
/// links, renaming any element, or rewiring a link's endpoints all produce a
/// different value.
pub fn compute_network_digest(network: &Network) -> u64 {
    // 1-based node index → ID lookup (same convention as the INP writer).
    let node_id: Vec<&str> = {
        let mut v = vec![""; network.nodes.len() + 1]; // index 0 unused
        for n in &network.nodes {
            if n.base.index < v.len() {
                v[n.base.index] = &n.base.id;
            }
        }
        v
    };

    let mut hash = FNV_OFFSET_BASIS;
    for node in &network.nodes {
        hash = fnv1a(hash, node.base.id.as_bytes());
        hash = fnv1a(hash, &[RECORD_SEP]);
    }
    hash = fnv1a(hash, &[BLOCK_SEP]);
    for link in &network.links {
        let from = node_id.get(link.base.from_node).copied().unwrap_or("");
        let to = node_id.get(link.base.to_node).copied().unwrap_or("");
        hash = fnv1a(hash, link.base.id.as_bytes());
        hash = fnv1a(hash, &[FIELD_SEP]);
        hash = fnv1a(hash, from.as_bytes());
        hash = fnv1a(hash, &[FIELD_SEP]);
        hash = fnv1a(hash, to.as_bytes());
        hash = fnv1a(hash, &[RECORD_SEP]);
    }
    hash
}

/// Fold `bytes` into a running FNV-1a 64-bit hash.
fn fnv1a(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::parse;

    /// A two-node, one-pipe network: R1 → J1 via P1.
    ///
    /// Node order in the parsed network is section order: J1 (junction) then
    /// R1 (reservoir).
    fn tiny_network() -> Network {
        let inp = "[JUNCTIONS]\nJ1  0  0\n\n\
                   [RESERVOIRS]\nR1  100\n\n\
                   [PIPES]\nP1  R1  J1  500  12  100  0  Open\n\n\
                   [OPTIONS]\nUnits  GPM\nHeadloss  H-W\n\n[END]\n";
        parse(inp.as_bytes()).expect("parse tiny network")
    }

    /// Known vector (spec §4.5.7): FNV-1a 64 of
    /// `b"J1\nR1\n\x00P1\x1fR1\x1fJ1\n"` = 0x451f672d2d21a3c4.
    #[test]
    fn digest_matches_known_vector() {
        let net = tiny_network();
        assert_eq!(compute_network_digest(&net), 0x451f_672d_2d21_a3c4);
    }

    /// The digest hashes exactly the byte stream defined by the spec.
    #[test]
    fn digest_equals_fnv_of_spec_byte_stream() {
        let net = tiny_network();
        let stream: &[u8] = b"J1\x0aR1\x0a\x00P1\x1fR1\x1fJ1\x0a";
        assert_eq!(
            compute_network_digest(&net),
            fnv1a(FNV_OFFSET_BASIS, stream)
        );
    }

    #[test]
    fn digest_is_deterministic() {
        let a = compute_network_digest(&tiny_network());
        let b = compute_network_digest(&tiny_network());
        assert_eq!(a, b);
    }

    /// Node order is significant: swapping the two nodes changes the digest.
    #[test]
    fn digest_is_sensitive_to_node_order() {
        let baseline = compute_network_digest(&tiny_network());
        let mut swapped = tiny_network();
        swapped.nodes.swap(0, 1);
        assert_ne!(baseline, compute_network_digest(&swapped));
        // Expected value for the stream "R1\nJ1\n\x00P1\x1fR1\x1fJ1\n".
        assert_eq!(compute_network_digest(&swapped), 0x03ff_3fc1_5f20_a264);
    }

    /// Link direction is significant: reversing from/to changes the digest.
    #[test]
    fn digest_is_sensitive_to_link_endpoints() {
        let baseline = compute_network_digest(&tiny_network());
        let mut reversed = tiny_network();
        let (from, to) = (
            reversed.links[0].base.from_node,
            reversed.links[0].base.to_node,
        );
        reversed.links[0].base.from_node = to;
        reversed.links[0].base.to_node = from;
        assert_ne!(baseline, compute_network_digest(&reversed));
        // Expected value for the stream "J1\nR1\n\x00P1\x1fJ1\x1fR1\n".
        assert_eq!(compute_network_digest(&reversed), 0x5cf9_98ba_eb9e_e334);
    }

    /// Renaming an element changes the digest.
    #[test]
    fn digest_is_sensitive_to_ids() {
        let baseline = compute_network_digest(&tiny_network());
        let mut renamed = tiny_network();
        renamed.links[0].base.id = "P2".to_string();
        assert_ne!(baseline, compute_network_digest(&renamed));
    }

    /// Non-topological property edits leave the digest unchanged.
    #[test]
    fn digest_ignores_property_edits() {
        let baseline = compute_network_digest(&tiny_network());
        let mut edited = tiny_network();
        edited.options.duration = 86_400.0;
        edited.nodes[0].base.elevation += 5.0;
        assert_eq!(baseline, compute_network_digest(&edited));
    }
}
