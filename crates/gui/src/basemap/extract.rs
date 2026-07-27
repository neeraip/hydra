//! Region download pipeline: plan a bbox extract against a PMTiles
//! archive, then stream the missing tiles into the shared store.
//!
//! Planning resolves every tile in the bbox/zoom window to its exact byte
//! range in the archive, subtracts what the store already holds, and
//! reports precise new-vs-already-stored byte counts — the numbers the
//! download confirmation UI shows. Running fetches only the missing
//! ranges, coalescing archive-adjacent tiles into batched reads, and
//! claims every tile (pre-existing ones included) for the new region so
//! deletion refcounts stay exact. Both phases are idempotent: re-running
//! a cancelled or failed download skips everything already stored.

use std::sync::atomic::{AtomicBool, Ordering};

use super::pmtiles::{Archive, RangeSource};
use super::store::{TileCoord, TileStore};

/// Merge fetches when the gap between archive ranges is below this.
const COALESCE_GAP: u64 = 64 * 1024;
/// Upper bound for one batched range read.
const MAX_BATCH_BYTES: u64 = 4 * 1024 * 1024;

/// A tile the plan wants downloaded, with its resolved archive range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedTile {
    pub coord: TileCoord,
    pub offset: u64,
    pub length: u32,
}

#[derive(Debug, Clone)]
pub struct ExtractPlan {
    pub bbox: (f64, f64, f64, f64),
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// Tiles to download, sorted by archive offset.
    pub missing: Vec<PlannedTile>,
    /// Tiles already in the store (will be claimed, not downloaded).
    pub present: Vec<TileCoord>,
    /// In-bbox tiles the archive has no data for (ocean, empty land).
    pub absent: u64,
    /// Exact bytes to download — distinct archive ranges only, since
    /// deduplicated archives may map many tiles to one range.
    pub new_bytes: u64,
    /// Bytes already on disk that the new region will share.
    pub shared_bytes: u64,
}

/// Enumerate the XYZ tiles covering `bbox` at zoom `z` (slippy scheme,
/// latitudes clamped to the Web Mercator domain).
pub fn tiles_in_bbox(bbox: (f64, f64, f64, f64), z: u8) -> Vec<TileCoord> {
    let (min_lon, min_lat, max_lon, max_lat) = bbox;
    let n = 1u32 << z;
    let max_index = n - 1;
    let x0 = lon_to_x(min_lon, z).min(max_index);
    let x1 = lon_to_x(max_lon, z).min(max_index);
    // Larger latitude → smaller y.
    let y0 = lat_to_y(max_lat, z).min(max_index);
    let y1 = lat_to_y(min_lat, z).min(max_index);
    let mut out = Vec::with_capacity(((x1 - x0 + 1) * (y1 - y0 + 1)) as usize);
    for x in x0..=x1 {
        for y in y0..=y1 {
            out.push(TileCoord { z, x, y });
        }
    }
    out
}

fn lon_to_x(lon: f64, z: u8) -> u32 {
    let n = f64::from(1u32 << z);
    (((lon + 180.0) / 360.0 * n).floor().max(0.0)) as u32
}

