// NOTE (Phase 3 polish): no panel-level "no save loaded" hint here — the
// top-bar already shows that state and plumbing the snapshot through to the
// panel just to repeat the message isn't worth the parameter churn. Revisit
// if/when the panel grows live-data sections (per-sector ship list, etc.).
use crate::theme;
use map_domain::ids::ObjectId;
use map_domain::universe::{GateType, Sector, Universe};
use map_domain::view::ViewMode;

pub struct SectorPanelResponse {
    pub open_3d_clicked: bool,
    pub back_to_map_clicked: bool,
    pub object_clicked: Option<ObjectId>,
    pub entity_clicked: Option<map_domain::world::EntityId>,
    pub back_to_parent_clicked: bool,
}

#[derive(Default)]
pub struct SectorPanel;

impl SectorPanel {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        sector: Option<&Sector>,
        universe: &Universe,
        view_mode: &ViewMode,
        world: Option<&map_domain::world::World>,
    ) -> SectorPanelResponse {
        ui.add_space(8.0);

        let Some(sector) = sector else {
            ui.colored_label(theme::TEXT_MUTED, "Select a sector");
            ui.add_space(4.0);
            ui.colored_label(theme::TEXT_MUTED, "Click on the map.");
            return SectorPanelResponse {
                open_3d_clicked: false,
                back_to_map_clicked: false,
                object_clicked: None,
                entity_clicked: None,
                back_to_parent_clicked: false,
            };
        };

        let back_clicked = ui.small_button("← Universe").clicked();
        ui.add_space(4.0);

        ui.colored_label(theme::TEXT_MUTED, "SECTOR");
        ui.add_space(2.0);
        ui.colored_label(theme::TEXT_PRIMARY, &sector.name);
        if let Some(faction_id) = sector.faction {
            let f_color = crate::colors::faction_color(universe, faction_id);
            let f_name = crate::colors::faction_name(universe, faction_id);
            ui.horizontal(|ui| {
                ui.colored_label(f_color, "●");
                ui.colored_label(theme::ACCENT, f_name);
            });
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        let mut object_clicked: Option<ObjectId> = None;
        let mut entity_clicked: Option<map_domain::world::EntityId> = None;
        let mut back_to_parent_clicked = false;

        // Reserve space at the bottom for the "Open 3D View" button so it stays visible.
        let scroll_height = (ui.available_height() - 44.0).max(80.0);
        let scroll_resp = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(scroll_height)
            .show(ui, |ui| {
                if let ViewMode::SectorView {
                    selected_obj,
                    selected_entity,
                    ..
                } = view_mode
                {
                    // ─── Static objects ──────────────────────────────────
                    egui::CollapsingHeader::new(format!(
                        "STATIC OBJECTS ({})",
                        sector.static_objects.len()
                    ))
                    .default_open(true)
                    .show(ui, |ui| {
                        for obj in &sector.static_objects {
                            let is_sel = *selected_obj == Some(obj.id);
                            let label = format!("{} {}", kind_icon(&obj.kind), &obj.name);
                            let color = if is_sel {
                                theme::ACCENT
                            } else {
                                theme::TEXT_PRIMARY
                            };
                            if ui.colored_label(color, &label).clicked() {
                                object_clicked = Some(obj.id);
                            }
                        }
                    });

                    // ─── Live entities, grouped ──────────────────────────
                    if let Some(world) = world {
                        use map_domain::world::LiveObjectKind;
                        let mut by_group: std::collections::HashMap<
                            &'static str,
                            Vec<map_domain::world::EntityId>,
                        > = std::collections::HashMap::new();
                        for &eid in world.entities_in_sector(sector.id) {
                            if world.parent_of(eid).is_some() {
                                continue;
                            }
                            let bucket = match world.kinds.get(&eid) {
                                Some(LiveObjectKind::Station) => "STATIONS",
                                Some(LiveObjectKind::ShipExtraLarge)
                                | Some(LiveObjectKind::ShipLarge) => "CAPITALS",
                                Some(LiveObjectKind::ShipMedium) => "MEDIUM",
                                Some(LiveObjectKind::ShipSmall) => "SMALL",
                                None => continue,
                            };
                            by_group.entry(bucket).or_default().push(eid);
                        }
                        for &group in &["STATIONS", "CAPITALS", "MEDIUM", "SMALL"] {
                            if let Some(eids) = by_group.get(group) {
                                egui::CollapsingHeader::new(format!("{} ({})", group, eids.len()))
                                    .default_open(group == "STATIONS")
                                    .show(ui, |ui| {
                                        for &eid in eids {
                                            let is_sel = *selected_entity == Some(eid);
                                            let (label, icon) = entity_row_label(world, eid);
                                            let color = if is_sel {
                                                theme::ACCENT
                                            } else {
                                                theme::TEXT_PRIMARY
                                            };
                                            let row = format!("{} {}", icon, label);
                                            if ui.colored_label(color, &row).clicked() {
                                                entity_clicked = Some(eid);
                                            }
                                            // Faction line under the row.
                                            if let Some(&fid) = world.factions.get(&eid) {
                                                let f_name =
                                                    crate::colors::faction_name(universe, fid);
                                                let f_color =
                                                    crate::colors::faction_color(universe, fid);
                                                ui.horizontal(|ui| {
                                                    ui.add_space(20.0);
                                                    ui.colored_label(f_color, "●");
                                                    ui.colored_label(theme::TEXT_MUTED, f_name);
                                                });
                                            }
                                        }
                                    });
                            }
                        }
                    }

                    // ─── SELECTED detail ─────────────────────────────────
                    if let Some(eid) = *selected_entity {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.colored_label(theme::TEXT_MUTED, "SELECTED");
                        if let Some(parent) = world.and_then(|w| w.parent_of(eid)) {
                            let parent_label = world
                                .map(|w| entity_row_label(w, parent).0)
                                .unwrap_or_default();
                            if ui.button(format!("← Back to {}", parent_label)).clicked() {
                                back_to_parent_clicked = true;
                            }
                        }
                        if let Some(world) = world {
                            let (label, icon) = entity_row_label(world, eid);
                            ui.colored_label(theme::ACCENT, format!("{} {}", icon, label));
                            if let Some(kind) = world.kinds.get(&eid) {
                                ui.colored_label(theme::TEXT_MUTED, format!("Type: {:?}", kind));
                            }
                            if let Some(&fid) = world.factions.get(&eid) {
                                let f_color = crate::colors::faction_color(universe, fid);
                                let f_name = crate::colors::faction_name(universe, fid);
                                ui.horizontal(|ui| {
                                    ui.colored_label(f_color, "●");
                                    ui.colored_label(theme::TEXT_MUTED, f_name);
                                });
                            }
                            if let Some(&pos) = world.positions.get(&eid) {
                                ui.colored_label(
                                    theme::TEXT_MUTED,
                                    format!("Pos: x {:.1} y {:.1} z {:.1} km", pos.x, pos.y, pos.z),
                                );
                            }
                            if matches!(
                                world.kinds.get(&eid),
                                Some(map_domain::world::LiveObjectKind::Station)
                            ) {
                                let offers = world.trade_offers_of(eid);
                                if !offers.is_empty() {
                                    ui.add_space(6.0);
                                    ui.colored_label(theme::TEXT_MUTED, "TRADE");
                                    render_trade_section(ui, offers, universe);
                                }
                            }
                            let kids = world.children_of(eid);
                            if !kids.is_empty() {
                                egui::CollapsingHeader::new(format!("DOCKED ({})", kids.len()))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        for &cid in kids {
                                            let (clabel, cicon) = entity_row_label(world, cid);
                                            if ui
                                                .colored_label(
                                                    theme::TEXT_PRIMARY,
                                                    format!("{} {}", cicon, clabel),
                                                )
                                                .clicked()
                                            {
                                                entity_clicked = Some(cid);
                                            }
                                        }
                                    });
                            }
                        }
                    } else if let Some(obj) = selected_obj
                        .and_then(|id| sector.static_objects.iter().find(|o| o.id == id))
                    {
                        // Existing static-object detail block.
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);
                        ui.colored_label(theme::TEXT_MUTED, "SELECTED");
                        ui.add_space(2.0);
                        ui.colored_label(theme::ACCENT, &obj.name);
                        ui.colored_label(
                            theme::TEXT_MUTED,
                            format!("Type: {}", kind_label(&obj.kind)),
                        );
                        ui.colored_label(
                            theme::TEXT_MUTED,
                            format!(
                                "x {:.1}  y {:.1}  z {:.1} km",
                                obj.position.x, obj.position.y, obj.position.z
                            ),
                        );
                        if let Some(f) = obj.faction {
                            let f_name = crate::colors::faction_name(universe, f);
                            ui.colored_label(theme::TEXT_MUTED, format!("Faction: {}", f_name));
                        }
                        if let Some((pitch, yaw, roll)) = obj.rotation {
                            ui.colored_label(
                                theme::TEXT_MUTED,
                                format!("pitch {:.1}°  yaw {:.1}°  roll {:.1}°", pitch, yaw, roll),
                            );
                        }
                        for (k, v) in &obj.details {
                            ui.colored_label(theme::TEXT_MUTED, format!("{}: {}", k, v));
                        }
                    }
                } else {
                    // UniverseMap branch — CONNECTIONS list.
                    ui.colored_label(theme::TEXT_MUTED, "CONNECTIONS");
                    ui.add_space(4.0);
                    let neighbours = universe.neighbour_ids(sector.id);
                    let conns = universe.connections_for(sector.id);
                    if neighbours.is_empty() {
                        ui.colored_label(theme::TEXT_MUTED, "None");
                    }
                    for nb_id in &neighbours {
                        if let Some(nb) = universe.sector(*nb_id) {
                            let gate_type = conns
                                .iter()
                                .find(|c| c.from == *nb_id || c.to == *nb_id)
                                .map(|c| &c.gate_type);
                            let prefix = match gate_type {
                                Some(GateType::Superhighway) => "⇒",
                                _ => "→",
                            };
                            ui.colored_label(
                                theme::TEXT_PRIMARY,
                                format!("{} {}", prefix, nb.name),
                            );
                        }
                    }
                }
            });
        let _ = scroll_resp;

        ui.add_space(12.0);
        let open_clicked = ui.button("▣  Open 3D View").clicked();

        SectorPanelResponse {
            open_3d_clicked: open_clicked,
            back_to_map_clicked: back_clicked,
            object_clicked,
            entity_clicked,
            back_to_parent_clicked,
        }
    }
}

