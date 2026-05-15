pub struct TopBar {
    pub search_text: String,
}

impl Default for TopBar {
    fn default() -> Self {
        Self { search_text: String::new() }
    }
}

impl TopBar {
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.colored_label(crate::theme::ACCENT, "FOUNDATIONS MAP");
            ui.add_space(16.0);
            let search = egui::TextEdit::singleline(&mut self.search_text)
                .hint_text("⌕ Search sectors, stations, ships...")
                .desired_width(300.0);
            ui.add(search);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_search_text_is_empty() {
        let bar = TopBar::default();
        assert!(bar.search_text.is_empty());
    }

    #[test]
    fn search_text_can_be_set() {
        let mut bar = TopBar::default();
        bar.search_text = "argon".into();
        assert_eq!(bar.search_text, "argon");
    }
}
