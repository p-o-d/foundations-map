//! Public entrypoint for the parallel save parser.
//!
//! Orchestrates four stages: gzip decompress + byte scan (overlapping),
//! rayon per-sector parse, single-threaded merge into a `World`.

pub mod decompress;
pub mod merge;
pub mod scan;
pub mod sector_chunk;
pub mod types;

use std::collections::HashMap;
use std::path::Path;

use map_domain::ids::SectorId;
use map_domain::world::{SnapshotMeta, World};

use crate::xml_parser::ParseError;

pub use types::FactionOverrides;

/// Parse an X4 save file in parallel.
///
/// Stub returning an error until Task 7 wires up the real orchestrator.
/// Existing callers handle errors gracefully (no entities → no live ships drawn).
pub fn parse_save(
    _path: &Path,
    _sector_macros: Option<&HashMap<String, SectorId>>,
) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
    Err(ParseError::MissingAttribute(
        "save_parser not yet wired (Task 7)".into(),
    ))
}
