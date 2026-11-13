//! Мастер установки и удаления. Открывается, когда программа запущена не из
//! папки установки (например, DnsSwitcher-Setup.exe из «Загрузок»), или по
//! `--uninstall` из «Приложений» Windows.

mod demo;
#[cfg(debug_assertions)]
pub mod shot;

use crate::elevate;
use crate::install::{self, InstallOptions, Step, UninstallOptions};
use crate::theme;
use eframe::egui::{self, Color32, CornerRadius, Key, Rect, Stroke, StrokeKind, UiBuilder, pos2, vec2};
use crate::kit::{self, BOUNCE, Face, SOFT, Tone, ease_out};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

pub const INSTALLER_TITLE: &str = "Установка: Переключатель DNS";
/// Окно мастера: постоянный размер, рамки нет.
pub const SIZE: [f32; 2] = [640.0, 640.0];
const PAD: f32 = 32.0;
/// Колонка подписей на последней странице.
const KEY_WIDTH: f32 = 112.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Uninstall,
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Welcome,
    Options,
    UninstallConfirm,
    Progress,
    Finish,
}

#[derive(Clone, Copy, PartialEq)]
enum Maintenance {
    Update,
    Remove,
}

#[derive(Clone, PartialEq)]
enum StepState {
    Pending,
    Running,
    Done,
    Failed(String),
}

enum WorkerMsg {
    Started(usize),
    Finished(usize, Result<(), String>),
}

pub enum Choice {
    Stay,
    /// «Запустить без установки».
    RunPortable,
}

pub struct Wizard {
    mode: Mode,
    page: Page,
    /// Уже установленная версия и её папка.
    existing: Option<(String, PathBuf)>,
    maintenance: Maintenance,
    opts: InstallOptions,
    dir_text: String,
    un: UninstallOptions,
    manages_dns: bool,
    plan: Vec<Step>,
    states: Vec<StepState>,
    worker: Option<Receiver<WorkerMsg>>,
    browse: Option<Receiver<Option<PathBuf>>>,
    launch_after: bool,
    entered_uninstall_directly: bool,
    is_admin: bool,
    page_changed: f64,
    /// Сколько секунд идёт картинка на первой странице при этом её показе.
    demo_clock: f32,
    /// Отладочная отрисовка в PNG: шаги не запускаются.
    #[cfg(debug_assertions)]
    headless: bool,
}

impl Wizard {
    pub fn new(ctx: &egui::Context, mode: Mode) -> Self {
        theme::install(ctx);
        let existing = install::installed_version().zip(install::installed_dir());
        let dir = existing.as_ref().map(|(_, d)| d.clone()).unwrap_or_else(install::default_dir);
        let manages_dns = install::manages_dns();
        // При обновлении галочки показывают то, что установлено сейчас.
        let opts = if existing.is_some() {
            install::current_options()
        } else {
            InstallOptions { dir: dir.clone(), start_menu: true, desktop: true, no_uac: true, background: true }
        };
        Self {
            mode,
            page: if mode == Mode::Uninstall { Page::UninstallConfirm } else { Page::Welcome },
            existing,
            maintenance: Maintenance::Update,
            dir_text: dir.to_string_lossy().to_string(),
            opts,
            un: UninstallOptions { reset_dns: manages_dns, remove_settings: false },
            manages_dns,
            plan: Vec::new(),
            states: Vec::new(),
            worker: None,
            browse: None,
            launch_after: true,
            entered_uninstall_directly: mode == Mode::Uninstall,
            is_admin: elevate::is_admin(),
            page_changed: f64::NEG_INFINITY,
            demo_clock: 0.0,
            #[cfg(debug_assertions)]
            headless: false,
        }
        .debug_page()
    }

    /// Только для отладочной сборки: открыть нужную страницу для снимков экрана.
    #[cfg_attr(not(debug_assertions), allow(unused_mut))]
    fn debug_page(mut self) -> Self {
        #[cfg(debug_assertions)]
        {
            let page = std::env::var("DNS_SWITCHER_DEBUG_PAGE").unwrap_or_default();
            let fake_progress = |w: &mut Self, mode: Mode, failed: bool| {
                let last = if failed {
                    StepState::Failed("Не удалось создать ярлык: доступ запрещён.".into())
                } else {
                    StepState::Running
                };
                w.fake_steps(mode, 2, last);
                w.page = Page::Progress;
            };
            match page.as_str() {
                "options" => self.page = Page::Options,
                "progress" => fake_progress(&mut self, Mode::Install, false),
                "failed" => fake_progress(&mut self, Mode::Install, true),
                "finish" => self.page = Page::Finish,
                "uninstall" => {
                    self.mode = Mode::Uninstall;
                    self.manages_dns = true;
                    self.un.reset_dns = true;
                    self.page = Page::UninstallConfirm;
                }
                "update" => self.existing = Some(("0.9.0".into(), install::default_dir())),
                _ => {}
            }
        }
        self
    }

