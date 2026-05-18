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
