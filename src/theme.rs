//! Единственный набор цветов и шрифтов программы и мастера установки: нейтральная бумага
//! днём, тёмное стекло ночью, цвет только там, где он что-то значит (работает, плохо, ошибка).
//! Светлая и тёмная схемы переключаются вместе с системной темой.
//! Контраст проверяется тестами внизу файла.

use eframe::egui::epaint::text::{FontTweak, VariationCoords};
use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Shadow, Stroke, Theme, Visuals,
};
use std::sync::Arc;

pub struct Palette {
    /// Фон окна.
    pub ground: Color32,
    /// Модальные окна, текст на чернильной заливке.
    pub sheet: Color32,
    /// Заливка простой кнопки и поля только для чтения.
    pub sheet_hover: Color32,
    /// Заливка выбранной строки, пункта и варианта: тихая, текст на ней остаётся чернильным.
    pub select: Color32,
    /// Основной текст, главная кнопка, выбранная строка.
    pub ink: Color32,
    /// Вторичный текст.
    pub ink_weak: Color32,
    /// Волосные линии.
    pub rule: Color32,
    /// Границы элементов управления (не ниже 3:1 к фону).
    pub rule_strong: Color32,
    /// Разрушающие действия и ошибки.
    pub danger: Color32,
    pub on_danger: Color32,
    /// Состояние «работает».
    pub good: Color32,
    /// Состояние «внимание».
    pub warn: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Palette = Palette {
    ground: hex(0xEFEADD),
    sheet: hex(0xFAF7EF),
    sheet_hover: hex(0xE7E1D2),
    select: hex(0xDDD6C3),
    ink: hex(0x1B1D21),
    ink_weak: hex(0x5B5A54),
    rule: hex(0xD8D2C1),
    rule_strong: hex(0x87826F),
    danger: hex(0xA6171F),
    on_danger: hex(0xFBF8F0),
    good: hex(0x12743A),
    warn: hex(0x7E4B00),
};

pub const DARK: Palette = Palette {
    ground: hex(0x101317),
    sheet: hex(0x191D23),
    sheet_hover: hex(0x20252C),
    select: hex(0x29303A),
    ink: hex(0xEDE7D9),
    ink_weak: hex(0xA9A598),
    rule: hex(0x2C3239),
    rule_strong: hex(0x7D8086),
    danger: hex(0xFF8F88),
    on_danger: hex(0x12151A),
    good: hex(0x4FCC7D),
    warn: hex(0xF2B84B),
};

pub fn palette(ui: &egui::Ui) -> &'static Palette {
    if ui.visuals().dark_mode { &DARK } else { &LIGHT }
}

// ---------- Шрифты ----------

const SEMI: &str = "semibold";
const DISPLAY: &str = "display";

pub fn semibold() -> FontFamily {
    FontFamily::Name(SEMI.into())
}

/// Узкое жирное начертание для одного крупного заголовка на экране.
pub fn display() -> FontFamily {
    FontFamily::Name(DISPLAY.into())
}

pub fn install(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, false));
    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, true));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        // Не меньше 24 px по высоте для любого элемента; свои элементы выше сами.
        style.spacing.interact_size.y = 24.0;
        style.spacing.window_margin = egui::Margin::same(20);
        for (text_style, font) in style.text_styles.iter_mut() {
            font.size = match text_style {
                egui::TextStyle::Heading => 21.0,
                egui::TextStyle::Small => 12.5,
                egui::TextStyle::Monospace => 13.0,
                _ => 14.0,
            };
        }
        if let Some(h) = style.text_styles.get_mut(&egui::TextStyle::Heading) {
            h.family = semibold();
        }
    });
}

/// Межстрочный зазор Bahnschrift, 0.2 кегля, egui кладёт под строку целиком, и текст в
/// кнопках и строках сидит выше середины. Половина зазора переносится наверх сдвигом букв.
pub const GLYPH_SHIFT: f32 = 0.1;

