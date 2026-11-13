//! Только для отладочной сборки: страницы мастера в PNG без окна. egui считает кадры сам
//! (`crate::shot`). Время идёт по шагам 1/60 с, шаги мастера не
//! выполняются, система не меняется.
//!
//!   dns-switcher.exe --render-installer папка [масштаб]

use super::{Maintenance, Mode, Page, StepState, Wizard};
use crate::kit;
use crate::shot::{Frames, STEP};
use eframe::egui::{self, Context, Event, Pos2, Theme, pos2};
use std::path::{Path, PathBuf};

struct Rig {
    frames: Frames,
    wizard: Wizard,
}

impl Rig {
    fn new(mode: Mode, ppp: f32, theme: Theme) -> Self {
        let ctx = Context::default();
        let mut wizard = Wizard::new(&ctx, mode);
        kit::set_animated(&ctx, true);
        wizard.headless = true;
        wizard.is_admin = true;
        wizard.existing = None;
        wizard.manages_dns = true;
        wizard.dir_text = r"C:\Program Files\DnsSwitcher".into();
        wizard.opts = crate::install::InstallOptions {
            dir: PathBuf::from(&wizard.dir_text),
            start_menu: true,
            desktop: true,
            no_uac: true,
            background: true,
        };
        wizard.un.reset_dns = true;
        Self { frames: Frames::new(ctx, (640.0, 640.0), ppp, theme), wizard }
    }

    fn frame(&mut self) {
        let wizard = &mut self.wizard;
        self.frames.frame(|ui| {
            wizard.ui(ui);
        });
    }

    fn run(&mut self, seconds: f32) {
        for _ in 0..(seconds / STEP).round() as usize {
            self.frame();
        }
    }

    fn hover(&mut self, at: Pos2) {
        self.frames.events.push(Event::PointerMoved(at));
        self.run(0.5);
    }

    fn click(&mut self, at: Pos2) {
        self.hover(at);
        for pressed in [true, false] {
            let button = egui::PointerButton::Primary;
            self.frames.events.push(Event::PointerButton { pos: at, button, pressed, modifiers: Default::default() });
            self.frame();
        }
        self.frame();
    }

    fn key(&mut self, key: egui::Key) {
        for pressed in [true, false] {
            let event = Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() };
            self.frames.events.push(event);
            self.frame();
        }
    }

    fn go(&mut self, page: Page) {
        let now = self.frames.now();
        self.wizard.go(page, now);
        self.frame();
    }

    fn shot(&self, path: &Path) {
        self.frames.shot(path);
    }
}

// ---------- Сценарий ----------

const FAILURE: &str = "Не удалось создать ярлык: доступ запрещён.";
const LONG_FAILURE: &str = "Не удалось заменить файл программы: он занят другим процессом. Закройте все окна программы и нажмите «Повторить», а если это не поможет, перезагрузите компьютер.";

fn set_steps(rig: &mut Rig, mode: Mode, done: usize, last: StepState) {
    rig.wizard.fake_steps(mode, done, last);
    rig.go(Page::Progress);
}

