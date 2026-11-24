//! Visual language: ink on paper. Strictly monochrome, two themes.
//!
//! The palette is deliberately small and untrendy: no brand colour, no
//! gradients, no glow. Just ink, paper and a handful of greys, with slightly
//! irregular geometry so the interface reads like a tool rather than a
//! generated dashboard.

use eframe::egui::style::WidgetVisuals;
use eframe::egui::{Color32, Rounding, Shadow, Stroke, Theme, vec2};

#[derive(Clone, Copy)]
pub struct Palette {
    pub stage: Color32,
    pub panel: Color32,
    pub panel_alt: Color32,
    pub paper: Color32,
    pub ink: Color32,
    pub ink_dim: Color32,
    pub ink_soft: Color32,
    pub hairline: Color32,
    pub hairline_strong: Color32,
    pub grid_minor: Color32,
    pub grid_major: Color32,
    pub axis: Color32,
    pub extreme_bg: Color32,
    pub code_bg: Color32,
    pub widget_inactive: Color32,
    pub widget_hover: Color32,
    pub active_fg: Color32,
    pub shadow: Color32,
}

const DARK: Palette = Palette {
    stage: Color32::from_rgb(13, 13, 13),
    panel: Color32::from_rgb(22, 22, 22),
    panel_alt: Color32::from_rgb(29, 29, 29),
    paper: Color32::from_rgb(17, 17, 17),
    ink: Color32::from_rgb(232, 232, 230),
    ink_dim: Color32::from_rgb(156, 156, 153),
    ink_soft: Color32::from_rgb(67, 67, 65),
    hairline: Color32::from_rgba_premultiplied(16, 16, 16, 16),
    hairline_strong: Color32::from_rgba_premultiplied(34, 34, 34, 34),
    grid_minor: Color32::from_rgba_premultiplied(11, 11, 11, 11),
    grid_major: Color32::from_rgba_premultiplied(24, 24, 24, 24),
    axis: Color32::from_rgb(123, 123, 120),
    extreme_bg: Color32::from_rgb(10, 10, 10),
    code_bg: Color32::from_rgb(26, 26, 26),
    widget_inactive: Color32::from_rgb(39, 39, 39),
    widget_hover: Color32::from_rgb(53, 53, 53),
    active_fg: Color32::from_rgb(13, 13, 13),
    shadow: Color32::from_black_alpha(70),
};

const LIGHT: Palette = Palette {
    stage: Color32::from_rgb(226, 226, 222),
    panel: Color32::from_rgb(243, 243, 240),
    panel_alt: Color32::from_rgb(250, 250, 247),
    paper: Color32::from_rgb(255, 255, 253),
    ink: Color32::from_rgb(26, 26, 25),
    ink_dim: Color32::from_rgb(94, 94, 91),
    ink_soft: Color32::from_rgb(174, 174, 169),
    hairline: Color32::from_rgba_premultiplied(0, 0, 0, 22),
    hairline_strong: Color32::from_rgba_premultiplied(0, 0, 0, 44),
    grid_minor: Color32::from_rgba_premultiplied(0, 0, 0, 14),
    grid_major: Color32::from_rgba_premultiplied(0, 0, 0, 30),
    axis: Color32::from_rgb(108, 108, 104),
    extreme_bg: Color32::from_rgb(255, 255, 255),
    code_bg: Color32::from_rgb(236, 236, 231),
    widget_inactive: Color32::from_rgb(229, 229, 225),
    widget_hover: Color32::from_rgb(214, 214, 209),
    active_fg: Color32::from_rgb(255, 255, 255),
    shadow: Color32::from_black_alpha(38),
};

pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Dark => DARK,
        Theme::Light => LIGHT,
    }
}

