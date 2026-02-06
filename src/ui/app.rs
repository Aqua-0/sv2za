use crate::{
    backend,
    cancel::CancelToken,
    config::AppConfig,
    progress::{ProgressEvent, ProgressSink},
    ui::donors::DonorsUi,
};
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Donors,
    Legacy,
    Progress,
}

pub struct SvZaApp {
    cfg: AppConfig,
    last_save_err: Option<String>,
    dirty: bool,
    last_edit: Instant,
    last_save: Instant,

    running: bool,
    cancel: Option<CancelToken>,
    progress_rx: Option<std::sync::mpsc::Receiver<ProgressEvent>>,

    phase: String,
    done: u64,
    total: u64,
    logs: Vec<String>,

    tab: Tab,
    donors_ui: DonorsUi,
}

impl SvZaApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, cfg: AppConfig) -> Self {
        Self {
            donors_ui: DonorsUi::new(&cfg),
            cfg,
            last_save_err: None,
            dirty: false,
            last_edit: Instant::now(),
            last_save: Instant::now(),
            running: false,
            cancel: None,
            progress_rx: None,
            phase: String::new(),
            done: 0,
            total: 0,
            logs: Vec::new(),
            tab: Tab::Donors,
        }
    }

    fn drain_progress(&mut self) {
        let Some(rx) = &self.progress_rx else {
            return;
        };
        let mut events = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            events.push(ev);
        }

        for ev in events {
            match ev {
                ProgressEvent::PhaseStart { name } => {
                    self.phase = name.clone();
                    self.logs.push(format!("[phase] {name}"));
                    self.done = 0;
                    self.total = 0;
                }
                ProgressEvent::PhaseEnd { name } => {
                    self.logs.push(format!("[done] {name}"));
                }
                ProgressEvent::Progress { done, total } => {
                    self.done = done;
                    self.total = total;
                }
                ProgressEvent::Info { msg } => self.logs.push(msg),
                ProgressEvent::Warn { msg } => self.logs.push(format!("[warn] {msg}")),
                ProgressEvent::Error { msg } => self.logs.push(format!("[error] {msg}")),
                ProgressEvent::Finished { ok } => {
                    self.running = false;
                    self.cancel = None;
                    self.progress_rx = None;
                    self.logs.push(format!("[run] finished ok={ok}"));
                }
            }
        }
    }

    fn start_run(&mut self) {
        if self.running {
            return;
        }
        self.last_save_err = None;
        if let Err(e) = self.cfg.save() {
            self.last_save_err = Some(e.to_string());
        }

        let (sink, rx) = ProgressSink::new();
        let cancel = CancelToken::new();
        let cfg = self.cfg.clone();
        let reporter = sink.clone();

        self.running = true;
        self.cancel = Some(cancel.clone());
        self.progress_rx = Some(rx);

        std::thread::spawn(move || {
            let res = backend::run(&cfg, sink, cancel);
            if let Err(e) = res {
                reporter.error(format!("run failed: {e:#}"));
                reporter.finished(false);
            } else {
                reporter.finished(true);
            }
        });
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.last_edit = Instant::now();
    }

    fn maybe_autosave(&mut self) {
        if !self.dirty {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.last_edit) < Duration::from_millis(500) {
            return;
        }
        if now.duration_since(self.last_save) < Duration::from_millis(250) {
            return;
        }

        self.last_save_err = None;
        if let Err(e) = self.cfg.save() {
            self.last_save_err = Some(e.to_string());
            return;
        }
        self.dirty = false;
        self.last_save = now;
    }

    fn dir_picker_row(ui: &mut egui::Ui, label: &str, value: &mut Option<PathBuf>) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(label);
            let mut s = value
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            if ui.text_edit_singleline(&mut s).changed() {
                *value = if s.trim().is_empty() {
                    None
                } else {
                    Some(s.into())
                };
                changed = true;
            }
            if ui.button("…").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    *value = Some(p);
                    changed = true;
                }
            }
        });
        changed
    }

    fn file_picker_row(ui: &mut egui::Ui, label: &str, value: &mut Option<PathBuf>) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(label);
            let mut s = value
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            if ui.text_edit_singleline(&mut s).changed() {
                *value = if s.trim().is_empty() {
                    None
                } else {
                    Some(s.into())
                };
                changed = true;
            }
            if ui.button("…").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_file() {
                    *value = Some(p);
                    changed = true;
                }
            }
        });
        changed
    }
}

