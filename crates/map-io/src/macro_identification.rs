//! Extracts the `<identification name="{p,t}"/>` ref from a macro definition file.

use quick_xml::Reader;
use quick_xml::events::Event;

/// Returns the raw `name` attribute of the first `<identification>` element in
/// a macro file (e.g. `"{20101,30804}"`), or `None` if absent.
///
/// The returned string is the per-macro display ref. Resolution to a final
/// display name happens later via the translation table and the X4
/// display-name extraction rules.
pub fn parse_macro_identification(xml: &[u8]) -> Option<String> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e))
                if e.name().as_ref() == b"identification" =>
            {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"name" {
                        return std::str::from_utf8(&attr.value).ok().map(str::to_string);
                    }
                }
                return None;
            }
            Ok(Event::Eof) => return None,
            Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_identification_name() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<macros>
  <macro name="ship_par_l_trans_container_03_a_macro" class="ship_l">
    <properties>
      <identification name="{20101,30804}" makerrace="paranid" />
    </properties>
  </macro>
</macros>"#;
        assert_eq!(
            parse_macro_identification(xml).as_deref(),
            Some("{20101,30804}")
        );
    }

    #[test]
    fn returns_none_when_no_identification() {
        let xml = br#"<macros><macro name="x"><properties/></macro></macros>"#;
        assert_eq!(parse_macro_identification(xml), None);
    }

    #[test]
    fn returns_none_when_no_name_attr() {
        let xml = br#"<macros><macro><properties>
          <identification makerrace="x" />
        </properties></macro></macros>"#;
        assert_eq!(parse_macro_identification(xml), None);
    }

    #[test]
    fn first_identification_wins() {
        let xml = br#"<macros>
  <macro name="a"><properties><identification name="{20101,1}"/></properties></macro>
  <macro name="b"><properties><identification name="{20101,2}"/></properties></macro>
</macros>"#;
        assert_eq!(
            parse_macro_identification(xml).as_deref(),
            Some("{20101,1}")
        );
    }
}
