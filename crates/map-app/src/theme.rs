use egui::{Color32, CornerRadius, Stroke, Style, Visuals};

pub const BG_DARK: Color32 = Color32::from_rgb(10, 12, 18);
pub const BG_PANEL: Color32 = Color32::from_rgb(20, 23, 33);
pub const BG_WIDGET: Color32 = Color32::from_rgb(30, 34, 53);
pub const ACCENT: Color32 = Color32::from_rgb(124, 58, 237); // purple
pub const ACCENT_DIM: Color32 = Color32::from_rgb(58, 63, 90);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(224, 228, 240);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(122, 138, 180);
pub const BORDER: Color32 = Color32::from_rgb(42, 45, 61);
pub const GATE_GREEN: Color32 = Color32::from_rgb(42, 170, 106);
#[allow(dead_code)]
pub const SHIP_YELLOW: Color32 = Color32::from_rgb(244, 180, 74);
#[allow(dead_code)]
pub const HOSTILE_RED: Color32 = Color32::from_rgb(239, 68, 68);

pub fn apply(ctx: &egui::Context) {
    let mut style = Style::default();
    style.visuals = dark_visuals();
    ctx.set_global_style(style);
}

fn dark_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.faint_bg_color = BG_DARK;
    v.extreme_bg_color = BG_DARK;
    v.widgets.noninteractive.bg_fill = BG_WIDGET;
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_MUTED);
    v.widgets.inactive.bg_fill = BG_WIDGET;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.hovered.bg_fill = ACCENT_DIM;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT_PRIMARY);
    v.selection.bg_fill = Color32::from_rgba_premultiplied(124, 58, 237, 40);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.window_corner_radius = CornerRadius::same(4);
    v.window_stroke = Stroke::new(1.0, BORDER);
    v
}
