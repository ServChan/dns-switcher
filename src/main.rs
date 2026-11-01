#![windows_subsystem = "windows"]

mod apply;
mod browsers;
mod config;
mod doh;
mod elevate;
mod health;
mod install;
mod installer;
mod kit;
mod monitor;
mod net;
#[cfg(debug_assertions)]
mod shot;
mod state;
mod sys;
mod tasks;
mod theme;
mod tray;
mod ui;
mod update;

use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn main() -> eframe::Result {
    // Только для отладочной сборки: страницы мастера в PNG, без окна и без изменений в системе.
    #[cfg(debug_assertions)]
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.first().is_some_and(|a| a == "--render-installer") {
            installer::shot::run(args.get(1).map_or(".", String::as_str), args.get(2).map(String::as_str));
            return Ok(());
        }
        if args.first().is_some_and(|a| a == "--render-app") {
            ui::shot::run(args.get(1).map_or(".", String::as_str), args.get(2).map(String::as_str));
            return Ok(());
        }
    }
    // Окна консоли у программы нет: сбой без этого пропал бы бесследно.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        state::log(&format!("Сбой программы: {info}"));
        default_hook(info);
    }));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    // Фоновая защита из Планировщика заданий: без окна, только слежение.
    if has("--watch") {
        // Без прав администратора слежение заняло бы место настоящего, но не смогло
        // бы менять DNS: защита молча перестала бы работать.
        if !elevate::is_admin() {
            return Ok(());
        }
        config::prepare_data_dir();
        monitor::run_loop(Arc::new(AtomicBool::new(false)), false);
        return Ok(());
    }
    // Окно, запущенное установщиком после обновления: каждый ранний выход пишем
    // в журнал, чтобы было видно, почему окно не появилось.
    let after_update = has("--after-update");
    let note_exit = |why: &str| {
        if after_update {
            state::log(&format!("Окно после обновления не открыто: {why}."));
        }
    };
    if !elevate::ensure_admin(&args) {
        note_exit("перезапуск с правами администратора");
        return Ok(()); // работу продолжает копия с правами администратора
    }
    config::prepare_data_dir();
    // Тихое обновление из скачанного установщика: без окна мастера.
    if has("--update") {
        if let Err(e) = install::silent_update() {
            state::log(&format!("Обновление не удалось: {e}"));
            elevate::message_box(&format!("Обновление не удалось.\n\n{e}"));
        }
        return Ok(());
    }

    // Запуск не из папки установки открывает мастер (в отладочной сборке не мешает).
    let wizard = if has("--uninstall") {
        Some(installer::Mode::Uninstall)
    } else if cfg!(debug_assertions) {
        std::env::var_os("DNS_SWITCHER_DEBUG_INSTALLER").map(|_| installer::Mode::Install)
    } else if !install::running_installed() && !has("--portable") {
        Some(installer::Mode::Install)
    } else {
        None
    };
    // Второе окно программы не открываем, а выводим вперёд первое.
    // Отладочной сборке правило не мешает: её окно открывается рядом с установленным.
    let gui_lock = match wizard {
        Some(_) if after_update => {
            note_exit("программа запущена не из папки установки");
            return Ok(());
        }
        Some(_) => match elevate::setup_lock() {
            Some(lock) => Some(lock),
            None => {
                elevate::message_box("Установщик уже открыт. Завершите работу с ним.");
                return Ok(());
            }
        },
        None if cfg!(debug_assertions) => None,
        None => match elevate::single_instance(has("--wait-gui") || after_update) {
            Some(lock) => Some(lock),
            None => {
                note_exit("уже открыто другое окно программы");
                return Ok(());
            }
        },
    };

    let (title, size, min) = match wizard {
        Some(_) => (installer::INSTALLER_TITLE, installer::SIZE, installer::SIZE),
        None => (elevate::WINDOW_TITLE, MAIN_SIZE, [760.0, 560.0]),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(size)
            .with_min_inner_size(min)
            .with_resizable(wizard.is_none())
            .with_decorations(wizard.is_none())
            .with_icon(egui::IconData {
                rgba: include_bytes!("../assets/icon-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    let result = eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            let root = match wizard {
                Some(mode) => Root::Wizard { wizard: installer::Wizard::new(&cc.egui_ctx, mode), _lock: gui_lock },
                None => Root::Main { app: ui::DnsApp::new(&cc.egui_ctx), _lock: gui_lock },
            };
            Ok(Box::new(root))
        }),
    );
    // Окно не открылось (например, не запустилась графика): без записи это не
    // оставило бы следа.
    if let Err(e) = &result {
        state::log(&format!("Окно программы не открылось: {e}"));
    }
    result
}

const MAIN_SIZE: [f32; 2] = [980.0, 780.0];

enum Root {
    Wizard { wizard: installer::Wizard, _lock: Option<elevate::GuiLock> },
    Main { app: ui::DnsApp, _lock: Option<elevate::GuiLock> },
}

impl eframe::App for Root {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self {
            Root::Main { app, .. } => app.ui(ui),
            Root::Wizard { wizard, .. } => {
                if let installer::Choice::RunPortable = wizard.ui(ui) {
                    let ctx = ui.ctx().clone();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(elevate::WINDOW_TITLE.into()));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Resizable(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(MAIN_SIZE.into()));
                    // Окно программы уже открыто: выводим его вперёд, а мастер закрываем.
                    match elevate::single_instance(false) {
                        Some(lock) => *self = Root::Main { app: ui::DnsApp::new(&ctx), _lock: Some(lock) },
                        None => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    }
                }
            }
        }
    }
}