fn entity_row_label(
    world: &map_domain::world::World,
    eid: map_domain::world::EntityId,
) -> (String, &'static str) {
    use map_domain::world::LiveObjectKind;
    let icon = match world.kinds.get(&eid) {
        Some(LiveObjectKind::Station) => "◼",
        Some(LiveObjectKind::ShipExtraLarge) | Some(LiveObjectKind::ShipLarge) => "▲",
        Some(LiveObjectKind::ShipMedium) => "▶",
        _ => "▴",
    };
    let code = world.codes.get(&eid).cloned();
    let macro_name = world.names.get(&eid).cloned().unwrap_or_default();
    let human = crate::colors::strip_macro(&macro_name);

    let label = match (code, &human) {
        (Some(c), h) if !h.is_empty() && h != &c => format!("{} — {}", c, h),
        (Some(c), _) => c,
        (None, h) if !h.is_empty() => h.clone(),
        _ => macro_name,
    };
    (label, icon)
}

fn kind_icon(kind: &map_domain::objects::StaticObjectKind) -> &'static str {
    use map_domain::objects::StaticObjectKind::*;
    match kind {
        Station => "◼",
        Gate => "◯",
        ResourceZone => "◎",
        Anomaly => "✦",
        Highway => "⇒",
    }
}

fn kind_label(kind: &map_domain::objects::StaticObjectKind) -> &'static str {
    use map_domain::objects::StaticObjectKind::*;
    match kind {
        Station => "Station",
        Gate => "Gate",
        ResourceZone => "Resource zone",
        Anomaly => "Anomaly",
        Highway => "Highway",
    }
}