/// Bahnschrift один переменный файл на все начертания: каждое становится отдельным шрифтом
/// с осями `wght` и `wdth` из `FontTweak::coords`, файл в памяти общий. Без него Segoe UI.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let read = |file: &str| std::fs::read(format!(r"C:\Windows\Fonts\{file}")).ok().map(Arc::new);
    let bahnschrift = read("bahnschrift.ttf");
    let mut chains: [Vec<String>; 3] = Default::default();
    if let Some(file) = &bahnschrift {
        let faces = [("bahnschrift", 400.0, 100.0), ("bahnschrift-semi", 600.0, 100.0), ("bahnschrift-display", 700.0, 75.0)];
        for (chain, (name, weight, width)) in chains.iter_mut().zip(faces) {
            let tweak = FontTweak {
                coords: VariationCoords::new([(b"wght", weight), (b"wdth", width)]),
                hinting: Some(false),
                y_offset_factor: GLYPH_SHIFT,
                ..Default::default()
            };
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_blob(file.clone(), 0).tweak(tweak)));
            chain.push(name.to_string());
        }
    }
    // Символы, которых нет в Bahnschrift, и вся программа, если его нет в системе.
    let mut load = |name: &str, file: &str| {
        let data = read(file);
        let found = data.is_some();
        if let Some(data) = data {
            fonts.font_data.insert(name.into(), Arc::new(FontData::from_blob(data, 0)));
        }
        found
    };
    let regular = load("segoe", "segoeui.ttf");
    let bold = load("segoe-semibold", "seguisb.ttf");
    let mono = load("consolas", "consola.ttf");
    for (i, chain) in chains.iter_mut().enumerate() {
        if i > 0 && bold {
            chain.push("segoe-semibold".to_string());
        }
        if regular {
            chain.push("segoe".to_string());
        }
        chain.extend(fallbacks.iter().cloned());
    }
    let [body, semi, narrow] = chains;
    fonts.families.insert(FontFamily::Proportional, body);
    fonts.families.insert(semibold(), semi);
    fonts.families.insert(display(), narrow);
    if mono {
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }
    ctx.set_fonts(fonts);
}

// ---------- Обычные элементы egui ----------

fn visuals(p: &Palette, dark: bool) -> Visuals {
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    let radius = CornerRadius::same(8);

    v.panel_fill = p.ground;
    v.window_fill = p.sheet;
    v.window_stroke = Stroke::new(1.0, p.rule_strong);
    v.window_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow::NONE;
    v.popup_shadow = Shadow::NONE;
    v.menu_corner_radius = radius;
    v.faint_bg_color = p.sheet;
    v.extreme_bg_color = p.sheet_hover;
    v.code_bg_color = p.sheet_hover;
    v.override_text_color = None;
    v.weak_text_color = Some(p.ink_weak);
    v.hyperlink_color = p.ink;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger;
    // Выделение лежит под текстом: сплошная чернильная заливка съела бы буквы.
    v.selection.bg_fill = p.ink.gamma_multiply(0.22);
    v.selection.stroke = Stroke::new(1.0, p.ink);
    v.text_cursor.stroke = Stroke::new(1.5, p.ink);

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.sheet;
    w.noninteractive.weak_bg_fill = p.sheet;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.rule);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.ink);
    w.noninteractive.corner_radius = radius;

    let rest = p.sheet_hover.gamma_multiply(0.45);
    w.inactive.bg_fill = p.sheet_hover;
    w.inactive.weak_bg_fill = rest;
    w.inactive.bg_stroke = Stroke::new(1.0, p.rule_strong);
    w.inactive.fg_stroke = Stroke::new(1.0, p.ink);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = p.sheet_hover;
    w.hovered.weak_bg_fill = p.sheet_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.ink);
    w.hovered.fg_stroke = Stroke::new(1.0, p.ink);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active = w.hovered;
    w.open = w.hovered;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        let channel = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn text_and_status_stay_readable_on_both_surfaces() {
        for p in [&LIGHT, &DARK] {
            for surface in [p.ground, p.sheet] {
                for text in [p.ink, p.ink_weak, p.danger, p.good, p.warn] {
                    assert!(contrast(text, surface) >= 4.5, "{text:?} на {surface:?}");
                }
                assert!(contrast(p.rule_strong, surface) >= 3.0);
            }
            assert!(contrast(p.ink_weak, p.sheet_hover) >= 4.5);
            // Метки в списке серверов.
            assert!(contrast(p.ink_weak, p.rule) >= 4.5);
            assert!(contrast(p.ink, p.rule) >= 4.5);
            assert!(contrast(p.on_danger, p.danger) >= 4.5);
            assert!(contrast(p.sheet, p.ink) >= 4.5);
            for text in [p.ink, p.ink_weak, p.danger, p.warn] {
                assert!(contrast(text, p.select) >= 4.5, "{text:?} на выделении");
            }
        }
    }
}
