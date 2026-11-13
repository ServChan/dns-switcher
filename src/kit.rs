//! Общие элементы окон: пружины, текст по базовой линии, кнопки, переключатели, строки выбора.
//! Ими рисуют и мастер установки, и сама программа, цвета берутся только из `theme.rs`.
//! Пружины лежат в контексте egui: их не нужно протаскивать через все функции окна.

use crate::theme::{self, palette};
use eframe::egui::text::{LayoutJob, TextWrapping};
use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, CursorIcon, FontFamily, FontId, Frame, Galley, Id, Margin, Pos2,
    Rect, Response, Role, Sense, Shape, Spinner, Stroke, StrokeKind, TextEdit, TextFormat, Ui, UiBuilder, Vec2,
    WidgetInfo, pos2, vec2,
};
use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

pub fn fade(color: Color32, alpha: f32) -> Color32 {
    color.gamma_multiply(alpha.clamp(0.0, 1.0))
}

// ---------- Движение ----------

#[derive(Clone, Copy)]
pub struct Tune {
    stiffness: f32,
    damping: f32,
}

/// Наведение и нажатие: без перелёта.
pub const SNAP: Tune = Tune { stiffness: 420.0, damping: 38.0 };
/// Выделение и фронт линии: с лёгким дыханием в конце.
pub const SOFT: Tune = Tune { stiffness: 190.0, damping: 23.0 };
/// Появление и переключатели: заметный, но короткий перелёт.
pub const BOUNCE: Tune = Tune { stiffness: 300.0, damping: 19.0 };

#[derive(Clone, Copy)]
struct Spring {
    value: f32,
    velocity: f32,
    target: f32,
}

impl Spring {
    fn new(value: f32) -> Self {
        Self { value, velocity: 0.0, target: value }
    }

    /// Шаг с мелким дроблением: при длинном кадре пружина не должна разлететься.
    fn step(&mut self, dt: f32, tune: Tune) {
        let mut left = dt.min(0.1);
        while left > 0.0 {
            let h = left.min(1.0 / 240.0);
            let force = (self.target - self.value) * tune.stiffness - self.velocity * tune.damping;
            self.velocity += force * h;
            self.value += self.velocity * h;
            left -= h;
        }
        if self.settled() {
            *self = Self::new(self.target);
        }
    }

    fn settled(&self) -> bool {
        (self.value - self.target).abs() < 0.0015 * self.target.abs().max(1.0) && self.velocity.abs() < 0.02
    }
}

struct Motion {
    springs: HashMap<Id, Spring>,
    dt: f32,
    on: bool,
    moving: bool,
    /// Медленная картинка просит кадры по ограниченной частоте.
    ambient: bool,
}

impl Motion {
    fn spring(&mut self, key: Id, start: f32, target: f32, tune: Tune) -> f32 {
        let on = self.on;
        let spring = self.springs.entry(key).or_insert_with(|| Spring::new(if on { start } else { target }));
        spring.target = target;
        if !on {
            *spring = Spring::new(target);
        } else if spring.value != target || !spring.settled() {
            spring.step(self.dt, tune);
            self.moving = true;
        }
        spring.value
    }
}

fn motion<R>(ctx: &Context, f: impl FnOnce(&mut Motion) -> R) -> R {
    let store: Arc<Mutex<Motion>> = ctx.data_mut(|d| {
        d.get_temp_mut_or_insert_with(Id::unique("kit-motion"), || {
            Arc::new(Mutex::new(Motion { springs: HashMap::new(), dt: 1.0 / 60.0, on: windows_animates(), moving: false, ambient: false }))
        })
        .clone()
    });
    let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

/// Начало кадра: окно вызывает это до первого элемента.
pub fn begin_frame(ctx: &Context) {
    let dt = ctx.input(|i| i.stable_dt);
    motion(ctx, |m| {
        m.dt = dt;
        m.moving = false;
        m.ambient = false;
    });
    track_keyboard(ctx);
}

/// Конец кадра: следующий заказывается, только пока что-то движется.
pub fn end_frame(ctx: &Context) {
    let (moving, ambient) = motion(ctx, |m| (m.moving, m.ambient));
    if moving {
        ctx.request_repaint();
    } else if ambient {
        ctx.request_repaint_after(AMBIENT_FRAME);
    }
}

/// Между кадрами удерживаемой сцены: около 30 в секунду.
const AMBIENT_FRAME: std::time::Duration = std::time::Duration::from_millis(33);

/// Заказать кадр по ограниченной частоте: что-то движется, но медленно.
pub fn ambient(ctx: &Context) {
    motion(ctx, |m| m.ambient = true);
}

/// Окно видно и впереди: только тогда идёт картинка на первой странице.
pub fn live(ctx: &Context) -> bool {
    ctx.input(|i| i.focused && i.viewport().minimized != Some(true))
}

/// Только для отладочной сборки: какие кадры заказаны, частые и редкие.
#[cfg(debug_assertions)]
pub fn wanted_frames(ctx: &Context) -> (bool, bool) {
    motion(ctx, |m| (m.moving, m.ambient))
}

/// Заказать следующий кадр: что-то движется не на пружине.
pub fn keep_moving(ctx: &Context) {
    motion(ctx, |m| m.moving = true);
}

/// Анимации Windows включены.
pub fn animated(ctx: &Context) -> bool {
    motion(ctx, |m| m.on)
}

/// Только для отладочной сборки: снимки не зависят от настроек этого компьютера.
#[cfg(debug_assertions)]
pub fn set_animated(ctx: &Context, on: bool) {
    motion(ctx, |m| m.on = on);
}

/// Значение, которое с первого кадра стоит на месте и потом следует за `target`.
pub fn follow(ctx: &Context, key: impl Hash + Debug, target: f32, tune: Tune) -> f32 {
    motion(ctx, |m| m.spring(Id::unique(key), target, target, tune))
}

pub fn tween(ctx: &Context, key: impl Hash + Debug, on: bool, tune: Tune) -> f32 {
    follow(ctx, key, if on { 1.0 } else { 0.0 }, tune)
}

/// Появление: с нуля до единицы.
pub fn pop(ctx: &Context, key: impl Hash + Debug, tune: Tune) -> f32 {
    motion(ctx, |m| m.spring(Id::unique(key), 0.0, 1.0, tune))
}

fn windows_animates() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW};
    let mut enabled = 1i32;
    let ok = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut enabled).cast(), 0) };
    ok == 0 || enabled != 0
}

