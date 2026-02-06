use crate::{
    backend::names,
    config::AppConfig,
    fb::trpmcatalog::CatalogDoc,
    paths::find_under,
    template::{preferred_template_dirs, DonorTemplate, Key, TemplateStore},
};
use eframe::egui;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
struct Row {
    key: Key,
    name: String,
    pm_variant: String,
    in_za: bool,
}

pub struct DonorsUi {
    tpl: DonorTemplate,
    tpl_path: PathBuf,
    store: TemplateStore,

    dirty: bool,
    last_edit: Instant,
    last_save: Instant,

    donors: Vec<Row>,
    targets: Vec<Row>,
    donor_by_key: BTreeMap<Key, Row>,

    current_donor: Option<Key>,
    target_selected: BTreeSet<usize>,
    last_clicked_target: Option<usize>,

    donor_search: String,
    target_search: String,
    show_in_za: bool,
}

impl DonorsUi {
    pub fn new(cfg: &AppConfig) -> Self {
        let dir = preferred_template_dirs()
            .into_iter()
            .find(|p| fs::create_dir_all(p).is_ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let store = TemplateStore::new(dir);
        let tpl_path = store.autosave_path();
        let mut tpl = store.load_or_default(Some(&tpl_path));
        if !cfg.language.trim().is_empty() {
            tpl.language = cfg.language.clone();
        }

        Self {
            tpl,
            tpl_path,
            store,
            dirty: false,
            last_edit: Instant::now(),
            last_save: Instant::now(),
            donors: Vec::new(),
            targets: Vec::new(),
            donor_by_key: BTreeMap::new(),
            current_donor: None,
            target_selected: BTreeSet::new(),
            last_clicked_target: None,
            donor_search: String::new(),
            target_search: String::new(),
            show_in_za: false,
        }
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
        if now.duration_since(self.last_edit) < Duration::from_millis(400) {
            return;
        }
        if now.duration_since(self.last_save) < Duration::from_millis(250) {
            return;
        }
        let _ = self.store.ensure_dir();
        let _ = self.store.save(&self.tpl, &self.tpl_path);
        self.dirty = false;
        self.last_save = now;
    }

    pub fn refresh_catalogs(&mut self, cfg: &AppConfig) {
        self.tpl.language = cfg.language.clone();

        let Some(za_dump) = cfg.za_dump.as_ref() else {
            return;
        };
        let Some(sv_root) = cfg.sv_root.as_ref() else {
            return;
        };

        let za_cat = find_under(
            za_dump,
            "ik_pokemon/catalog/catalog/poke_resource_table.trpmcatalog",
            "poke_resource_table.trpmcatalog",
        );
        let sv_cat = find_under(
            sv_root,
            "catalog/catalog/poke_resource_table.trpmcatalog",
            "poke_resource_table.trpmcatalog",
        );
        let (Ok(za_cat), Ok(sv_cat)) = (za_cat, sv_cat) else {
            return;
        };
        let (Ok(za_doc), Ok(sv_doc)) = (read_catalog_doc(&za_cat), read_catalog_doc(&sv_cat))
        else {
            return;
        };

        let name_map = names::load_monsname_map(za_dump, &self.tpl.language).unwrap_or_default();
        let za_keys: BTreeSet<Key> = za_doc.entries.iter().map(|e| Key::from(e.key)).collect();

        self.donors = build_rows(&za_doc, &name_map, &za_keys, true);
        self.targets = build_rows(&sv_doc, &name_map, &za_keys, false);
        self.donor_by_key = self.donors.iter().cloned().map(|r| (r.key, r)).collect();

        if self.current_donor.is_none() {
            self.current_donor = self.tpl.default_donor;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cfg: &mut AppConfig) {
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                self.refresh_catalogs(cfg);
            }

            ui.separator();
            ui.label("Template");

            if ui.button("Load…").clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("json", &["json"])
                    .pick_file()
                {
                    if let Ok(text) = fs::read_to_string(&p) {
                        if let Ok(tpl) = serde_json::from_str::<DonorTemplate>(&text) {
                            self.tpl = tpl;
                            self.tpl_path = p;
                            self.current_donor = self.tpl.default_donor;
                            self.target_selected.clear();
                            self.last_clicked_target = None;
                        }
                    }
                }
            }

            if ui.button("Save As…").clicked() {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("json", &["json"])
                    .save_file()
                {
                    let _ = self.store.save(&self.tpl, &p);
                    self.tpl_path = p;
                }
            }

            if ui.button("Open folder").clicked() {
                open_folder(self.tpl_path.parent().unwrap_or_else(|| Path::new(".")));
            }
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.tpl.include_targets_already_in_za,
                "Show already-in-ZA",
            );
            ui.add_space(8.0);
            ui.checkbox(&mut self.show_in_za, "Filter: only already-in-ZA");
            ui.add_space(8.0);
            ui.checkbox(&mut cfg.generate_reports, "Generate reports");
            ui.add_space(8.0);
            ui.checkbox(&mut cfg.no_head_look_at, "No head look-at (tralk)");
            if ui.button("Clear assignments").clicked() {
                self.tpl.assignments.clear();
                self.mark_dirty();
            }
        });

        ui.separator();

        let assignments = self.tpl.assignment_map();
        let selected_set = self.tpl.selected_set();

        ui.columns(2, |cols| {
            let left = &mut cols[0];
            left.heading("ZA donors");
            left.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.donor_search);
            });

            let avail_h = left.available_height();
            let min_palette_h = 150.0;
            let min_donor_h = 160.0;
            let donor_list_h = if avail_h > (min_donor_h + min_palette_h) {
                (avail_h * 0.70).clamp(min_donor_h, avail_h - min_palette_h)
            } else {
                (avail_h * 0.60).max(80.0)
            };
            left.allocate_ui(egui::vec2(left.available_width(), donor_list_h), |ui| {
                egui::ScrollArea::vertical()
                    .id_source("donors_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut clicked: Option<Key> = None;
                        let mut pin: Option<Key> = None;
                        for r in self
                            .donors
                            .iter()
                            .filter(|r| row_match(r, &self.donor_search))
                        {
                            let label =
                                format!("{}  (#{})  {}", r.name, r.key.species, r.pm_variant);
                            let is_cur = self.current_donor == Some(r.key);
                            let resp = ui.selectable_label(is_cur, label);
                            if resp.double_clicked() {
                                clicked = Some(r.key);
                                pin = Some(r.key);
                            } else if resp.clicked() {
                                clicked = Some(r.key);
                            }
                        }
                        if let Some(k) = clicked {
                            self.current_donor = Some(k);
                            self.tpl.default_donor = Some(k);
                            self.mark_dirty();
                        }
                        if let Some(k) = pin {
                            if !self.tpl.donor_palette.contains(&k) {
                                self.tpl.donor_palette.push(k);
                                self.mark_dirty();
                            }
                        }
                    });
            });

            left.allocate_ui(
                egui::vec2(left.available_width(), left.available_height()),
                |ui| {
                    ui.separator();
                    ui.heading("Palette");
                    ui.horizontal(|ui| {
                        if ui.button("Pin current").clicked() {
                            if let Some(k) = self.current_donor {
                                if !self.tpl.donor_palette.contains(&k) {
                                    self.tpl.donor_palette.push(k);
                                    self.mark_dirty();
                                }
                            }
                        }
                        if ui.button("Clear").clicked() {
                            self.tpl.donor_palette.clear();
                            self.mark_dirty();
                        }
                    });

                    let mut clicked_palette: Option<Key> = None;
                    egui::ScrollArea::vertical()
                        .id_source("palette_list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (i, k) in self.tpl.donor_palette.iter().copied().enumerate() {
                                let Some(r) = self.donor_by_key.get(&k) else {
                                    continue;
                                };
                                let label = format!("{}: {} (#{})", i + 1, r.name, r.key.species);
                                if ui
                                    .selectable_label(self.current_donor == Some(k), label)
                                    .clicked()
                                {
                                    clicked_palette = Some(k);
                                }
                            }
                        });

                    if let Some(k) = clicked_palette {
                        self.current_donor = Some(k);
                        self.tpl.default_donor = Some(k);
                        self.mark_dirty();
                    }
                },
            );

            let right = &mut cols[1];
            right.heading("SV targets");
            right.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.target_search);
            });
            right.horizontal(|ui| {
                if ui.button("Assign donor to selected").clicked() {
                    if let Some(dk) = self.current_donor {
                        for &idx in &self.target_selected {
                            if let Some(t) = self.targets.get(idx) {
                                self.tpl.set_assignment(t.key, dk);
                            }
                        }
                        self.mark_dirty();
                    }
                }
                if ui.button("Toggle selected as convert").clicked() {
                    for &idx in &self.target_selected {
                        if let Some(t) = self.targets.get(idx) {
                            toggle_selected(&mut self.tpl.selected_targets, t.key);
                        }
                    }
                    self.mark_dirty();
                }
            });

            let avail_h = right.available_height();
            let min_set_h = 160.0;
            let min_target_h = 160.0;
            let target_list_h = if avail_h > (min_target_h + min_set_h) {
                (avail_h * 0.70).clamp(min_target_h, avail_h - min_set_h)
            } else {
                (avail_h * 0.60).max(80.0)
            };
            right.allocate_ui(egui::vec2(right.available_width(), target_list_h), |ui| {
                egui::ScrollArea::vertical()
                    .id_source("targets_list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (idx, r) in self
                            .targets
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| row_match(r, &self.target_search))
                            .filter(|(_, r)| if self.show_in_za { r.in_za } else { true })
                            .filter(|(_, r)| {
                                if self.tpl.include_targets_already_in_za {
                                    true
                                } else {
                                    !r.in_za
                                }
                            })
                        {
                            let is_sel = self.target_selected.contains(&idx);
                            let is_enabled = selected_set.contains(&r.key);
                            let donor = assignments.get(&r.key).copied().or(self.tpl.default_donor);
                            let donor_s = donor
                                .and_then(|k| self.donor_by_key.get(&k).map(|r| r.name.clone()))
                                .unwrap_or_else(|| "-".to_string());

                            let label = format!(
                                "{}  (#{})  [{}]  donor: {}{}",
                                r.name,
                                r.key.species,
                                if is_enabled { "convert" } else { "skip" },
                                donor_s,
                                if r.in_za { "  (in ZA)" } else { "" }
                            );
                            let resp = ui.selectable_label(is_sel, label);
                            if resp.clicked() {
                                apply_selection_click(
                                    idx,
                                    resp.ctx.input(|i| i.modifiers.shift),
                                    resp.ctx.input(|i| i.modifiers.ctrl || i.modifiers.command),
                                    &mut self.target_selected,
                                    &mut self.last_clicked_target,
                                );
                            }
                        }
                    });
            });

            right.separator();
            right.heading(format!("Set Pokemon ({})", self.tpl.selected_targets.len()));
            right.horizontal(|ui| {
                if ui.button("Clear").clicked() {
                    self.tpl.selected_targets.clear();
                    self.target_selected.clear();
                    self.last_clicked_target = None;
                    self.mark_dirty();
                }
            });

            right.allocate_ui(
                egui::vec2(right.available_width(), right.available_height()),
                |ui| {
                    let idx_by_key: BTreeMap<Key, usize> = self
                        .targets
                        .iter()
                        .enumerate()
                        .map(|(i, r)| (r.key, i))
                        .collect();
                    let mut unset = Vec::<Key>::new();
                    let mut select_idx: Option<usize> = None;

                    egui::ScrollArea::vertical()
                        .id_source("set_targets_list")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for &k in &self.tpl.selected_targets {
                                let Some(r) = self.targets.iter().find(|r| r.key == k) else {
                                    continue;
                                };
                                let donor =
                                    assignments.get(&r.key).copied().or(self.tpl.default_donor);
                                let donor_s = donor
                                    .and_then(|k| self.donor_by_key.get(&k).map(|r| r.name.clone()))
                                    .unwrap_or_else(|| "-".to_string());

                                ui.horizontal(|ui| {
                                    if ui.small_button("Unset").clicked() {
                                        unset.push(r.key);
                                    }
                                    let label = format!(
                                        "{} (#{})  donor: {}",
                                        r.name, r.key.species, donor_s
                                    );
                                    if ui.selectable_label(false, label).clicked() {
                                        select_idx = idx_by_key.get(&r.key).copied();
                                    }
                                });
                            }
                        });

                    if let Some(idx) = select_idx {
                        self.target_selected.clear();
                        self.target_selected.insert(idx);
                        self.last_clicked_target = Some(idx);
                    }
                    if !unset.is_empty() {
                        for k in unset {
                            toggle_selected(&mut self.tpl.selected_targets, k);
                        }
                        self.mark_dirty();
                    }
                },
            );
        });

        ui.separator();
        ui.horizontal(|ui| {
            let legacy_label = "Legacy mode";
            if ui.checkbox(&mut cfg.legacy_mode, legacy_label).changed() {
                // no-op
            }
        });

        self.maybe_autosave();
    }
}