    /// Только для отладочной сборки: придуманные состояния шагов, настоящие шаги не идут.
    #[cfg(debug_assertions)]
    fn fake_steps(&mut self, mode: Mode, done: usize, last: StepState) {
        self.mode = mode;
        self.plan = match mode {
            Mode::Install => install::install_plan(),
            Mode::Uninstall => install::uninstall_plan(&UninstallOptions { reset_dns: true, remove_settings: false }),
        };
        self.states = vec![StepState::Pending; self.plan.len()];
        self.states[..done].fill(StepState::Done);
        self.states[done] = last;
    }

    fn go(&mut self, page: Page, now: f64) {
        self.page = page;
        self.page_changed = now;
    }

    // ---------- Работа ----------

    fn start_install(&mut self, ctx: &egui::Context, now: f64) {
        self.mode = Mode::Install;
        let mut dir = PathBuf::from(self.dir_text.trim());
        // Своя подпапка и для набранного вручную пути: программа не ложится россыпью
        // в общую папку вроде D:\Tools.
        if !self.updating() && !dir.ends_with("DnsSwitcher") {
            dir.push("DnsSwitcher");
        }
        if !install::is_protected_dir(&dir) {
            self.opts.no_uac = false;
            self.opts.background = false;
        }
        self.opts.dir = dir;
        self.plan = install::install_plan();
        self.run_from(ctx, 0, now);
    }

    fn start_uninstall(&mut self, ctx: &egui::Context, now: f64) {
        self.mode = Mode::Uninstall;
        self.plan = install::uninstall_plan(&self.un);
        self.run_from(ctx, 0, now);
    }

