//! Parses `index/macros.xml` (and DLC overlays) into a macro-name → cat-archive-path map.

use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::Event;

/// Parse `index/macros.xml`. Each `<entry name="X" value="path"/>` becomes
/// `("x".to_lowercase(), "path/with/forward/slashes.xml")`.
///
/// X4 uses backslashes in the `value` attribute (Windows path-style); we
/// rewrite them to forward slashes and append `.xml` so the result can be
/// passed straight to `cat_reader::read_game_file`.
pub fn parse_macros_index(xml: &[u8]) -> HashMap<String, String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut out: HashMap<String, String> = HashMap::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e))
                if e.name().as_ref() == b"entry" =>
            {
                let mut name: Option<String> = None;
                let mut value: Option<String> = None;
                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"name" => {
                            name = std::str::from_utf8(&attr.value)
                                .ok()
                                .map(|s| s.to_lowercase());
                        }
                        b"value" => {
                            value = std::str::from_utf8(&attr.value)
                                .ok()
                                .map(|s| format!("{}.xml", s.replace('\\', "/")));
                        }
                        _ => {}
                    }
                }
                if let (Some(n), Some(v)) = (name, value) {
                    out.insert(n, v);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_and_normalises_paths() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<index>
  <entry name="station_gen_factory_base_01_macro" value="assets\structures\macros\station_gen_factory_base_01_macro" />
  <entry name="ship_par_l_trans_container_03_a_macro" value="assets\units\size_l\macros\ship_par_l_trans_container_03_a_macro" />
  <entry name="MIXED_Case_Macro" value="assets\foo" />
</index>"#;
        let m = parse_macros_index(xml);
        assert_eq!(
            m.get("station_gen_factory_base_01_macro").map(String::as_str),
            Some("assets/structures/macros/station_gen_factory_base_01_macro.xml")
        );
        assert_eq!(
            m.get("ship_par_l_trans_container_03_a_macro").map(String::as_str),
            Some("assets/units/size_l/macros/ship_par_l_trans_container_03_a_macro.xml")
        );
        // Key is lowercased.
        assert_eq!(m.get("mixed_case_macro").map(String::as_str), Some("assets/foo.xml"));
        // Original case not present.
        assert!(m.get("MIXED_Case_Macro").is_none());
    }

    #[test]
    fn missing_attrs_skipped() {
        let xml = br#"<index>
  <entry name="only_name"/>
  <entry value="only_value"/>
  <entry name="both" value="path"/>
</index>"#;
        let m = parse_macros_index(xml);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("both").map(String::as_str), Some("path.xml"));
    }
}