impl eframe::App for SvZaApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_progress();
        let mut cfg_changed = false;

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("SV2ZA Converter");
                ui.add_space(12.0);
                if self.running {
                    if ui.button("Cancel").clicked() {
                        if let Some(c) = &self.cancel {
                            c.cancel();
                        }
                    }
                } else if ui.button("Run").clicked() {
                    self.start_run();
                    self.tab = Tab::Progress;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.separator();

            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Donors, "Donors");
                ui.selectable_value(&mut self.tab, Tab::Legacy, "Legacy");
                ui.selectable_value(&mut self.tab, Tab::Progress, "Progress");
            });

            ui.separator();
            ui.label("Paths");
            cfg_changed |= Self::dir_picker_row(ui, "ZA dump", &mut self.cfg.za_dump);
            cfg_changed |= Self::dir_picker_row(ui, "SV dump", &mut self.cfg.sv_root);
            cfg_changed |= Self::dir_picker_row(ui, "Output", &mut self.cfg.out_root);
            ui.horizontal(|ui| {
                cfg_changed |= ui
                    .checkbox(&mut self.cfg.texture_convert, "Convert textures")
                    .changed();
                ui.add_enabled_ui(self.cfg.texture_convert, |ui| {
                    cfg_changed |= Self::file_picker_row(
                        ui,
                        "ultimate_tex_cli",
                        &mut self.cfg.ultimate_tex_cli,
                    );
                });
            });
            ui.add_enabled_ui(self.cfg.texture_convert, |ui| {
                cfg_changed |= ui
                    .checkbox(
                        &mut self.cfg.texture_allow_resize,
                        "Allow texture resize (icons)",
                    )
                    .changed();
            });
            cfg_changed |= Self::file_picker_row(ui, "flatc", &mut self.cfg.flatc);
            cfg_changed |=
                Self::dir_picker_row(ui, "pkNX personal dir", &mut self.cfg.pknx_personal_dir);
            ui.horizontal(|ui| {
                ui.label("Language (ik_message/dat/...)");
                cfg_changed |= ui.text_edit_singleline(&mut self.cfg.language).changed();
            });

            if cfg_changed {
                self.donors_ui.refresh_catalogs(&self.cfg);
            }

            ui.separator();
            match self.tab {
                Tab::Donors => {
                    self.donors_ui.ui(ui, &mut self.cfg);
                }
                Tab::Legacy => {
                    ui.add_enabled_ui(self.cfg.legacy_mode, |ui| {
                        ui.label("Toggles");
                        cfg_changed |= ui
                            .checkbox(&mut self.cfg.use_za_base_config, "Use ZA base-config donor")
                            .changed();
                        ui.add_enabled_ui(self.cfg.use_za_base_config, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Donor pm_variant");
                                cfg_changed |= ui
                                    .text_edit_singleline(&mut self.cfg.za_base_donor_pm_variant)
                                    .changed();
                            });
                        });
                        ui.add_enabled_ui(self.cfg.use_za_base_config, |ui| {
                            cfg_changed |= ui
                                .checkbox(
                                    &mut self.cfg.no_head_look_at,
                                    "No head look-at (ZA tralk patch)",
                                )
                                .changed();
                        });
                        cfg_changed |= ui
                            .checkbox(
                                &mut self.cfg.skip_pokemon_already_in_za,
                                "Skip pokemon already in ZA",
                            )
                            .changed();
                        ui.horizontal(|ui| {
                            ui.label("Donor dev (param arrays)");
                            let mut s = self.cfg.donor_dev.to_string();
                            if ui.text_edit_singleline(&mut s).changed() {
                                if let Ok(v) = s.trim().parse::<u32>() {
                                    self.cfg.donor_dev = v;
                                    cfg_changed = true;
                                }
                            }
                        });
                    });
                    if !self.cfg.legacy_mode {
                        ui.label("Legacy mode is off (enable it in the Donors tab).");
                    }
                }
                Tab::Progress => {
                    ui.label("Progress");
                    let pct = if self.total > 0 {
                        (self.done as f32) * 100.0 / (self.total as f32)
                    } else {
                        0.0
                    };
                    ui.label(format!(
                        "Phase: {}",
                        if self.phase.is_empty() {
                            "-"
                        } else {
                            &self.phase
                        }
                    ));
                    ui.add(egui::ProgressBar::new(pct / 100.0).text(format!("{pct:.1}%")));

                    if let Some(e) = &self.last_save_err {
                        ui.colored_label(egui::Color32::YELLOW, format!("config save failed: {e}"));
                    }

                    ui.separator();
                    ui.label("Logs");
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &self.logs {
                                ui.label(line);
                            }
                        });
                }
            }
        });

        if cfg_changed {
            self.mark_dirty();
        }
        self.maybe_autosave();
        ctx.request_repaint();
    }
}
