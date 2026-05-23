//! Shared helpers for resolving X4 translation refs into human display names.
//!
//! `replace_translation_refs` substitutes every `{page,id}` substring with its
//! resolved translation (recursing up to 4 levels so chained X4 entries fully
//! resolve). `extract_x4_display_name` applies X4's display conventions to a
//! resolved string: leading parenthetical = display name, trailing
//! parenthetical = description, plain text = as-is.

use std::collections::HashMap;

/// Substitute every `{page,id}` substring with its resolved translation, leaving
/// other text intact. Used for compound names like `{p,t} ({p,t})` and for
/// literal user-renamed ships (which contain no braces and pass through unchanged).
/// Unknown translation keys and malformed brace groups are left as-is, which
/// helps spot missing IDs while debugging.
///
/// Substituted strings may themselves contain `{p,t}` refs (X4 chains class
/// names through other entries). Resolution iterates up to 4 times to handle
/// these compound forms while terminating safely on self-referential loops.
pub fn replace_translation_refs(
    s: &str,
    translations: &HashMap<(u32, u32), String>,
) -> String {
    let mut current = s.to_string();
    for _ in 0..4 {
        let next = replace_translation_refs_once(&current, translations);
        if next == current {
            return current;
        }
        current = next;
    }
    current
}

fn replace_translation_refs_once(
    s: &str,
    translations: &HashMap<(u32, u32), String>,
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

/// Extract the human display name from an X4 translation entry following its
/// pluralistic conventions:
///   - `(NAME)rest`  → `NAME`  (leading parenthetical is the display name;
///                              the trailing text is X4 internal composition)
///   - `NAME(desc)`  → `NAME`  (trailing parenthetical is a description)
///   - plain text    → as-is
///
/// Whitespace around the result is trimmed.
pub fn extract_x4_display_name(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(stripped) = trimmed.strip_prefix('(') {
        if let Some(close) = stripped.find(')') {
            return stripped[..close].trim().to_string();
        }
    }
    if let Some(open) = trimmed.find('(') {
        return trimmed[..open].trim().to_string();
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_translations() -> HashMap<(u32, u32), String> {
        let mut m = HashMap::new();
        m.insert((20101, 122701), "Cerberus Vanguard".into());
        m.insert((20203, 401), "Argon Federation".into());
        m
    }

    fn translations_for_recursion() -> HashMap<(u32, u32), String> {
        let mut m = HashMap::new();
        m.insert((20101, 30804), "(Helios E){20101,30801} {20111,5462}".into());
        m.insert((20101, 30801), "Helios".into());
        m.insert((20111, 5462), "E".into());
        m.insert((20101, 122701), "Wayfinder(ALI Expedition ship)".into());
        m.insert((20101, 10101), "Discoverer".into());
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

    #[test]
    fn replace_translation_refs_recurses_into_substituted_text() {
        let t = translations_for_recursion();
        assert_eq!(
            replace_translation_refs("{20101,30804}", &t),
            "(Helios E)Helios E"
        );
    }

    #[test]
    fn replace_translation_refs_terminates_on_self_referential_loop() {
        let mut t = HashMap::new();
        t.insert((1, 1), "{1,1}".into());
        let result = replace_translation_refs("{1,1}", &t);
        assert!(result.len() < 10_000);
    }

    #[test]
    fn extract_x4_display_name_leading_paren_wins() {
        assert_eq!(extract_x4_display_name("(Helios E)Helios E"), "Helios E");
        assert_eq!(
            extract_x4_display_name("(Discoverer Vanguard)Discoverer Vanguard"),
            "Discoverer Vanguard"
        );
    }

    #[test]
    fn extract_x4_display_name_trailing_paren_treated_as_description() {
        assert_eq!(extract_x4_display_name("Wayfinder(ALI Expedition ship)"), "Wayfinder");
        assert_eq!(extract_x4_display_name("Cerberus (Vanguard)"), "Cerberus");
    }

    #[test]
    fn extract_x4_display_name_plain_text_returned_as_is() {
        assert_eq!(extract_x4_display_name("Discoverer"), "Discoverer");
        assert_eq!(extract_x4_display_name(""), "");
    }

    #[test]
    fn extract_x4_display_name_handles_paren_only() {
        assert_eq!(extract_x4_display_name("(Helios E)"), "Helios E");
    }
}
