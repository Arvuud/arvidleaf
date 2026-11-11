//! Dark/light theme, shared with arvidlab (same palette, fonts, spacing).

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};

#[allow(dead_code)] // the block-diagram colours are part of the shared arvidlab palette
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub error: Color32,
    pub warn: Color32,
    pub ok: Color32,
    pub canvas: Color32,
    pub grid: Color32,
    pub block: Color32,
    pub block_border: Color32,
    pub wire: Color32,
    pub keyword: Color32,
    pub string: Color32,
    pub comment: Color32,
    pub number: Color32,
    pub func: Color32,
    /// LaTeX: math mode
    pub math: Color32,
    /// LaTeX: `&`, `~`, `\\`, `^`, `_`
    pub special: Color32,
    /// LaTeX: `[options]`
    pub opt: Color32,
}

pub fn dark() -> Palette {
    Palette {
        bg: Color32::from_rgb(0x17, 0x19, 0x1d),
        panel: Color32::from_rgb(0x1f, 0x22, 0x27),
        panel2: Color32::from_rgb(0x27, 0x2b, 0x31),
        border: Color32::from_rgb(0x34, 0x39, 0x41),
        text: Color32::from_rgb(0xdc, 0xe0, 0xe6),
        dim: Color32::from_rgb(0x8b, 0x93, 0x9e),
        accent: Color32::from_rgb(0x4c, 0x9a, 0xff),
        accent2: Color32::from_rgb(0x2d, 0xd4, 0xbf),
        error: Color32::from_rgb(0xff, 0x6b, 0x6b),
        warn: Color32::from_rgb(0xff, 0xb8, 0x4d),
        ok: Color32::from_rgb(0x4a, 0xde, 0x80),
        canvas: Color32::from_rgb(0x14, 0x16, 0x1a),
        grid: Color32::from_rgb(0x22, 0x26, 0x2c),
        block: Color32::from_rgb(0x26, 0x2b, 0x33),
        block_border: Color32::from_rgb(0x5a, 0x66, 0x75),
        wire: Color32::from_rgb(0xa8, 0xb3, 0xc2),
        keyword: Color32::from_rgb(0xc6, 0x78, 0xdd),
        string: Color32::from_rgb(0x98, 0xc3, 0x79),
        comment: Color32::from_rgb(0x6a, 0x73, 0x7d),
        number: Color32::from_rgb(0xd1, 0x9a, 0x66),
        func: Color32::from_rgb(0x61, 0xaf, 0xef),
        math: Color32::from_rgb(0xe5, 0xc0, 0x7b),
        special: Color32::from_rgb(0xff, 0xb8, 0x4d),
        opt: Color32::from_rgb(0x2d, 0xd4, 0xbf),
    }
}

pub fn light() -> Palette {
    Palette {
        bg: Color32::from_rgb(0xf3, 0xf4, 0xf6),
        panel: Color32::from_rgb(0xff, 0xff, 0xff),
        panel2: Color32::from_rgb(0xee, 0xf0, 0xf3),
        border: Color32::from_rgb(0xd6, 0xda, 0xe0),
        text: Color32::from_rgb(0x1f, 0x23, 0x28),
        dim: Color32::from_rgb(0x6b, 0x72, 0x7c),
        accent: Color32::from_rgb(0x1f, 0x6f, 0xeb),
        accent2: Color32::from_rgb(0x0e, 0x9f, 0x8e),
        error: Color32::from_rgb(0xd0, 0x2f, 0x2f),
        warn: Color32::from_rgb(0xb8, 0x6e, 0x00),
        ok: Color32::from_rgb(0x1a, 0x9a, 0x4b),
        canvas: Color32::from_rgb(0xfa, 0xfb, 0xfc),
        grid: Color32::from_rgb(0xe9, 0xec, 0xf0),
        block: Color32::from_rgb(0xff, 0xff, 0xff),
        block_border: Color32::from_rgb(0x55, 0x60, 0x6e),
        wire: Color32::from_rgb(0x3b, 0x44, 0x50),
        keyword: Color32::from_rgb(0x00, 0x33, 0xcc),
        string: Color32::from_rgb(0xa0, 0x20, 0xf0),
        comment: Color32::from_rgb(0x22, 0x8b, 0x22),
        number: Color32::from_rgb(0x9a, 0x5c, 0x00),
        func: Color32::from_rgb(0x1f, 0x5f, 0xbf),
        math: Color32::from_rgb(0x9a, 0x5c, 0x00),
        special: Color32::from_rgb(0xc0, 0x4a, 0x00),
        opt: Color32::from_rgb(0x0e, 0x9f, 0x8e),
    }
}

