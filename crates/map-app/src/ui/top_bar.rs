use map_domain::world::SnapshotMeta;

pub struct TopBar {
    pub search_text: String,
}

impl Default for TopBar {
    fn default() -> Self {
        Self { search_text: String::new() }
    }
}

#[derive(Default)]
pub struct TopBarResponse {
    pub refresh_clicked: bool,
}

impl TopBar {
    pub fn show(&mut self, ui: &mut egui::Ui, snapshot: Option<&SnapshotMeta>) -> TopBarResponse {
        let mut resp = TopBarResponse::default();
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.colored_label(crate::theme::ACCENT, "FOUNDATIONS MAP");
            ui.add_space(16.0);
            let search = egui::TextEdit::singleline(&mut self.search_text)
                .hint_text("⌕ Search sectors, stations, ships...")
                .desired_width(300.0);
            ui.add(search);

            ui.add_space(8.0);
            if ui.button("Refresh").clicked() {
                resp.refresh_clicked = true;
            }

            ui.add_space(16.0);
            if let Some(meta) = snapshot {
                ui.label(format!(
                    "Snapshot: {}  •  ${}  •  game t={:.0}s",
                    format_age(meta.mtime),
                    format_money(meta.player_money),
                    meta.game_time_seconds,
                ));
            } else {
                ui.colored_label(crate::theme::TEXT_MUTED, "No save loaded yet");
            }
        });
        resp
    }
}

fn format_age(mtime: std::time::SystemTime) -> String {
    let elapsed = std::time::SystemTime::now()
        .duration_since(mtime)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match elapsed {
        0..=4         => "just now".to_string(),
        5..=59        => format!("{}s ago", elapsed),
        60..=3599     => format!("{}m ago", elapsed / 60),
        3600..=86399  => format!("{}h ago", elapsed / 3600),
        _             => format!("{}d ago", elapsed / 86400),
    }
}

/// Integer thousands separator with commas. No extra crates.
fn format_money(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(b as char);
    }
    out
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

    #[test]
    fn format_money_inserts_commas() {
        assert_eq!(format_money(0), "0");
        assert_eq!(format_money(42), "42");
        assert_eq!(format_money(999), "999");
        assert_eq!(format_money(1_000), "1,000");
        assert_eq!(format_money(40_000), "40,000");
        assert_eq!(format_money(1_234_567), "1,234,567");
        assert_eq!(format_money(1_000_000_000), "1,000,000,000");
    }

    #[test]
    fn format_age_buckets() {
        use std::time::{Duration, SystemTime};
        let now = SystemTime::now();
        assert_eq!(format_age(now), "just now");
        assert_eq!(format_age(now - Duration::from_secs(30)), "30s ago");
        assert_eq!(format_age(now - Duration::from_secs(120)), "2m ago");
        assert_eq!(format_age(now - Duration::from_secs(2 * 3600)), "2h ago");
        assert_eq!(format_age(now - Duration::from_secs(3 * 86400)), "3d ago");
    }
}
