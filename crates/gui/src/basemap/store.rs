//! Shared offline-basemap tile store.
//!
//! One global SQLite database holds every downloaded tile exactly once,
//! keyed by `(z, x, y)`. "Regions" are pure metadata: a bounding box plus a
//! membership table recording which tiles the region claims. Overlapping
//! regions therefore share tiles by construction — a download never stores
//! the same tile twice — and deleting a region removes only the tiles no
//! other region still claims. `auto_vacuum=INCREMENTAL` is set so that
//! deletions return disk space to the OS rather than leaving dead pages in
//! the file.

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// XYZ tile coordinate (the scheme MapLibre requests and PMTiles stores).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

/// A stored region row plus its usage accounting.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionInfo {
    pub id: String,
    pub name: String,
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// Unix seconds at creation.
    pub created_at: i64,
    /// Identifier of the planet build the tiles came from, when known.
    pub planet_build: Option<String>,
    /// Bytes of tile data only this region claims.
    pub unique_bytes: u64,
    /// Bytes of tile data shared with at least one other region.
    pub shared_bytes: u64,
    pub tile_count: u64,
    /// Project IDs referencing this region.
    pub project_ids: Vec<String>,
}

pub struct TileStore {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl TileStore {
    /// Open (creating if absent) the store at `path`.
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        // auto_vacuum must be in place before the first tables are written;
        // on a database created without it, VACUUM applies the change.
        let av: i64 = conn
            .query_row("PRAGMA auto_vacuum", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if av != 2 {
            conn.execute_batch("PRAGMA auto_vacuum=INCREMENTAL; VACUUM;")
                .map_err(|e| e.to_string())?;
        }
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS tiles(
                 z INTEGER NOT NULL,
                 x INTEGER NOT NULL,
                 y INTEGER NOT NULL,
                 bytes BLOB NOT NULL,
                 size INTEGER NOT NULL,
                 PRIMARY KEY (z, x, y)
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS regions(
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 min_lon REAL NOT NULL,
                 min_lat REAL NOT NULL,
                 max_lon REAL NOT NULL,
                 max_lat REAL NOT NULL,
                 min_zoom INTEGER NOT NULL,
                 max_zoom INTEGER NOT NULL,
                 created_at INTEGER NOT NULL,
                 planet_build TEXT
             );
             CREATE TABLE IF NOT EXISTS region_tiles(
                 region_id TEXT NOT NULL REFERENCES regions(id) ON DELETE CASCADE,
                 z INTEGER NOT NULL,
                 x INTEGER NOT NULL,
                 y INTEGER NOT NULL,
                 PRIMARY KEY (region_id, z, x, y)
             ) WITHOUT ROWID;
             CREATE INDEX IF NOT EXISTS region_tiles_by_tile
                 ON region_tiles(z, x, y);
             CREATE TABLE IF NOT EXISTS project_regions(
                 project_id TEXT NOT NULL,
                 region_id TEXT NOT NULL REFERENCES regions(id) ON DELETE CASCADE,
                 PRIMARY KEY (project_id, region_id)
             ) WITHOUT ROWID;
             CREATE TABLE IF NOT EXISTS meta(
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: path.to_path_buf(),
        })
    }

    /// Store a tile if absent. Returns `true` when the tile was newly
    /// inserted, `false` when an identical coordinate was already present
    /// (the dedupe path — the incoming bytes are discarded).
    pub fn insert_tile(&self, c: TileCoord, bytes: &[u8]) -> Result<bool, String> {
        let conn = self.conn.lock();
        let n = conn
            .execute(
                "INSERT INTO tiles(z, x, y, bytes, size) VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(z, x, y) DO NOTHING",
                params![c.z, c.x, c.y, bytes, bytes.len() as i64],
            )
            .map_err(|e| e.to_string())?;
        Ok(n > 0)
    }

    /// Fetch a tile's stored bytes.
    pub fn get_tile(&self, c: TileCoord) -> Result<Option<Vec<u8>>, String> {
        self.conn
            .lock()
            .query_row(
                "SELECT bytes FROM tiles WHERE z=?1 AND x=?2 AND y=?3",
                params![c.z, c.x, c.y],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }

    /// How many of `coords` are present in the store.
    pub fn count_present(&self, coords: &[TileCoord]) -> Result<u64, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare_cached("SELECT 1 FROM tiles WHERE z=?1 AND x=?2 AND y=?3")
            .map_err(|e| e.to_string())?;
        let mut present = 0;
        for c in coords {
            if stmt
                .query_row(params![c.z, c.x, c.y], |_| Ok(()))
                .optional()
                .map_err(|e| e.to_string())?
                .is_some()
            {
                present += 1;
            }
        }
        Ok(present)
    }