    /// Запускает шаги начиная с `first` (так работает и «Повторить»).
    fn run_from(&mut self, ctx: &egui::Context, first: usize, now: f64) {
        #[cfg(debug_assertions)]
        if self.headless {
            return;
        }
        if first == 0 {
            self.states = vec![StepState::Pending; self.plan.len()];
        }
        for s in &mut self.states[first..] {
            *s = StepState::Pending;
        }
        self.go(Page::Progress, now);
        let (tx, rx) = channel();
        let plan = self.plan.clone();
        let (mode, opts, dir) = (self.mode, self.opts.clone(), self.existing.as_ref().map(|(_, d)| d.clone()));
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            for (i, step) in plan.iter().enumerate().skip(first) {
                let _ = tx.send(WorkerMsg::Started(i));
                ctx.request_repaint();
                let result = match mode {
                    Mode::Install => install::run_install_step(*step, &opts),
                    Mode::Uninstall => install::run_uninstall_step(*step, dir.as_deref()),
                };
                let failed = result.is_err();
                let _ = tx.send(WorkerMsg::Finished(i, result));
                ctx.request_repaint();
                if failed {
                    break;
                }
            }
        });
        self.worker = Some(rx);
    }

    fn poll(&mut self, now: f64) {
        if let Some(rx) = &self.worker {
            loop {
                match rx.try_recv() {
                    Ok(WorkerMsg::Started(i)) => self.states[i] = StepState::Running,
                    Ok(WorkerMsg::Finished(i, Ok(()))) => self.states[i] = StepState::Done,
                    Ok(WorkerMsg::Finished(i, Err(e))) => self.states[i] = StepState::Failed(e),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    // Поток шагов упал: показываем сбой, иначе окно ждало бы вечно.
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        // После неудачного шага поток завершается сам: это не сбой.
                        let failed = self.states.iter().any(|s| matches!(s, StepState::Failed(_)));
                        if !failed {
                            if let Some(s) = self.states.iter_mut().find(|s| **s != StepState::Done) {
                                *s = StepState::Failed("внутренняя ошибка установщика".into());
                            }
                        }
                        break;
                    }
                }
            }
            if self.states.iter().all(|s| *s == StepState::Done) {
                self.worker = None;
                self.go(Page::Finish, now);
            }
        }
        if let Some(rx) = &self.browse {
            if let Ok(picked) = rx.try_recv() {
                self.browse = None;
                if let Some(mut dir) = picked {
                    if !dir.ends_with("DnsSwitcher") {
                        dir.push("DnsSwitcher");
                    }
                    self.dir_text = dir.to_string_lossy().to_string();
                }
            }
        }
    }

    fn browse_folder(&mut self) {
        let (tx, rx) = channel();
        let start = PathBuf::from(self.dir_text.trim());
        std::thread::spawn(move || {
            let start = start.parent().map(PathBuf::from).unwrap_or(start);
            let picked = rfd::FileDialog::new().set_title("Куда установить программу").set_directory(start).pick_folder();
            let _ = tx.send(picked);
        });
        self.browse = Some(rx);
    }

    fn failed_index(&self) -> Option<usize> {
        self.states.iter().position(|s| matches!(s, StepState::Failed(_)))
    }

    fn updating(&self) -> bool {
        self.existing.is_some()
    }

    /// Пока идут шаги, окно не закрывается, если только один из них не упал.
    fn can_close(&self) -> bool {
        self.worker.is_none() || self.failed_index().is_some()
    }

    // ---------- Отрисовка ----------

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Choice {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.poll(now);
        // Пока идут шаги, окно не закрывается: прерванная установка оставила бы
        // программу наполовину (файл скопирован, а записи в «Приложениях» нет).
        if !self.can_close() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.page_changed.is_infinite() {
            // Первый кадр: окно готово, схема на первой странице начинает расти отсюда.
            self.page_changed = now;
            kit::round_corners(INSTALLER_TITLE);
        }
        kit::begin_frame(&ctx);
        // Часы картинки идут только на её странице и когда окно впереди.
        if self.page != Page::Welcome {
            self.demo_clock = 0.0;
        } else if kit::animated(&ctx) && kit::live(&ctx) {
            self.demo_clock += ctx.input(|i| i.stable_dt).min(0.1);
        }
        let p = theme::palette(ui);
        let screen = ctx.content_rect();
        let (w, h) = (screen.width(), screen.height());
        ui.painter().rect_filled(screen, 0.0, p.ground);
        self.header(ui, w);

        let body = Rect::from_min_max(pos2(PAD, 172.0), pos2(w - PAD, h - 72.0));
        // Страница въезжает справа: шаг вперёд виден сам по себе.
        let phase = if kit::animated(&ctx) { ease_out(((now - self.page_changed) / 0.28) as f32) } else { 1.0 };
        if phase < 1.0 {
            kit::keep_moving(&ctx);
        }
        let mut page = ui.new_child(UiBuilder::new().max_rect(body.translate(vec2((1.0 - phase) * 26.0, 0.0))));
        page.set_clip_rect(Rect::from_min_max(pos2(0.0, body.top() - 8.0), pos2(w, body.bottom())));
        page.multiply_opacity(phase);
        let area = page.max_rect();
        let mut choice = Choice::Stay;
        match self.page {
            Page::Welcome => {
                if self.welcome(&mut page, area) {
                    choice = Choice::RunPortable;
                }
            }
            Page::Options => self.options(&mut page, area),
            Page::UninstallConfirm => self.uninstall_confirm(&mut page, area),
            Page::Progress => self.progress(&mut page, area, now),
            Page::Finish => self.finish(&mut page, area),
        }

        self.footer(ui, Rect::from_min_max(pos2(PAD, h - 58.0), pos2(w - PAD, h - 18.0)), now);
        self.caption(ui, w);
        kit::end_frame(&ctx);
        choice
    }

    /// Верхняя полоса таскает окно, правая ячейка закрывает его.
    fn caption(&mut self, ui: &mut egui::Ui, w: f32) {
        let ctx = ui.ctx().clone();
        let band = Rect::from_min_max(pos2(0.0, 0.0), pos2(w - 46.0, 36.0));
        if ctx.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|at| band.contains(at))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let cell = Rect::from_min_max(pos2(w - 46.0, 0.0), pos2(w, 36.0));
        if kit::close_cell(ui, cell) && self.can_close() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, w: f32) {
        let p = theme::palette(ui);
        let title = match self.mode {
            Mode::Install if self.updating() => "Обновление",
            Mode::Install => install::APP_NAME,
            Mode::Uninstall => "Удаление",
        };
        kit::text_at(ui, title, Face::Display, 52.0, pos2(PAD, 22.0), p.ink);
        kit::line_right(ui, &format!("Версия {}", install::VERSION), Face::Body, 12.5, w - PAD, 76.0, p.ink_weak);
        let stops: &[(&str, Page)] = match self.mode {
            Mode::Install => {
                &[("Начало", Page::Welcome), ("Параметры", Page::Options), ("Установка", Page::Progress), ("Готово", Page::Finish)]
            }
            Mode::Uninstall => &[("Что удалить", Page::UninstallConfirm), ("Удаление", Page::Progress), ("Готово", Page::Finish)],
        };
        let current = stops.iter().position(|(_, page)| *page == self.page).unwrap_or(0);
        let names: Vec<&str> = stops.iter().map(|(name, _)| *name).collect();
        let strip = Rect::from_min_max(pos2(PAD, 104.0), pos2(w - PAD, 152.0));
        route_strip(ui, strip, &names, current);
    }

    /// Картинка в оставшемся месте. Возвращает занятую высоту: без места картинки нет.
    fn demo(&self, ui: &mut egui::Ui, x: f32, y: f32, width: f32, room: f32) -> f32 {
        let height = room.min(demo::HEIGHT);
        if height < demo::MIN_HEIGHT {
            return 0.0;
        }
        demo::draw(ui, Rect::from_min_size(pos2(x, y), vec2(width, height)), self.demo_clock);
        height
    }

    fn welcome(&mut self, ui: &mut egui::Ui, area: Rect) -> bool {
        let p = theme::palette(ui);
        let (x, width) = (area.left(), area.width());
        let mut y = area.top();
        if !self.is_admin {
            y += admin_notice(ui, area) + 14.0;
        }
        if let Some((version, dir)) = self.existing.clone() {
            y += kit::paragraph(ui, "Программа уже установлена", Face::Semi, 21.0, pos2(x, y), width, p.ink) + 4.0;
            let note = format!("Версия {version} в папке {}.", dir.display());
            y += kit::paragraph(ui, &note, Face::Body, 14.0, pos2(x, y), width, p.ink_weak) + 16.0;
            let update = Rect::from_min_size(pos2(x, y), vec2(width, 62.0));
            let label = format!("Обновить до версии {}", install::VERSION);
            let selected = self.maintenance == Maintenance::Update;
            if kit::choice_at(ui, update, selected, &label, "Список серверов, настройки и журнал сохранятся.") {
                self.maintenance = Maintenance::Update;
            }
            let remove = update.translate(vec2(0.0, 70.0));
            let selected = self.maintenance == Maintenance::Remove;
            if kit::choice_at(ui, remove, selected, "Удалить программу", "Можно сразу вернуть DNS подключений на автоматический.") {
                self.maintenance = Maintenance::Remove;
            }
            let top = remove.bottom() + 10.0;
            self.demo(ui, x, top, width, area.bottom() - top);
            return false;
        }

        y += kit::paragraph(ui, "Быстрая смена DNS и защита от сбоев", Face::Semi, 21.0, pos2(x, y), width, p.ink) + 6.0;
        let text = "Переключайте DNS в один щелчок, открывайте ИИ-сервисы через специальные серверы \
                    и не теряйте интернет, если какой-то сервер перестал отвечать.";
        y += kit::paragraph(ui, text, Face::Body, 14.5, pos2(x, y), width, p.ink_weak) + 12.0;
        // Внизу остаются строка про шифрование и кнопка запуска без установки.
        y += self.demo(ui, x, y, width, area.bottom() - y - 12.0 - 66.0) + 12.0;
        let text = "DNS-over-HTTPS в Windows 11: провайдер не видит и не подменяет запросы.";
        y += kit::paragraph(ui, text, Face::Body, 14.5, pos2(x, y), width, p.ink_weak) + 8.0;
        let label = "Запустить без установки";
        let quiet = Rect::from_min_size(pos2(x - 12.0, y), vec2(kit::text_width(ui, label, Face::Body, 14.0) + 24.0, 32.0));
        kit::button_at(ui, "portable", quiet, label, Tone::Quiet, true).clicked()
    }

    fn options(&mut self, ui: &mut egui::Ui, area: Rect) {
        let p = theme::palette(ui);
        let (x, width) = (area.left(), area.width());
        let mut y = area.top();
        y += kit::paragraph(ui, "Папка установки", Face::Semi, 15.0, pos2(x, y), width, p.ink) + 6.0;
        let updating = self.updating();
        if updating {
            kit::read_only_field(ui, Rect::from_min_size(pos2(x, y), vec2(width, 36.0)), &self.dir_text);
        } else {
            let field = Rect::from_min_size(pos2(x, y), vec2(width - 104.0, 36.0));
            kit::input_at(ui, "install-dir", field, &mut self.dir_text, "");
            let browse = Rect::from_min_size(pos2(x + width - 96.0, y), vec2(96.0, 36.0));
            let picking = self.browse.is_some();
            if kit::button_at(ui, "browse", browse, "Обзор", Tone::Plain, !picking).clicked() {
                self.browse_folder();
            }
        }
        y += 42.0;
        let dir = PathBuf::from(self.dir_text.trim());
        let note = if updating {
            Some(("При обновлении папка не меняется.", p.ink_weak))
        } else if self.dir_text.trim().is_empty() {
            Some(("Укажите папку установки.", p.danger))
        } else if !dir.is_absolute() {
            Some((r"Укажите полный путь, например C:\Program Files\DnsSwitcher.", p.danger))
        } else if !install::is_protected_dir(&dir) {
            let text = "Папка вне Program Files: файл программы сможет подменить любая программа. \
                        Поэтому запуск без UAC и фоновая защита здесь недоступны. Надёжнее оставить Program Files.";
            Some((text, p.danger))
        } else {
            None
        };
        y += match note {
            Some((text, color)) => kit::paragraph(ui, text, Face::Body, 12.5, pos2(x, y), width, color) + 14.0,
            None => 8.0,
        };

        y += kit::paragraph(ui, "Ярлыки", Face::Semi, 15.0, pos2(x, y), width, p.ink) + 2.0;
        let half = (width / 2.0).floor();
        let menu = Rect::from_min_size(pos2(x, y), vec2(half, 32.0));
        kit::toggle_at(ui, "menu", menu, "В меню «Пуск»", &mut self.opts.start_menu, true);
        let desk = menu.translate(vec2(half, 0.0));
        kit::toggle_at(ui, "desktop", desk, "На рабочем столе", &mut self.opts.desktop, true);
        y += 32.0 + 14.0;

        y += kit::paragraph(ui, "Запуск и защита", Face::Semi, 15.0, pos2(x, y), width, p.ink) + 2.0;
        let tasks_allowed = install::is_protected_dir(&dir);
        if !tasks_allowed {
            self.opts.no_uac = false;
            self.opts.background = false;
        }
        let row = Rect::from_min_size(pos2(x, y), vec2(width, 32.0));
        kit::toggle_at(ui, "no-uac", row, "Открывать без запроса прав администратора (UAC)", &mut self.opts.no_uac, tasks_allowed);
        y += 32.0;
        let text = "Программе нужны права администратора, чтобы менять DNS. С этой настройкой ярлык \
                    открывает её сразу, без окна подтверждения: через задание в Планировщике.";
        y += kit::paragraph(ui, text, Face::Body, 12.5, pos2(x + 42.0, y), width - 42.0, p.ink_weak) + 6.0;
        let row = Rect::from_min_size(pos2(x, y), vec2(width, 32.0));
        kit::toggle_at(ui, "background", row, "Фоновая защита от сбоев", &mut self.opts.background, tasks_allowed);
        y += 32.0;
        let text = "Запускается при входе в Windows и работает, даже когда окно закрыто: если DNS-сервер \
                    перестал отвечать, переключает на резервный.";
        kit::paragraph(ui, text, Face::Body, 12.5, pos2(x + 42.0, y), width - 42.0, p.ink_weak);
    }

    fn uninstall_confirm(&mut self, ui: &mut egui::Ui, area: Rect) {
        let p = theme::palette(ui);
        let (x, width) = (area.left(), area.width());
        let mut y = area.top();
        if !self.is_admin {
            y += admin_notice(ui, area) + 14.0;
        }
        y += kit::paragraph(ui, "Удалить Переключатель DNS?", Face::Semi, 21.0, pos2(x, y), width, p.ink) + 6.0;
        let text = "Будут удалены программа, ярлыки, задания Планировщика и запись в «Приложениях» Windows.";
        y += kit::paragraph(ui, text, Face::Body, 14.5, pos2(x, y), width, p.ink_weak) + 20.0;
        if self.manages_dns {
            let row = Rect::from_min_size(pos2(x, y), vec2(width, 32.0));
            kit::toggle_at(ui, "reset-dns", row, "Вернуть DNS подключений на автоматический", &mut self.un.reset_dns, true);
            y += 34.0;
            let text = "Подключения снова получат DNS от роутера или провайдера. Если снять галочку, \
                        останутся текущие серверы, но следить за ними будет некому.";
            y += kit::paragraph(ui, text, Face::Body, 12.5, pos2(x + 42.0, y), width - 42.0, p.ink_weak) + 12.0;
        }
        let row = Rect::from_min_size(pos2(x, y), vec2(width, 32.0));
        kit::toggle_at(ui, "remove-settings", row, "Удалить список серверов, настройки и журнал", &mut self.un.remove_settings, true);
        let text = "Оставьте, если собираетесь установить программу снова.";
        kit::paragraph(ui, text, Face::Body, 12.5, pos2(x + 42.0, y + 34.0), width - 42.0, p.ink_weak);
    }

    fn progress(&mut self, ui: &mut egui::Ui, area: Rect, now: f64) {
        let p = theme::palette(ui);
        let (x, width) = (area.left(), area.width());
        let headline = match (self.mode, self.failed_index().is_some()) {
            (_, true) => "Не получилось",
            (Mode::Install, false) => "Устанавливаю…",
            (Mode::Uninstall, false) => "Удаляю…",
        };
        let mut y = area.top();
        y += kit::paragraph(ui, headline, Face::Semi, 21.0, pos2(x, y), width, p.ink) + 14.0;
        let count = self.plan.len();
        for (i, (step, state)) in self.plan.iter().zip(&self.states).enumerate() {
            let c = pos2(x + 12.0, y + 18.0);
            let fill = kit::tween(ui.ctx(), ("step-line", i), *state == StepState::Done, SOFT);
            // Текст ошибки раздвигает строки, линия тянется до следующей станции.
            let error = if let StepState::Failed(error) = state { kit::paragraph_height(ui, error, Face::Body, 14.0, width - 40.0) + 4.0 } else { 0.0 };
            if i + 1 < count {
                let pitch = 40.0 + error;
                ui.painter().line_segment([c, c + vec2(0.0, pitch)], Stroke::new(2.0, p.rule_strong));
                if fill > 0.01 {
                    ui.painter().line_segment([c, c + vec2(0.0, pitch * fill)], Stroke::new(5.0, p.ink));
                }
            }
            match state {
                StepState::Pending => kit::circle(ui, c, 7.0, p.ground, Stroke::new(2.0, p.rule_strong)),
                StepState::Running => {
                    // Станция, на которой идёт работа: вокруг неё расходится сигнал.
                    if kit::animated(ui.ctx()) {
                        let phase = (now / 0.7).fract() as f32;
                        let ring = Stroke::new(2.0, kit::fade(p.ink, 0.6 * (1.0 - phase)));
                        kit::circle(ui, c, 8.0 + 12.0 * ease_out(phase), Color32::TRANSPARENT, ring);
                        kit::keep_moving(ui.ctx());
                    }
                    kit::circle(ui, c, 8.0, p.sheet, Stroke::new(3.0, p.ink));
                }
                StepState::Done => {
                    let pop = kit::pop(ui.ctx(), ("step-done", i), BOUNCE);
                    kit::circle(ui, c, 8.0 * pop.max(0.2), p.ink, Stroke::NONE);
                }
                StepState::Failed(_) => {
                    kit::circle(ui, c, 10.0, p.danger, Stroke::NONE);
                    let cross = Stroke::new(2.0, p.on_danger);
                    ui.painter().line_segment([c + vec2(-3.5, -3.5), c + vec2(3.5, 3.5)], cross);
                    ui.painter().line_segment([c + vec2(3.5, -3.5), c + vec2(-3.5, 3.5)], cross);
                }
            }
            let (face, color) = match state {
                StepState::Pending => (Face::Body, p.ink_weak),
                StepState::Running => (Face::Semi, p.ink),
                StepState::Done => (Face::Body, p.ink),
                StepState::Failed(_) => (Face::Semi, p.danger),
            };
            kit::line(ui, step.title(), face, 15.0, c.x + 26.0, c.y, width - 40.0, color);
            y += 40.0;
            if let StepState::Failed(text) = state {
                kit::paragraph(ui, text, Face::Body, 14.0, pos2(c.x + 26.0, y - 8.0), width - 40.0, p.ink);
                y += error;
            }
        }
    }

    fn finish(&mut self, ui: &mut egui::Ui, area: Rect) {
        let p = theme::palette(ui);
        let (x, width) = (area.left(), area.width());
        let (title, text) = match self.mode {
            Mode::Install => (
                if self.updating() { "Программа обновлена" } else { "Программа установлена" },
                "Переключатель DNS есть в меню «Пуск». Выберите сервер и нажмите «Применить», \
                 после этого защита от сбоев начнёт следить за ним.",
            ),
            Mode::Uninstall => (
                "Программа удалена",
                if self.un.remove_settings {
                    "Удалены программа, задания, ярлыки и настройки."
                } else {
                    "Список серверов и настройки остались в %ProgramData%\\DnsSwitcher."
                },
            ),
        };
        let mut y = area.top();
        y += kit::paragraph(ui, title, Face::Semi, 21.0, pos2(x, y), width, p.ink) + 6.0;
        y += kit::paragraph(ui, text, Face::Body, 14.5, pos2(x, y), width, p.ink_weak) + 18.0;
        if self.mode == Mode::Install {
            let shortcuts = match (self.opts.start_menu, self.opts.desktop) {
                (true, true) => "меню «Пуск» и рабочий стол",
                (true, false) => "меню «Пуск»",
                (false, true) => "рабочий стол",
                (false, false) => "не созданы",
            };
            let dir = self.opts.dir.to_string_lossy().to_string();
            let rows = [
                ("Папка", dir.as_str()),
                ("Ярлыки", shortcuts),
                ("Запуск без UAC", if self.opts.no_uac { "включён" } else { "выключен" }),
                ("Фоновая защита", if self.opts.background { "включена" } else { "выключена" }),
            ];
            for (key, value) in rows {
                kit::line(ui, key, Face::Body, 12.5, x, y + 9.0, KEY_WIDTH - 6.0, p.ink_weak);
                y += kit::paragraph(ui, value, Face::Body, 14.0, pos2(x + KEY_WIDTH, y), width - KEY_WIDTH, p.ink).max(18.0) + 6.0;
            }
            let row = Rect::from_min_size(pos2(x, y + 10.0), vec2(width, 32.0));
            kit::toggle_at(ui, "launch", row, "Открыть Переключатель DNS", &mut self.launch_after, true);
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui, rect: Rect, now: f64) {
        let p = theme::palette(ui);
        let ctx = ui.ctx().clone();
        let close = || ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        let (screen_w, line_y) = (ctx.content_rect().width(), rect.top() - 12.0);
        ui.painter().hline(0.0..=screen_w, line_y, Stroke::new(1.0, p.rule));
        let enter = ui.input(|i| i.key_pressed(Key::Enter)) && !ctx.egui_wants_keyboard_input();
        let escape = ui.input(|i| i.key_pressed(Key::Escape));
        let running = self.worker.is_some() && self.failed_index().is_none();
        let (w, h) = (rect.width(), rect.height());
        let left = Rect::from_min_size(rect.min, vec2(120.0, h));
        let primary = Rect::from_min_size(rect.min + vec2(w - 150.0, 0.0), vec2(150.0, h));
        let second = Rect::from_min_size(rect.min + vec2(w - 270.0, 0.0), vec2(112.0, h));

        let can_cancel = !running && self.page != Page::Finish;
        let label = if self.page == Page::Progress { "Закрыть" } else { "Отмена" };
        if can_cancel && (kit::button_at(ui, "cancel", left, label, Tone::Plain, true).clicked() || escape) {
            close();
        }
        match self.page {
            Page::Welcome => {
                if kit::button_at(ui, "next", primary, "Далее", Tone::Primary, true).clicked() || enter {
                    let remove = self.updating() && self.maintenance == Maintenance::Remove;
                    self.go(if remove { Page::UninstallConfirm } else { Page::Options }, now);
                }
            }
            Page::Options => {
                let ok = !self.dir_text.trim().is_empty() && PathBuf::from(self.dir_text.trim()).is_absolute() && self.is_admin;
                let label = if self.updating() { "Обновить" } else { "Установить" };
                let tone = if ok { Tone::Primary } else { Tone::Plain };
                let resp = kit::button_at(ui, "install", primary, label, tone, ok);
                let resp = if self.is_admin { resp } else { resp.on_hover_text("Нужны права администратора.") };
                if resp.clicked() || (enter && ok) {
                    self.start_install(&ctx, now);
                }
                if kit::button_at(ui, "back", second, "Назад", Tone::Plain, true).clicked() {
                    self.go(Page::Welcome, now);
                }
            }
            Page::UninstallConfirm => {
                if kit::button_at(ui, "remove", primary, "Удалить", Tone::Danger, self.is_admin).clicked() {
                    self.start_uninstall(&ctx, now);
                }
                if !self.entered_uninstall_directly && kit::button_at(ui, "back", second, "Назад", Tone::Plain, true).clicked() {
                    self.go(Page::Welcome, now);
                }
            }
            Page::Progress => {
                if let Some(i) = self.failed_index()
                    && kit::button_at(ui, "retry", primary, "Повторить", Tone::Primary, true).clicked()
                {
                    self.run_from(&ctx, i, now);
                }
            }
            Page::Finish => {
                if kit::button_at(ui, "done", primary, "Готово", Tone::Primary, true).clicked() || enter {
                    if self.mode == Mode::Install && self.launch_after {
                        let _ = install::launch(&self.opts.dir);
                    }
                    close();
                }
            }
        }
    }
}

