//! Экран настроек: разделы слева, содержимое справа. Изменения сохраняются сразу.

use super::diagram::{NodeState, state_icon};
use super::*;

const SECTIONS: [(Section, &str); 7] = [
    (Section::Appearance, "Внешний вид"),
    (Section::Protection, "Защита от сбоев"),
    (Section::Checks, "Проверка серверов"),
    (Section::Notifications, "Уведомления"),
    (Section::Browsers, "Браузеры"),
    (Section::Updates, "Обновления"),
    (Section::About, "О программе"),
];

impl DnsApp {
    pub(super) fn settings_view(&mut self, ui: &mut egui::Ui, section: Section) {
        let p = palette(ui);
        ui.horizontal(|ui| {
            if kit::plain(ui, "Назад").clicked() {
                self.view = View::Main;
            }
            ui.label(RichText::new("Настройки").family(semibold()).size(21.0));
        });
        ui.add_space(12.0);
        ui.horizontal_top(|ui| {
            // Разделы.
            ui.vertical(|ui| {
                ui.set_width(200.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                for (s, title) in SECTIONS {
                    if kit::nav_item(ui, s == section, title).clicked() {
                        self.view = View::Settings(s);
                    }
                }
            });
            ui.add_space(8.0);
            let top = ui.min_rect().top();
            let x = ui.cursor().left();
            ui.painter().vline(x, top..=ui.max_rect().bottom(), Stroke::new(1.0, p.rule));
            ui.add_space(20.0);
            // Содержимое.
            let width = (ui.available_width()).min(760.0);
            egui::ScrollArea::vertical().auto_shrink([true, false]).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(width - 12.0);
                    match section {
                        Section::Appearance => self.settings_appearance(ui),
                        Section::Protection => self.settings_protection(ui, p),
                        Section::Checks => self.settings_checks(ui, p),
                        Section::Notifications => self.settings_notifications(ui, p),
                        Section::Browsers => self.settings_browsers(ui, p),
                        Section::Updates => self.settings_updates(ui, p),
                        Section::About => self.settings_about(ui, p),
                    }
                });
            });
        });
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Режим");
        let mut mode = self.config.ui_mode;
        if kit::choice(ui, mode == UiMode::Simple, "Простой", "Одна понятная карточка: всё ли работает, где проблема и кнопка «Подобрать лучший сервер».") {
            mode = UiMode::Simple;
        }
        if kit::choice(ui, mode == UiMode::Advanced, "Подробный", "Список всех серверов, ручной выбор, порядок резерва, проверка каждого адреса и прокси.") {
            mode = UiMode::Advanced;
        }
        if mode != self.config.ui_mode {
            self.config.ui_mode = mode;
            self.save_config();
        }

        ui.add_space(16.0);
        heading(ui, "Тема");
        let mut theme_choice = self.config.theme;
        ui.horizontal(|ui| {
            let themes = [(ThemeChoice::System, "Как в Windows"), (ThemeChoice::Light, "Светлая"), (ThemeChoice::Dark, "Тёмная")];
            kit::segments(ui, &mut theme_choice, &themes);
        });
        if theme_choice != self.config.theme {
            self.config.theme = theme_choice;
            apply_theme(ui.ctx(), theme_choice);
            self.save_config();
        }
    }

    fn settings_protection(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        let mut f = self.config.failover.clone();
        heading(ui, "Автопереключение");
        kit::toggle(ui, "Переключать на резервный сервер, если текущий работает плохо", &mut f.enabled);
        hint(
            ui,
            p,
            "«Плохо» значит: не отвечает, отвечает дольше 1,5 с или не открывает ИИ-сервисы две проверки подряд. \
             Если пропал весь интернет, программа ничего не переключает.",
        );
        ui.add_enabled_ui(f.enabled, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.set_min_height(32.0);
                ui.label("Проверять каждые");
                kit::segments(ui, &mut f.interval_secs, &INTERVALS);
            });
            ui.add_space(6.0);
            kit::toggle(ui, "Возвращаться к выбранному серверу, когда он снова заработает", &mut f.return_to_preferred);
            ui.add_space(10.0);
            ui.label("Если не подходит ни один сервер для ИИ:");
            if kit::choice(ui, f.last_resort == LastResort::Regular, "Переключать на обычные DNS", "Интернет работает, ИИ-сервисы в это время не открываются.") {
                f.last_resort = LastResort::Regular;
            }
            if kit::choice(ui, f.last_resort == LastResort::Dhcp, "Сразу переключать на DNS от роутера", "Если не отвечают и обычные DNS, всегда включается DNS от роутера, чтобы интернет не пропал.") {
                f.last_resort = LastResort::Dhcp;
            }
        });

        ui.add_space(16.0);
        heading(ui, "Страховка");
        kit::toggle(ui, "Дописывать запасные адреса после выбранного сервера", &mut f.safety_net);
        hint(
            ui,
            p,
            "Если выбранный сервер не ответит, Windows сама перейдёт на запасные адреса, даже когда программа \
             закрыта: сначала на другой рабочий сервер для ИИ (обход не прервётся), затем на 1.1.1.1 и 8.8.8.8 \
             (интернет не пропадёт). Действует со следующего применения сервера.",
        );

        ui.add_space(16.0);
        heading(ui, "Работа в фоне");
        let mut background = f.background;
        ui.add_enabled_ui(f.enabled && !self.task_busy, |ui| {
            kit::toggle(ui, "Следить, даже когда окно закрыто", &mut background);
        });
        hint(
            ui,
            p,
            "Запускается при входе в Windows (задание «DnsSwitcher Watchdog» в Планировщике). Значок в области \
             уведомлений показывает, что защита работает.",
        );
        if self.task_busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Настраиваю Планировщик…");
            });
        }

        ui.add_space(16.0);
        heading(ui, "Очередь резервных серверов");
        note(
            ui,
            p,
            "Резерв выбирается по проверке: сначала полностью рабочие, среди равных более надёжные по истории \
             последних проверок, затем стоящие выше в списке. Порядок меняется в подробном режиме кнопками \
             «Выше» и «Ниже».",
        );

        let background_changed = background != self.config.failover.background;
        if f.enabled != self.config.failover.enabled
            || f.interval_secs != self.config.failover.interval_secs
            || f.return_to_preferred != self.config.failover.return_to_preferred
            || f.last_resort != self.config.failover.last_resort
            || f.safety_net != self.config.failover.safety_net
        {
            self.config.failover = Failover { background: self.config.failover.background, ..f };
            self.save_config();
        }
        if background_changed && !self.task_busy {
            self.config.failover.background = background;
            self.save_config();
            self.set_background(&ctx, background);
        }
    }

    fn settings_checks(&mut self, ui: &mut egui::Ui, p: &Palette) {
        heading(ui, "Шифрование");
        if kit::toggle(ui, "Шифровать DNS-запросы (DoH)", &mut self.config.use_doh).changed() {
            self.save_config();
        }
        hint(
            ui,
            p,
            "Провайдер не видит и не подменяет запросы. Нужна Windows 11. Для серверов «только DoH» шифрование \
             обязательно. Действует со следующего «Применить» или переключения.",
        );

        ui.add_space(16.0);
        heading(ui, "Какие сервисы проверять");
        note(
            ui,
            p,
            "Через каждый сервер для ИИ программа проверяет, открываются ли эти сервисы: устанавливает с ними \
             защищённое соединение через прокси сервера. Чем больше сервисов, тем дольше проверка.",
        );
        ui.add_space(6.0);
        let defaults: Vec<String> = crate::config::default_probes().into_iter().map(|p| p.name).collect();
        let mut remove = None;
        let mut changed = false;
        for (i, probe) in self.config.probes.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                changed |= kit::toggle(ui, &probe.name, &mut probe.enabled).changed();
                ui.label(RichText::new(&probe.domain).monospace().color(p.ink_weak));
                if !defaults.contains(&probe.name) && kit::plain(ui, "Убрать").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.config.probes.remove(i);
            changed = true;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            kit::input(ui, "probe-name", 140.0, &mut self.new_probe.0, "Название");
            kit::input(ui, "probe-domain", 220.0, &mut self.new_probe.1, "адрес, например claude.ai");
            let name = self.new_probe.0.trim().to_string();
            let domain = crate::config::probe_domain(&self.new_probe.1);
            let valid = !name.is_empty() && domain.is_some();
            let domain = domain.unwrap_or_default();
            if kit::Button::new("Добавить", Tone::Plain).enabled(valid).show(ui).clicked() {
                self.config.probes.push(crate::config::Probe { name, domain, enabled: true });
                self.new_probe = Default::default();
                changed = true;
            }
        });
        if changed {
            self.save_config();
        }

        if !self.config.ipv6_disabled.is_empty() {
            ui.add_space(16.0);
            heading(ui, "IPv6");
            note(ui, p, "Программа выключила IPv6 на этих подключениях, чтобы сервисы не открывались мимо прокси.");
            for name in self.config.ipv6_disabled.clone() {
                ui.horizontal(|ui| {
                    ui.label(&name);
                    if kit::plain(ui, "Включить IPv6 обратно").clicked() {
                        self.set_ipv6(&name, true);
                    }
                });
            }
        }
    }

    fn settings_notifications(&mut self, ui: &mut egui::Ui, p: &Palette) {
        heading(ui, "Уведомления Windows");
        let mut changed = false;
        changed |= kit::toggle(ui, "О переключении на резервный сервер и возврате", &mut self.config.notify.switches).changed();
        changed |= kit::toggle(ui, "О проблемах, которые нужно исправить: браузер, IPv6", &mut self.config.notify.problems)
            .changed();
        if changed {
            self.save_config();
        }
        hint(
            ui,
            p,
            "Уведомления показывает процесс, который ведёт защиту: окно программы или фоновая защита. Их значок \
             виден в области уведомлений рядом с часами.",
        );
    }

    fn settings_browsers(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        heading(ui, "Браузеры и их DNS");
        note(
            ui,
            p,
            "Браузер со своим защищённым DNS не спрашивает Windows, поэтому обход в нём не работает. Исправление \
             переключает его на DNS Windows; шифрование запросов при этом делает Windows. Браузер должен быть \
             закрыт, рядом с его настройками остаётся резервная копия.",
        );
        ui.add_space(8.0);
        if self.browsers.is_empty() {
            ui.label(RichText::new("Браузеры не найдены или ещё проверяются.").color(p.ink_weak));
        }
        for b in self.browsers.clone() {
            ui.horizontal(|ui| {
                let state = if b.bypasses_windows() { NodeState::Warn } else { NodeState::Ok };
                let (r, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                state_icon(ui.painter(), p, r.center(), 8.0, state);
                ui.label(RichText::new(b.name).family(semibold()));
                let text = match b.mode {
                    browsers::DnsMode::Own => format!("свой DNS: {}", if b.template.is_empty() { "задан" } else { &b.template }),
                    browsers::DnsMode::Automatic => "«автоматически»: может сам перейти на Cloudflare или Google".into(),
                    browsers::DnsMode::System => "DNS Windows".into(),
                };
                ui.label(RichText::new(text).color(p.ink_weak));
            });
            if b.bypasses_windows() {
                ui.horizontal(|ui| {
                    ui.add_space(30.0);
                    if b.can_fix() {
                        let label = if b.running { format!("Закрыть {} и исправить", b.name) } else { format!("Исправить {}", b.name) };
                        let busy = self.fixing_browsers.contains(b.name);
                        if kit::Button::new(&label, Tone::Plain).enabled(!busy).show(ui).clicked() {
                            self.fix_browser(&ctx, &b);
                        }
                        if let Some((ok, note)) = self.browser_note.get(b.name) {
                            ui.label(RichText::new(note).small().color(if *ok { p.ink_weak } else { p.danger }));
                        }
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "Задано политикой организации. Вручную: {} → «Безопасный DNS» → выключить.",
                                    b.settings_url
                                ))
                                .small()
                                .color(p.ink_weak),
                            )
                            .wrap(),
                        );
                    }
                });
            }
            ui.add_space(4.0);
        }
        ui.add_space(8.0);
        if kit::plain(ui, "Проверить браузеры ещё раз").clicked() {
            self.start_browser_scan(&ctx);
        }
    }

    fn settings_updates(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let ctx = ui.ctx().clone();
        heading(ui, "Версия");
        ui.label(format!("Установлена версия {}.", install::VERSION));
        if !update::configured() {
            ui.add_space(6.0);
            note(
                ui,
                p,
                "Обновления включатся с первой подписанной версией из репозитория выпусков. Эта сборка их ещё не \
                 проверяет.",
            );
            return;
        }
        if let Some(m) = self.updates.available.clone().filter(|m| crate::update::newer(&m.version, crate::install::VERSION)) {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Доступна версия {}", m.version)).family(semibold()).color(p.ink));
            if !m.notes.is_empty() {
                ui.add(egui::Label::new(RichText::new(&m.notes).color(p.ink_weak)).wrap());
            }
            if accent_button(ui, "Обновить", self.update_busy.is_some(), self.update_busy.is_none(), false).clicked() {
                self.download_update(&ctx, m);
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if kit::Button::new("Проверить сейчас", Tone::Plain).enabled(self.update_busy.is_none()).show(ui).clicked() {
                self.check_update(&ctx);
            }
            if let Some(b) = &self.update_busy {
                ui.spinner();
                ui.label(b);
            }
        });

        ui.add_space(16.0);
        heading(ui, "Автоматически");
        let mut changed = false;
        changed |= kit::toggle(ui, "Раз в день проверять новую версию программы", &mut self.config.updates.app).changed();
        hint(ui, p, "Устанавливается только по вашей кнопке «Обновить». Файл проверяется цифровой подписью.");
        changed |= kit::toggle(ui, "Раз в день обновлять адреса серверов", &mut self.config.updates.server_list).changed();
        hint(
            ui,
            p,
            "Адреса таких DNS меняются. Подписанный список меняет только стандартные серверы, которые вы не \
             редактировали, и не возвращает удалённые вами.",
        );
        if changed {
            self.save_config();
        }
        ui.add_space(6.0);
        ui.label(RichText::new(format!("Версия списка серверов: {}", self.config.remote_list_version)).color(p.ink_weak));
        if kit::Button::new("Обновить список сейчас", Tone::Plain).enabled(self.update_busy.is_none()).show(ui).clicked() {
            self.update_list_now(&ctx);
        }
    }

    fn settings_about(&mut self, ui: &mut egui::Ui, p: &Palette) {
        heading(ui, install::APP_NAME);
        ui.label(format!("Версия {}", install::VERSION));
        ui.add_space(10.0);
        let data = crate::config::data_dir();
        ui.label(RichText::new("Настройки, список серверов и журнал:").color(p.ink_weak));
        ui.horizontal(|ui| {
            ui.label(RichText::new(data.to_string_lossy()).monospace());
            if kit::plain(ui, "Открыть папку").clicked() {
                crate::sys::open_in_explorer(&data);
            }
        });
        if let Some(dir) = install::installed_dir() {
            ui.add_space(6.0);
            ui.label(RichText::new("Программа установлена в:").color(p.ink_weak));
            ui.label(RichText::new(dir.to_string_lossy()).monospace());
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if kit::link(ui, "Как это работает", 14.0).clicked() {
                self.view = View::Help;
            }
            ui.label(RichText::new("·").color(p.ink_weak));
            if kit::link(ui, "Выпуски и список изменений", 14.0).clicked() {
                crate::sys::open_in_explorer(format!("https://github.com/{}/releases", update::REPO));
            }
        });
    }
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).family(semibold()).size(15.0));
    ui.add_space(4.0);
}

/// Пояснение под переключателем: по левому краю его подписи.
fn hint(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(42.0);
        ui.add(egui::Label::new(RichText::new(text).small().color(p.ink_weak)).wrap());
    });
}

/// Пояснение ко всему разделу.
fn note(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).small().color(p.ink_weak)).wrap());
}

pub(super) fn apply_theme(ctx: &egui::Context, t: ThemeChoice) {
    ctx.set_theme(match t {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}
