//! Картинка на первой странице: как программа работает на деле. Строки серверов, метки, значки
//! состояния и узлы схемы те же, что в окне программы, данные придуманы, ничего не проверяется.
//! Серверы проверяются, используемый перестаёт отвечать, защита переводит на следующий,
//! всё снова работает, затем тот же итог глазами простого режима.
//! Всё считается из часов демонстрации: они стоят, пока окно не впереди, и начинаются заново
//! вместе со страницей.

use crate::kit::{self, Face, Mark, ServerRow, ease_out};
use crate::theme::{self, Palette};
use eframe::egui::{self, Color32, CornerRadius, Rect, Stroke, StrokeKind, pos2, vec2};

/// Высота картинки, если на странице хватает места.
pub const HEIGHT: f32 = 190.0;
/// Ниже этого картинка не читается, и страница обходится без неё.
pub const MIN_HEIGHT: f32 = 140.0;
/// Одна сцена: смена и удержание.
const SLOT: f32 = 3.2;
pub const LOOP: f32 = SCENES.len() as f32 * SLOT;
/// К этому времени смена сцены закончилась, дальше кадры идут реже.
const MORPH: f32 = 0.8;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Scene {
    /// Серверы проверяются, задержки приходят и меняются.
    Checking,
    /// Используемый сервер перестал отвечать.
    Failure,
    /// Защита переводит на следующий.
    Switching,
    /// Резервный работает, защита следит за ним.
    Working,
    /// То же самое в простом режиме.
    Simple,
}

const SCENES: [Scene; 5] = [Scene::Checking, Scene::Failure, Scene::Switching, Scene::Working, Scene::Simple];
/// С выключенными анимациями стоит этот момент: все серверы проверены и работают.
const STILL: f32 = 2.6;

/// Какая сцена идёт, какая была перед ней и сколько секунд прошло со смены.
fn moment(t: f32) -> (Scene, Scene, f32) {
    let t = t.rem_euclid(LOOP);
    let i = ((t / SLOT) as usize).min(SCENES.len() - 1);
    (SCENES[i], SCENES[(i + SCENES.len() - 1) % SCENES.len()], t - i as f32 * SLOT)
}

const SERVERS: [(&str, u32); 3] = [("GeoHide EU", 38), ("Comss DNS", 64), ("Cloudflare", 12)];

/// Задержка живая: меняется пару раз в секунду.
fn latency(i: usize, t: f32) -> u32 {
    let step = (t * 1.6).floor();
    let wobble = (step * 1.7 + i as f32 * 2.1).sin() * 3.0 + (step * 0.6 + i as f32).sin() * 2.0;
    (SERVERS[i].1 as f32 + wobble).max(4.0) as u32
}

fn working(i: usize, t: f32) -> String {
    format!("работает · {} мс", latency(i, t))
}

/// Подпись строки в этот момент истории.
fn status(p: &Palette, scene: Scene, tau: f32, t: f32, i: usize) -> (String, Color32) {
    let silent = ("не отвечает".to_string(), p.danger);
    let fine = (working(i, t), p.ink_weak);
    match scene {
        Scene::Checking if tau < 0.35 + i as f32 * 0.4 => ("проверка…".to_string(), p.ink_weak),
        Scene::Failure if i == 0 && tau >= 0.7 => silent,
        Scene::Switching | Scene::Working | Scene::Simple if i == 0 => silent,
        _ => fine,
    }
}