pub fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

/// Смена экрана: 0.28 с, проявление со сдвигом справа. `key` меняется вместе с экраном.
pub fn page<R>(ui: &mut Ui, key: impl Hash + Debug, add: impl FnOnce(&mut Ui) -> R) -> R {
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let slot = Id::unique("kit-page").with(ui.unique_id());
    let id = Id::unique(key);
    let started = ctx.data_mut(|d| {
        let entry = d.get_temp_mut_or_insert_with(slot, || (id, f64::NEG_INFINITY));
        if entry.0 != id {
            *entry = (id, now);
        }
        entry.1
    });
    let phase = if animated(&ctx) { ease_out(((now - started) / 0.28) as f32) } else { 1.0 };
    if phase < 1.0 {
        keep_moving(&ctx);
    }
    let rect = ui.max_rect().translate(vec2((1.0 - phase) * 26.0, 0.0));
    let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(*ui.layout()));
    child.multiply_opacity(phase);
    add(&mut child)
}

// ---------- Текст ----------

#[derive(Clone, Copy, PartialEq)]
pub enum Face {
    Body,
    Semi,
    /// Узкое жирное начертание для заголовка.
    Display,
}

pub fn font(face: Face, size: f32) -> FontId {
    let family = match face {
        Face::Body => FontFamily::Proportional,
        Face::Semi => theme::semibold(),
        Face::Display => theme::display(),
    };
    FontId::new(size, family)
}

/// От верха строки до базовой линии, в долях кегля: половина межстрочного зазора Bahnschrift
/// (0.1) плюс его высота над линией (0.794). Ставим строку по базовой линии, а не по верху.
const BASELINE: f32 = 0.894;
/// От центра строки до базовой линии.
const CENTER_TO_BASELINE: f32 = 0.294;

fn job(text: &str, face: Face, size: f32, color: Color32) -> LayoutJob {
    let format = TextFormat {
        font_id: font(face, size),
        color,
        extra_letter_spacing: if face == Face::Display { size * -0.005 } else { 0.0 },
        ..Default::default()
    };
    LayoutJob::single_section(text.to_owned(), format)
}

fn laid_out(ui: &Ui, job: LayoutJob) -> Arc<Galley> {
    ui.fonts_mut(|fonts| fonts.layout_job(job))
}

/// Первая базовая линия относительно верха раскладки. Буквы уже сдвинуты шрифтом.
fn first_baseline(galley: &Galley, size: f32, ppp: f32) -> f32 {
    let row = galley.rows.first().and_then(|row| row.glyphs.first().map(|glyph| row.pos.y + glyph.pos.y));
    row.unwrap_or(0.0) + (size * theme::GLYPH_SHIFT * ppp).round() / ppp
}

/// Буквы стоят на целом пикселе экрана, иначе мелкий текст замыливается.
fn draw_on_baseline(ui: &Ui, galley: Arc<Galley>, x: f32, baseline: f32, size: f32, color: Color32) {
    let at = pos2(x, baseline - first_baseline(&galley, size, ui.pixels_per_point()));
    ui.painter().galley(at, galley, color);
}

