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
                            value = std::str::from_utf8(&attr.value).ok().map(|s| {
                                let slashed = s.replace('\\', "/");
                                // DLC entries prefix the path with
                                // `extensions/<dlc_name>/`, but the file inside
                                // the DLC's cat archive is stored without that
                                // prefix. Strip it so cat lookup finds the file.
                                let stripped = strip_extensions_dlc_prefix(&slashed);
                                format!("{stripped}.xml")
                            });
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

/// Strip a leading `extensions/<dlc_name>/` segment from `path` if present.
/// DLC `index/macros.xml` entries qualify their `value` paths with this
/// prefix, but the files inside the DLC cat archive are stored without it.
fn strip_extensions_dlc_prefix(path: &str) -> &str {
    let Some(rest) = path.strip_prefix("extensions/") else {
        return path;
    };
    match rest.find('/') {
        Some(next_slash) => &rest[next_slash + 1..],
        None => path,
    }
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
    fn strips_extensions_dlc_prefix_so_cat_lookup_works() {
        let xml = br#"<index>
  <entry name="ship_spl_s_scout_01_a_macro" value="extensions\ego_dlc_split\assets\units\size_s\macros\ship_spl_s_scout_01_a_macro" />
  <entry name="ship_arg_l_destroyer_01_a_macro" value="assets\units\size_l\macros\ship_arg_l_destroyer_01_a_macro" />
  <entry name="weirdly_namespaced_macro" value="extensions\ego_dlc_terran\some\other\path" />
</index>"#;
        let m = parse_macros_index(xml);
        // DLC entry: prefix `extensions/ego_dlc_split/` stripped so the
        // resulting path matches what the DLC's cat archive lists.
        assert_eq!(
            m.get("ship_spl_s_scout_01_a_macro").map(String::as_str),
            Some("assets/units/size_s/macros/ship_spl_s_scout_01_a_macro.xml")
        );
        // Main entry untouched.
        assert_eq!(
            m.get("ship_arg_l_destroyer_01_a_macro").map(String::as_str),
            Some("assets/units/size_l/macros/ship_arg_l_destroyer_01_a_macro.xml")
        );
        // Terran example also gets its prefix stripped.
        assert_eq!(
            m.get("weirdly_namespaced_macro").map(String::as_str),
            Some("some/other/path.xml")
        );
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
