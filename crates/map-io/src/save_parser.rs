//! X4 save-game parser.
//!
//! Reads `~/.config/EgoSoft/X4/<id>/save/{quicksave,save_NNN}.xml.gz`. Files are
//! gzip-compressed XML, ~30 MB compressed / ~300 MB uncompressed. We stream via
//! `quick_xml` over a `flate2::read::GzDecoder` to avoid loading the whole DOM.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use flate2::read::GzDecoder;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use map_domain::world::{SnapshotMeta, World};

use crate::xml_parser::ParseError;

/// Map of sector_macro (lowercase) → faction string from the save's per-sector
/// `<component class="sector" owner="..."/>` attribute. Caller resolves to FactionId.
pub type FactionOverrides = HashMap<String, String>;

/// Parse an X4 save file. Returns snapshot metadata, a `World` of live ships +
/// stations, and per-sector faction overrides.
pub fn parse_save(path: &Path) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
    let file = File::open(path)?;
    let mtime = file
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or(std::time::UNIX_EPOCH);
    let gz = GzDecoder::new(file);
    let mut reader = Reader::from_reader(BufReader::new(gz));
    reader.config_mut().trim_text(true);

    let mut meta = SnapshotMeta {
        path: path.to_path_buf(),
        mtime,
        game_time_seconds: 0.0,
        player_money: 0,
        player_location_name: String::new(),
    };

    let world = World::new();
    let overrides = FactionOverrides::new();

    let mut buf = Vec::new();
    let mut info_done = false;

    while !info_done {
        match reader.read_event_into(&mut buf)? {
            Event::Empty(ref e) => match e.name().as_ref() {
                b"game" => {
                    if let Some(t) = attr_value(e, b"time") {
                        meta.game_time_seconds = t.parse().unwrap_or(0.0);
                    }
                }
                b"player" => {
                    if let Some(m) = attr_value(e, b"money") {
                        meta.player_money = m.parse().unwrap_or(0);
                    }
                    if let Some(loc) = attr_value(e, b"location") {
                        meta.player_location_name = loc;
                    }
                }
                _ => {}
            },
            Event::End(ref e) if e.name().as_ref() == b"info" => info_done = true,
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    // Universe tree walk happens in Task 4.

    Ok((meta, world, overrides))
}

fn attr_value(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini_save.xml.gz")
    }

    #[test]
    fn parse_mini_save_meta() {
        let (meta, _, _) = parse_save(&fixture_path()).expect("parse");
        assert_eq!(meta.player_money, 40000);
        assert!((meta.game_time_seconds - 1734.285).abs() < 1e-2);
        assert_eq!(meta.player_location_name, "{20004,10011}");
    }
}
