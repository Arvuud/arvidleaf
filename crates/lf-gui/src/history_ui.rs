//! History view: snapshots, changed files, line diff, restore.

use std::sync::Arc;

use eframe::egui::{self, RichText};
use lf_core::history::{self, Change, History, LineKind, Snapshot};

use crate::theme::Palette;

pub struct HistoryView {
    snapshots: Vec<Snapshot>,
    selected: Option<u64>,
    file: Option<String>,
    /// Compare with the current files instead of the previous snapshot.
    vs_current: bool,
    label_input: String,
    rename: Option<(u64, String)>,
    status: Option<(bool, String)>,
    current: std::collections::BTreeMap<String, String>,
    confirm_restore_all: bool,
}

pub enum HistoryAction {
    Close,
    /// Files on disk changed (reload editors).
    Restored,
}

fn fmt_time(ms: u64) -> String {
    use chrono::TimeZone;
    match chrono::Local.timestamp_millis_opt(ms as i64) {
        chrono::LocalResult::Single(t) => {
            let today = chrono::Local::now().date_naive();
            if t.date_naive() == today { t.format("Today %H:%M:%S").to_string() } else { t.format("%Y-%m-%d %H:%M").to_string() }
        }
        _ => ms.to_string(),
    }
}

impl HistoryView {
    pub fn new(h: &History) -> HistoryView {
        let mut v = HistoryView {
            snapshots: Vec::new(),
            selected: None,
            file: None,
            vs_current: false,
            label_input: String::new(),
            rename: None,
            status: None,
            current: Default::default(),
            confirm_restore_all: false,
        };
        v.refresh(h);
        v.selected = v.snapshots.last().map(|s| s.id);
        v
    }