    /// Total stored bytes of the subset of `coords` present in the store.
    pub fn bytes_present(&self, coords: &[TileCoord]) -> Result<u64, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare_cached("SELECT size FROM tiles WHERE z=?1 AND x=?2 AND y=?3")
            .map_err(|e| e.to_string())?;
        let mut total: u64 = 0;
        for c in coords {
            if let Some(size) = stmt
                .query_row(params![c.z, c.x, c.y], |r| r.get::<_, i64>(0))
                .optional()
                .map_err(|e| e.to_string())?
            {
                total += size as u64;
            }
        }
        Ok(total)
    }

    /// Create a region row and return its ID.
    #[allow(clippy::too_many_arguments)]
    pub fn create_region(
        &self,
        name: &str,
        bbox: (f64, f64, f64, f64),
        min_zoom: u8,
        max_zoom: u8,
        created_at: i64,
        planet_build: Option<&str>,
    ) -> Result<String, String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn
            .lock()
            .execute(
                "INSERT INTO regions(id, name, min_lon, min_lat, max_lon, max_lat,
                                     min_zoom, max_zoom, created_at, planet_build)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    id,
                    name,
                    bbox.0,
                    bbox.1,
                    bbox.2,
                    bbox.3,
                    min_zoom,
                    max_zoom,
                    created_at,
                    planet_build
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(id)
    }

    /// Record that `region_id` claims `c` (idempotent). The tile itself may
    /// have been stored by an earlier region — claiming is what makes it
    /// count as shared rather than re-downloaded.
    pub fn claim_tile(&self, region_id: &str, c: TileCoord) -> Result<(), String> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO region_tiles(region_id, z, x, y) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(region_id, z, x, y) DO NOTHING",
                params![region_id, c.z, c.x, c.y],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Delete a region, dropping only tiles no other region claims.
    /// Returns the number of bytes of tile data freed.
    pub fn delete_region(&self, region_id: &str) -> Result<u64, String> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let freed: u64 = tx
            .query_row(
                "SELECT COALESCE(SUM(t.size), 0) FROM tiles t
                 WHERE EXISTS (SELECT 1 FROM region_tiles rt
                               WHERE rt.region_id = ?1
                                 AND rt.z = t.z AND rt.x = t.x AND rt.y = t.y)
                   AND NOT EXISTS (SELECT 1 FROM region_tiles rt
                                   WHERE rt.region_id <> ?1
                                     AND rt.z = t.z AND rt.x = t.x AND rt.y = t.y)",
                params![region_id],
                |r| r.get::<_, i64>(0),
            )
            .map_err(|e| e.to_string())? as u64;
        tx.execute(
            "DELETE FROM tiles
             WHERE EXISTS (SELECT 1 FROM region_tiles rt
                           WHERE rt.region_id = ?1
                             AND rt.z = tiles.z AND rt.x = tiles.x AND rt.y = tiles.y)
               AND NOT EXISTS (SELECT 1 FROM region_tiles rt
                               WHERE rt.region_id <> ?1
                                 AND rt.z = tiles.z AND rt.x = tiles.x AND rt.y = tiles.y)",
            params![region_id],
        )
        .map_err(|e| e.to_string())?;
        // Cascades region_tiles and project_regions rows.
        tx.execute("DELETE FROM regions WHERE id = ?1", params![region_id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        // Return the freed pages to the OS.
        conn.execute_batch("PRAGMA incremental_vacuum;")
            .map_err(|e| e.to_string())?;
        Ok(freed)
    }

    /// All regions with usage accounting, newest first.
    pub fn list_regions(&self) -> Result<Vec<RegionInfo>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, min_lon, min_lat, max_lon, max_lat,
                        min_zoom, max_zoom, created_at, planet_build
                 FROM regions ORDER BY created_at DESC",
            )
            .map_err(|e| e.to_string())?;
        type RegionRow = (
            String,
            String,
            f64,
            f64,
            f64,
            f64,
            u8,
            u8,
            i64,
            Option<String>,
        );
        let rows: Vec<RegionRow> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                    r.get(9)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;

        let mut out = Vec::with_capacity(rows.len());
        for (
            id,
            name,
            min_lon,
            min_lat,
            max_lon,
            max_lat,
            min_zoom,
            max_zoom,
            created_at,
            planet_build,
        ) in rows
        {
            let (unique_bytes, shared_bytes, tile_count) = region_usage(&conn, &id)?;
            let project_ids = project_ids_for(&conn, &id)?;
            out.push(RegionInfo {
                id,
                name,
                min_lon,
                min_lat,
                max_lon,
                max_lat,
                min_zoom,
                max_zoom,
                created_at,
                planet_build,
                unique_bytes,
                shared_bytes,
                tile_count,
                project_ids,
            });
        }
        Ok(out)
    }

    /// Associate a project with a region (idempotent).
    pub fn link_project(&self, project_id: &str, region_id: &str) -> Result<(), String> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO project_regions(project_id, region_id) VALUES(?1, ?2)
                 ON CONFLICT(project_id, region_id) DO NOTHING",
                params![project_id, region_id],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Drop every region association of a project (e.g. on project delete).
    /// Regions themselves are kept — they surface as "unused" in the UI.
    pub fn unlink_project(&self, project_id: &str) -> Result<(), String> {
        self.conn
            .lock()
            .execute(
                "DELETE FROM project_regions WHERE project_id = ?1",
                params![project_id],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// IDs of regions no project references.
    pub fn unused_regions(&self) -> Result<Vec<String>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id FROM regions r
                 WHERE NOT EXISTS (SELECT 1 FROM project_regions pr WHERE pr.region_id = r.id)",
            )
            .map_err(|e| e.to_string())?;
        let ids = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        Ok(ids)
    }

    /// Actual on-disk footprint of the store (db + WAL + shm), in bytes.
    pub fn disk_bytes(&self) -> u64 {
        ["", "-wal", "-shm"]
            .iter()
            .filter_map(|suffix| {
                let mut p = self.path.as_os_str().to_owned();
                p.push(suffix);
                std::fs::metadata(PathBuf::from(p)).ok().map(|m| m.len())
            })
            .sum()
    }

    /// Store-wide metadata (e.g. the source archive's tile compression).
    pub fn meta_get(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .lock()
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| e.to_string())
    }

    pub fn meta_set(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// (unique_bytes, shared_bytes, tile_count) for one region.
fn region_usage(conn: &Connection, region_id: &str) -> Result<(u64, u64, u64), String> {
    conn.query_row(
        "SELECT
             COALESCE(SUM(CASE WHEN other.n IS NULL THEN t.size ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN other.n IS NOT NULL THEN t.size ELSE 0 END), 0),
             COUNT(*)
         FROM region_tiles rt
         JOIN tiles t ON t.z = rt.z AND t.x = rt.x AND t.y = rt.y
         LEFT JOIN (SELECT z, x, y, COUNT(*) AS n FROM region_tiles
                    WHERE region_id <> ?1 GROUP BY z, x, y) other
                ON other.z = rt.z AND other.x = rt.x AND other.y = rt.y
         WHERE rt.region_id = ?1",
        params![region_id],
        |r| {
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
            ))
        },
    )
    .map_err(|e| e.to_string())
}