/// Tries to use a nicer system UI font when available.
pub fn install_fonts(ctx: &egui::Context) {
    let candidates = [
        "/usr/share/fonts/inter/Inter-Regular.otf",
        "/usr/share/fonts/inter/InterVariable.ttf",
        "/usr/share/fonts/TTF/Inter-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    let mono = [
        "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/TTF/FiraCode-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    ];
    let mut fonts = egui::FontDefinitions::default();
    if let Some(data) = candidates.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("ui".into(), egui::FontData::from_owned(data).into());
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    if let Some(data) = mono.iter().find_map(|p| std::fs::read(p).ok()) {
        fonts.font_data.insert("mono".into(), egui::FontData::from_owned(data).into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "mono".into());
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, dark_mode: bool) {
    let p = if dark_mode { dark() } else { light() };
    let mut v = if dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = p.panel2;
    v.code_bg_color = p.panel2;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.hyperlink_color = p.accent;
    v.error_fg_color = p.error;
    v.warn_fg_color = p.warn;
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.bg_fill = p.panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.bg_fill = p.panel2;
    v.widgets.inactive.weak_bg_fill = p.panel2;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.bg_fill = p.border;
    v.widgets.hovered.weak_bg_fill = p.border;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.accent.gamma_multiply(0.6));
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.bg_fill = p.accent.gamma_multiply(0.6);
    v.widgets.active.weak_bg_fill = p.accent.gamma_multiply(0.6);
    v.widgets.active.corner_radius = r;
    v.widgets.open.bg_fill = p.panel2;
    v.widgets.open.corner_radius = r;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.menu_margin = Margin::same(6);
        s.spacing.window_margin = Margin::same(10);
        s.text_styles.insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(14.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(11.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace));
        s.text_styles.insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional));
    });
}

pub fn palette(ctx: &egui::Context) -> Palette {
    if ctx.global_style().visuals.dark_mode { dark() } else { light() }
}

/// The arvidleaf mark: a leaf in the teal shades of the arvidlab logo.
pub fn logo(painter: &egui::Painter, rect: egui::Rect) {
    use egui::{pos2, Shape};
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 2.0;
    let p = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    // tip top-right, stem bottom-left; two curved halves
    let n = 14;
    let curve = |side: f32| -> Vec<egui::Pos2> {
        (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                // along the axis from (-0.75, 0.75) to (0.8, -0.8)
                let ax = -0.75 + 1.55 * t;
                let ay = 0.75 - 1.55 * t;
                let bulge = (t * std::f32::consts::PI).sin() * 0.55 * side;
                p(ax + bulge * 0.707, ay + bulge * 0.707)
            })
            .collect()
    };
    let upper = curve(-1.0);
    let lower = curve(1.0);
    let axis0 = p(-0.75, 0.75);
    let axis1 = p(0.8, -0.8);
    let mut a = upper.clone();
    a.push(axis0);
    let mut b = lower.clone();
    b.push(axis0);
    painter.add(Shape::convex_polygon(a, Color32::from_rgb(0x13, 0xac, 0xab), egui::Stroke::NONE));
    painter.add(Shape::convex_polygon(b, Color32::from_rgb(0x0b, 0x5e, 0x5d), egui::Stroke::NONE));
    painter.line_segment([p(-0.95, 0.95), axis1], egui::Stroke::new((s * 0.12).max(1.0), Color32::from_rgb(0x15, 0xc6, 0xb6)));
}

#[cfg(test)]
mod tests {
    /// Every symbol used in the UI must exist in the bundled/system fonts,
    /// otherwise egui draws an empty box.
    #[test]
    fn ui_symbols_have_glyphs() {
        let ctx = eframe::egui::Context::default();
        super::install_fonts(&ctx);
        let mut o = ctx.run_ui(Default::default(), |_| {});
        o.textures_delta.clear();
        let mut used = String::new();
        for src in [include_str!("app.rs"), include_str!("editor.rs"), include_str!("pdfview.rs"), include_str!("history_ui.rs"), include_str!("templates_ui.rs")] {
            // only string literals count (comments may use any character)
            for line in src.lines().filter(|l| !l.trim_start().starts_with("//")) {
                let mut in_str = false;
                for c in line.chars() {
                    if c == '"' {
                        in_str = !in_str;
                    } else if in_str && !c.is_ascii() && !used.contains(c) {
                        used.push(c);
                    }
                }
            }
        }
        let missing: String = used.chars().filter(|c| !ctx.fonts_mut(|f| f.has_glyph(&eframe::egui::FontId::proportional(14.0), *c))).collect();
        assert!(missing.is_empty(), "no glyph for: {missing}");
    }
}
