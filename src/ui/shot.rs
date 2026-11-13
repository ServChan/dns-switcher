//! Только для отладочной сборки: экраны программы в PNG на придуманных данных. Окно не
//! создаётся, сеть, файлы и настройки Windows не трогаются: приложение собирается здесь
//! напрямую, без `DnsApp::new`, и `headless` выключает всё периодическое. Щелчков по кнопкам,
//! которые что-то делают, в сценарии нет.
//!
//!   dns-switcher.exe --render-app папка [масштаб]

use super::*;
use crate::health::{IpHealth, ServiceHealth};
use crate::shot::{Frames, STEP};
use eframe::egui::{Context, Event, Pos2, Theme};
use std::net::IpAddr;
use std::path::Path;

const SIZE: (f32, f32) = (980.0, 780.0);
const NARROW: (f32, f32) = (760.0, 560.0);

fn adapter(name: &str, guid: &str, up: bool, dns: &[&str], static_dns: bool) -> Adapter {
    let dns: Vec<IpAddr> = dns.iter().filter_map(|ip| ip.parse().ok()).collect();
    Adapter {
        index: 1,
        ipv6_index: 0,
        guid: guid.into(),
        name: name.into(),
        description: format!("{name}, тестовый адаптер"),
        up,
        has_gateway: up,
        doh_ips: dns.clone(),
        dns,
        static_dns,
        has_ipv6: false,
    }
}

fn service(name: &str, domain: &str, proxies: &[Result<u32, &str>]) -> ServiceHealth {
    ServiceHealth {
        name: name.into(),
        domain: domain.into(),
        proxied: Some(true),
        ipv6_leak: false,
        dns_error: None,
        addresses: Vec::new(),
        proxies: proxies.iter().map(|r| ("1.2.3.4".parse().unwrap(), r.map_err(|e| e.to_string()))).collect(),
    }
}

fn health(ips: &[(&str, Result<u32, &str>)], services: Vec<ServiceHealth>) -> HealthView {
    let ips = ips.iter().map(|(ip, r)| IpHealth { ip: ip.to_string(), result: r.map_err(|e| e.to_string()) }).collect();
    HealthView::Done(Health { method: Method::Doh, ips, services, plain_ms: None })
}

/// Приложение на придуманных данных: без потоков и без чтения системы.
fn fake_app(ctx: &Context) -> DnsApp {
    theme::install(ctx);
    let (tx, rx) = channel();
    let config = Config::default();
    let first = config.servers.first().map(|s| s.name.clone()).unwrap_or_default();
    let ip = config.servers.first().and_then(|s| s.ipv4.first().cloned()).unwrap_or_default();
    let mut app = DnsApp {
        runtime: RuntimeState { preferred: Some(first.clone()), active: Some(first.clone()), adapters: vec!["a".into()], on_dhcp_fallback: false },
        log_tail: vec![
            "01.11 14:02:11  Применён MagicDNS на Ethernet".into(),
            "01.11 14:20:43  Comss DNS не отвечал две проверки подряд, переключаю".into(),
        ],
        adapters: vec![adapter("Ethernet", "a", true, &[&ip, "1.1.1.1"], true), adapter("Wi-Fi", "b", false, &[], false)],
        adapter_error: None,
        chosen: HashSet::from(["a".to_string()]),
        show_inactive: false,
        selected: Some(0),
        editor: None,
        confirm: None,
        show_log: false,
        view: View::Main,
        browsers: Vec::new(),
        last_browsers: Instant::now(),
        e2e: None,
        updates: update::UpdateState::default(),
        update_busy: None,
        auto_picking: false,
        action_epoch: 0,
        new_probe: Default::default(),
        browser_note: HashMap::new(),
        fixing_browsers: HashSet::new(),
        ctx: ctx.clone(),
        config_stamp: None,
        status: None,
        busy: None,
        task_busy: false,
        health: HashMap::new(),
        last_refresh: Instant::now(),
        last_sweep: Instant::now(),
        tx,
        rx,
        is_admin: true,
        _watch_stop: Arc::new(AtomicBool::new(false)),
        headless: true,
        config,
    };
    app.config.ui_mode = UiMode::Advanced;
    app.config.theme = ThemeChoice::System;
    let good = |ms| health(&[("217.144.190.216", Ok(ms)), ("109.120.187.249", Ok(ms + 9))], vec![service("ChatGPT", "chatgpt.com", &[Ok(120), Ok(140)]), service("Claude", "claude.ai", &[Ok(130)])]);
    let names: Vec<String> = app.config.servers.iter().map(|s| s.name.clone()).collect();
    for (i, name) in names.iter().enumerate() {
        let view = match i {
            0 => good(38),
            1 => health(&[("83.220.169.155", Ok(64))], vec![service("ChatGPT", "chatgpt.com", &[Ok(120), Err("нет ответа")])]),
            2 => health(&[("45.13.237.213", Ok(1820))], vec![service("ChatGPT", "chatgpt.com", &[Ok(300)])]),
            3 => health(&[("37.221.93.128", Err("время ожидания истекло"))], Vec::new()),
            4 => health(&[("192.255.159.240", Ok(77))], vec![service("Gemini", "gemini.google.com", &[Err("сброс соединения")])]),
            5 => HealthView::Running,
            6 => {
                let mut h = health(&[("143.20.64.192", Err("нет соединения"))], Vec::new());
                if let HealthView::Done(h) = &mut h {
                    h.plain_ms = Some(45);
                }
                h
            }
            _ => good(12 + i as u32),
        };
        app.health.insert(name.clone(), view);
    }
    app
}