pub fn text_width(ui: &Ui, text: &str, face: Face, size: f32) -> f32 {
    laid_out(ui, job(text, face, size, Color32::PLACEHOLDER)).size().x
}

fn top_baseline(ui: &Ui, top: f32, size: f32) -> f32 {
    let ppp = ui.pixels_per_point();
    ((top + size * BASELINE) * ppp).round() / ppp
}

fn centered_baseline(ui: &Ui, y: f32, size: f32) -> f32 {
    let ppp = ui.pixels_per_point();
    ((y + size * CENTER_TO_BASELINE) * ppp).floor() / ppp
}

/// Заголовок и другие подписи от левого верхнего угла.
pub fn text_at(ui: &Ui, text: &str, face: Face, size: f32, at: Pos2, color: Color32) {
    let baseline = top_baseline(ui, at.y, size);
    draw_on_baseline(ui, laid_out(ui, job(text, face, size, color)), at.x, baseline, size, color);
}

/// Одна строка с центром по `y`, обрезанная многоточием до `width`.
#[allow(clippy::too_many_arguments)]
pub fn line(ui: &Ui, text: &str, face: Face, size: f32, x: f32, y: f32, width: f32, color: Color32) {
    let mut job = job(text, face, size, color);
    job.wrap = TextWrapping::truncate_at_width(width);
    let galley = laid_out(ui, job);
    draw_on_baseline(ui, galley, x, centered_baseline(ui, y, size), size, color);
}

/// Строка до правого края. Возвращает ширину.
pub fn line_right(ui: &Ui, text: &str, face: Face, size: f32, right: f32, y: f32, color: Color32) -> f32 {
    let galley = laid_out(ui, job(text, face, size, color));
    let width = galley.size().x;
    draw_on_baseline(ui, galley, right - width, centered_baseline(ui, y, size), size, color);
    width
}

/// Абзац с переносами. Возвращает высоту.
pub fn paragraph(ui: &Ui, text: &str, face: Face, size: f32, at: Pos2, width: f32, color: Color32) -> f32 {
    let mut job = job(text, face, size, color);
    job.wrap.max_width = width;
    let galley = laid_out(ui, job);
    let height = galley.size().y;
    let baseline = top_baseline(ui, at.y, size);
    draw_on_baseline(ui, galley, at.x, baseline, size, color);
    height
}

/// Высота абзаца: нужна, чтобы подвинуть то, что рисуется раньше него.
pub fn paragraph_height(ui: &Ui, text: &str, face: Face, size: f32, width: f32) -> f32 {
    let mut job = job(text, face, size, Color32::PLACEHOLDER);
    job.wrap.max_width = width;
    laid_out(ui, job).size().y
}

/// Единственный крупный заголовок экрана, с переносами, в общей раскладке.
/// Обычной ширины и полужирный: крупнее остального, но не кричит.
pub fn display_title(ui: &mut Ui, text: &str, size: f32) {
    let p = palette(ui);
    let width = ui.available_width();
    let height = paragraph_height(ui, text, Face::Semi, size, width);
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Heading, true, text));
    paragraph(ui, text, Face::Semi, size, rect.min, width, p.ink);
}

// ---------- Линии ----------

pub fn circle(ui: &Ui, center: Pos2, radius: f32, fill: Color32, stroke: Stroke) {
    ui.painter().circle(center, radius, fill, stroke);
}

/// Ломаная с круглыми концами.
pub fn polyline(ui: &Ui, points: &[Pos2], width: f32, color: Color32) {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return };
    let painter = ui.painter();
    if points.len() > 1 {
        painter.add(Shape::line(points.to_vec(), Stroke::new(width, color)));
    }
    painter.circle_filled(*first, width / 2.0, color);
    painter.circle_filled(*last, width / 2.0, color);
}

/// Волосная линия через всю доступную ширину.
pub fn hairline(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, palette(ui).rule);
}

/// Значок состояния: галочка, крест, восклицательный знак или точка. Состояние передаётся им
/// и словами рядом, а не только цветом.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mark {
    Ok,
    Warn,
    Bad,
    Neutral,
}

pub fn status_icon(painter: &egui::Painter, p: &theme::Palette, c: Pos2, r: f32, mark: Mark, alpha: f32) {
    let k = r / 9.0;
    let ground = fade(p.sheet, alpha);
    let disc = |color: Color32| painter.circle_filled(c, r, fade(color, alpha));
    match mark {
        Mark::Ok => {
            disc(p.good);
            let s = Stroke::new(2.0 * k, ground);
            painter.line_segment([c + vec2(-4.0, 0.5) * k, c + vec2(-1.2, 3.3) * k], s);
            painter.line_segment([c + vec2(-1.2, 3.3) * k, c + vec2(4.5, -3.0) * k], s);
        }
        Mark::Bad => {
            disc(p.danger);
            let s = Stroke::new(2.0 * k, ground);
            painter.line_segment([c + vec2(-3.5, -3.5) * k, c + vec2(3.5, 3.5) * k], s);
            painter.line_segment([c + vec2(3.5, -3.5) * k, c + vec2(-3.5, 3.5) * k], s);
        }
        Mark::Warn => {
            disc(p.warn);
            let s = Stroke::new(2.0 * k, ground);
            painter.line_segment([c + vec2(0.0, -4.5) * k, c + vec2(0.0, 1.0) * k], s);
            painter.circle_filled(c + vec2(0.0, 4.0) * k, 1.2 * k, ground);
        }
        Mark::Neutral => {
            painter.circle_stroke(c, r - 1.0, Stroke::new(1.5, fade(p.rule_strong, alpha)));
            painter.circle_filled(c, 2.5 * k, fade(p.rule_strong, alpha));
        }
    }
}

