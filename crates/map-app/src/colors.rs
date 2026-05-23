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

/// Resolve a human label for one live entity, used by the side panel and
/// the 3D hover tooltip.
///
/// Returns `"Class Name (CODE)"` when both the resolved class name and the
/// short code are known; falls back to the class name alone, then the code
/// alone, and finally to `strip_macro(macro_name)` if nothing else is
/// available. Returns an empty string when the entity id is unknown to the
/// World.
pub fn resolve_entity_label(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> String {
    if !world.names.contains_key(&eid) {
        return String::new();
    }
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    let class_name: Option<String> = world.display_name_refs.get(&eid).map(|raw| {
        replace_translation_refs(raw, &universe.translations)
    });
    let class_name: Option<String> = class_name
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .or_else(|| {
            let stripped = strip_macro(&macro_name);
            if stripped.is_empty() { None } else { Some(stripped) }
        });
    let code = world.codes.get(&eid).cloned();
    match (class_name, code) {
        (Some(c), Some(code)) => format!("{c} ({code})"),
        (Some(c), None)       => c,
        (None, Some(code))    => code,
        (None, None)          => macro_name,
    }
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

    use map_domain::ids::{FactionId, SectorId};
    use map_domain::universe::Universe;
    use map_domain::world::{LiveObjectKind, World};

    fn sample_universe() -> Universe {
        let mut u = Universe::default();
        u.translations = sample_translations();
        u.current_locale = 44;
        u
    }

    fn sample_world() -> World {
        let mut w = World::new();
        // Entity 1: has both a display_name_ref and a code.
        w.insert_entity(
            1, "ship_par_l_trans_container_03_a_macro".into(),
            LiveObjectKind::ShipLarge, Some(FactionId(1)),
            glam::Vec3::ZERO, SectorId(1), None, Some("AKV-484".into()),
        );
        w.display_name_refs.insert(1, "{20101,122701}".into());

        // Entity 2: literal name + code (player-renamed ship).
        w.insert_entity(
            2, "ship_arg_s_scout_01_a_macro".into(),
            LiveObjectKind::ShipSmall, None, glam::Vec3::ZERO,
            SectorId(1), None, Some("MBS-001".into()),
        );
        w.display_name_refs.insert(2, "My Best Ship".into());

        // Entity 3: no display_name_ref, no code — pure macro fallback.
        w.insert_entity(
            3, "ship_xen_n_fighter_01_a_macro".into(),
            LiveObjectKind::ShipSmall, None, glam::Vec3::ZERO,
            SectorId(1), None, None,
        );
        // Entity 4: has display_name_ref, no code.
        w.insert_entity(
            4, "ship_xen_p_destroyer_01_a_macro".into(),
            LiveObjectKind::ShipLarge, None, glam::Vec3::ZERO,
            SectorId(1), None, None,
        );
        w.display_name_refs.insert(4, "{20101,122701}".into());

        w
    }

    #[test]
    fn resolve_entity_label_name_and_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 1), "Cerberus Vanguard (AKV-484)");
    }

    #[test]
    fn resolve_entity_label_literal_name_and_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 2), "My Best Ship (MBS-001)");
    }

    #[test]
    fn resolve_entity_label_macro_fallback_when_nothing_known() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 3), "ship xen n fighter 01 a");
    }

    #[test]
    fn resolve_entity_label_name_only_when_no_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 4), "Cerberus Vanguard");
    }

    #[test]
    fn resolve_entity_label_unknown_entity_id_falls_back_to_empty() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(resolve_entity_label(&w, &u, 999), "");
    }
}
