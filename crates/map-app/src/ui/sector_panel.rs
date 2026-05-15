use map_domain::ids::ObjectId;
use map_domain::universe::{Sector, Universe, GateType};
use map_domain::view::ViewMode;
use crate::theme;

pub struct SectorPanelResponse {
    pub open_3d_clicked:     bool,
    pub back_to_map_clicked: bool,
    pub object_clicked:      Option<ObjectId>,
}

#[derive(Default)]
pub struct SectorPanel;

impl SectorPanel {
    pub fn show(
        &mut self,
        ui:        &mut egui::Ui,
        sector:    Option<&Sector>,
        universe:  &Universe,
        view_mode: &ViewMode,
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
            };
        };

        let back_clicked = ui.small_button("← Universe").clicked();
        ui.add_space(4.0);

        ui.colored_label(theme::TEXT_MUTED, "SECTOR");
        ui.add_space(2.0);
        ui.colored_label(theme::TEXT_PRIMARY, &sector.name);
        if let Some(faction_id) = sector.faction {
            ui.colored_label(theme::ACCENT, format!("Faction #{}", faction_id.0));
        }

        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);

        let mut object_clicked = None;

        if let ViewMode::SectorView { selected_obj, .. } = view_mode {
            ui.colored_label(theme::TEXT_MUTED, "OBJECTS");
            ui.add_space(4.0);
            if sector.static_objects.is_empty() {
                ui.colored_label(theme::TEXT_MUTED, "None loaded");
            }
            for obj in &sector.static_objects {
                let is_sel = *selected_obj == Some(obj.id);
                let label = format!("{} {}", kind_icon(&obj.kind), &obj.name);
                let color = if is_sel { theme::ACCENT } else { theme::TEXT_PRIMARY };
                if ui.colored_label(color, &label).clicked() {
                    object_clicked = Some(obj.id);
                }
            }
        } else {
            ui.colored_label(theme::TEXT_MUTED, "CONNECTIONS");
            ui.add_space(4.0);
            let neighbours = universe.neighbour_ids(sector.id);
            let conns = universe.connections_for(sector.id);
            if neighbours.is_empty() {
                ui.colored_label(theme::TEXT_MUTED, "None");
            }
            for nb_id in &neighbours {
                if let Some(nb) = universe.sector(*nb_id) {
                    let gate_type = conns.iter()
                        .find(|c| c.from == *nb_id || c.to == *nb_id)
                        .map(|c| &c.gate_type);
                    let prefix = match gate_type {
                        Some(GateType::Superhighway) => "⇒",
                        _ => "→",
                    };
                    ui.colored_label(theme::TEXT_PRIMARY, format!("{} {}", prefix, nb.name));
                }
            }
        }

        ui.add_space(12.0);
        let open_clicked = ui.button("▣  Open 3D View").clicked();

        SectorPanelResponse {
            open_3d_clicked: open_clicked,
            back_to_map_clicked: back_clicked,
            object_clicked,
        }
    }
}

fn kind_icon(kind: &map_domain::objects::StaticObjectKind) -> &'static str {
    use map_domain::objects::StaticObjectKind::*;
    match kind {
        Station      => "◼",
        Gate         => "◯",
        ResourceZone => "◎",
        Anomaly      => "✦",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_handles_no_selection() {
        let _panel = SectorPanel::default();
    }
}
