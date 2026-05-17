//! Parse `libraries/factions.xml` + `libraries/colors.xml` to build a per-faction
//! mapping of display-name translation refs and palette colour.

use std::collections::HashMap;

/// One faction entry as read from `libraries/factions.xml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactionDef {
    /// Translation reference: (page_id, text_id).
    pub name_textref: (u32, u32),
    /// The mapping id used in `libraries/colors.xml`, e.g. "faction_argon".
    pub color_mapping: String,
}

/// Parse a `{pageId,textId}` translation reference string.
fn parse_textref(s: &str) -> Option<(u32, u32)> {
    // Format: "{20203,201}" or "{20203, 201}".
    let inner = s.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut parts = inner.split(',');
    let p = parts.next()?.trim().parse().ok()?;
    let t = parts.next()?.trim().parse().ok()?;
    Some((p, t))
}

/// Extract a single `<faction …/>` element into `out`.
fn handle_faction(
    e: &quick_xml::events::BytesStart<'_>,
    out: &mut HashMap<String, FactionDef>,
) {
    let mut id_opt: Option<String> = None;
    let mut name_opt: Option<(u32, u32)> = None;
    for attr in e.attributes().filter_map(Result::ok) {
        let key = attr.key.as_ref();
        let val = String::from_utf8_lossy(&attr.value).into_owned();
        match key {
            b"id" => id_opt = Some(val.to_lowercase()),
            b"name" => name_opt = parse_textref(&val),
            _ => {}
        }
    }
    if let (Some(id), Some(textref)) = (id_opt, name_opt) {
        let color_mapping = format!("faction_{}", id);
        out.insert(id, FactionDef { name_textref: textref, color_mapping });
    }
}

/// Parse the XML body of a `libraries/factions.xml` file and return a map of
/// lowercase faction-id → FactionDef.
pub fn parse_factions_xml(xml: &str) -> HashMap<String, FactionDef> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"faction" => {
                handle_faction(e, &mut out);
            }
            Ok(Event::Eof) => break,
            Err(e) => { eprintln!("[map] parse_factions_xml: XML error: {e}"); break; }
            _ => {}
        }
        buf.clear();
    }
    out
}

/// Parse `libraries/colors.xml`. Returns two maps:
/// 1. color id → RGBA bytes (e.g. "azure_dark" → [40,100,180,220])
/// 2. mapping id → color-ref id (e.g. "faction_argon" → "azure_dark")
pub fn parse_colors_xml(xml: &str) -> (HashMap<String, [u8; 4]>, HashMap<String, String>) {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut colors = HashMap::new();
    let mut mappings = HashMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                match e.name().as_ref() {
                    b"color" => {
                        if let Some(id) = attr_str(e, b"id") {
                            let r = attr_u8(e, b"r").unwrap_or(0);
                            let g = attr_u8(e, b"g").unwrap_or(0);
                            let b = attr_u8(e, b"b").unwrap_or(0);
                            let a = attr_u8(e, b"a").unwrap_or(255);
                            colors.insert(id, [r, g, b, a]);
                        }
                    }
                    b"mapping" => {
                        if let (Some(id), Some(rf)) = (attr_str(e, b"id"), attr_str(e, b"ref")) {
                            mappings.insert(id, rf);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                eprintln!("[map] parse_colors_xml: XML error: {e}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    (colors, mappings)
}

/// Resolve a faction's mapping id to its RGBA. None if either the mapping or
/// the referenced colour entry is absent.
pub fn resolve_faction_color(
    mapping_id: &str,
    colors: &HashMap<String, [u8; 4]>,
    mappings: &HashMap<String, String>,
) -> Option<[u8; 4]> {
    let color_id = mappings.get(mapping_id)?;
    colors.get(color_id).copied()
}

fn attr_str(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes().filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

fn attr_u8(e: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<u8> {
    attr_str(e, name)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_factions() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/factions_mini.xml"),
        )
        .unwrap()
    }

    #[test]
    fn parse_factions_extracts_id_and_name_textref() {
        let map = parse_factions_xml(&fixture_factions());
        assert_eq!(map.len(), 2);
        let argon = map.get("argon").expect("argon present");
        assert_eq!(argon.name_textref, (20203, 201));
        assert_eq!(argon.color_mapping, "faction_argon");

        let teladi = map.get("teladi").expect("teladi present");
        assert_eq!(teladi.name_textref, (20203, 1801));
        assert_eq!(teladi.color_mapping, "faction_teladi");
    }

    fn fixture_colors() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/colors_mini.xml"),
        )
        .unwrap()
    }

    #[test]
    fn parse_colors_resolves_mapping_chain() {
        let (colors, mappings) = parse_colors_xml(&fixture_colors());
        assert_eq!(colors.get("azure_dark"), Some(&[40, 100, 180, 220]));
        assert_eq!(mappings.get("faction_argon"), Some(&"azure_dark".to_string()));

        let resolved = resolve_faction_color("faction_argon", &colors, &mappings);
        assert_eq!(resolved, Some([40, 100, 180, 220]));

        let missing = resolve_faction_color("faction_unknown", &colors, &mappings);
        assert_eq!(missing, None);
    }
}
