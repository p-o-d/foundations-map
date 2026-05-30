//! Public entrypoint for the parallel save parser.
//!
//! Orchestrates four stages:
//! 1. Spawn gzip producer thread.
//! 2. Byte-scan chunks → SnapshotMeta + FactionOverrides + sector byte ranges.
//! 3. Rayon over sector chunks → per-worker Vec<EntityRecord>.
//! 4. Single-threaded merge into a `World`.
//!
//! Logs per-stage timings to stderr.

pub mod decompress;
pub mod merge;
pub mod scan;
pub mod sector_chunk;
pub mod types;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;

use map_domain::ids::{FactionId, SectorId};
use map_domain::world::{SnapshotMeta, World};

use crate::xml_parser::ParseError;

pub use types::FactionOverrides;

pub fn parse_save(
    path: &Path,
    sector_macros: Option<&HashMap<String, SectorId>>,
    zone_positions: &HashMap<String, (f32, f32, f32)>,
    faction_strings: &mut HashMap<String, FactionId>,
    next_faction_id: &mut u32,
) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
    let t_total = Instant::now();

    let mtime = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or(std::time::UNIX_EPOCH);

    // Stage 1 + 2: gzip producer thread + byte scanner.
    let t_stage1 = Instant::now();
    let decompress::Decompressor { rx, handle } =
        decompress::spawn_decompressor(path).map_err(ParseError::Io)?;

    let scan_out = scan::run_scan(rx, path.to_path_buf(), mtime).map_err(ParseError::Io)?;

    // Join producer thread; surface IO errors that happened mid-stream.
    match handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(ParseError::Io(e)),
        Err(_) => {
            return Err(ParseError::Io(std::io::Error::other(
                "decompressor thread panicked",
            )));
        }
    }
    let stage12_ms = t_stage1.elapsed().as_millis();

    // Stage 3: rayon per-sector.
    let t_stage3 = Instant::now();
    let bytes = Arc::new(scan_out.bytes);
    let chunks = scan_out.chunks;
    let entity_lists: Vec<Vec<types::EntityRecord>> = chunks
        .par_iter()
        .map(|chunk| {
            let slice = &bytes[chunk.byte_range.clone()];
            sector_chunk::parse_sector_chunk(slice, &chunk.sector_macro)
        })
        .collect();
    let stage3_ms = t_stage3.elapsed().as_millis();

    let chunk_count = entity_lists.len();
    let entity_count: usize = entity_lists.iter().map(Vec::len).sum();

    // Stage 4: merge.
    let t_stage4 = Instant::now();
    let world = merge::merge(
        entity_lists,
        sector_macros,
        zone_positions,
        faction_strings,
        next_faction_id,
    );
    let stage4_ms = t_stage4.elapsed().as_millis();

    let total_ms = t_total.elapsed().as_millis();
    eprintln!(
        "[parse] stage1+2={}ms stage3={}ms stage4={}ms total={}ms chunks={} entities={}",
        stage12_ms, stage3_ms, stage4_ms, total_ms, chunk_count, entity_count
    );

    Ok((scan_out.meta, world, scan_out.overrides))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini_save.xml.gz")
    }

    #[test]
    fn parse_mini_save_meta_and_overrides() {
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut nx = 1u32;
        let zp = HashMap::new();
        let (meta, _world, overrides) =
            parse_save(&fixture_path(), None, &zp, &mut fs, &mut nx).unwrap();
        assert_eq!(meta.player_money, 40000);
        assert!((meta.game_time_seconds - 1734.285).abs() < 1e-2);
        assert_eq!(meta.player_location_name, "{20004,10011}");
        assert_eq!(overrides.len(), 2);
    }

    #[test]
    fn parse_mini_save_entities_resolved_via_sector_macros() {
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("cluster_01_sector001_macro".into(), SectorId(1));
        sm.insert("cluster_06_sector001_macro".into(), SectorId(2));
        let mut fs: HashMap<String, FactionId> = HashMap::new();
        let mut nx = 1u32;
        let zp = HashMap::new();
        let (_meta, world, _) =
            parse_save(&fixture_path(), Some(&sm), &zp, &mut fs, &mut nx).unwrap();
        assert_eq!(world.names.len(), 4);
        assert_eq!(world.entities_in_sector(SectorId(1)).len(), 2);
        assert_eq!(world.entities_in_sector(SectorId(2)).len(), 2);
    }
}