pub fn arrow(painter: &egui::Painter, color: Color32, from: Pos2, to: Pos2) {
    let stroke = Stroke::new(1.6, color);
    painter.line_segment([from, to], stroke);
    let dir = (to - from).normalized();
    let side = vec2(-dir.y, dir.x);
    let head = 6.0;
    painter.line_segment([to, to - dir * head + side * head * 0.6], stroke);
    painter.line_segment([to, to - dir * head - side * head * 0.6], stroke);
}

// ---------- Управление ----------

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Primary,
    Danger,
    Plain,
    Quiet,
}

/// Тихое выделение: заливка чуть темнее фона, текст остаётся чернильным.
/// `bar` добавляет короткую полоску слева, как у пунктов списка в Windows 11.
pub fn select_fill(ui: &Ui, rect: Rect, lit: f32, hover: f32, bar: Option<Color32>) {
    let p = palette(ui);
    let fill = mix(fade(p.sheet_hover, hover), p.select, lit);
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    if let Some(color) = bar.filter(|_| lit > 0.01) {
        let h = (rect.height() * 0.42).min(18.0) * lit;
        let bar = Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, h));
        ui.painter().rect_filled(bar, CornerRadius::same(2), color);
    }
}

/// Рамка фокуса нужна, когда ходят клавишей Tab, а не мышью.
fn keyboard_focus(ui: &Ui) -> bool {
    ui.data(|d| d.get_temp::<bool>(Id::unique("kit-keyboard")).unwrap_or(false))
}

/// Запоминает, чем пользователь сейчас управляет окном.
fn track_keyboard(ctx: &Context) {
    let (tab, pressed) = ctx.input(|i| (i.key_pressed(egui::Key::Tab), i.pointer.any_pressed()));
    if tab || pressed {
        ctx.data_mut(|d| d.insert_temp(Id::unique("kit-keyboard"), tab));
    }
}

pub fn focus_ring(ui: &Ui, rect: Rect, radius: f32, color: Color32) {
    if keyboard_focus(ui) {
        let shape = rect.shrink(2.0);
        ui.painter().rect_stroke(shape, CornerRadius::same(radius as u8 + 2), Stroke::new(2.0, color), StrokeKind::Middle);
    }
}

fn widget(ui: &Ui, key: Id, rect: Rect, enabled: bool) -> Response {
    let response = ui.interact(rect, key, if enabled { Sense::click() } else { Sense::hover() });
    if enabled { response.on_hover_cursor(CursorIcon::PointingHand) } else { response }
}

/// Занимает место в раскладке. Выключенный элемент остаётся выключенным и для подсказок.
fn allocate(ui: &mut Ui, size: Vec2, enabled: bool) -> (Rect, Response) {
    // Место занимает сам ряд, а не вложенная область: иначе перенос строки не сработает.
    let id = ui.next_auto_id();
    let (_, rect) = ui.allocate_space(size);
    let mut builder = UiBuilder::new().max_rect(rect);
    if !enabled {
        builder = builder.disabled();
    }
    let response = ui.new_child(builder).interact(rect, id, Sense::click());
    let response = if response.enabled() { response.on_hover_cursor(CursorIcon::PointingHand) } else { response };
    (rect, response)
}