/// Новая версия для снимков. Ничего не скачивается: нажатий в сценарии нет.
fn fake_release() -> update::Manifest {
    let text = r#"{"version":"9.9.9","notes":"Исправления и новый вид окна.","url":"","sha256":"","signature":""}"#;
    serde_json::from_str(text).expect("описание выпуска")
}

struct Rig {
    frames: Frames,
    app: DnsApp,
}

impl Rig {
    fn new(size: (f32, f32), ppp: f32, theme: Theme) -> Self {
        let ctx = Context::default();
        let app = fake_app(&ctx);
        kit::set_animated(&ctx, true);
        Self { frames: Frames::new(ctx, size, ppp, theme), app }
    }

    fn run(&mut self, seconds: f32) {
        for _ in 0..(seconds / STEP).round() as usize {
            let app = &mut self.app;
            self.frames.frame(|ui| app.ui(ui));
        }
    }

    fn hover(&mut self, at: Pos2) {
        self.frames.events.push(Event::PointerMoved(at));
        self.run(0.6);
    }

    fn tab(&mut self, times: usize) {
        for _ in 0..times {
            for pressed in [true, false] {
                let key = egui::Key::Tab;
                self.frames.events.push(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Default::default() });
                self.run(STEP);
            }
        }
        self.run(0.4);
    }
}

fn long_names(app: &mut DnsApp) {
    app.config.servers[0].name = "Очень длинное название сервера, которое пользователь придумал сам и не сократил".into();
    app.config.servers[1].doh_only = true;
    app.config.servers[1].name = "Ещё один сервер с длинным именем и только DoH-адресом для проверки".into();
    app.runtime.active = Some(app.config.servers[0].name.clone());
    app.runtime.preferred = app.runtime.active.clone();
    app.adapters[0].name = "Подключение по локальной сети с очень длинным именем адаптера".into();
    let long = health(&[("37.230.192.51", Ok(38))], vec![service("ChatGPT", "chatgpt.com", &[Ok(120)])]);
    app.health.insert(app.config.servers[0].name.clone(), long);
}

fn browsers_fake(app: &mut DnsApp) {
    app.browsers = crate::browsers::preview();
}

