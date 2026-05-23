//! Centralised faction colour + name resolution. Reads from `Universe.faction_table`.
//! Also provides shared string-utility helpers used across UI modules.

pub use map_domain::translations::x4_display_name;

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

/// Resolve the class-name portion of an entity label via lookups only.
///
/// Tries:
///   1. The entity's per-instance `display_name_ref` from the save (may be a
///      literal, a `{p,t}` ref, or a compound form).
///   2. The macro definition file's `<identification name=...>` ref, looked up
///      by the lowercased macro name in `universe.macro_identifications`.
///
/// Returns `None` when neither lookup yields a usable display name.
fn resolve_class_name(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
    macro_name: &str,
) -> Option<String> {
    if let Some(raw) = world.display_name_refs.get(&eid) {
        let display = x4_display_name(raw, &universe.translations);
        if !display.is_empty() && !display.starts_with('{') {
            return Some(display);
        }
    }
    if let Some(macro_ref) = universe
        .macro_identifications
        .get(&macro_name.to_lowercase())
    {
        let display = x4_display_name(macro_ref, &universe.translations);
        if !display.is_empty() && !display.starts_with('{') {
            return Some(display);
        }
    }
    None
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
    let class_name = resolve_class_name(world, universe, eid, &macro_name).or_else(|| {
        let stripped = strip_macro(&macro_name);
        if stripped.is_empty() {
            None
        } else {
            Some(stripped)
        }
    });
    let code = world.codes.get(&eid).cloned();
    match (class_name, code) {
        (Some(c), Some(code)) => format!("{c} ({code})"),
        (Some(c), None) => c,
        (None, Some(code)) => code,
        (None, None) => macro_name,
    }
}

/// Resolve only the class-name portion of the entity label (no code in parens).
/// Used by callers that render the code on its own line.
pub fn resolve_entity_label_without_code(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> String {
    if !world.names.contains_key(&eid) {
        return String::new();
    }
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    resolve_class_name(world, universe, eid, &macro_name).unwrap_or_else(|| {
        let stripped = strip_macro(&macro_name);
        if stripped.is_empty() {
            macro_name
        } else {
            stripped
        }
    })
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

    use map_domain::ids::{FactionId, SectorId};
    use map_domain::universe::Universe;
    use map_domain::world::{LiveObjectKind, World};

    fn sample_universe() -> Universe {
        let mut u = Universe::default();
        u.translations = sample_translations();
        // Add additional translations for the macro-identification test cases.
        u.translations.insert((20101, 30801), "Helios".into());
        u.translations.insert((20111, 5462), "E".into());
        u.translations.insert(
            (20101, 30804),
            "(Helios E){20101,30801} {20111,5462}".into(),
        );
        u.current_locale = 44;
        // Map "ship_par_l_trans_container_03_a_macro" to its identification ref.
        u.macro_identifications.insert(
            "ship_par_l_trans_container_03_a_macro".into(),
            "{20101,30804}".into(),
        );
        u
    }

    fn sample_world() -> World {
        let mut w = World::new();
        // Entity 1: has both a display_name_ref and a code.
        w.insert_entity(
            1,
            "ship_par_l_trans_container_03_a_macro".into(),
            LiveObjectKind::ShipLarge,
            Some(FactionId(1)),
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            Some("AKV-484".into()),
        );
        w.display_name_refs.insert(1, "{20101,122701}".into());

        // Entity 2: literal name + code (player-renamed ship).
        w.insert_entity(
            2,
            "ship_arg_s_scout_01_a_macro".into(),
            LiveObjectKind::ShipSmall,
            None,
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            Some("MBS-001".into()),
        );
        w.display_name_refs.insert(2, "My Best Ship".into());

        // Entity 3: no display_name_ref, no code — pure macro fallback.
        w.insert_entity(
            3,
            "ship_xen_n_fighter_01_a_macro".into(),
            LiveObjectKind::ShipSmall,
            None,
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            None,
        );
        // Entity 4: has display_name_ref, no code.
        w.insert_entity(
            4,
            "ship_xen_p_destroyer_01_a_macro".into(),
            LiveObjectKind::ShipLarge,
            None,
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            None,
        );
        w.display_name_refs.insert(4, "{20101,122701}".into());

        // Entity 5: no display_name_ref, but macro is in macro_identifications.
        w.insert_entity(
            5,
            "ship_par_l_trans_container_03_a_macro".into(),
            LiveObjectKind::ShipLarge,
            None,
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            Some("AKV-484".into()),
        );

        w
    }

    #[test]
    fn resolve_entity_label_name_and_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(
            resolve_entity_label(&w, &u, 1),
            "Cerberus Vanguard (AKV-484)"
        );
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

    #[test]
    fn resolve_entity_label_without_code_omits_code() {
        let u = sample_universe();
        let w = sample_world();
        assert_eq!(
            resolve_entity_label_without_code(&w, &u, 1),
            "Cerberus Vanguard"
        );
        assert_eq!(resolve_entity_label_without_code(&w, &u, 2), "My Best Ship");
        assert_eq!(
            resolve_entity_label_without_code(&w, &u, 3),
            "ship xen n fighter 01 a"
        );
    }

    #[test]
    fn resolve_entity_label_uses_macro_identification_when_no_display_name_ref() {
        let u = sample_universe();
        let w = sample_world();
        // Entity 5: no display_name_ref. Macro lookup yields {20101,30804}
        // → x4_display_name("{20101,30804}") → resolves to "(Helios E){...} {…}"
        // → leading paren → "Helios E". With code → "Helios E (AKV-484)".
        assert_eq!(resolve_entity_label(&w, &u, 5), "Helios E (AKV-484)");
    }

    #[test]
    fn resolve_entity_label_save_attr_takes_precedence_over_macro_identification() {
        let mut u = sample_universe();
        // Make entity 1's macro also match a macro_identifications entry.
        u.macro_identifications.insert(
            "ship_par_l_trans_container_03_a_macro".into(),
            "{20101,30804}".into(),
        );
        let w = sample_world();
        // Entity 1 has display_name_ref="{20101,122701}" → "Cerberus Vanguard"
        // → extract → "Cerberus Vanguard (AKV-484)".
        // It does NOT fall through to macro_identifications because the save
        // attribute resolved successfully.
        assert_eq!(
            resolve_entity_label(&w, &u, 1),
            "Cerberus Vanguard (AKV-484)"
        );
    }

    #[test]
    fn resolve_entity_label_compound_save_attr_extracts_correctly() {
        // Compound form "(Name)Description" via display_name_ref.
        let u = sample_universe();
        let mut w = sample_world();
        // Add entity 6 whose save name= is itself a compound ref.
        w.insert_entity(
            6,
            "ship_test".into(),
            LiveObjectKind::ShipLarge,
            None,
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            Some("XYZ-1".into()),
        );
        w.display_name_refs.insert(6, "{20101,30804}".into());
        assert_eq!(resolve_entity_label(&w, &u, 6), "Helios E (XYZ-1)");
    }
}