fn project_ids_for(conn: &Connection, region_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare_cached(
            "SELECT project_id FROM project_regions WHERE region_id = ?1 ORDER BY project_id",
        )
        .map_err(|e| e.to_string())?;
    let ids = stmt
        .query_map(params![region_id], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<String>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, TileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = TileStore::open(&dir.path().join("basemaps.db")).unwrap();
        (dir, store)
    }

    fn tile(z: u8, x: u32, y: u32) -> TileCoord {
        TileCoord { z, x, y }
    }

    #[test]
    fn insert_is_deduplicated() {
        let (_dir, s) = store();
        assert!(s.insert_tile(tile(7, 1, 2), b"abc").unwrap());
        assert!(!s.insert_tile(tile(7, 1, 2), b"different bytes").unwrap());
        // First writer wins; the stored bytes are unchanged.
        assert_eq!(s.get_tile(tile(7, 1, 2)).unwrap().unwrap(), b"abc");
    }

    #[test]
    fn get_missing_tile_is_none() {
        let (_dir, s) = store();
        assert!(s.get_tile(tile(9, 0, 0)).unwrap().is_none());
    }

    #[test]
    fn count_present_counts_only_stored() {
        let (_dir, s) = store();
        s.insert_tile(tile(8, 1, 1), b"x").unwrap();
        s.insert_tile(tile(8, 1, 2), b"y").unwrap();
        let n = s
            .count_present(&[tile(8, 1, 1), tile(8, 1, 2), tile(8, 1, 3)])
            .unwrap();
        assert_eq!(n, 2);
    }

    /// Two overlapping regions: deleting one keeps the shared tile, drops
    /// the exclusive one, and reports only the exclusive bytes as freed.
    #[test]
    fn delete_region_respects_shared_claims() {
        let (_dir, s) = store();
        let a = s
            .create_region("A", (0.0, 0.0, 1.0, 1.0), 7, 15, 100, None)
            .unwrap();
        let b = s
            .create_region("B", (0.5, 0.5, 1.5, 1.5), 7, 15, 200, None)
            .unwrap();
        let shared = tile(10, 5, 5);
        let only_a = tile(10, 5, 6);
        s.insert_tile(shared, &[0u8; 100]).unwrap();
        s.insert_tile(only_a, &[0u8; 40]).unwrap();
        s.claim_tile(&a, shared).unwrap();
        s.claim_tile(&a, only_a).unwrap();
        s.claim_tile(&b, shared).unwrap();

        let freed = s.delete_region(&a).unwrap();
        assert_eq!(freed, 40);
        assert!(s.get_tile(shared).unwrap().is_some());
        assert!(s.get_tile(only_a).unwrap().is_none());
        let remaining = s.list_regions().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, b);
    }

    #[test]
    fn region_usage_splits_unique_and_shared() {
        let (_dir, s) = store();
        let a = s
            .create_region("A", (0.0, 0.0, 1.0, 1.0), 7, 15, 100, Some("20260101"))
            .unwrap();
        let b = s
            .create_region("B", (0.0, 0.0, 1.0, 1.0), 7, 15, 200, None)
            .unwrap();
        let shared = tile(12, 0, 0);
        let unique = tile(12, 0, 1);
        s.insert_tile(shared, &[0u8; 70]).unwrap();
        s.insert_tile(unique, &[0u8; 30]).unwrap();
        s.claim_tile(&a, shared).unwrap();
        s.claim_tile(&a, unique).unwrap();
        s.claim_tile(&b, shared).unwrap();

        let regions = s.list_regions().unwrap();
        let ra = regions.iter().find(|r| r.id == a).unwrap();
        assert_eq!(ra.unique_bytes, 30);
        assert_eq!(ra.shared_bytes, 70);
        assert_eq!(ra.tile_count, 2);
        assert_eq!(ra.planet_build.as_deref(), Some("20260101"));
        let rb = regions.iter().find(|r| r.id == b).unwrap();
        assert_eq!(rb.unique_bytes, 0);
        assert_eq!(rb.shared_bytes, 70);
    }

    #[test]
    fn unused_regions_and_project_links() {
        let (_dir, s) = store();
        let a = s
            .create_region("A", (0.0, 0.0, 1.0, 1.0), 7, 15, 100, None)
            .unwrap();
        let b = s
            .create_region("B", (0.0, 0.0, 1.0, 1.0), 7, 15, 200, None)
            .unwrap();
        s.link_project("proj-1", &a).unwrap();
        s.link_project("proj-1", &a).unwrap(); // idempotent
        assert_eq!(s.unused_regions().unwrap(), vec![b.clone()]);

        // Dropping the project's links surfaces A as unused too.
        s.unlink_project("proj-1").unwrap();
        let mut unused = s.unused_regions().unwrap();
        unused.sort();
        let mut expected = vec![a.clone(), b.clone()];
        expected.sort();
        assert_eq!(unused, expected);
    }

    #[test]
    fn project_links_cascade_on_region_delete() {
        let (_dir, s) = store();
        let a = s
            .create_region("A", (0.0, 0.0, 1.0, 1.0), 7, 15, 100, None)
            .unwrap();
        s.link_project("proj-1", &a).unwrap();
        s.delete_region(&a).unwrap();
        // Re-creating and listing shows no stale project links anywhere.
        let b = s
            .create_region("B", (0.0, 0.0, 1.0, 1.0), 7, 15, 200, None)
            .unwrap();
        let regions = s.list_regions().unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].id, b);
        assert!(regions[0].project_ids.is_empty());
    }

    #[test]
    fn meta_roundtrip_and_overwrite() {
        let (_dir, s) = store();
        assert!(s.meta_get("tile_compression").unwrap().is_none());
        s.meta_set("tile_compression", "gzip").unwrap();
        s.meta_set("tile_compression", "none").unwrap();
        assert_eq!(s.meta_get("tile_compression").unwrap().unwrap(), "none");
    }

    #[test]
    fn reopen_preserves_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("basemaps.db");
        {
            let s = TileStore::open(&path).unwrap();
            s.insert_tile(tile(5, 1, 1), b"persisted").unwrap();
        }
        let s = TileStore::open(&path).unwrap();
        assert_eq!(s.get_tile(tile(5, 1, 1)).unwrap().unwrap(), b"persisted");
        assert!(s.disk_bytes() > 0);
    }
}
