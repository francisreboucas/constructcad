//! Visual theme: color tokens, typography and spacing applied to egui.
//!
//! Colors are defined once in [`Palette`] and every panel reads them from
//! [`PALETTE`] instead of hard-coding values, so the look stays consistent.
//! [`LIGHT`] and [`DARK`] hold the tokens of the two themes; `PALETTE` reads
//! the one the interface shows ([`sync`] follows the egui theme each frame),
//! and [`ThemeChoice`] is the user's preference: light, dark or the system's.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText, Shadow, Stroke};
use egui::{TextStyle, Visuals};
use std::borrow::Cow;
use std::cell::Cell;
use std::sync::Arc;

/// Color tokens of a theme.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Application background behind panels (ribbon strip, dock gaps).
    pub app_bg: Color32,
    /// Side panels, ribbon and popups.
    pub panel: Color32,
    /// Section headers and subtle fills inside panels.
    pub surface: Color32,
    /// Background of view tabs and the home screen.
    pub canvas: Color32,
    /// Paper under 2D drawings: white in both themes, since drawings are
    /// dark lines and text.
    pub paper: Color32,
    /// Area around a sheet.
    pub sheet_bg: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_text: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub danger: Color32,
    pub warning: Color32,
    /// Positive actions: badges of commands that create something.
    pub success: Color32,
    /// Border of a view with temporary hide/isolate.
    pub temporary: Color32,
    /// Plain interface icons (strokes).
    pub icon_ink: Color32,
    /// Second tone of two-tone icons (their soft fills).
    pub icon_tint: Color32,
    /// Whether this is a dark theme (egui base visuals).
    pub dark: bool,
}

/// Color tokens of the light theme.
pub const LIGHT: Palette = Palette {
    app_bg: Color32::from_rgb(0xE9, 0xEB, 0xEF),
    panel: Color32::from_rgb(0xF7, 0xF8, 0xFA),
    surface: Color32::from_rgb(0xEF, 0xF1, 0xF4),
    canvas: Color32::WHITE,
    paper: Color32::WHITE,
    sheet_bg: Color32::from_rgb(0xC9, 0xCD, 0xD3),
    border: Color32::from_rgb(0xD9, 0xDC, 0xE1),
    border_strong: Color32::from_rgb(0xB8, 0xBE, 0xC6),
    text: Color32::from_rgb(0x1F, 0x23, 0x28),
    text_muted: Color32::from_rgb(0x5B, 0x63, 0x6E),
    text_faint: Color32::from_rgb(0x8A, 0x92, 0x9C),
    accent: Color32::from_rgb(0x1F, 0x6F, 0xEB),
    accent_soft: Color32::from_rgb(0xDC, 0xE9, 0xFD),
    accent_text: Color32::from_rgb(0x12, 0x4F, 0xB8),
    hover: Color32::from_rgb(0xE6, 0xE9, 0xED),
    pressed: Color32::from_rgb(0xD6, 0xDB, 0xE1),
    danger: Color32::from_rgb(0xC9, 0x2A, 0x2A),
    warning: Color32::from_rgb(0xB2, 0x6B, 0x00),
    success: Color32::from_rgb(0x1E, 0x8A, 0x4C),
    temporary: Color32::from_rgb(0x0E, 0x8C, 0x8F),
    icon_ink: Color32::from_rgb(0x1F, 0x23, 0x28),
    icon_tint: Color32::from_rgb(0x1F, 0x6F, 0xEB),
    dark: false,
};

/// Color tokens of the dark theme.
pub const DARK: Palette = Palette {
    app_bg: Color32::from_rgb(0x17, 0x19, 0x1D),
    panel: Color32::from_rgb(0x21, 0x24, 0x29),
    surface: Color32::from_rgb(0x2A, 0x2E, 0x34),
    canvas: Color32::from_rgb(0x1C, 0x1F, 0x23),
    paper: Color32::WHITE,
    sheet_bg: Color32::from_rgb(0x36, 0x3B, 0x42),
    border: Color32::from_rgb(0x38, 0x3D, 0x44),
    border_strong: Color32::from_rgb(0x4C, 0x53, 0x5C),
    text: Color32::from_rgb(0xE6, 0xE8, 0xEB),
    text_muted: Color32::from_rgb(0xAE, 0xB4, 0xBC),
    text_faint: Color32::from_rgb(0x80, 0x87, 0x90),
    accent: Color32::from_rgb(0x4C, 0x8D, 0xF6),
    accent_soft: Color32::from_rgb(0x1F, 0x35, 0x57),
    accent_text: Color32::from_rgb(0x8C, 0xB8, 0xFF),
    hover: Color32::from_rgb(0x2F, 0x33, 0x39),
    pressed: Color32::from_rgb(0x3A, 0x3F, 0x46),
    danger: Color32::from_rgb(0xF0, 0x6A, 0x6A),
    warning: Color32::from_rgb(0xE0, 0xA4, 0x3A),
    success: Color32::from_rgb(0x4C, 0xC3, 0x8A),
    temporary: Color32::from_rgb(0x3C, 0xC4, 0xC6),
    icon_ink: Color32::from_rgb(0xE6, 0xE8, 0xEB),
    icon_tint: Color32::from_rgb(0x6A, 0xA2, 0xFA),
    dark: true,
};