// ---------- Мелкие элементы ----------

/// Просит права администратора. Возвращает занятую высоту.
fn admin_notice(ui: &mut egui::Ui, area: Rect) -> f32 {
    const HEIGHT: f32 = 84.0;
    let p = theme::palette(ui);
    let rect = Rect::from_min_size(area.min, vec2(area.width(), HEIGHT));
    let radius = CornerRadius::same(10);
    ui.painter().rect_stroke(rect, radius, Stroke::new(1.0, p.danger), StrokeKind::Middle);
    let (x, y) = (rect.left() + 16.0, rect.top());
    kit::line(ui, "Нужны права администратора", Face::Semi, 15.0, x, y + 26.0, 250.0, p.danger);
    let text = "Без них не получится записать файлы в Program Files и изменить настройки Windows.";
    kit::line(ui, text, Face::Body, 13.0, x, y + 60.0, rect.width() - 32.0, p.ink_weak);
    let button = Rect::from_min_size(pos2(rect.right() - 12.0 - 304.0, y + 10.0), vec2(304.0, 32.0));
    if kit::button_at(ui, "relaunch", button, "Перезапустить с правами администратора", Tone::Plain, true).clicked() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if elevate::relaunch_as_admin(&args).is_ok() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    HEIGHT
}

/// Шаги мастера как участок линии: толстая линия дорастает до текущей станции.
fn route_strip(ui: &egui::Ui, rect: Rect, stops: &[&str], current: usize) {
    let p = theme::palette(ui);
    let count = stops.len().max(2);
    let y = rect.top() + 10.0;
    let x = |i: f32| rect.left() + 10.0 + (rect.width() - 20.0) * i / (count - 1) as f32;
    ui.painter().line_segment([pos2(x(0.0), y), pos2(x(count as f32 - 1.0), y)], Stroke::new(2.0, p.rule_strong));
    let front = kit::follow(ui.ctx(), "wizard-front", current as f32, SOFT);
    if front > 0.001 {
        kit::polyline(ui, &[pos2(x(0.0), y), pos2(x(front), y)], 6.0, p.ink);
    }
    for (i, name) in stops.iter().enumerate() {
        let c = pos2(x(i as f32), y);
        let passed = (front - i as f32 + 0.5).clamp(0.0, 0.5) * 2.0;
        if i == current {
            kit::circle(ui, c, 9.0, p.sheet, Stroke::new(3.0, p.ink));
            kit::circle(ui, c, 4.0, p.ink, Stroke::NONE);
        } else {
            kit::circle(ui, c, 6.5, kit::mix(p.ground, p.ink, passed), Stroke::new(2.0, kit::mix(p.rule_strong, p.ink, passed)));
        }
        let (face, color) = if i == current { (Face::Semi, p.ink) } else { (Face::Body, p.ink_weak) };
        let width = kit::text_width(ui, name, face, 13.0);
        let left = match i {
            0 => c.x - 10.0,
            _ if i + 1 == count => c.x + 10.0 - width,
            _ => c.x - width / 2.0,
        };
        kit::line(ui, name, face, 13.0, left, rect.top() + 34.0, width + 2.0, color);
    }
}
