pub struct TopBar;

impl TopBar {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.label("FOUNDATIONS MAP");
    }
}