/// The palette of the theme `ctx` shows.
pub fn palette(ctx: &egui::Context) -> &'static Palette {
    if ctx.theme() == egui::Theme::Dark { &DARK } else { &LIGHT }
}

thread_local! {
    /// Whether the interface painted on this thread shows the dark theme.
    static DARK_SHOWN: Cell<bool> = const { Cell::new(false) };
    /// Multiplier of the interface text sizes (the font size preference).
    static FONT_SCALE: Cell<f32> = const { Cell::new(1.0) };
}

/// Smallest and largest font size multipliers.
pub const FONT_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.8..=1.5;

/// The multiplier of the interface text sizes (see [`set_font_scale`]).
pub fn font_scale() -> f32 {
    FONT_SCALE.with(Cell::get)
}

/// Make the interface text `scale` times its normal size (independent of
/// the interface zoom): the text styles of both themes and the property
/// tables follow it. Out-of-range values are clamped.
pub fn set_font_scale(ctx: &egui::Context, scale: f32) {
    let (lo, hi) = (*FONT_SCALE_RANGE.start(), *FONT_SCALE_RANGE.end());
    let scale = if scale.is_finite() { scale.clamp(lo, hi) } else { 1.0 };
    FONT_SCALE.with(|f| f.set(scale));
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(theme, |style| style.text_styles = text_styles(scale));
    }
}

/// The palette of the theme shown: read like a constant (`PALETTE.text`).
#[derive(Debug, Clone, Copy)]
pub struct ShownPalette;

impl std::ops::Deref for ShownPalette {
    type Target = Palette;
    fn deref(&self) -> &Palette {
        if DARK_SHOWN.with(Cell::get) { &DARK } else { &LIGHT }
    }
}

/// The colors of the theme the interface shows (see [`sync`]).
pub static PALETTE: ShownPalette = ShownPalette;

/// Make [`PALETTE`] follow the theme `ctx` shows; call it at the start of
/// every frame (the system theme may change at any time).
pub fn sync(ctx: &egui::Context) {
    DARK_SHOWN.with(|d| d.set(ctx.theme() == egui::Theme::Dark));
}

/// The user's theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeChoice {
    /// Follow the operating system.
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    /// Stored form of the preference.
    pub fn code(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.code() == code)
    }

    pub fn label(self) -> String {
        match self {
            ThemeChoice::System => crate::t!("theme-system"),
            ThemeChoice::Light => crate::t!("theme-light"),
            ThemeChoice::Dark => crate::t!("theme-dark"),
        }
    }

    fn preference(self) -> egui::ThemePreference {
        match self {
            ThemeChoice::System => egui::ThemePreference::System,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
        }
    }
}

/// Show the theme `choice` asks for.
pub fn set_theme(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(choice.preference());
    sync(ctx);
}

/// The application icon (256 px PNG).
pub const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/icons/app-icon-256.png");

/// Texture of the application icon, loaded by [`apply`] for the ribbon.
pub fn logo(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    ctx.data(|d| d.get_temp(egui::Id::new("app-logo")))
}

fn load_logo(ctx: &egui::Context) {
    let Ok(icon) = eframe::icon_data::from_png_bytes(APP_ICON_PNG) else { return };
    let size = [icon.width as usize, icon.height as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &icon.rgba);
    let options = egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear));
    let texture = ctx.load_texture("app-logo", image, options);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("app-logo"), texture));
}

/// Corner radius of buttons, fields and popups.
pub const RADIUS: u8 = 5;

/// Font family of the semibold weight, for titles and emphasis.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Section or panel title text.
pub fn title(text: impl Into<String>) -> RichText {
    RichText::new(text).family(semibold()).size(13.0).color(PALETTE.text)
}