fn fmt_thousands(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "" };
    let abs = n.unsigned_abs().to_string();
    let bytes = abs.as_bytes();
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i != 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    format!("{sign}{out}")
}

fn render_trade_section(
    ui: &mut egui::Ui,
    offers: &[map_domain::world::TradeOffer],
    universe: &map_domain::universe::Universe,
) {
    use map_domain::world::TradeDirection;

    let mut buys: Vec<&map_domain::world::TradeOffer> = offers
        .iter()
        .filter(|o| o.direction == TradeDirection::Buy)
        .collect();
    let mut sells: Vec<&map_domain::world::TradeOffer> = offers
        .iter()
        .filter(|o| o.direction == TradeDirection::Sell)
        .collect();
    let name_for = |o: &map_domain::world::TradeOffer| -> String {
        universe
            .ware_names
            .get(&o.ware_id)
            .cloned()
            .unwrap_or_else(|| o.ware_id.clone())
    };
    buys.sort_by_cached_key(|o| name_for(o));
    sells.sort_by_cached_key(|o| name_for(o));

    render_offer_group(ui, "BUYS", &buys, &name_for);
    render_offer_group(ui, "SELLS", &sells, &name_for);
}

fn render_offer_group(
    ui: &mut egui::Ui,
    label: &str,
    offers: &[&map_domain::world::TradeOffer],
    name_for: &dyn Fn(&map_domain::world::TradeOffer) -> String,
) {
    if offers.is_empty() {
        return;
    }
    egui::CollapsingHeader::new(format!("{} ({})", label, offers.len()))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new(format!("trade-grid-{}", label))
                .num_columns(3)
                .spacing([10.0, 2.0])
                .show(ui, |ui| {
                    for o in offers {
                        ui.colored_label(theme::TEXT_PRIMARY, name_for(o));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.colored_label(
                                theme::TEXT_MUTED,
                                format!("{} Cr", fmt_thousands(o.price)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.colored_label(
                                theme::TEXT_MUTED,
                                format!(
                                    "{} / {}",
                                    fmt_thousands(o.amount),
                                    fmt_thousands(o.desired),
                                ),
                            );
                        });
                        ui.end_row();
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_handles_no_selection() {
        let _panel = SectorPanel::default();
    }

    #[test]
    fn strip_macro_removes_suffix_and_underscores() {
        assert_eq!(
            crate::colors::strip_macro("cluster_709_sector001_macro"),
            "cluster 709 sector001"
        );
        assert_eq!(
            crate::colors::strip_macro("argon_prime_macro"),
            "argon prime"
        );
        assert_eq!(crate::colors::strip_macro("no_suffix"), "no suffix");
    }

    #[test]
    fn fmt_thousands_formats_with_separators() {
        assert_eq!(fmt_thousands(0), "0");
        assert_eq!(fmt_thousands(7), "7");
        assert_eq!(fmt_thousands(100), "100");
        assert_eq!(fmt_thousands(1_000), "1,000");
        assert_eq!(fmt_thousands(1_234), "1,234");
        assert_eq!(fmt_thousands(1_234_567), "1,234,567");
        assert_eq!(fmt_thousands(-1_000), "-1,000");
        assert_eq!(
            fmt_thousands(i64::MIN),
            format!("-{}", {
                // The function uses unsigned_abs(), so this should not panic.
                // i64::MIN.unsigned_abs() == 9_223_372_036_854_775_808
                "9,223,372,036,854,775,808"
            })
        );
    }
}
