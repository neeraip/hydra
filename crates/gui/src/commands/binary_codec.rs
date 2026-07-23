//! Compact little-endian binary encodings for the network snapshot (layout v3
//! with vertices and pipe initial status) and per-period results, plus the
//! snapshot read command.

use super::network_dto::{NetworkDto, NetworkState, NetworkStateInner};

/// Version stamped into the first header word of the binary network snapshot.
const NETWORK_SNAPSHOT_VERSION: u32 = 3;
/// Flag bit set in the header's `flags` word when the payload carries a
/// snapshot. Clear = "no network for this target" — the binary equivalent of
/// the old `null` return from `load_project_network`.
const NETWORK_SNAPSHOT_FLAG_PRESENT: u32 = 1;

/// Encode the cached DTO's nodes + links into the compact little-endian
/// columnar layout consumed by the frontend's `decodeNetworkSnapshot`
/// (`hooks/network.ts`), mirroring the `encode_period_results` pattern.
///
/// Layout version 3 (adds the per-link `initialStatus` u8 column to
/// version 2, which added `[VERTICES]` link polylines to version 1):
///
/// ```text
/// offset  size          content
/// 0       4             version     (u32 LE, = NETWORK_SNAPSHOT_VERSION = 3)
/// 4       4             flags       (u32 LE; bit 0 = snapshot present)
/// 8       4             n_nodes     (u32 LE)
/// 12      4             n_links     (u32 LE)
/// 16      4             total_verts (u32 LE; Σ vertices over all links)
/// 20      12            reserved    (u32 LE × 3, all 0)
/// 32      8·n_nodes     node x                  (f64 LE)
/// …       8·n_nodes     node y                  (f64 LE)
/// …       8·total_verts vertex x                (f64 LE; link order)
/// …       8·total_verts vertex y                (f64 LE; link order)
/// …       4·n_nodes     node elevation          (f32 LE, m)
/// …       4·n_nodes     node base_demand        (f32 LE, L/s)
/// …       4·n_nodes     node pressure           (f32 LE; NaN = absent)
/// …       4·n_nodes     node demand             (f32 LE; NaN = absent)
/// …       4·n_nodes     node tank_min_level     (f32 LE; NaN = absent)
/// …       4·n_nodes     node tank_max_level     (f32 LE; NaN = absent)
/// …       4·n_nodes     node tank_initial_level (f32 LE; NaN = absent)
/// …       4·n_nodes     node tank_diameter      (f32 LE; NaN = absent)
/// …       4·n_links     link velocity           (f32 LE)
/// …       4·n_links     link diameter           (f32 LE, mm)
/// …       4·n_links     link length             (f32 LE, m)
/// …       4·n_links     link roughness          (f32 LE)
/// …       4·n_links     link pump_power_kw      (f32 LE; NaN = absent)
/// …       4·n_links     link pump_speed         (f32 LE; NaN = absent)
/// …       4·n_links     link valve_setting      (f32 LE; NaN = absent)
/// …       1·n_nodes     node kind (u8: 0 junction, 1 tank, 2 reservoir)
/// …       1·n_links     link kind (u8: 0 pipe, 1 pump, 2 valve)
/// …       1·n_links     link initial status (u8: 0 open, 1 closed, 2 cv;
///                       pumps/valves always 0)
/// …       4·n_links     link vertex count (u32 LE; may be unaligned)
/// then 9 string columns, each `u32 LE byte_len` + newline-joined UTF-8:
///   node id | node tank_volume_curve | node head_pattern |
///   link id | link from_id | link to_id |
///   link pump_curve | link valve_type | link valve_curve
/// ```
///
/// The per-link vertex arrays are concatenated in link order — the same
/// order the link columns are emitted — so link *i*'s vertices are the
/// `vertex_count[i]` entries starting at `Σ vertex_count[0..i]`.
///
/// Column ordering keeps every f64 column 8-byte-aligned and every f32
/// column 4-byte-aligned relative to the buffer start, so the decoder can
/// use zero-copy typed-array views (the trailing u32 vertex-count column may
/// be unaligned; the decoder copies it). Optional numeric fields use an NaN
/// sentinel (`None` ⇔ NaN — real values are never NaN here, see
/// `node_to_dto` / `link_to_dto`), preserving the null-vs-0 distinction.
/// Optional string columns encode `None` as an empty string (IDs are never
/// empty, and INP IDs cannot contain whitespace, so `\n` is a safe joiner).
///
/// Compared to the previous JSON `NetworkSnapshotDto` (~15 MB at 46k nodes +
/// 46k links) this is ~5 MB with no JSON parse on the webview main thread.
pub(crate) fn encode_network_snapshot(dto: &NetworkDto) -> Vec<u8> {
    fn push_f32s<T>(buf: &mut Vec<u8>, items: &[T], get: impl Fn(&T) -> f64) {
        for it in items {
            buf.extend_from_slice(&(get(it) as f32).to_le_bytes());
        }
    }
    fn push_opt_f32s<T>(buf: &mut Vec<u8>, items: &[T], get: impl Fn(&T) -> Option<f64>) {
        for it in items {
            let v = get(it).map_or(f32::NAN, |x| x as f32);
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    /// Write one string column: u32 LE byte length + newline-joined values.
    fn push_str_col<'a, T>(buf: &mut Vec<u8>, items: &'a [T], get: impl Fn(&'a T) -> &'a str) {
        let len_pos = buf.len();
        buf.extend_from_slice(&0u32.to_le_bytes());
        let start = buf.len();
        for (i, it) in items.iter().enumerate() {
            if i > 0 {
                buf.push(b'\n');
            }
            buf.extend_from_slice(get(it).as_bytes());
        }
        let byte_len = (buf.len() - start) as u32;
        buf[len_pos..len_pos + 4].copy_from_slice(&byte_len.to_le_bytes());
    }

    let nodes = &dto.nodes;
    let links = &dto.links;
    let n = nodes.len();
    let m = links.len();
    // Vertices for link `i`; `link_vertices` is parallel to `links`, but a
    // DTO built without vertex context (e.g. `NetworkDto::default()`) may
    // carry an empty vec — treat missing entries as "no vertices".
    const NO_VERTS: &[(f64, f64)] = &[];
    let verts_for =
        |i: usize| -> &[(f64, f64)] { dto.link_vertices.get(i).map_or(NO_VERTS, |v| v.as_slice()) };
    let total_verts: usize = (0..m).map(|i| verts_for(i).len()).sum();

    // Fixed-width section is exact; string columns get a rough per-ID guess.
    let mut buf =
        Vec::with_capacity(32 + 49 * n + 33 * m + 16 * total_verts + 12 * n + 30 * m + 9 * 4);
    buf.extend_from_slice(&NETWORK_SNAPSHOT_VERSION.to_le_bytes());
    buf.extend_from_slice(&NETWORK_SNAPSHOT_FLAG_PRESENT.to_le_bytes());
    buf.extend_from_slice(&(n as u32).to_le_bytes());
    buf.extend_from_slice(&(m as u32).to_le_bytes());
    buf.extend_from_slice(&(total_verts as u32).to_le_bytes());
    buf.extend_from_slice(&[0u8; 12]); // reserved (u32 × 3)

    for nd in nodes {
        buf.extend_from_slice(&nd.x.to_le_bytes());
    }
    for nd in nodes {
        buf.extend_from_slice(&nd.y.to_le_bytes());
    }
    for i in 0..m {
        for (vx, _) in verts_for(i) {
            buf.extend_from_slice(&vx.to_le_bytes());
        }
    }
    for i in 0..m {
        for (_, vy) in verts_for(i) {
            buf.extend_from_slice(&vy.to_le_bytes());
        }
    }
    push_f32s(&mut buf, nodes, |nd| nd.elevation);
    push_f32s(&mut buf, nodes, |nd| nd.base_demand);
    push_opt_f32s(&mut buf, nodes, |nd| nd.pressure);
    push_opt_f32s(&mut buf, nodes, |nd| nd.demand);
    push_opt_f32s(&mut buf, nodes, |nd| nd.tank_min_level);
    push_opt_f32s(&mut buf, nodes, |nd| nd.tank_max_level);
    push_opt_f32s(&mut buf, nodes, |nd| nd.tank_initial_level);
    push_opt_f32s(&mut buf, nodes, |nd| nd.tank_diameter);
    push_f32s(&mut buf, links, |l| l.velocity);
    push_f32s(&mut buf, links, |l| l.diameter);
    push_f32s(&mut buf, links, |l| l.length);
    push_f32s(&mut buf, links, |l| l.roughness);
    push_opt_f32s(&mut buf, links, |l| l.pump_power_kw);
    push_opt_f32s(&mut buf, links, |l| l.pump_speed);
    push_opt_f32s(&mut buf, links, |l| l.valve_setting);

    for nd in nodes {
        // `network_to_dto` is the only producer of these kind strings.
        let code: u8 = match nd.kind.as_str() {
            "junction" => 0,
            "tank" => 1,
            "reservoir" => 2,
            other => {
                debug_assert!(false, "unknown node kind {other:?}");
                0
            }
        };
        buf.push(code);
    }
    for l in links {
        let code: u8 = match l.kind.as_str() {
            "pipe" => 0,
            "pump" => 1,
            "valve" => 2,
            other => {
                debug_assert!(false, "unknown link kind {other:?}");
                0
            }
        };
        buf.push(code);
    }

    // Per-link initial status (0 open, 1 closed, 2 cv). `link_initial_status`
    // is parallel to `links`, but a DTO built without that context (e.g.
    // `NetworkDto::default()`) may carry an empty vec — missing entries
    // default to 0 (open), mirroring `verts_for` above.
    for i in 0..m {
        buf.push(dto.link_initial_status.get(i).copied().unwrap_or(0));
    }

    // Per-link vertex counts (u32 LE; may be unaligned after the u8 columns).
    for i in 0..m {
        buf.extend_from_slice(&(verts_for(i).len() as u32).to_le_bytes());
    }

    push_str_col(&mut buf, nodes, |nd| &nd.id);
    push_str_col(&mut buf, nodes, |nd| {
        nd.tank_volume_curve.as_deref().unwrap_or("")
    });
    push_str_col(&mut buf, nodes, |nd| {
        nd.head_pattern.as_deref().unwrap_or("")
    });
    push_str_col(&mut buf, links, |l| &l.id);
    push_str_col(&mut buf, links, |l| &l.from_id);
    push_str_col(&mut buf, links, |l| &l.to_id);
    push_str_col(&mut buf, links, |l| l.pump_curve.as_deref().unwrap_or(""));
    push_str_col(&mut buf, links, |l| l.valve_type.as_deref().unwrap_or(""));
    push_str_col(&mut buf, links, |l| l.valve_curve.as_deref().unwrap_or(""));
    buf
}

/// Header-only payload with the "present" flag clear — the binary equivalent
/// of the old `null` return from `load_project_network` (target INP missing).
/// Uses the full 32-byte v3 header (flags / counts / reserved words all 0).
pub(crate) fn encode_network_snapshot_absent() -> Vec<u8> {
    let mut buf = Vec::with_capacity(32);
    buf.extend_from_slice(&NETWORK_SNAPSHOT_VERSION.to_le_bytes());
    buf.extend_from_slice(&[0u8; 28]); // flags, n_nodes, n_links, total_verts, reserved × 3
    buf
}

#[tauri::command(async)]
/// Return nodes + links in one compact binary payload for the loaded network
/// (see [`encode_network_snapshot`] for the byte layout). An empty state
/// encodes as a present-but-empty snapshot.
pub fn get_network_snapshot(state: tauri::State<'_, NetworkState>) -> tauri::ipc::Response {
    // Encoding is a single pure read pass over the cached DTO — doing it
    // under the lock is cheaper than the full nodes+links clone it replaced.
    let bytes = match &*state.0.lock() {
        NetworkStateInner::Loaded { dto, .. } => encode_network_snapshot(dto),
        NetworkStateInner::Empty => encode_network_snapshot(&NetworkDto::default()),
    };
    tauri::ipc::Response::new(bytes)
}

/// Flag bit set in the `get_period_results` binary header when the per-node /
/// per-link quality arrays are present.
const PERIOD_RESULTS_FLAG_QUALITY: u32 = 1;

/// Encode one period's flat result arrays into the compact little-endian
/// binary layout consumed by the frontend's `decodePeriodResults`:
///
/// ```text
/// offset  size            content
/// 0       4               n_nodes  (u32 LE)
/// 4       4               n_links  (u32 LE)
/// 8       4               flags    (u32 LE; bit 0 = quality arrays present)
/// 12      4·n_nodes       node_demand   (f32 LE, L/s)
/// …       4·n_nodes       node_head     (f32 LE, m)
/// …       4·n_nodes       node_pressure (f32 LE, m)
/// …       4·n_links       link_flow     (f32 LE, L/s)
/// …       4·n_links       link_velocity (f32 LE, m/s)
/// …       4·n_links       link_headloss (f32 LE)
/// …       4·n_links       link_status   (f32 LE)
/// …       4·n_nodes       node_quality  (f32 LE; only when flag bit 0)
/// …       4·n_links       link_quality  (f32 LE; only when flag bit 0)
/// ```
///
/// Compared to the previous JSON DTO (~3.2 MB per timeline step at 46k nodes +
/// 46k links) this is ~1.3 MB with no number-to-text round-trip.
pub(crate) fn encode_period_results(
    pr: &hydra::io::out_reader::PeriodResult,
    has_quality: bool,
) -> Vec<u8> {
    let n_nodes = pr.node_demand.len();
    let n_links = pr.link_flow.len();
    let mut len = 12 + 4 * (3 * n_nodes + 4 * n_links);
    if has_quality {
        len += 4 * (n_nodes + n_links);
    }
    let mut buf = Vec::with_capacity(len);
    buf.extend_from_slice(&(n_nodes as u32).to_le_bytes());
    buf.extend_from_slice(&(n_links as u32).to_le_bytes());
    let flags: u32 = if has_quality {
        PERIOD_RESULTS_FLAG_QUALITY
    } else {
        0
    };
    buf.extend_from_slice(&flags.to_le_bytes());
    let mut push = |values: &[f32]| {
        for v in values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    };
    push(&pr.node_demand);
    push(&pr.node_head);
    push(&pr.node_pressure);
    push(&pr.link_flow);
    push(&pr.link_velocity);
    push(&pr.link_headloss);
    push(&pr.link_status);
    if has_quality {
        push(&pr.node_quality);
        push(&pr.link_quality);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::network_dto::{LinkDto, NodeDto};

    // ── period-results binary encoding ────────────────────────────────────

    fn read_f32s(buf: &[u8], offset: usize, count: usize) -> Vec<f32> {
        (0..count)
            .map(|i| {
                let start = offset + 4 * i;
                f32::from_le_bytes(buf[start..start + 4].try_into().unwrap())
            })
            .collect()
    }

    #[test]
    fn encode_period_results_layout_roundtrips() {
        let pr = hydra::io::out_reader::PeriodResult {
            node_demand: vec![1.0, 2.0],
            node_head: vec![3.0, 4.0],
            node_pressure: vec![5.0, 6.0],
            node_quality: vec![7.0, 8.0],
            link_flow: vec![9.0, 10.0, 11.0],
            link_velocity: vec![12.0, 13.0, 14.0],
            link_headloss: vec![15.0, 16.0, 17.0],
            link_quality: vec![18.0, 19.0, 20.0],
            link_status: vec![1.0, 0.0, 1.0],
            link_setting: vec![0.0, 0.0, 0.0],
            link_reaction_rate: vec![0.0, 0.0, 0.0],
            link_friction_factor: vec![0.0, 0.0, 0.0],
        };

        // Without quality arrays.
        let buf = encode_period_results(&pr, false);
        assert_eq!(buf.len(), 12 + 4 * (3 * 2 + 4 * 3));
        assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(buf[4..8].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(buf[8..12].try_into().unwrap()), 0);
        assert_eq!(read_f32s(&buf, 12, 2), vec![1.0, 2.0]); // node_demand
        assert_eq!(read_f32s(&buf, 12 + 8, 2), vec![3.0, 4.0]); // node_head
        assert_eq!(read_f32s(&buf, 12 + 16, 2), vec![5.0, 6.0]); // node_pressure
        assert_eq!(read_f32s(&buf, 12 + 24, 3), vec![9.0, 10.0, 11.0]); // link_flow
        assert_eq!(read_f32s(&buf, 12 + 36, 3), vec![12.0, 13.0, 14.0]); // link_velocity
        assert_eq!(read_f32s(&buf, 12 + 48, 3), vec![15.0, 16.0, 17.0]); // link_headloss
        assert_eq!(read_f32s(&buf, 12 + 60, 3), vec![1.0, 0.0, 1.0]); // link_status

        // With quality arrays appended.
        let buf = encode_period_results(&pr, true);
        assert_eq!(buf.len(), 12 + 4 * (3 * 2 + 4 * 3) + 4 * (2 + 3));
        assert_eq!(
            u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            PERIOD_RESULTS_FLAG_QUALITY
        );
        assert_eq!(read_f32s(&buf, 12 + 72, 2), vec![7.0, 8.0]); // node_quality
        assert_eq!(read_f32s(&buf, 12 + 80, 3), vec![18.0, 19.0, 20.0]); // link_quality
    }

    // ── network-snapshot binary encoding ──────────────────────────────────

    fn read_f64s(buf: &[u8], offset: usize, count: usize) -> Vec<f64> {
        (0..count)
            .map(|i| {
                let start = offset + 8 * i;
                f64::from_le_bytes(buf[start..start + 8].try_into().unwrap())
            })
            .collect()
    }

    /// Read one string column (u32 LE byte length + newline-joined UTF-8) at
    /// `offset`; returns the joined string and the offset just past it.
    fn read_str_col(buf: &[u8], offset: usize) -> (String, usize) {
        let len = u32::from_le_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;
        let start = offset + 4;
        let s = std::str::from_utf8(&buf[start..start + len]).unwrap();
        (s.to_string(), start + len)
    }

    /// One node of each kind + one link of each kind, exercising every
    /// optional column in both present and absent states.
    fn snapshot_test_dto() -> NetworkDto {
        let node = |id: &str, kind: &str, x: f64, y: f64, elevation: f64| NodeDto {
            id: id.into(),
            kind: kind.into(),
            x,
            y,
            elevation,
            base_demand: 0.0,
            pressure: None,
            demand: None,
            tank_min_level: None,
            tank_max_level: None,
            tank_initial_level: None,
            tank_diameter: None,
            tank_volume_curve: None,
            head_pattern: None,
        };
        let link = |id: &str, kind: &str, from: &str, to: &str| LinkDto {
            id: id.into(),
            kind: kind.into(),
            from_id: from.into(),
            to_id: to.into(),
            velocity: 0.0,
            diameter: 0.0,
            length: 0.0,
            roughness: 0.0,
            pump_curve: None,
            pump_power_kw: None,
            pump_speed: None,
            valve_type: None,
            valve_setting: None,
            valve_curve: None,
        };

        let mut j1 = node("J1", "junction", 1.5, 2.5, 10.5);
        j1.base_demand = 5.25;
        // Explicit zero must survive as 0, distinct from the NaN "absent".
        j1.demand = Some(0.0);
        let mut t1 = node("T1", "tank", 3.0, 4.0, 50.0);
        t1.tank_min_level = Some(1.5);
        t1.tank_max_level = Some(6.5);
        t1.tank_initial_level = Some(2.25);
        t1.tank_diameter = Some(20.0);
        t1.tank_volume_curve = Some("VC1".into());
        let mut r1 = node("R1", "reservoir", -1.0, 0.0, 100.0);
        r1.head_pattern = Some("PAT7".into());

        let mut p1 = link("P1", "pipe", "J1", "T1");
        p1.velocity = 0.5;
        p1.diameter = 300.0;
        p1.length = 1200.0;
        p1.roughness = 100.0;
        let mut pu1 = link("PU1", "pump", "R1", "J1");
        pu1.pump_curve = Some("C1".into());
        pu1.pump_power_kw = Some(15.5);
        pu1.pump_speed = Some(1.0);
        let mut v1 = link("V1", "valve", "T1", "J1");
        v1.valve_type = Some("PRV".into());
        v1.valve_setting = Some(35.5);

        NetworkDto {
            nodes: vec![j1, t1, r1],
            links: vec![p1, pu1, v1],
            // Vertices on some links only: P1 has 2, PU1 none, V1 one.
            link_vertices: vec![vec![(10.0, 11.0), (12.0, 13.0)], vec![], vec![(20.5, 21.5)]],
            // P1 is a closed pipe; pump/valve are always 0.
            link_initial_status: vec![1, 0, 0],
            ..NetworkDto::default()
        }
    }

    #[test]
    fn encode_network_snapshot_layout_roundtrips() {
        let dto = snapshot_test_dto();
        let buf = encode_network_snapshot(&dto);

        // Header (32 bytes in v3).
        assert_eq!(
            u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            NETWORK_SNAPSHOT_VERSION
        );
        assert_eq!(
            u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            NETWORK_SNAPSHOT_FLAG_PRESENT
        );
        assert_eq!(u32::from_le_bytes(buf[8..12].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(buf[12..16].try_into().unwrap()), 3);
        assert_eq!(
            u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            3,
            "total_verts = 2 (P1) + 0 (PU1) + 1 (V1)"
        );
        assert_eq!(&buf[20..32], &[0u8; 12], "reserved words are zero");

        // f64 coordinate columns (8-byte aligned at offset 32).
        assert_eq!(read_f64s(&buf, 32, 3), vec![1.5, 3.0, -1.0]); // x
        assert_eq!(read_f64s(&buf, 56, 3), vec![2.5, 4.0, 0.0]); // y

        // f64 vertex columns, concatenated in link order.
        assert_eq!(read_f64s(&buf, 80, 3), vec![10.0, 12.0, 20.5]); // vertex x
        assert_eq!(read_f64s(&buf, 104, 3), vec![11.0, 13.0, 21.5]); // vertex y

        // f32 node columns.
        assert_eq!(read_f32s(&buf, 128, 3), vec![10.5, 50.0, 100.0]); // elevation
        assert_eq!(read_f32s(&buf, 140, 3), vec![5.25, 0.0, 0.0]); // base_demand
        let pressure = read_f32s(&buf, 152, 3);
        assert!(pressure.iter().all(|v| v.is_nan()), "pressure all absent");
        let demand = read_f32s(&buf, 164, 3);
        assert_eq!(demand[0], 0.0, "explicit Some(0.0) is 0, not NaN");
        assert!(demand[1].is_nan() && demand[2].is_nan());
        let tank_min = read_f32s(&buf, 176, 3);
        assert!(tank_min[0].is_nan() && tank_min[2].is_nan());
        assert_eq!(tank_min[1], 1.5);
        assert_eq!(read_f32s(&buf, 188, 3)[1], 6.5); // tank_max_level
        assert_eq!(read_f32s(&buf, 200, 3)[1], 2.25); // tank_initial_level
        assert_eq!(read_f32s(&buf, 212, 3)[1], 20.0); // tank_diameter

        // f32 link columns.
        assert_eq!(read_f32s(&buf, 224, 3), vec![0.5, 0.0, 0.0]); // velocity
        assert_eq!(read_f32s(&buf, 236, 3), vec![300.0, 0.0, 0.0]); // diameter
        assert_eq!(read_f32s(&buf, 248, 3), vec![1200.0, 0.0, 0.0]); // length
        assert_eq!(read_f32s(&buf, 260, 3), vec![100.0, 0.0, 0.0]); // roughness
        let power = read_f32s(&buf, 272, 3);
        assert!(power[0].is_nan() && power[2].is_nan());
        assert_eq!(power[1], 15.5);
        assert_eq!(read_f32s(&buf, 284, 3)[1], 1.0); // pump_speed
        let setting = read_f32s(&buf, 296, 3);
        assert!(setting[0].is_nan() && setting[1].is_nan());
        assert_eq!(setting[2], 35.5);

        // u8 kind columns.
        assert_eq!(&buf[308..311], &[0, 1, 2], "junction, tank, reservoir");
        assert_eq!(&buf[311..314], &[0, 1, 2], "pipe, pump, valve");

        // u8 initial-status column (v3): closed pipe, open pump, open valve.
        assert_eq!(&buf[314..317], &[1, 0, 0], "closed pipe = 1; pump/valve 0");

        // u32 per-link vertex counts (unaligned after the u8 columns).
        let counts: Vec<u32> = (0..3)
            .map(|i| u32::from_le_bytes(buf[317 + 4 * i..321 + 4 * i].try_into().unwrap()))
            .collect();
        assert_eq!(counts, vec![2, 0, 1]);

        // String columns: newline-joined, empty string = absent.
        let mut off = 329;
        for expected in [
            "J1\nT1\nR1",  // node id
            "\nVC1\n",     // tank_volume_curve
            "\n\nPAT7",    // head_pattern
            "P1\nPU1\nV1", // link id
            "J1\nR1\nT1",  // from_id
            "T1\nJ1\nJ1",  // to_id
            "\nC1\n",      // pump_curve
            "\n\nPRV",     // valve_type
            "\n\n",        // valve_curve (all absent)
        ] {
            let (col, next) = read_str_col(&buf, off);
            assert_eq!(col, expected);
            off = next;
        }
        assert_eq!(off, buf.len(), "no trailing bytes");
    }

    /// v3 end-to-end: a closed pipe and a CV pipe parsed from INP surface as
    /// codes 1 and 2 in the initial-status column (engine model: `Closed` on
    /// `LinkBase::initial_status`, CV as `Pipe::check_valve`).
    #[test]
    fn encode_network_snapshot_carries_pipe_initial_status_from_inp() {
        const STATUS_INP: &str = "\
[JUNCTIONS]
J1  10  5

[RESERVOIRS]
R1  100

[PIPES]
P1  R1  J1  1000  12  100  0  Open
P2  J1  R1  800   10  100  0  Closed
P3  R1  J1  900   10  100  0  CV

[COORDINATES]
J1  1.0  2.0
R1  0.0  0.0

[OPTIONS]
Units  GPM

[TIMES]
Duration  0

[END]
";
        let network = hydra::io::parse(STATUS_INP.as_bytes()).unwrap();
        let dto = crate::commands::network_dto::network_to_dto(&network);
        assert_eq!(dto.link_initial_status, vec![0, 1, 2], "open, closed, cv");

        let buf = encode_network_snapshot(&dto);
        let (n, m) = (dto.nodes.len(), dto.links.len());
        assert_eq!(
            u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            3,
            "snapshot layout v3"
        );
        // Fixed-width section: header + f64 coords (no vertices here) +
        // 8 f32 node columns + 7 f32 link columns + node/link kind u8s.
        let status_off = 32 + 16 * n + 4 * 8 * n + 4 * 7 * m + n + m;
        assert_eq!(
            &buf[status_off..status_off + m],
            &[0, 1, 2],
            "initialStatus column sits between linkKind and vertexCount"
        );
    }

    #[test]
    fn encode_network_snapshot_empty_and_absent() {
        // Empty-but-present: 32-byte header + nine zero-length string columns.
        let buf = encode_network_snapshot(&NetworkDto::default());
        assert_eq!(buf.len(), 32 + 9 * 4);
        assert_eq!(
            u32::from_le_bytes(buf[4..8].try_into().unwrap()),
            NETWORK_SNAPSHOT_FLAG_PRESENT
        );
        assert_eq!(u32::from_le_bytes(buf[8..12].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(buf[12..16].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(buf[16..20].try_into().unwrap()), 0);
        let mut off = 32;
        for _ in 0..9 {
            let (col, next) = read_str_col(&buf, off);
            assert_eq!(col, "");
            off = next;
        }

        // Absent: 32-byte header only, "present" flag clear.
        let buf = encode_network_snapshot_absent();
        assert_eq!(buf.len(), 32);
        assert_eq!(
            u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            NETWORK_SNAPSHOT_VERSION
        );
        assert_eq!(&buf[4..32], &[0u8; 28]);
    }
}
