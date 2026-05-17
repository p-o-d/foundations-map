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
}