fn paint_button(ui: &Ui, response: &Response, rect: Rect, label: &str, tone: Tone, busy: bool, enabled: bool) {
    let p = palette(ui);
    let ctx = ui.ctx();
    let id = response.id;
    let live = enabled && !busy;
    let hover = tween(ctx, (id, "hover"), live && response.hovered(), SNAP);
    let press = tween(ctx, (id, "press"), live && response.is_pointer_button_down_on(), SNAP);
    let shown = Rect::from_center_size(rect.center(), rect.size() * (1.0 - 0.03 * press));
    let (fill, text) = match tone {
        Tone::Primary => (mix(p.ink, p.ink_weak, 0.25 * hover), p.sheet),
        Tone::Danger => (mix(p.danger, p.ink, 0.18 * hover), p.on_danger),
        Tone::Plain => (mix(p.sheet, p.sheet_hover, hover), p.ink),
        Tone::Quiet => (fade(p.sheet_hover, hover), p.ink),
    };
    // Занятая кнопка остаётся яркой: «идёт работа» не значит «нельзя».
    let dim = if enabled || busy { 1.0 } else { 0.5 };
    ui.painter().rect_filled(shown, CornerRadius::same(8), fade(fill, dim));
    if tone == Tone::Plain {
        let stroke = Stroke::new(1.0, fade(mix(p.rule, p.rule_strong, hover), dim));
        ui.painter().rect_stroke(shown, CornerRadius::same(8), stroke, StrokeKind::Middle);
    }
    let face = if matches!(tone, Tone::Primary | Tone::Danger) { Face::Semi } else { Face::Body };
    let width = text_width(ui, label, face, 14.0);
    let center = rect.center();
    let room = (rect.width() - 16.0 - if busy { 22.0 } else { 0.0 }).max(1.0);
    let shown_width = width.min(room);
    let left = center.x - shown_width / 2.0 + if busy { 11.0 } else { 0.0 };
    if busy {
        let at = Rect::from_center_size(pos2(left - 13.0, center.y), Vec2::splat(16.0));
        Spinner::new().color(text).paint_at(ui, at);
    }
    line(ui, label, face, 14.0, left, center.y, room + 2.0, fade(text, dim));
    if response.has_focus() {
        focus_ring(ui, rect, 8.0, text);
    }
}

/// Кнопка на заданном месте. `enabled = false` оставляет подсказку, поэтому ответ целиком.
pub fn button_at(ui: &Ui, key: &str, rect: Rect, label: &str, tone: Tone, enabled: bool) -> Response {
    let response = widget(ui, Id::unique(("kit-button", key)), rect, enabled);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, label));
    paint_button(ui, &response, rect, label, tone, false, enabled);
    response
}

/// Кнопка в общей раскладке.
pub struct Button<'a> {
    label: &'a str,
    tone: Tone,
    min: Vec2,
    enabled: bool,
    busy: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str, tone: Tone) -> Self {
        Self { label, tone, min: vec2(0.0, 32.0), enabled: true, busy: false }
    }

    pub fn min(mut self, min: Vec2) -> Self {
        self.min = min;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Идёт работа: крутится индикатор, нажать нельзя, но кнопка не гаснет.
    pub fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let face = if matches!(self.tone, Tone::Primary | Tone::Danger) { Face::Semi } else { Face::Body };
        let wanted = text_width(ui, self.label, face, 14.0) + 28.0 + if self.busy { 22.0 } else { 0.0 };
        let room = if ui.layout().main_wrap { f32::INFINITY } else { ui.available_width().max(self.min.x) };
        let width = wanted.max(self.min.x).min(room);
        let enabled = self.enabled && ui.is_enabled();
        let (rect, response) = allocate(ui, vec2(width, self.min.y), self.enabled && !self.busy);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, self.label));
        paint_button(ui, &response, rect, self.label, self.tone, self.busy, enabled);
        response
    }
}

pub fn plain(ui: &mut Ui, label: &str) -> Response {
    Button::new(label, Tone::Plain).show(ui)
}

/// Ссылка отличается от текста подчёркиванием, а не только цветом.
pub fn link(ui: &mut Ui, label: &str, size: f32) -> Response {
    let p = palette(ui);
    let width = text_width(ui, label, Face::Body, size);
    let (rect, response) = allocate(ui, vec2(width, 22.0), true);
    response.widget_info(|| WidgetInfo::labeled(Role::Link, true, label));
    let hover = tween(ui.ctx(), (response.id, "hover"), response.hovered(), SNAP);
    line(ui, label, Face::Body, size, rect.left(), rect.center().y, width + 2.0, p.ink);
    let under = rect.center().y + size * 0.62;
    ui.painter().hline(rect.x_range(), under, Stroke::new(1.0, mix(p.rule_strong, p.ink, hover)));
    if response.has_focus() {
        focus_ring(ui, rect.expand(4.0), 4.0, p.ink);
    }
    response
}

fn paint_toggle(ui: &Ui, response: &Response, rect: Rect, label: &str, value: bool, enabled: bool) {
    let p = palette(ui);
    let ctx = ui.ctx();
    let id = response.id;
    let on = tween(ctx, (id, "on"), value, BOUNCE);
    let hover = tween(ctx, (id, "hover"), enabled && response.hovered(), SNAP);
    let dim = if enabled { 1.0 } else { 0.5 };
    let y = rect.center().y;
    let (x0, x1) = (rect.left() + 7.0, rect.left() + 29.0);
    let dot = x0 + (x1 - x0) * on;
    polyline(ui, &[pos2(x0, y), pos2(x1, y)], 4.0, fade(p.rule_strong, dim));
    if on > 0.01 {
        polyline(ui, &[pos2(x0, y), pos2(dot.max(x0 + 0.1), y)], 4.0, fade(p.ink, dim));
    }
    circle(ui, pos2(dot, y), 6.0 + hover, mix(p.ground, p.sheet, dim), Stroke::new(2.5, fade(p.ink, dim)));
    line(ui, label, Face::Body, 14.0, rect.left() + 42.0, y, rect.width() - 42.0, fade(p.ink, dim));
    if response.has_focus() {
        focus_ring(ui, rect, 6.0, p.ink);
    }
}