fn lat_to_y(lat: f64, z: u8) -> u32 {
    // Clamp to the Web Mercator latitude domain.
    let lat = lat.clamp(-85.051_128_78, 85.051_128_78);
    let n = f64::from(1u32 << z);
    let rad = lat.to_radians();
    ((1.0 - (rad.tan() + 1.0 / rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n)
        .floor()
        .max(0.0) as u32
}

/// Resolve a bbox/zoom window against the archive and the store.
pub fn plan_extract<S: RangeSource>(
    store: &TileStore,
    archive: &mut Archive<S>,
    bbox: (f64, f64, f64, f64),
    min_zoom: u8,
    max_zoom: u8,
) -> Result<ExtractPlan, String> {
    let mut missing = Vec::new();
    let mut present = Vec::new();
    let mut absent = 0u64;
    for z in min_zoom..=max_zoom {
        for coord in tiles_in_bbox(bbox, z) {
            if store.get_tile(coord)?.is_some() {
                present.push(coord);
                continue;
            }
            match archive.locate(coord.z, coord.x, coord.y)? {
                Some((offset, length)) => missing.push(PlannedTile {
                    coord,
                    offset,
                    length,
                }),
                None => absent += 1,
            }
        }
    }
    missing.sort_by_key(|t| (t.offset, t.coord.z, t.coord.x, t.coord.y));
    // Distinct ranges only: a deduplicated archive can alias many tiles
    // to one payload, which we download (and count) once.
    let mut new_bytes = 0u64;
    let mut last_range = None;
    for t in &missing {
        if last_range != Some((t.offset, t.length)) {
            new_bytes += u64::from(t.length);
            last_range = Some((t.offset, t.length));
        }
    }
    let shared_bytes = store.bytes_present(&present)?;
    Ok(ExtractPlan {
        bbox,
        min_zoom,
        max_zoom,
        missing,
        present,
        absent,
        new_bytes,
        shared_bytes,
    })
}

/// Progress callback: `(bytes_done, bytes_total)`.
pub type ProgressFn<'a> = &'a mut dyn FnMut(u64, u64);

/// Execute a plan: create the region, claim already-present tiles, fetch
/// the missing ranges in coalesced batches, and record store metadata.
/// Returns the new region ID. A `cancel` set mid-run stops after the
/// current batch with the partial region kept (re-running dedupes).
pub fn run_extract<S: RangeSource>(
    store: &TileStore,
    archive: &mut Archive<S>,
    plan: &ExtractPlan,
    region_name: &str,
    planet_build: Option<&str>,
    progress: ProgressFn<'_>,
    cancel: &AtomicBool,
) -> Result<String, String> {
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let region_id = store.create_region(
        region_name,
        plan.bbox,
        plan.min_zoom,
        plan.max_zoom,
        created_at,
        planet_build,
    )?;
    store.meta_set(
        "tile_compression",
        archive.header.tile_compression.encoding_name(),
    )?;

    for coord in &plan.present {
        store.claim_tile(&region_id, *coord)?;
    }

    let total = plan.new_bytes;
    let mut done = 0u64;
    progress(0, total);

    for batch in coalesce(&plan.missing) {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let base = batch.first().expect("batches are non-empty").offset;
        let last = batch.last().expect("batches are non-empty");
        let span = last.offset + u64::from(last.length) - base;
        let bytes = archive.read_at(base, span)?;
        let mut counted_range = None;
        for tile in batch {
            let start = (tile.offset - base) as usize;
            let end = start + tile.length as usize;
            let payload = bytes
                .get(start..end)
                .ok_or("archive range shorter than directory entry")?;
            store.insert_tile(tile.coord, payload)?;
            store.claim_tile(&region_id, tile.coord)?;
            if counted_range != Some((tile.offset, tile.length)) {
                done += u64::from(tile.length);
                counted_range = Some((tile.offset, tile.length));
            }
        }
        progress(done.min(total), total);
    }
    Ok(region_id)
}

/// Group offset-sorted tiles into batches whose archive spans stay under
/// `MAX_BATCH_BYTES`, merging across gaps smaller than `COALESCE_GAP`.
fn coalesce(missing: &[PlannedTile]) -> Vec<&[PlannedTile]> {
    let mut batches = Vec::new();
    let mut start = 0usize;
    for i in 1..missing.len() {
        let prev = &missing[i - 1];
        let cur = &missing[i];
        let prev_end = prev.offset + u64::from(prev.length);
        let gap_too_big = cur.offset.saturating_sub(prev_end) > COALESCE_GAP;
        let base = missing[start].offset;
        let span_too_big = cur.offset + u64::from(cur.length) - base > MAX_BATCH_BYTES;
        if gap_too_big || span_too_big {
            batches.push(&missing[start..i]);
            start = i;
        }
    }
    if start < missing.len() {
        batches.push(&missing[start..]);
    }
    batches
}

#[cfg(test)]
mod tests {
    use super::super::pmtiles::test_support::{build_archive, MemSource};
    use super::super::pmtiles::Archive;
    use super::*;

    fn open_store() -> (tempfile::TempDir, TileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TileStore::open(&dir.path().join("basemaps.db")).unwrap();
        (dir, store)
    }

    #[test]
    fn bbox_tiles_cover_expected_range() {
        // Whole world at z0/z1.
        assert_eq!(tiles_in_bbox((-180.0, -85.0, 180.0, 85.0), 0).len(), 1);
        assert_eq!(tiles_in_bbox((-180.0, -85.0, 180.0, 85.0), 1).len(), 4);
        // A Manhattan-ish bbox at z12 stays small and non-empty.
        let tiles = tiles_in_bbox((-74.03, 40.70, -73.93, 40.80), 12);
        assert!(!tiles.is_empty() && tiles.len() < 30, "got {}", tiles.len());
        // Every coordinate is in range for its zoom.
        for t in tiles {
            assert!(t.x < (1 << 12) && t.y < (1 << 12));
        }
        // Out-of-domain latitudes clamp instead of panicking.
        let clamped = tiles_in_bbox((-180.0, -90.0, 180.0, 90.0), 1);
        assert_eq!(clamped.len(), 4);
    }

    /// Build an archive covering a 2×2 block at z7 and extract it.
    #[test]
    fn plan_and_run_extract_end_to_end() {
        let (_dir, store) = open_store();
        // z7 tiles around lon/lat (0..~5.6 deg): x=64..65, y=63..64.
        let tiles: Vec<(u8, u32, u32, Vec<u8>)> = [(64, 63), (65, 63), (64, 64), (65, 64)]
            .iter()
            .map(|&(x, y)| (7u8, x, y, format!("payload-{x}-{y}").into_bytes()))
            .collect();
        let bytes = build_archive(&tiles, false);
        let mut archive = Archive::open(MemSource(bytes)).unwrap();

        let bbox = (0.1, -2.5, 5.5, 2.5);
        let plan = plan_extract(&store, &mut archive, bbox, 7, 7).unwrap();
        assert_eq!(plan.missing.len(), 4);
        assert!(plan.present.is_empty());
        let expected_bytes: u64 = tiles.iter().map(|(_, _, _, b)| b.len() as u64).sum();
        assert_eq!(plan.new_bytes, expected_bytes);

        let mut updates = Vec::new();
        let cancel = AtomicBool::new(false);
        let region = run_extract(
            &store,
            &mut archive,
            &plan,
            "Test region",
            Some("20260101"),
            &mut |done, total| updates.push((done, total)),
            &cancel,
        )
        .unwrap();

        // All four tiles stored with their exact payloads.
        for (z, x, y, payload) in &tiles {
            let got = store
                .get_tile(TileCoord {
                    z: *z,
                    x: *x,
                    y: *y,
                })
                .unwrap()
                .unwrap();
            assert_eq!(&got, payload);
        }
        // Progress ended complete, and meta recorded the compression.
        assert_eq!(updates.last().unwrap(), &(expected_bytes, expected_bytes));
        assert_eq!(store.meta_get("tile_compression").unwrap().unwrap(), "gzip");
        // The region accounts every tile as unique.
        let info = store.list_regions().unwrap();
        assert_eq!(info[0].id, region);
        assert_eq!(info[0].tile_count, 4);
        assert_eq!(info[0].unique_bytes, expected_bytes);
    }

    /// A second overlapping extract downloads nothing new and claims the
    /// shared tiles.
    #[test]
    fn overlapping_extract_downloads_nothing() {
        let (_dir, store) = open_store();
        let tiles: Vec<(u8, u32, u32, Vec<u8>)> = [(64, 63), (65, 63)]
            .iter()
            .map(|&(x, y)| (7u8, x, y, vec![7u8; 50]))
            .collect();
        let bytes = build_archive(&tiles, false);
        let mut archive = Archive::open(MemSource(bytes)).unwrap();
        let bbox = (0.1, 0.1, 5.5, 2.5);

        let plan1 = plan_extract(&store, &mut archive, bbox, 7, 7).unwrap();
        let cancel = AtomicBool::new(false);
        run_extract(
            &store,
            &mut archive,
            &plan1,
            "A",
            None,
            &mut |_, _| {},
            &cancel,
        )
        .unwrap();

        let plan2 = plan_extract(&store, &mut archive, bbox, 7, 7).unwrap();
        assert!(plan2.missing.is_empty());
        assert_eq!(plan2.new_bytes, 0);
        assert_eq!(plan2.shared_bytes, 100);
        run_extract(
            &store,
            &mut archive,
            &plan2,
            "B",
            None,
            &mut |_, _| {},
            &cancel,
        )
        .unwrap();

        let regions = store.list_regions().unwrap();
        let b = regions.iter().find(|r| r.name == "B").unwrap();
        assert_eq!(b.unique_bytes, 0);
        assert_eq!(b.shared_bytes, 100);
        // Deleting the original keeps the shared tiles alive for B.
        let a_id = regions.iter().find(|r| r.name == "A").unwrap().id.clone();
        let freed = store.delete_region(&a_id).unwrap();
        assert_eq!(freed, 0);
        assert!(store
            .get_tile(TileCoord { z: 7, x: 64, y: 63 })
            .unwrap()
            .is_some());
    }

    #[test]
    fn cancel_stops_between_batches_and_rerun_completes() {
        let (_dir, store) = open_store();
        let tiles: Vec<(u8, u32, u32, Vec<u8>)> = [(64, 63), (65, 63)]
            .iter()
            .map(|&(x, y)| (7u8, x, y, vec![1u8; 10]))
            .collect();
        let bytes = build_archive(&tiles, false);
        let mut archive = Archive::open(MemSource(bytes)).unwrap();
        let bbox = (0.1, 0.1, 5.5, 2.5);
        let plan = plan_extract(&store, &mut archive, bbox, 7, 7).unwrap();

        // Pre-cancelled: no tiles land, the region row exists but empty.
        let cancel = AtomicBool::new(true);
        let err = run_extract(
            &store,
            &mut archive,
            &plan,
            "Partial",
            None,
            &mut |_, _| {},
            &cancel,
        )
        .unwrap_err();
        assert_eq!(err, "cancelled");

        // Re-planning and running to completion works.
        let plan2 = plan_extract(&store, &mut archive, bbox, 7, 7).unwrap();
        let cancel = AtomicBool::new(false);
        run_extract(
            &store,
            &mut archive,
            &plan2,
            "Full",
            None,
            &mut |_, _| {},
            &cancel,
        )
        .unwrap();
        assert_eq!(
            store
                .count_present(&plan2.missing.iter().map(|t| t.coord).collect::<Vec<_>>())
                .unwrap(),
            2
        );
    }

    #[test]
    fn coalesce_respects_gap_and_size_limits() {
        let t = |offset: u64, length: u32| PlannedTile {
            coord: TileCoord { z: 7, x: 0, y: 0 },
            offset,
            length,
        };
        // Adjacent + small gap merge; giant gap splits.
        let tiles = vec![t(0, 100), t(100, 50), t(200, 10), t(10_000_000, 5)];
        let batches = coalesce(&tiles);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[1].len(), 1);
        // Span cap splits even contiguous runs.
        let big = vec![t(0, 3_000_000), t(3_000_000, 3_000_000)];
        assert_eq!(coalesce(&big).len(), 2);
        assert!(coalesce(&[]).is_empty());
    }
}