fn scenes(out: &Path, theme: Theme, ppp: f32, suffix: &str) {
    let name = if theme == Theme::Dark { "dark" } else { "light" };
    let file = |scene: &str| out.join(format!("{scene}-{name}{suffix}.png"));
    let shot = |rig: &mut Rig, scene: &str| {
        rig.run(1.0);
        rig.frames.shot(&file(scene));
    };
    let rig = || Rig::new(SIZE, ppp, theme);

    // Подробный режим.
    let mut r = rig();
    shot(&mut r, "advanced");
    r.hover(pos2(250.0, 457.0));
    r.frames.shot(&file("advanced-hover"));
    r.hover(pos2(5.0, 5.0));
    r.tab(4);
    r.frames.shot(&file("advanced-focus"));
    r.tab(4);
    r.frames.shot(&file("advanced-focus2"));

    let mut r = rig();
    r.app.selected = None;
    shot(&mut r, "advanced-no-selection");

    let mut r = rig();
    r.app.config.servers.clear();
    r.app.selected = None;
    r.app.health.clear();
    r.app.runtime.active = None;
    r.app.runtime.preferred = None;
    r.app.adapter_error = Some("служба списка сетей не отвечает".into());
    r.app.log_tail.clear();
    shot(&mut r, "advanced-empty-error");

    let mut r = rig();
    long_names(&mut r.app);
    shot(&mut r, "advanced-long");

    let mut r = Rig::new(NARROW, ppp, theme);
    long_names(&mut r.app);
    shot(&mut r, "advanced-narrow");

    let mut r = rig();
    r.app.is_admin = false;
    browsers_fake(&mut r.app);
    r.app.status = Some(Status { kind: StatusKind::Error, text: "Не удалось применить сервер: Ethernet отказал в доступе.".into() });
    shot(&mut r, "advanced-banners");

    let mut r = rig();
    r.app.busy = Some("Применяю…".into());
    r.app.selected = Some(1);
    r.app.updates.available = Some(fake_release());
    r.app.update_busy = Some("Скачиваю 40 %".into());
    shot(&mut r, "advanced-busy");

    let mut r = rig();
    r.app.health.insert(r.app.config.servers[0].name.clone(), HealthView::Running);
    r.app.status = Some(Status { kind: StatusKind::Success, text: "GeoHide EU применён на Ethernet.".into() });
    shot(&mut r, "advanced-checking");

    // Простой режим.
    let simple = |r: &mut Rig| r.app.config.ui_mode = UiMode::Simple;
    let mut r = rig();
    simple(&mut r);
    shot(&mut r, "simple-ok");
    r.hover(pos2(150.0, 298.0));
    r.frames.shot(&file("simple-hover"));
    r.hover(pos2(5.0, 5.0));
    r.tab(3);
    r.frames.shot(&file("simple-focus"));

    let mut r = rig();
    simple(&mut r);
    r.app.runtime.active = None;
    r.app.runtime.preferred = None;
    r.app.adapters[0].static_dns = false;
    r.app.adapters[0].dns.clear();
    r.app.log_tail.clear();
    shot(&mut r, "simple-empty");

    let mut r = rig();
    simple(&mut r);
    let dead = health(&[("37.230.192.51", Err("время ожидания истекло"))], Vec::new());
    r.app.health.insert(r.app.config.servers[0].name.clone(), dead);
    r.app.status = Some(Status { kind: StatusKind::Warning, text: "Переключаю на резервный сервер.".into() });
    shot(&mut r, "simple-bad");

    let mut r = rig();
    simple(&mut r);
    r.app.auto_picking = true;
    r.app.health.insert(r.app.config.servers[0].name.clone(), HealthView::Running);
    shot(&mut r, "simple-busy");

    let mut r = rig();
    simple(&mut r);
    r.app.is_admin = false;
    shot(&mut r, "simple-no-admin");

    let mut r = rig();
    simple(&mut r);
    long_names(&mut r.app);
    shot(&mut r, "simple-long");

    let mut r = Rig::new(NARROW, ppp, theme);
    simple(&mut r);
    shot(&mut r, "simple-narrow");

    // Настройки.
    for (section, scene) in [
        (Section::Appearance, "settings-appearance"),
        (Section::Protection, "settings-protection"),
        (Section::Checks, "settings-checks"),
        (Section::Notifications, "settings-notifications"),
        (Section::Browsers, "settings-browsers"),
        (Section::Updates, "settings-updates"),
        (Section::About, "settings-about"),
    ] {
        let mut r = rig();
        r.app.view = View::Settings(section);
        browsers_fake(&mut r.app);
        r.app.updates.available = Some(fake_release());
        shot(&mut r, scene);
    }
    let mut r = rig();
    r.app.view = View::Settings(Section::Protection);
    r.app.config.failover.enabled = false;
    shot(&mut r, "settings-protection-off");
    let mut r = rig();
    r.app.view = View::Settings(Section::Browsers);
    shot(&mut r, "settings-browsers-empty");
    let mut r = Rig::new(NARROW, ppp, theme);
    r.app.view = View::Settings(Section::Protection);
    shot(&mut r, "settings-narrow");

    // Диалоги и справка.
    let mut r = rig();
    r.app.editor = Some(Editor::new_server());
    shot(&mut r, "dialog-editor");
    let mut r = rig();
    let mut editor = Editor::new_server();
    editor.error = Some("Сервер с таким названием уже есть. Выберите другое название.".into());
    r.app.editor = Some(editor);
    shot(&mut r, "dialog-editor-error");
    let mut r = rig();
    r.app.confirm = Some(Confirm::Delete("GeoHide EU".into()));
    shot(&mut r, "dialog-delete");
    let mut r = rig();
    r.app.confirm = Some(Confirm::Reset);
    shot(&mut r, "dialog-reset");
    let mut r = rig();
    r.app.show_log = true;
    shot(&mut r, "dialog-log");
    let mut r = rig();
    r.app.show_log = true;
    r.app.log_tail.clear();
    shot(&mut r, "dialog-log-empty");

    let mut r = rig();
    r.app.view = View::Help;
    shot(&mut r, "help");
}