/// Переключатель на заданном месте: кольцо едет по короткому участку линии.
pub fn toggle_at(ui: &Ui, key: &str, rect: Rect, label: &str, value: &mut bool, enabled: bool) {
    let mut response = widget(ui, Id::unique(("kit-toggle", key)), rect, enabled);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let on_now = *value;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on_now, label));
    paint_toggle(ui, &response, rect, label, *value, enabled);
}

/// Переключатель в общей раскладке. `changed()` сообщает о щелчке.
pub fn toggle(ui: &mut Ui, label: &str, value: &mut bool) -> Response {
    let width = (text_width(ui, label, Face::Body, 14.0) + 50.0).min(ui.available_width());
    let enabled = ui.is_enabled();
    let (rect, mut response) = allocate(ui, vec2(width, 32.0), true);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let on_now = *value;
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, enabled, on_now, label));
    paint_toggle(ui, &response, rect, label, *value, enabled);
    response
}

fn paint_choice(ui: &Ui, response: &Response, rect: Rect, selected: bool, title: &str, text: &str) {
    let p = palette(ui);
    let ctx = ui.ctx();
    let id = response.id;
    let lit = tween(ctx, (id, "lit"), selected, SOFT);
    let hover = tween(ctx, (id, "hover"), response.hovered(), SNAP);
    let radius = CornerRadius::same(10);
    ui.painter().rect_filled(rect, radius, mix(fade(p.sheet_hover, hover), p.select, lit));
    ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, mix(p.rule, p.rule_strong, hover)), StrokeKind::Inside);
    // Выбор виден по кружку, а не только по заливке.
    let c = pos2(rect.left() + 24.0, rect.top() + 21.0);
    circle(ui, c, 8.0, Color32::TRANSPARENT, Stroke::new(1.5, mix(p.rule_strong, p.ink, lit)));
    if lit > 0.01 {
        circle(ui, c, 4.0 * lit, p.ink, Stroke::NONE);
    }
    let (ink, weak) = (p.ink, p.ink_weak);
    let x = rect.left() + 42.0;
    let width = rect.right() - 16.0 - x;
    let face = if selected { Face::Semi } else { Face::Body };
    line(ui, title, face, 14.5, x, rect.top() + 21.0, width, ink);
    paragraph(ui, text, Face::Body, 13.0, pos2(x, rect.top() + 34.0), width, weak);
    if response.has_focus() {
        focus_ring(ui, rect, 10.0, ink);
    }
}

/// Строка выбора на заданном месте: кружок слева, выбранная чуть темнее.
pub fn choice_at(ui: &Ui, rect: Rect, selected: bool, title: &str, text: &str) -> bool {
    let response = widget(ui, Id::unique(("kit-choice", title)), rect, true);
    response.widget_info(|| WidgetInfo::selected(Role::RadioButton, true, selected, title));
    paint_choice(ui, &response, rect, selected, title, text);
    response.clicked()
}

/// Строка выбора в общей раскладке: по высоте вмещает текст.
pub fn choice(ui: &mut Ui, selected: bool, title: &str, text: &str) -> bool {
    let width = ui.available_width();
    let height = (paragraph_height(ui, text, Face::Body, 13.0, width - 58.0) + 44.0).max(62.0);
    let enabled = ui.is_enabled();
    let (rect, response) = allocate(ui, vec2(width, height), true);
    response.widget_info(|| WidgetInfo::selected(Role::RadioButton, enabled, selected, title));
    if enabled {
        paint_choice(ui, &response, rect, selected, title, text);
    } else {
        ui.scope(|ui| {
            ui.multiply_opacity(0.5);
            paint_choice(ui, &response, rect, selected, title, text);
        });
    }
    response.clicked()
}