fn style_for(theme: Theme) -> eframe::egui::Style {
    let p = palette(theme);
    let mut style = eframe::egui::Style::default();
    let visuals = &mut style.visuals;

    visuals.dark_mode = matches!(theme, Theme::Dark);
    visuals.override_text_color = Some(p.ink);
    visuals.panel_fill = p.stage;
    visuals.window_fill = p.panel_alt;
    visuals.window_stroke = Stroke::new(1.0_f32, p.hairline_strong);
    visuals.window_rounding = Rounding::same(6.0);
    visuals.menu_rounding = Rounding::same(5.0);
    visuals.window_shadow = Shadow {
        offset: vec2(0.0, 6.0),
        blur: 14.0,
        spread: -2.0,
        color: p.shadow,
    };
    visuals.popup_shadow = visuals.window_shadow;
    visuals.extreme_bg_color = p.extreme_bg;
    visuals.faint_bg_color = p.panel;
    visuals.code_bg_color = p.code_bg;
    visuals.hyperlink_color = p.ink;
    visuals.selection.bg_fill = alpha(p.ink, 0.22);
    visuals.selection.stroke = Stroke::new(1.0_f32, p.ink_dim);
    // Warnings stay monochrome too; hierarchy comes from weight and labels.
    visuals.warn_fg_color = p.ink;
    visuals.error_fg_color = p.ink;
    visuals.button_frame = true;
    visuals.slider_trailing_fill = true;

    fn widget(fg: Color32, bg: Color32, round: f32) -> WidgetVisuals {
        WidgetVisuals {
            bg_fill: bg,
            weak_bg_fill: bg,
            bg_stroke: Stroke::NONE,
            rounding: Rounding::same(round),
            fg_stroke: Stroke::new(1.0_f32, fg),
            expansion: 0.0,
        }
    }

    visuals.widgets.noninteractive = widget(p.ink_dim, p.panel_alt, 4.0);
    visuals.widgets.inactive = widget(p.ink, p.widget_inactive, 5.0);
    visuals.widgets.hovered = widget(p.ink, p.widget_hover, 5.0);
    visuals.widgets.active = widget(p.active_fg, p.ink, 5.0);
    visuals.widgets.open = widget(p.ink, p.ink_soft, 5.0);

    style.spacing.item_spacing = vec2(7.0, 7.0);
    style.spacing.button_padding = vec2(10.0, 6.0);
    style.spacing.slider_width = 120.0;
    style.spacing.window_margin = eframe::egui::Margin::same(14.0);
    let mut scroll = eframe::egui::style::ScrollStyle::solid();
    scroll.bar_width = 7.0;
    style.spacing.scroll = scroll;

    style.text_styles.insert(
        eframe::egui::TextStyle::Heading,
        eframe::egui::FontId::proportional(18.0),
    );
    style.text_styles.insert(
        eframe::egui::TextStyle::Button,
        eframe::egui::FontId::proportional(13.0),
    );
    style.text_styles.insert(
        eframe::egui::TextStyle::Body,
        eframe::egui::FontId::proportional(13.0),
    );
    style.text_styles.insert(
        eframe::egui::TextStyle::Small,
        eframe::egui::FontId::proportional(11.0),
    );
    style.text_styles.insert(
        eframe::egui::TextStyle::Monospace,
        eframe::egui::FontId::monospace(12.5),
    );
    style
}

pub fn install(ctx: &eframe::egui::Context) {
    ctx.set_style_of(Theme::Dark, style_for(Theme::Dark));
    ctx.set_style_of(Theme::Light, style_for(Theme::Light));
    ctx.set_theme(eframe::egui::ThemePreference::System);
    sync(ctx);

    // Canvas zoom is handled manually so Ctrl+scroll never changes global UI scale.
    ctx.options_mut(|options| options.scroll_zoom_speed = 0.0);
}

/// Mirror the active egui theme into a cheap atomic so painting code can
/// ask for colours without threading a Context through every helper.
static DARK_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn sync(ctx: &eframe::egui::Context) {
    DARK_ACTIVE.store(
        matches!(ctx.theme(), Theme::Dark),
        std::sync::atomic::Ordering::Relaxed,
    );
}

fn current() -> Palette {
    if DARK_ACTIVE.load(std::sync::atomic::Ordering::Relaxed) {
        DARK
    } else {
        LIGHT
    }
}

pub fn stage() -> Color32 {
    current().stage
}
pub fn panel() -> Color32 {
    current().panel
}
pub fn panel_alt() -> Color32 {
    current().panel_alt
}
pub fn paper() -> Color32 {
    current().paper
}
pub fn ink() -> Color32 {
    current().ink
}
pub fn ink_dim() -> Color32 {
    current().ink_dim
}
pub fn ink_soft() -> Color32 {
    current().ink_soft
}
pub fn hairline() -> Color32 {
    current().hairline
}
pub fn hairline_strong() -> Color32 {
    current().hairline_strong
}
pub fn grid_minor() -> Color32 {
    current().grid_minor
}
pub fn grid_major() -> Color32 {
    current().grid_major
}
pub fn axis() -> Color32 {
    current().axis
}

/// Fade a premultiplied colour toward transparent, for animated highlights.
pub fn alpha(color: Color32, factor: f32) -> Color32 {
    Color32::from_rgba_premultiplied(
        (color.r() as f32 * factor).round() as u8,
        (color.g() as f32 * factor).round() as u8,
        (color.b() as f32 * factor).round() as u8,
        (color.a() as f32 * factor).clamp(0.0, 255.0).round() as u8,
    )
}