/// Кадры для страницы проекта на сайте: тёмная тема, без тестовой кнопки обновления,
/// все серверы в рабочем состоянии. Поломки из сценариев выше приписали бы настоящим
/// сервисам то, чего с ними нет.
fn site(out: &Path) {
    let file = |scene: &str| out.join(format!("{scene}.png"));
    let rig = || {
        let mut r = Rig::new(SIZE, 1.0, Theme::Dark);
        let servers = r.app.config.servers.clone();
        for (i, s) in servers.iter().enumerate() {
            let ip = s.ipv4.first().cloned().unwrap_or_default();
            let ms = 24 + (i as u32 * 17) % 90;
            let services = if s.ai_bypass {
                vec![service("ChatGPT", "chatgpt.com", &[Ok(ms + 70)]), service("Claude", "claude.ai", &[Ok(ms + 85)])]
            } else {
                Vec::new()
            };
            r.app.health.insert(s.name.clone(), health(&[(ip.as_str(), Ok(ms))], services));
        }
        r.app.log_tail = vec!["13.11 14:02:11  Применён MagicDNS на Ethernet".into()];
        r
    };
    let shot = |r: &mut Rig, scene: &str| {
        r.run(1.0);
        r.frames.shot(&file(scene));
    };
    let mut r = rig();
    r.app.config.ui_mode = UiMode::Simple;
    shot(&mut r, "simple");
    let mut r = rig();
    shot(&mut r, "servers");
    for (section, scene) in [(Section::Protection, "failover"), (Section::Checks, "checks"), (Section::Browsers, "browsers")] {
        let mut r = rig();
        r.app.view = View::Settings(section);
        browsers_fake(&mut r.app);
        shot(&mut r, scene);
    }
}

/// `масштаб` = `site` рисует только кадры для сайта.
pub fn run(dir: &str, scale: Option<&str>) {
    let out = Path::new(dir);
    let _ = std::fs::create_dir_all(out);
    if scale == Some("site") {
        site(out);
        return;
    }
    scenes(out, Theme::Light, 1.0, "");
    scenes(out, Theme::Dark, 1.0, "");
    if let Some(ppp) = scale.and_then(|s| s.parse::<f32>().ok()) {
        scenes(out, Theme::Dark, ppp, &format!("-{ppp}x"));
    }
}