/// Несколько взаимоисключающих значений в строку: выбранное чуть темнее, подпись ярче.
pub fn segments<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = palette(ui);
    let mut changed = false;
    let enabled = ui.is_enabled();
    ui.scope(|ui| {
        ui.set_min_height(32.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        for (option, label) in options {
            let selected = *value == *option;
            let width = text_width(ui, label, Face::Body, 14.0) + 24.0;
            let (rect, response) = allocate(ui, vec2(width, 32.0), true);
            response.widget_info(|| WidgetInfo::selected(Role::RadioButton, enabled, selected, *label));
            let lit = tween(ui.ctx(), (response.id, "lit"), selected, SOFT);
            let hover = tween(ui.ctx(), (response.id, "hover"), enabled && response.hovered(), SNAP);
            let dim = if enabled { 1.0 } else { 0.5 };
            let fill = mix(fade(p.sheet_hover, hover), p.select, lit);
            ui.painter().rect_filled(rect, CornerRadius::same(8), fade(fill, dim));
            let color = mix(p.ink_weak, p.ink, lit.max(hover));
            line(ui, label, Face::Body, 14.0, rect.left() + 12.0, rect.center().y, width, fade(color, dim));
            if response.has_focus() {
                focus_ring(ui, rect, 8.0, color);
            }
            if response.clicked() && !selected {
                *value = *option;
                changed = true;
            }
        }
    });
    changed
}

/// Пункт бокового списка разделов.
pub fn nav_item(ui: &mut Ui, selected: bool, label: &str) -> Response {
    let p = palette(ui);
    let (rect, response) = allocate(ui, vec2(ui.available_width(), 36.0), true);
    response.widget_info(|| WidgetInfo::selected(Role::Tab, true, selected, label));
    let lit = tween(ui.ctx(), (response.id, "lit"), selected, SOFT);
    let hover = tween(ui.ctx(), (response.id, "hover"), response.hovered(), SNAP);
    select_fill(ui, rect, lit, hover, Some(p.ink));
    let color = p.ink;
    let face = if selected { Face::Semi } else { Face::Body };
    line(ui, label, face, 14.0, rect.left() + 12.0, rect.center().y, rect.width() - 24.0, color);
    if response.has_focus() {
        focus_ring(ui, rect, 8.0, color);
    }
    response
}

/// Поле ввода на заданном месте. Текст и курсор делает `TextEdit`, рамка и заливка здесь.
pub fn input_at(ui: &mut Ui, key: &str, rect: Rect, value: &mut String, hint: &str) -> Response {
    let p = palette(ui);
    let id = Id::unique(("kit-input", key));
    let focused = ui.memory(|mem| mem.has_focus(id));
    let glow = tween(ui.ctx(), (id, "focus"), focused, SNAP);
    let radius = CornerRadius::same(9);
    ui.painter().rect_filled(rect, radius, fade(p.sheet_hover, 0.55 + 0.45 * glow));
    ui.painter().rect_stroke(rect, radius, Stroke::new(1.0 + glow, mix(p.rule_strong, p.ink, glow)), StrokeKind::Middle);
    let inner = rect.shrink2(vec2(12.0, 6.0));
    let mut edit = TextEdit::singleline(value)
        .id(id)
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .font(font(Face::Body, 14.0))
        .text_color(p.ink)
        .desired_width(inner.width())
        .align(Align2::LEFT_CENTER);
    if !hint.is_empty() {
        edit = edit.hint_text(hint);
    }
    // Отдельная область: поле уже заняло своё место, отступ до соседа не должен пропасть.
    ui.new_child(UiBuilder::new().max_rect(inner)).add(edit)
}

/// Поле ввода в общей раскладке.
pub fn input(ui: &mut Ui, key: &str, width: f32, value: &mut String, hint: &str) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 36.0), Sense::hover());
    input_at(ui, key, rect, value, hint)
}

/// Поле, которое нельзя изменить: при обновлении папка остаётся прежней.
pub fn read_only_field(ui: &Ui, rect: Rect, value: &str) {
    let p = palette(ui);
    ui.painter().rect_filled(rect, CornerRadius::same(9), p.sheet_hover);
    line(ui, value, Face::Body, 14.0, rect.left() + 12.0, rect.center().y, rect.width() - 24.0, p.ink_weak);
}

/// Что показывает строка списка серверов.
pub struct ServerRow<'a> {
    pub name: &'a str,
    pub doh_only: bool,
    pub status: Option<(String, Color32)>,
    pub in_use: bool,
    pub selected: bool,
    /// Насколько строка выбрана и под мышью, 0..1.
    pub lit: f32,
    pub hover: f32,
    pub alpha: f32,
    /// Насколько выделенная строка покраснела: сервер перестал отвечать.
    pub alert: f32,
    /// Заливку выделения рисует вызывающий: она едет через несколько строк.
    pub bare: bool,
}