/// Страницы одной темы. `name` попадает в имя файла.
fn pages(out: &Path, theme: Theme, ppp: f32, suffix: &str) {
    let theme_name = if theme == Theme::Dark { "dark" } else { "light" };
    let file = |name: &str| out.join(format!("{name}-{theme_name}{suffix}.png"));

    // Полный круг, кадр каждые 0.8 с, на обеих формах страницы.
    for (form, update) in [("loop", false), ("loop-update", true)] {
        let mut rig = Rig::new(Mode::Install, ppp, theme);
        if update {
            rig.wizard.existing = Some(("0.9.0".into(), PathBuf::from(r"C:\Program Files\DnsSwitcher")));
        }
        for i in 0..=((super::demo::LOOP / 0.8) as usize) {
            rig.run(if i == 0 { 0.4 } else { 0.8 });
            rig.shot(&file(&format!("{form}-{i:02}")));
        }
    }

    // Когда идут кадры.
    let mut rig = Rig::new(Mode::Install, ppp, theme);
    rig.run(1.9);
    rig.shot(&file("welcome"));
    assert_eq!(kit::wanted_frames(&rig.frames.ctx), (false, true), "на удержании кадры ограничены");
    let running = rig.wizard.demo_clock;
    assert!(running > 1.5, "часы картинки не идут");
    rig.frames.focused = false;
    rig.run(1.0);
    assert_eq!(rig.wizard.demo_clock, running, "часы идут, когда окно не впереди");
    assert_eq!(kit::wanted_frames(&rig.frames.ctx), (false, false), "кадры идут, когда окно не впереди");
    assert!(!rig.frames.ctx.has_requested_repaint(), "заказан кадр, когда окно не впереди");
    rig.frames.focused = true;
    rig.frames.minimized = true;
    rig.run(0.5);
    assert_eq!(rig.wizard.demo_clock, running, "часы идут в свёрнутом окне");
    rig.frames.minimized = false;
    rig.run(0.4);
    assert!(rig.wizard.demo_clock > running, "картинка не продолжилась");
    rig.go(Page::Options);
    rig.run(2.0);
    assert_eq!(rig.wizard.demo_clock, 0.0, "часы идут на другой странице");
    assert_eq!(kit::wanted_frames(&rig.frames.ctx), (false, false), "кадры идут на другой странице");
    assert!(!rig.frames.ctx.has_requested_repaint(), "заказан кадр на другой странице");
    rig.go(Page::Welcome);
    rig.run(0.1);
    assert!(rig.wizard.demo_clock < 0.3, "круг не начался заново");
    rig.hover(pos2(530.0, 601.0));
    rig.shot(&file("welcome-hover-primary"));
    rig.hover(pos2(620.0, 18.0));
    rig.shot(&file("welcome-hover-close"));
    rig.hover(pos2(320.0, 320.0));
    rig.key(egui::Key::Tab);
    rig.key(egui::Key::Tab);
    rig.run(0.3);
    rig.shot(&file("welcome-focus"));

    // Анимации Windows выключены: всё сразу на месте, кадры не заказываются.
    let mut rig = Rig::new(Mode::Install, ppp, theme);
    kit::set_animated(&rig.frames.ctx, false);
    rig.frame();
    rig.shot(&file("welcome-still"));
    rig.run(0.2);
    assert!(!rig.frames.ctx.has_requested_repaint(), "мастер заказывает кадры, хотя ничего не движется");
    assert_eq!(rig.wizard.demo_clock, 0.0, "картинка идёт с выключенными анимациями");
    rig.wizard.existing = Some(("0.9.0".into(), PathBuf::from(r"C:\Program Files\DnsSwitcher")));
    rig.run(0.2);
    rig.shot(&file("update-still"));

    let mut rig = Rig::new(Mode::Install, ppp, theme);
    rig.wizard.is_admin = false;
    rig.run(2.2);
    rig.shot(&file("welcome-no-admin"));
    rig.go(Page::Options);
    rig.run(1.0);
    rig.shot(&file("options-no-admin"));

    let mut rig = Rig::new(Mode::Install, ppp, theme);
    rig.wizard.existing = Some(("0.9.0".into(), PathBuf::from(r"C:\Program Files\DnsSwitcher")));
    rig.run(1.0);
    rig.shot(&file("update"));
    rig.wizard.maintenance = Maintenance::Remove;
    rig.run(1.0);
    rig.shot(&file("update-remove"));
    rig.wizard.maintenance = Maintenance::Update;
    rig.go(Page::Options);
    rig.run(1.0);
    rig.shot(&file("options-update"));

    let mut rig = Rig::new(Mode::Install, ppp, theme);
    rig.go(Page::Options);
    rig.run(1.0);
    rig.shot(&file("options"));
    rig.wizard.dir_text = r"D:\Tools\DnsSwitcher".into();
    rig.run(1.0);
    rig.shot(&file("options-outside"));
    rig.wizard.dir_text = "Tools".into();
    rig.run(0.5);
    rig.shot(&file("options-relative"));

    // Настоящие щелчки по страницам. «Установить» не нажимается: шагов здесь нет вообще.
    let mut rig = Rig::new(Mode::Install, ppp, theme);
    rig.run(0.5);
    rig.click(pos2(533.0, 601.0));
    assert!(rig.wizard.page == Page::Options, "«Далее» не открыл параметры");
    rig.run(0.5);
    rig.click(pos2(60.0, 282.0));
    assert!(!rig.wizard.opts.start_menu, "переключатель не сработал");
    rig.click(pos2(250.0, 214.0));
    rig.frames.events.push(Event::Text("x".into()));
    rig.run(0.4);
    assert!(rig.wizard.dir_text.contains('x'), "поле не принимает текст");
    rig.shot(&file("options-typing"));
    rig.click(pos2(394.0, 601.0));
    assert!(rig.wizard.page == Page::Welcome, "«Назад» не вернул на первую страницу");

    let mut rig = Rig::new(Mode::Install, ppp, theme);
    set_steps(&mut rig, Mode::Install, 2, StepState::Running);
    // Поток шагов будто работает: закрыть нельзя, кнопок внизу нет.
    let (keep_alive, rx) = std::sync::mpsc::channel();
    rig.wizard.worker = Some(rx);
    assert!(!rig.wizard.can_close(), "окно закрывается посреди шагов");
    rig.run(1.2);
    rig.shot(&file("progress"));
    rig.wizard.states[2] = StepState::Failed(FAILURE.to_string());
    assert!(rig.wizard.can_close(), "после сбоя окно должно закрываться");
    rig.wizard.worker = None;
    drop(keep_alive);
    set_steps(&mut rig, Mode::Install, 2, StepState::Failed(FAILURE.to_string()));
    rig.run(1.0);
    rig.shot(&file("progress-failed"));
    set_steps(&mut rig, Mode::Install, 2, StepState::Failed(LONG_FAILURE.to_string()));
    rig.run(1.0);
    rig.shot(&file("progress-failed-long"));
    rig.wizard.opts.dir = PathBuf::from(r"C:\Program Files\DnsSwitcher");
    rig.go(Page::Finish);
    rig.run(1.0);
    rig.shot(&file("finish"));

    let mut rig = Rig::new(Mode::Uninstall, ppp, theme);
    rig.run(1.0);
    rig.shot(&file("uninstall"));
    set_steps(&mut rig, Mode::Uninstall, 3, StepState::Running);
    rig.run(1.2);
    rig.shot(&file("uninstall-progress"));
    rig.go(Page::Finish);
    rig.run(1.0);
    rig.shot(&file("uninstall-finish"));
}

pub fn run(dir: &str, scale: Option<&str>) {
    let out = Path::new(dir);
    let _ = std::fs::create_dir_all(out);
    pages(out, Theme::Light, 1.0, "");
    pages(out, Theme::Dark, 1.0, "");
    if let Some(ppp) = scale.and_then(|s| s.parse::<f32>().ok()) {
        pages(out, Theme::Light, ppp, &format!("-{ppp}x"));
    }
}