/// Строка защиты в окне программы.
fn protection(p: &Palette, scene: Scene, tau: f32) -> (Mark, String, Color32) {
    match scene {
        Scene::Checking if tau < 1.5 => (Mark::Neutral, "проверяю серверы, каждый адрес и прокси".to_string(), p.ink_weak),
        Scene::Checking => (Mark::Ok, "следит за «GeoHide EU», проверка каждые 30 с".to_string(), p.ink_weak),
        Scene::Failure if tau < 0.7 => (Mark::Ok, "следит за «GeoHide EU», проверка каждые 30 с".to_string(), p.ink_weak),
        Scene::Failure => (Mark::Bad, "«GeoHide EU» не отвечает, вторая проверка подряд".to_string(), p.danger),
        Scene::Switching if tau < 1.3 => (Mark::Warn, "выбираю лучший из остальных…".to_string(), p.warn),
        Scene::Switching => (Mark::Warn, "«GeoHide EU» не отвечал, сейчас работает резервный «Comss DNS»".to_string(), p.warn),
        _ => (Mark::Ok, "следит за «Comss DNS», проверка каждые 30 с".to_string(), p.ink_weak),
    }
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Где стоит выделение: 0 на первой строке, 1 на второй.
fn selection(scene: Scene, tau: f32) -> f32 {
    match scene {
        Scene::Checking => ease_in_out(1.0 - tau / MORPH),
        Scene::Failure => 0.0,
        Scene::Switching => ease_in_out((tau - 1.0) / 0.55),
        Scene::Working | Scene::Simple => 1.0,
    }
}

/// Насколько выделение покраснело.
fn alarm(scene: Scene, tau: f32) -> f32 {
    match scene {
        Scene::Failure => ((tau - 0.7) / 0.3).clamp(0.0, 1.0),
        Scene::Switching => 1.0 - ((tau - 1.0) / 0.45).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// Насколько на экране простой режим.
fn simple_share(scene: Scene, tau: f32) -> f32 {
    match scene {
        Scene::Simple => ease_in_out(tau / MORPH),
        Scene::Checking => 1.0 - ease_in_out(tau / MORPH),
        _ => 0.0,
    }
}

const ROW: f32 = 44.0;
const TOP: f32 = 46.0;

/// Строка, на которой стоит выделение.
fn block_at(rect: Rect, at: f32) -> Rect {
    Rect::from_min_size(pos2(rect.left(), rect.top() + TOP + at * (ROW + 2.0)), vec2(rect.width(), ROW))
}

const NODE_GAP: f32 = 30.0;

/// Узел простого режима.
fn node_at(rect: Rect, i: usize) -> Rect {
    let width = (rect.width() - NODE_GAP * 2.0) / 3.0;
    Rect::from_min_size(pos2(rect.left() + i as f32 * (width + NODE_GAP), rect.top() + 90.0), vec2(width, 62.0))
}

/// `morphing`: значок и выделение рисует переход, они превращаются в детали простого режима.
#[allow(clippy::too_many_arguments)]
fn list(ui: &egui::Ui, p: &Palette, rect: Rect, scene: Scene, tau: f32, t: f32, at: f32, alpha: f32, morphing: bool) {
    let x = rect.left();
    let width = rect.width();
    let y = rect.top();

    let (mark, text, color) = protection(p, scene, tau);
    let line = y + 14.0;
    if !morphing {
        kit::status_icon(ui.painter(), p, pos2(x + 9.0, line), 8.0, mark, alpha);
    }
    let title = "Защита от сбоев";
    let title_width = kit::text_width(ui, title, Face::Semi, 14.0);
    kit::line(ui, title, Face::Semi, 14.0, x + 26.0, line, title_width + 2.0, kit::fade(p.ink, alpha));
    let left = x + 26.0 + title_width + 10.0;
    kit::line(ui, &text, Face::Body, 13.0, left, line, x + width - left, kit::fade(color, alpha));
    ui.painter().hline(x..=x + width, y + 34.0, Stroke::new(1.0, kit::fade(p.rule, alpha)));

    // Выделение одно на список: оно едет со строки на строку, а при сбое краснеет.
    let red = alarm(scene, tau);
    if !morphing {
        kit::selection_slab(ui, block_at(rect, at), 1.0, 0.0, red, alpha);
    }
    for (i, (name, _)) in SERVERS.into_iter().enumerate() {
        let row = block_at(rect, i as f32);
        // Подпись меняет цвет, когда выделение накрыло строку больше чем наполовину.
        let lit = ((0.62 - (at - i as f32).abs()) / 0.24).clamp(0.0, 1.0);
        kit::server_row(ui, row, &ServerRow {
            name,
            doh_only: false,
            status: Some(status(p, scene, tau, t, i)),
            in_use: lit > 0.5,
            selected: lit > 0.5,
            lit,
            alert: if lit > 0.0 { red } else { 0.0 },
            bare: true,
            hover: 0.0,
            alpha,
        });
    }
}

/// Простой режим: итог одной фразой и цепочка узлов.
fn simple(ui: &egui::Ui, p: &Palette, rect: Rect, tau: f32, alpha: f32, morphing: bool) {
    let (x, y) = (rect.left(), rect.top());
    if !morphing {
        kit::status_icon(ui.painter(), p, pos2(x + 18.0, y + 24.0), 12.0, Mark::Ok, alpha);
    }
    kit::text_at(ui, "ИИ-сервисы открываются", Face::Semi, 26.0, pos2(x + 46.0, y + 8.0), kit::fade(p.ink, alpha));
    let text = "Работает через «Comss DNS». Защита от сбоев следит за ним, пока окно открыто.";
    kit::line(ui, text, Face::Body, 13.5, x + 46.0, y + 62.0, rect.width() - 46.0, kit::fade(p.ink_weak, alpha));

    let nodes = [("Компьютер", "Ethernet", Mark::Neutral), ("Comss DNS", "отвечает за 64 мс", Mark::Ok), ("ИИ-сервисы", "открываются", Mark::Ok)];
    let width = node_at(rect, 0).width();
    for (i, (title, detail, mark)) in nodes.into_iter().enumerate() {
        let appear = ease_out((tau - 0.3 - i as f32 * 0.12) / 0.3) * alpha;
        let node = node_at(rect, i);
        let stroke = p.rule;
        // Рамку среднего узла во время перехода рисует сам переход.
        if !(morphing && i == 1) {
            ui.painter().rect_stroke(node, CornerRadius::same(10), Stroke::new(1.0, kit::fade(stroke, appear)), StrokeKind::Inside);
        }
        kit::status_icon(ui.painter(), p, node.left_top() + vec2(21.0, 21.0), 9.0, mark, appear);
        kit::line(ui, title, Face::Semi, 14.0, node.left() + 38.0, node.top() + 21.0, width - 48.0, kit::fade(p.ink, appear));
        kit::line(ui, detail, Face::Body, 12.5, node.left() + 12.0, node.top() + 44.0, width - 24.0, kit::fade(p.ink_weak, appear));
        if i < 2 {
            let (from, to) = (pos2(node.right() + 5.0, node.center().y), pos2(node.right() + NODE_GAP - 5.0, node.center().y));
            kit::arrow(ui.painter(), kit::fade(p.ink_weak, appear), from, to);
        }
    }
}

/// Рисует картинку на момент `clock` и заказывает кадры, которые ей нужны.
pub fn draw(ui: &mut egui::Ui, rect: Rect, clock: f32) {
    let p = theme::palette(ui);
    let ctx = ui.ctx().clone();
    let ctx = &ctx;
    let calm = !kit::animated(ctx);
    let t = if calm { STILL } else { clock.rem_euclid(LOOP) };
    let (scene, _, tau) = moment(t);
    if !calm && kit::live(ctx) {
        if tau < MORPH || scene == Scene::Switching && tau < 1.7 {
            kit::keep_moving(ctx);
        } else {
            kit::ambient(ctx);
        }
    }
    // Самый первый круг начинается со списка, а не со смены простого режима.
    let first = !calm && clock < MORPH;
    let share = if first { 0.0 } else { simple_share(scene, tau) };
    let at = if first { 0.0 } else { selection(scene, tau) };
    let entrance = if first { ease_out(clock / 0.4) } else { 1.0 };
    let morphing = share > 0.0 && share < 1.0;

    let clip = rect.intersect(ui.clip_rect());
    let mut area = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    area.set_clip_rect(clip);
    // Сначала уходят строки, потом проявляется простой режим, наложения нет.
    if share < 0.45 {
        list(&area, p, rect, scene, tau, t, at, (1.0 - share / 0.45) * entrance, morphing);
    }
    if share > 0.45 {
        let shown = (share - 0.45) / 0.55;
        simple(&area, p, rect, tau + if scene == Scene::Checking { SLOT } else { 0.0 }, shown, morphing);
    }
    if morphing {
        // Значок растёт в значок заголовка, выделенная строка сжимается в узел цепочки.
        let c = rect.left_top() + vec2(9.0, 14.0);
        let c = c + (rect.left_top() + vec2(18.0, 24.0) - c) * share;
        kit::status_icon(area.painter(), p, c, 8.0 + 4.0 * share, Mark::Ok, 1.0);
        let (from, to) = (block_at(rect, at), node_at(rect, 1));
        let shape = Rect::from_min_max(from.min + (to.min - from.min) * share, from.max + (to.max - from.max) * share);
        let radius = CornerRadius::same(8 + (2.0 * share) as u8);
        area.painter().rect_filled(shape, radius, kit::fade(p.select, 1.0 - share));
        area.painter().rect_stroke(shape, radius, Stroke::new(1.0, kit::fade(p.rule, share)), StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_story_goes_in_order_and_starts_over() {
        let mut seen = Vec::new();
        let mut t = 0.0;
        while t < LOOP {
            let (scene, _, tau) = moment(t);
            assert!((0.0..=SLOT + 0.001).contains(&tau));
            if seen.last() != Some(&scene) {
                seen.push(scene);
            }
            t += 0.05;
        }
        assert_eq!(seen, SCENES);
        assert_eq!(moment(LOOP + 0.1).0, Scene::Checking);
        assert_eq!(moment(LOOP + 0.1).1, Scene::Simple);
    }

    #[test]
    fn the_selection_moves_only_once_the_failure_is_seen() {
        assert_eq!(selection(Scene::Failure, 2.0), 0.0);
        assert_eq!(selection(Scene::Switching, 0.5), 0.0);
        assert_eq!(selection(Scene::Switching, 3.0), 1.0);
        assert_eq!(alarm(Scene::Failure, 3.0), 1.0);
        assert_eq!(alarm(Scene::Working, 1.0), 0.0);
    }

    #[test]
    fn every_failure_is_said_in_words() {
        for scene in SCENES {
            for tau in [0.0, 1.0, 2.0, 3.0] {
                assert!(!status(&theme::LIGHT, scene, tau, 1.0, 0).0.is_empty());
                assert!(!protection(&theme::LIGHT, scene, tau).1.is_empty());
            }
        }
        assert_eq!(status(&theme::LIGHT, Scene::Working, 1.0, 1.0, 0).0, "не отвечает");
    }
}
