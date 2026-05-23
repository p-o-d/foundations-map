//! Centralised faction colour + name resolution. Reads from `Universe.faction_table`.
//! Also provides shared string-utility helpers used across UI modules.

use map_domain::ids::FactionId;
use map_domain::universe::Universe;

pub fn faction_color(universe: &Universe, id: FactionId) -> egui::Color32 {
    universe
        .faction_table
        .get(&id)
        .map(|m| {
            egui::Color32::from_rgba_unmultiplied(m.color[0], m.color[1], m.color[2], m.color[3])
        })
        .unwrap_or(crate::theme::TEXT_MUTED)
}

pub fn faction_name<'a>(universe: &'a Universe, id: FactionId) -> &'a str {
    universe
        .faction_table
        .get(&id)
        .map(|m| m.display_name.as_str())
        .unwrap_or("Unknown")
}

/// Strip the trailing `_macro` suffix (case-insensitive) and replace underscores
/// with spaces. Used to derive a human-readable label from an X4 macro name.
pub fn strip_macro(s: &str) -> String {
    let s = s.to_lowercase();
    let s = s.strip_suffix("_macro").unwrap_or(&s).to_owned();
    s.replace('_', " ")
}

/// Substitute every `{page,id}` substring with its resolved translation, leaving
/// other text intact. Used for compound names like `{p,t} ({p,t})` and for
/// literal user-renamed ships (which contain no braces and pass through unchanged).
/// Unknown translation keys and malformed brace groups are left as-is, which
/// helps spot missing IDs while debugging.
pub fn replace_translation_refs(
    s: &str,
    translations: &std::collections::HashMap<(u32, u32), String>,
) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        match rest.find('{') {
            None => {
                out.push_str(rest);
                break;
            }
            Some(open) => {
                out.push_str(&rest[..open]);
                let after_open = &rest[open + 1..];
                match after_open.find('}') {
                    None => {
                        // No closing brace — emit the literal `{` and continue past it.
                        out.push('{');
                        rest = after_open;
                    }
                    Some(close_rel) => {
                        let inner = &after_open[..close_rel];
                        let parsed: Option<(u32, u32)> = inner
                            .split_once(',')
                            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
                        match parsed.and_then(|(p, t)| translations.get(&(p, t))) {
                            Some(text) => out.push_str(text),
                            None => {
                                out.push('{');
                                out.push_str(inner);
                                out.push('}');
                            }
                        }
                        rest = &after_open[close_rel + 1..];
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn sample_translations() -> HashMap<(u32, u32), String> {
        let mut m = HashMap::new();
        m.insert((20101, 122701), "Cerberus Vanguard".into());
        m.insert((20203, 401), "Argon Federation".into());
        m
    }

    #[test]
    fn replace_translation_refs_single_ref() {
        let t = sample_translations();
        assert_eq!(
            replace_translation_refs("{20101,122701}", &t),
            "Cerberus Vanguard"
        );
    }

    #[test]
    fn replace_translation_refs_compound() {
        let t = sample_translations();
        assert_eq!(
            replace_translation_refs("{20101,122701} ({20203,401})", &t),
            "Cerberus Vanguard (Argon Federation)"
        );
    }

    #[test]
    fn replace_translation_refs_literal_passes_through() {
        let t = sample_translations();
        assert_eq!(
            replace_translation_refs("My Best Ship", &t),
            "My Best Ship"
        );
    }

    #[test]
    fn replace_translation_refs_unknown_key_left_intact() {
        let t = sample_translations();
        // Useful for debugging — missing translation IDs stay visible.
        assert_eq!(
            replace_translation_refs("{99999,1}", &t),
            "{99999,1}"
        );
    }

    #[test]
    fn replace_translation_refs_malformed_left_intact() {
        let t = sample_translations();
        assert_eq!(replace_translation_refs("{not,a,ref}", &t), "{not,a,ref}");
        assert_eq!(replace_translation_refs("{",          &t), "{");
        assert_eq!(replace_translation_refs("plain text", &t), "plain text");
    }
}