    pub fn refresh(&mut self, h: &History) {
        self.snapshots = h.list();
        self.current = h.current().unwrap_or_default();
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, pal: &Palette, h: &Arc<History>) -> Option<HistoryAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("🕘 History").strong().size(16.0));
            ui.label(RichText::new(format!("{} snapshots", self.snapshots.len())).small().color(pal.dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Back to editor").clicked() {
                    action = Some(HistoryAction::Close);
                }
                if ui.button("📸 Snapshot").on_hover_text("Save the current state now (with the label, if given)").clicked() {
                    let label = Some(self.label_input.trim().to_string()).filter(|s| !s.is_empty());
                    match h.snapshot(label, false, "manual") {
                        Ok(Some(s)) => {
                            self.status = Some((true, "Snapshot saved.".into()));
                            self.selected = Some(s.id);
                        }
                        Ok(None) => self.status = Some((true, "Nothing changed since the last snapshot.".into())),
                        Err(e) => self.status = Some((false, e.to_string())),
                    }
                    self.label_input.clear();
                    self.refresh(h);
                }
                ui.add(egui::TextEdit::singleline(&mut self.label_input).hint_text("Label (optional)").desired_width(160.0));
            });
        });
        if let Some((ok, msg)) = &self.status {
            ui.colored_label(if *ok { pal.ok } else { pal.error }, msg);
        }
        ui.separator();

        egui::Panel::left("history-list").resizable(true).default_size(250.0).frame(egui::Frame::new()).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let list = self.snapshots.clone();
                for (i, s) in list.iter().enumerate().rev() {
                    let prev = if i > 0 { Some(&list[i - 1].files) } else { None };
                    let empty = Default::default();
                    let n = history::changes(prev.unwrap_or(&empty), &s.files).len();
                    let sel = self.selected == Some(s.id);
                    let frame = egui::Frame::new()
                        .fill(if sel { pal.accent.gamma_multiply(0.18) } else { pal.panel2 })
                        .corner_radius(6)
                        .inner_margin(6)
                        .stroke(egui::Stroke::new(1.0, if sel { pal.accent } else { pal.border }));
                    let r = frame
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if let Some(l) = &s.label {
                                ui.label(RichText::new(format!("🏷 {l}")).strong().color(pal.accent2));
                            }
                            ui.label(RichText::new(fmt_time(s.id)).strong());
                            let why = if s.auto { s.reason.clone() } else { "manual".into() };
                            ui.label(RichText::new(format!("{why} · {n} file{} changed", if n == 1 { "" } else { "s" })).small().color(pal.dim));
                        })
                        .response;
                    let r = ui.interact(r.rect, ui.scope_id().with(("snap", s.id)), egui::Sense::click());
                    if r.clicked() {
                        self.selected = Some(s.id);
                        self.confirm_restore_all = false;
                    }
                    r.context_menu(|ui| {
                        if ui.button("Set label…").clicked() {
                            self.rename = Some((s.id, s.label.clone().unwrap_or_default()));
                            ui.close();
                        }
                        if s.label.is_some() && ui.button("Remove label").clicked() {
                            let _ = h.set_label(s.id, None);
                            self.refresh(h);
                            ui.close();
                        }
                    });
                    ui.add_space(4.0);
                }
                if list.is_empty() {
                    ui.label(RichText::new("No snapshots yet. They are taken automatically after each successful compile and every 10 minutes while you edit.").color(pal.dim));
                }
            });
        });

        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin { left: 10, ..Default::default() })).show(ui, |ui| {
            let Some(idx) = self.selected.and_then(|id| self.snapshots.iter().position(|s| s.id == id)) else {
                ui.label(RichText::new("Select a snapshot.").color(pal.dim));
                return;
            };
            let snap = self.snapshots[idx].clone();
            let (base, base_name) = if self.vs_current {
                (self.current.clone(), "current files")
            } else if idx > 0 {
                (self.snapshots[idx - 1].files.clone(), "previous snapshot")
            } else {
                (Default::default(), "empty project")
            };
            let base = &base;
            // changes are shown from `base` to `snap` (or snap → current)
            let changes = if self.vs_current { history::changes(&snap.files, base) } else { history::changes(base, &snap.files) };
            ui.horizontal(|ui| {
                ui.label(RichText::new(fmt_time(snap.id)).strong());
                ui.label(RichText::new("compared with").color(pal.dim));
                if ui.selectable_label(!self.vs_current, "previous snapshot").clicked() {
                    self.vs_current = false;
                }
                if ui.selectable_label(self.vs_current, "current files").clicked() {
                    self.vs_current = true;
                    self.current = h.current().unwrap_or_default();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.confirm_restore_all {
                        if ui.button(RichText::new("Yes, restore everything").color(pal.warn)).clicked() {
                            match h.restore_all(&snap) {
                                Ok(()) => {
                                    self.status = Some((true, format!("Project restored to {}. The previous state was saved as a snapshot.", fmt_time(snap.id))));
                                    action = Some(HistoryAction::Restored);
                                }
                                Err(e) => self.status = Some((false, e.to_string())),
                            }
                            self.confirm_restore_all = false;
                            self.refresh(h);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_restore_all = false;
                        }
                    } else if ui.button("⟲ Restore project").on_hover_text("Make all files match this snapshot (the current state is saved first)").clicked() {
                        self.confirm_restore_all = true;
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if changes.is_empty() {
                    ui.label(RichText::new(format!("No differences to the {base_name}.")).color(pal.dim));
                }
                for (f, c) in &changes {
                    let (sym, col) = match c {
                        Change::Added => ("+", pal.ok),
                        Change::Removed => ("−", pal.error),
                        Change::Modified => ("~", pal.warn),
                    };
                    let sel = self.file.as_ref() == Some(f);
                    if ui.selectable_label(sel, RichText::new(format!("{sym} {f}")).color(col)).clicked() {
                        self.file = Some(f.clone());
                    }
                }
            });
            if self.file.as_ref().is_none_or(|f| !changes.iter().any(|(c, _)| c == f)) {
                self.file = changes.first().map(|(f, _)| f.clone());
            }
            ui.separator();
            let Some(file) = self.file.clone() else { return };
            let load = |files: &std::collections::BTreeMap<String, String>| files.get(&file).and_then(|hh| h.read_object(hh).ok());
            let (old, new) = if self.vs_current {
                (load(&snap.files), std::fs::read(h.root().join(&file)).ok())
            } else {
                (load(base), load(&snap.files))
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(&file).monospace().strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if snap.files.contains_key(&file) && ui.button("⟲ Restore this file").on_hover_text("Bring back this file as it was in the snapshot").clicked() {
                        match h.restore_file(&snap, &file) {
                            Ok(()) => {
                                self.status = Some((true, format!("{file} restored.")));
                                action = Some(HistoryAction::Restored);
                            }
                            Err(e) => self.status = Some((false, e.to_string())),
                        }
                        self.refresh(h);
                    }
                });
            });
            let text = |d: &Option<Vec<u8>>| d.as_ref().filter(|d| history::is_text(d)).map(|d| String::from_utf8_lossy(d).into_owned());
            let binary = old.as_ref().is_some_and(|d| !history::is_text(d)) || new.as_ref().is_some_and(|d| !history::is_text(d));
            if binary {
                ui.label(RichText::new("Binary file – no text diff.").color(pal.dim));
                return;
            }
            let diff = history::diff_lines(&text(&old).unwrap_or_default(), &text(&new).unwrap_or_default());
            let font = egui::FontId::monospace(12.5);
            // show changed lines with 3 lines of context, collapse the rest
            let near: Vec<bool> = (0..diff.len())
                .map(|i| diff[i.saturating_sub(3)..(i + 4).min(diff.len())].iter().any(|d| d.kind != LineKind::Same))
                .collect();
            egui::ScrollArea::both().auto_shrink([false, false]).id_salt("diff").show(ui, |ui| {
                let mut i = 0;
                while i < diff.len() {
                    if near[i] {
                        diff_row(ui, pal, &diff[i], &font);
                        i += 1;
                    } else {
                        let run = near[i..].iter().take_while(|n| !**n).count();
                        ui.label(RichText::new(format!("      … {run} unchanged line{}", if run == 1 { "" } else { "s" })).small().color(pal.dim));
                        i += run;
                    }
                }
            });
        });

        if let Some((id, mut label)) = self.rename.take() {
            let mut keep = true;
            egui::Modal::new(egui::Id::new("snap-label")).show(ui.ctx(), |ui| {
                ui.label("Label for this snapshot");
                let r = ui.text_edit_singleline(&mut label);
                r.request_focus();
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        let _ = h.set_label(id, Some(label.clone()));
                        self.refresh(h);
                        keep = false;
                    }
                    if ui.button("Cancel").clicked() {
                        keep = false;
                    }
                });
            });
            if keep {
                self.rename = Some((id, label));
            }
        }
        action
    }
}


fn diff_row(ui: &mut egui::Ui, pal: &Palette, d: &history::DiffLine, font: &egui::FontId) {
    let (bg, sign, col) = match d.kind {
        LineKind::Same => (egui::Color32::TRANSPARENT, ' ', pal.text),
        LineKind::Added => (pal.ok.gamma_multiply(0.15), '+', pal.text),
        LineKind::Removed => (pal.error.gamma_multiply(0.15), '-', pal.text),
    };
    let num = |n: Option<usize>| n.map_or("    ".to_string(), |n| format!("{n:>4}"));
    let text = format!("{} {} {} {}", num(d.old_no), num(d.new_no), sign, d.text);
    egui::Frame::new().fill(bg).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(text).font(font.clone()).color(col));
    });
}
