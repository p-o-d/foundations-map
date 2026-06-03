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
    // Stations: prefer the production module's identification (e.g.
    // "Microchip Production") over the generic basename ("Factory").
    if let Some(prod_macro) = world.production_modules.get(&eid) {
        if let Some(prod_ref) = universe.macro_identifications.get(prod_macro) {
            let display = x4_display_name(prod_ref, &universe.translations);
            if !display.is_empty() && !display.starts_with('{') {
                return Some(display);
            }
        }
    }
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

/// Convert 1-based `n` to a Roman numeral (e.g. 1 → "I", 4 → "IV"). The game
/// renders an object's `nameindex` this way ("Solar Power Plant I"). Returns
/// `None` for 0 or values past the conventional 3999 ceiling.
fn roman(mut n: u32) -> Option<String> {
    if n == 0 || n > 3999 {
        return None;
    }
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for &(v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    Some(out)
}

/// For a station, resolve the factory name the game derives from its primary
/// production ware: production-module macro → ware id → `wares.xml`
/// `factoryname` (e.g. "Solar Power Plant"). `None` when the station has no
/// tracked production module or the ware lacks a factory name.
fn station_factory_name(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> Option<String> {
    let prod_macro = world.production_modules.get(&eid)?;
    let ware = universe.prod_module_wares.get(prod_macro)?;
    universe.ware_factory_names.get(ware).cloned()
}

/// Compose the full label minus the trailing `(CODE)`. Mirrors the game's
/// generated names:
///   - Player-renamed entities (literal `name=`): the custom name verbatim.
///   - Stations: `{faction short} {factory name | class name} {roman(nameindex)}`.
///   - Ships:    `{faction short} {job name} {class name (incl. variation)} {roman}`.
/// Empty pieces are dropped; falls back to the raw macro if nothing resolves.
fn compose_body(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
    macro_name: &str,
) -> String {
    // A literal (non-ref) save name is a player-assigned custom name: show it
    // alone, exactly as the game does.
    if let Some(raw) = world.display_name_refs.get(&eid) {
        if !raw.trim_start().starts_with('{') {
            return raw.clone();
        }
    }

    let mut parts: Vec<String> = Vec::new();

    // Owner faction short name ("ARG").
    if let Some(meta) = world
        .factions
        .get(&eid)
        .and_then(|fid| universe.faction_table.get(fid))
    {
        if !meta.short_name.is_empty() {
            parts.push(meta.short_name.clone());
        }
    }

    use map_domain::world::LiveObjectKind;
    let is_station = matches!(world.kinds.get(&eid), Some(LiveObjectKind::Station));

    if is_station {
        let body = station_factory_name(world, universe, eid)
            .or_else(|| resolve_class_name(world, universe, eid, macro_name))
            .unwrap_or_else(|| strip_macro(macro_name));
        if !body.is_empty() {
            parts.push(body);
        }
    } else {
        // Ship job name prefix ("Builder Ship") for NPC, job-spawned ships.
        if let Some(job) = world
            .entity_jobs
            .get(&eid)
            .and_then(|j| universe.job_names.get(j))
        {
            if !job.is_empty() {
                parts.push(job.clone());
            }
        }
        let body = resolve_class_name(world, universe, eid, macro_name)
            .unwrap_or_else(|| strip_macro(macro_name));
        if !body.is_empty() {
            parts.push(body);
        }
    }

    // Trailing roman numeral from the save's `nameindex`.
    if let Some(r) = world.name_index.get(&eid).copied().and_then(roman) {
        parts.push(r);
    }

    let body = parts.join(" ");
    if body.trim().is_empty() {
        macro_name.to_string()
    } else {
        body
    }
}

/// Resolve a human label for one live entity, used by the side panel and
/// the 3D hover tooltip. Returns `"<composed name> (CODE)"`, or the composed
/// name alone when no code is known. Empty string when the entity id is
/// unknown to the World.
pub fn resolve_entity_label(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> String {
    if !world.names.contains_key(&eid) {
        return String::new();
    }
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    let body = compose_body(world, universe, eid, &macro_name);
    match world.codes.get(&eid) {
        Some(code) => format!("{body} ({code})"),
        None => body,
    }
}

/// Resolve the composed label without the trailing `(CODE)`. Used by callers
/// that render the code on its own line.
pub fn resolve_entity_label_without_code(
    world: &map_domain::world::World,
    universe: &map_domain::universe::Universe,
    eid: map_domain::world::EntityId,
) -> String {
    if !world.names.contains_key(&eid) {
        return String::new();
    }
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    compose_body(world, universe, eid, &macro_name)
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
    fn resolve_entity_label_uses_production_module_for_factory() {
        let mut u = sample_universe();
        // Production module identification.
        u.translations
            .insert((20104, 11901), "Microchip Production".into());
        u.macro_identifications
            .insert("prod_gen_microchips_macro".into(), "{20104,11901}".into());
        // Generic factory basename.
        u.translations.insert((20102, 1701), "Factory".into());
        let mut w = sample_world();
        // Entity 7: station with basename + production module.
        w.insert_entity(
            7,
            "station_gen_factory_base_01_macro".into(),
            LiveObjectKind::Station,
            None,
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            Some("FAC-001".into()),
        );
        w.display_name_refs.insert(7, "{20102,1701}".into());
        w.production_modules
            .insert(7, "prod_gen_microchips_macro".into());
        assert_eq!(
            resolve_entity_label(&w, &u, 7),
            "Microchip Production (FAC-001)"
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

    #[test]
    fn roman_numerals_basic() {
        assert_eq!(roman(1).as_deref(), Some("I"));
        assert_eq!(roman(4).as_deref(), Some("IV"));
        assert_eq!(roman(9).as_deref(), Some("IX"));
        assert_eq!(roman(14).as_deref(), Some("XIV"));
        assert_eq!(roman(0), None);
        assert_eq!(roman(4000), None);
    }

    /// Argon faction with a short name, used for the prefix tests.
    fn universe_with_argon() -> Universe {
        let mut u = sample_universe();
        u.faction_table.insert(
            FactionId(1),
            map_domain::universe::FactionMeta {
                display_name: "Argon Federation".into(),
                short_name: "ARG".into(),
                color: [0, 0, 0, 255],
            },
        );
        u
    }

    #[test]
    fn ship_label_has_faction_short_and_job_prefix() {
        let mut u = universe_with_argon();
        u.job_names.insert(
            "argon_construction_vessel_xl_focused".into(),
            "Builder Ship".into(),
        );
        // Mammoth builder: macro identification resolves "Helios E" here (reusing
        // the sample ref) — we only assert the prefix composition.
        let mut w = sample_world();
        // Entity 1 is an Argon ship (faction 1) with ref name "Cerberus Vanguard".
        w.entity_jobs
            .insert(1, "argon_construction_vessel_xl_focused".into());
        assert_eq!(
            resolve_entity_label(&w, &u, 1),
            "ARG Builder Ship Cerberus Vanguard (AKV-484)"
        );
    }

    #[test]
    fn station_label_uses_factory_name_short_and_roman() {
        let mut u = universe_with_argon();
        // energycells → "Solar Power Plant" factory name.
        u.prod_module_wares
            .insert("prod_gen_energycells_macro".into(), "energycells".into());
        u.ware_factory_names
            .insert("energycells".into(), "Solar Power Plant".into());
        let mut w = sample_world();
        // Entity 8: Argon station with an energy production module, nameindex 1.
        w.insert_entity(
            8,
            "station_gen_factory_base_01_macro".into(),
            LiveObjectKind::Station,
            Some(FactionId(1)),
            glam::Vec3::ZERO,
            SectorId(1),
            None,
            Some("BPF-030".into()),
        );
        w.production_modules
            .insert(8, "prod_gen_energycells_macro".into());
        w.name_index.insert(8, 1);
        assert_eq!(
            resolve_entity_label(&w, &u, 8),
            "ARG Solar Power Plant I (BPF-030)"
        );
    }

    #[test]
    fn player_renamed_literal_skips_composition() {
        let mut u = universe_with_argon();
        u.job_names.insert("somejob".into(), "Some Job".into());
        let mut w = sample_world();
        // Entity 2 has a literal name "My Best Ship"; even with faction + job set,
        // the custom name is shown verbatim.
        w.factions.insert(2, FactionId(1));
        w.entity_jobs.insert(2, "somejob".into());
        assert_eq!(resolve_entity_label(&w, &u, 2), "My Best Ship (MBS-001)");
    }
}