fn toggle_selected(list: &mut Vec<Key>, k: Key) {
    if let Some(i) = list.iter().position(|x| *x == k) {
        list.remove(i);
    } else {
        list.push(k);
    }
}

fn apply_selection_click(
    idx: usize,
    shift: bool,
    ctrl: bool,
    selected: &mut BTreeSet<usize>,
    last_clicked: &mut Option<usize>,
) {
    if shift {
        if let Some(last) = *last_clicked {
            let (a, b) = if last <= idx {
                (last, idx)
            } else {
                (idx, last)
            };
            for i in a..=b {
                selected.insert(i);
            }
        } else {
            selected.insert(idx);
        }
    } else if ctrl {
        if selected.contains(&idx) {
            selected.remove(&idx);
        } else {
            selected.insert(idx);
        }
        *last_clicked = Some(idx);
    } else {
        selected.clear();
        selected.insert(idx);
        *last_clicked = Some(idx);
    }
}

fn row_match(r: &Row, q: &str) -> bool {
    let q = q.trim();
    if q.is_empty() {
        return true;
    }
    let ql = q.to_ascii_lowercase();
    r.name.to_ascii_lowercase().contains(&ql)
        || r.pm_variant.to_ascii_lowercase().contains(&ql)
        || format!("{}", r.key.species).contains(&ql)
}