/// Строка списка серверов. Её же рисует мастер установки на первой странице, поэтому
/// там показано то, что человек увидит в программе.
pub fn server_row(ui: &Ui, rect: Rect, row: &ServerRow) {
    let p = palette(ui);
    let a = row.alpha;
    if !row.bare {
        selection_slab(ui, rect, row.lit, row.hover, row.alert, a);
    }
    let y = rect.center().y;
    let (ink, weak) = (p.ink, p.ink_weak);
    let left = rect.left() + 14.0;
    let mut right = rect.right() - 14.0;
    if let Some((text, color)) = &row.status {
        right -= line_right(ui, text, Face::Body, 12.5, right, y, fade(*color, a)) + 8.0;
    }
    if row.in_use {
        // Галочка и слово вместо плашки: видно и без цвета, и не спорит с именем.
        let w = line_right(ui, "используется", Face::Body, 12.5, right, y, fade(p.ink, a));
        let c = pos2(right - w - 10.0, y);
        let s = Stroke::new(1.6, fade(p.ink, a));
        ui.painter().line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.2, 3.0)], s);
        ui.painter().line_segment([c + vec2(-1.2, 3.0), c + vec2(4.0, -3.5)], s);
        right -= w + 18.0 + 12.0;
    }
    let face = if row.selected { Face::Semi } else { Face::Body };
    let room = (right - left).max(40.0);
    let mut name_width = text_width(ui, row.name, face, 14.0);
    if row.doh_only {
        let tag = "только DoH";
        let tag_width = text_width(ui, tag, Face::Body, 12.5) + 16.0;
        name_width = name_width.min((room - tag_width - 8.0).max(40.0));
        let tag_left = left + name_width + 8.0;
        let tag_rect = Rect::from_min_size(pos2(tag_left, y - 10.0), vec2(tag_width, 20.0));
        ui.painter().rect_stroke(tag_rect, CornerRadius::same(4), Stroke::new(1.0, fade(p.rule_strong, 0.6 * a)), StrokeKind::Inside);
        line(ui, tag, Face::Body, 12.5, tag_rect.left() + 8.0, y, tag_width, fade(weak, a));
    }
    line(ui, row.name, face, 14.0, left, y, name_width.min(room) + 2.0, fade(ink, a));
}

/// Заливка выбранной строки списка с полоской слева. Мастер установки рисует её сам,
/// когда она едет со строки на строку. `alert` красит полоску и чуть подкрашивает заливку:
/// сервер перестал отвечать.
pub fn selection_slab(ui: &Ui, rect: Rect, lit: f32, hover: f32, alert: f32, alpha: f32) {
    let p = palette(ui);
    let base = mix(fade(p.sheet_hover, hover), p.select, lit);
    let fill = mix(base, mix(p.select, p.danger, 0.16), alert * lit);
    ui.painter().rect_filled(rect, CornerRadius::same(8), fade(fill, alpha));
    if lit > 0.01 {
        let h = (rect.height() * 0.42).min(18.0) * lit;
        let bar = Rect::from_center_size(pos2(rect.left() + 1.5, rect.center().y), vec2(3.0, h));
        ui.painter().rect_filled(bar, CornerRadius::same(2), fade(mix(p.ink, p.danger, alert), alpha));
    }
}

/// Значок закрытия и верхняя полоса окна: ячейка заливается красным при наведении.
pub fn close_cell(ui: &Ui, rect: Rect) -> bool {
    let p = palette(ui);
    let id = Id::unique("kit-close");
    let response = widget(ui, id, rect, true);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, "Закрыть"));
    let hover = tween(ui.ctx(), (id, "hover"), response.hovered(), SNAP);
    ui.painter().rect_filled(rect, CornerRadius::ZERO, fade(p.danger, hover));
    let color = mix(p.ink, p.on_danger, hover);
    let c = rect.center();
    let thin = Stroke::new(1.3, color);
    ui.painter().line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], thin);
    ui.painter().line_segment([c + vec2(5.0, -5.0), c + vec2(-5.0, 5.0)], thin);
    if response.has_focus() {
        focus_ring(ui, rect, 0.0, color);
    }
    response.clicked()
}

/// Windows 11 скругляет окно сам, если попросить: без рамки он этого не делает.
pub fn round_corners(title: &str) {
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;
    let title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if !hwnd.is_null() {
        let preference = DWMWCP_ROUND;
        let size = size_of_val(&preference) as u32;
        unsafe { DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&raw const preference).cast(), size) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_arrives_and_stops() {
        let mut spring = Spring::new(0.0);
        spring.target = 3.0;
        for _ in 0..300 {
            spring.step(1.0 / 60.0, SOFT);
        }
        assert_eq!(spring.value, 3.0);
    }

    #[test]
    fn long_frame_does_not_blow_up() {
        let mut spring = Spring::new(0.0);
        spring.target = 1.0;
        spring.step(2.0, BOUNCE);
        assert!(spring.value.abs() < 2.0);
    }

    #[test]
    fn easing_ends_are_exact() {
        assert!(ease_out(0.0).abs() < 1e-5);
        assert!((ease_out(1.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn calm_windows_land_at_once() {
        let ctx = Context::default();
        set_animated(&ctx, false);
        assert_eq!(pop(&ctx, "dot", BOUNCE), 1.0);
        assert!(!motion(&ctx, |m| m.moving));
    }
}