/// Full-width header strip at the top of a side panel.
pub fn panel_header(ui: &mut egui::Ui, title_text: &str, subtitle: Option<&str>) {
    let response = egui::Frame::new().fill(PALETTE.surface).inner_margin(Margin::symmetric(12, 7)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(title(title_text));
            if let Some(sub) = subtitle.filter(|s| !s.is_empty()) {
                ui.label(RichText::new(sub).size(12.0).color(PALETTE.text_faint));
            }
        });
    });
    let r = response.response.rect;
    ui.painter().hline(r.x_range(), r.bottom(), Stroke::new(1.0, PALETTE.border));
}

/// Small uppercase-free caption, used for group labels.
pub fn caption(text: impl Into<String>) -> RichText {
    RichText::new(text).size(11.0).color(PALETTE.text_faint)
}

/// Secondary text.
pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(PALETTE.text_muted)
}

/// Install fonts, text styles, spacing and colors.
pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    load_logo(ctx);
    set_theme(ctx, ThemeChoice::Light);
    for (theme, palette) in [(egui::Theme::Light, &LIGHT), (egui::Theme::Dark, &DARK)] {
        ctx.style_mut_of(theme, |style| style_of(style, palette));
    }
}

fn style_of(style: &mut egui::Style, palette: &Palette) {
    style.visuals = visuals(palette);
    style.text_styles = text_styles(font_scale());
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(8.0, 6.0);
    s.button_padding = egui::vec2(8.0, 4.0);
    s.interact_size = egui::vec2(36.0, 24.0);
    s.menu_margin = Margin::same(6);
    s.window_margin = Margin::same(12);
    s.indent = 16.0;
    s.combo_height = 320.0;
}

/// The text styles at `scale` times their normal size.
fn text_styles(scale: f32) -> std::collections::BTreeMap<TextStyle, FontId> {
    [
        (TextStyle::Small, FontId::proportional(11.5 * scale)),
        (TextStyle::Body, FontId::proportional(13.5 * scale)),
        (TextStyle::Button, FontId::proportional(13.5 * scale)),
        (TextStyle::Heading, FontId::new(17.0 * scale, semibold())),
        (TextStyle::Monospace, FontId::monospace(12.5 * scale)),
    ]
    .into()
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-Regular.ttf"))),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf"))),
    );
    fonts.font_data.insert(
        "noto-arabic".into(),
        Arc::new(FontData::from_static(include_bytes!("../../../assets/fonts/NotoSansArabic-Regular.ttf"))),
    );
    // Inter first; egui's bundled fonts stay as fallbacks for symbols and emoji.
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "inter".into());
    proportional.push("noto-arabic".into());
    let fallbacks: Vec<String> = proportional[1..].to_vec();
    let mut semibold_family = vec!["inter-semibold".to_string()];
    semibold_family.extend(fallbacks);
    fonts.families.insert(semibold(), semibold_family);

    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace.push("noto-arabic".into());
    // Chinese, Japanese, Korean and Lao text from the system's fonts, last.
    for (i, fallback) in cc_views::fonts::fallbacks().enumerate() {
        let name = format!("fallback-{i}");
        let data = FontData { font: Cow::Borrowed(&fallback.data), index: fallback.index, tweak: Default::default() };
        fonts.font_data.insert(name.clone(), Arc::new(data));
        for family in fonts.families.values_mut() {
            family.push(name.clone());
        }
    }
    fonts
}

fn visuals(p: &Palette) -> Visuals {
    let r = CornerRadius::same(RADIUS);
    let field = if p.dark { p.app_bg } else { Color32::WHITE };
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.override_text_color = None;
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.window_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(36) };
    v.popup_shadow = Shadow { offset: [0, 3], blur: 10, spread: 0, color: Color32::from_black_alpha(30) };
    v.faint_bg_color = p.surface;
    v.extreme_bg_color = field;
    v.code_bg_color = p.surface;
    v.hyperlink_color = p.accent_text;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.danger;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent_text);
    v.striped = true;
    v.indent_has_left_vline = false;
    v.collapsing_header_frame = false;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;

    w.inactive.bg_fill = field;
    w.inactive.weak_bg_fill = Color32::TRANSPARENT;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;

    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = p.pressed;
    w.active.weak_bg_fill = p.pressed;
    w.active.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = r;
    w.active.expansion = 0.0;

    w.open.bg_fill = p.hover;
    w.open.weak_bg_fill = p.hover;
    w.open.bg_stroke = Stroke::new(1.0, p.border_strong);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = r;
    v
}