fn read_catalog_doc(path: &Path) -> anyhow::Result<CatalogDoc> {
    let b = fs::read(path)?;
    crate::fb::trpmcatalog::read_doc(b)
}

fn build_rows(
    doc: &CatalogDoc,
    name_map: &BTreeMap<u16, String>,
    za_keys: &BTreeSet<Key>,
    is_za: bool,
) -> Vec<Row> {
    let mut out = Vec::with_capacity(doc.entries.len());
    for e in &doc.entries {
        let key = Key::from(e.key);
        let name = name_map
            .get(&key.species)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("#{:#05}", key.species));
        let (_pm, pm_variant) = parse_pm_variant(&e.model_path).unwrap_or_default();
        out.push(Row {
            key,
            name,
            pm_variant,
            in_za: if is_za { true } else { za_keys.contains(&key) },
        });
    }
    out.sort_by_key(|r| (r.key.species, r.key.form, r.key.gender));
    out
}

fn parse_pm_variant(model_path: &str) -> Option<(String, String)> {
    let mp = model_path.replace('\\', "/");
    let mut parts = mp.split('/').filter(|s| !s.is_empty());
    let pm = parts.next()?.to_string();
    let pm_variant = parts.next()?.to_string();
    Some((pm, pm_variant))
}

fn open_folder(path: &Path) {
    #[cfg(target_os = "windows")]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";

    let _ = std::process::Command::new(cmd).arg(path).spawn();
}