/// Dock (view tabs) style matching the theme.
pub fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let p = PALETTE;
    let mut s = egui_dock::Style::from_egui(style);
    s.main_surface_border_stroke = Stroke::NONE;
    s.separator.width = 4.0;
    s.separator.color_idle = p.app_bg;
    s.separator.color_hovered = p.border_strong;
    s.separator.color_dragged = p.accent;
    s.tab_bar.bg_fill = p.surface;
    s.tab_bar.height = 30.0;
    s.tab_bar.hline_color = p.border;
    s.tab_bar.corner_radius = CornerRadius::ZERO;
    s.tab.spacing = 2.0;
    s.tab.minimum_width = Some(90.0);
    s.tab.hline_below_active_tab_name = false;
    let tab_radius = CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 };
    let tab = |bg: Color32, text: Color32, outline: Color32| egui_dock::TabInteractionStyle {
        outline_color: outline,
        corner_radius: tab_radius,
        bg_fill: bg,
        text_color: text,
    };
    s.tab.inactive = tab(p.surface, p.text_muted, Color32::TRANSPARENT);
    s.tab.inactive_with_kb_focus = s.tab.inactive.clone();
    s.tab.hovered = tab(p.hover, p.text, Color32::TRANSPARENT);
    s.tab.active = tab(p.canvas, p.text, p.border);
    s.tab.active_with_kb_focus = s.tab.active.clone();
    s.tab.focused = tab(p.canvas, p.accent_text, p.border);
    s.tab.focused_with_kb_focus = s.tab.focused.clone();
    s.tab.tab_body.bg_fill = p.canvas;
    s.tab.tab_body.stroke = Stroke::new(1.0, p.border);
    s.tab.tab_body.corner_radius = CornerRadius::ZERO;
    s.tab.tab_body.inner_margin = Margin::ZERO;
    s.buttons.close_tab_color = p.text_faint;
    s.buttons.close_tab_active_color = p.text;
    s.buttons.close_tab_bg_fill = p.pressed;
    s.overlay.selection_color = p.accent.gamma_multiply(0.35);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(c: Color32) -> f32 {
        let channel = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    /// Text and icons stay readable on panels in both themes.
    #[test]
    fn palettes_have_readable_contrast() {
        for p in [&LIGHT, &DARK] {
            for bg in [p.panel, p.surface, p.hover] {
                assert!(contrast(p.text, bg) >= 7.0, "text on {bg:?}");
                assert!(contrast(p.icon_ink, bg) >= 7.0, "icons on {bg:?}");
                assert!(contrast(p.text_muted, bg) >= 4.5, "muted text on {bg:?}");
            }
            assert!(contrast(p.accent_text, p.accent_soft) >= 4.5, "selected text");
            for c in [p.danger, p.warning, p.success, p.icon_tint] {
                assert!(contrast(c, p.panel) >= 3.0, "{c:?} on the panel");
            }
        }
    }

    /// `PALETTE` reads the colors of the theme shown, and the preference
    /// is stored by its code.
    #[test]
    fn shown_palette_follows_the_chosen_theme() {
        let ctx = egui::Context::default();
        set_theme(&ctx, ThemeChoice::Dark);
        assert!(PALETTE.dark);
        assert_eq!(PALETTE.panel, DARK.panel);
        set_theme(&ctx, ThemeChoice::Light);
        assert!(!PALETTE.dark);
        // Drawings keep their white paper in both themes.
        assert_eq!(LIGHT.paper, DARK.paper);
        for t in ThemeChoice::ALL {
            assert_eq!(ThemeChoice::from_code(t.code()), Some(t));
        }
        assert_eq!(ThemeChoice::from_code("sepia"), None);
    }

    /// The font size preference scales the text styles of both themes, clamped.
    #[test]
    fn font_scale_resizes_text_styles() {
        let ctx = egui::Context::default();
        apply(&ctx);
        let body = |theme| ctx.style_of(theme).text_styles[&TextStyle::Body].size;
        assert_eq!(body(egui::Theme::Light), 13.5);
        set_font_scale(&ctx, 1.2);
        assert!((body(egui::Theme::Light) - 16.2).abs() < 1e-4);
        assert!((body(egui::Theme::Dark) - 16.2).abs() < 1e-4);
        // Applying the theme again keeps the scale.
        apply(&ctx);
        assert!((body(egui::Theme::Dark) - 16.2).abs() < 1e-4);
        set_font_scale(&ctx, f32::NAN);
        assert_eq!(font_scale(), 1.0);
        set_font_scale(&ctx, 9.0);
        assert_eq!(font_scale(), 1.5);
        set_font_scale(&ctx, 1.0);
    }
}
