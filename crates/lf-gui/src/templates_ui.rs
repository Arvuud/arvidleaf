//! Template gallery ("New Project") and template management.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions, vec2};
use lf_core::templates::{self, Template, TemplateMeta, Vars};
use lf_tex::Engine;

use crate::theme::Palette;

// ------------------------------------------------------------------ previews

/// Renders the first page of each template in the background
/// (cached in `~/.cache/arvidleaf/previews`).
pub struct Previews {
    tx: Sender<Template>,
    rx: Receiver<(String, Result<lf_pdf::Image, String>)>,
    textures: HashMap<String, Result<TextureHandle, String>>,
    requested: std::collections::HashSet<String>,
}

fn preview_key(t: &Template) -> String {
    format!("{}-{}", templates::slug(&t.id), t.content_hash())
}

fn build_preview(t: &Template) -> Result<lf_pdf::Image, String> {
    let dir = lf_core::paths::cache_dir().join("previews");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let key = preview_key(t);
    let cached = dir.join(format!("{key}.ppm"));
    if let Ok(d) = std::fs::read(&cached)
        && let Ok(img) = lf_pdf::parse_ppm(&d) {
            return Ok(img);
        }
    // a user template may ship its own preview.png
    if let Some(folder) = t.folder()
        && let Ok(img) = image::open(folder.join("preview.png")) {
            let rgb = img.thumbnail(320, 450).to_rgb8();
            return Ok(lf_pdf::Image { width: rgb.width() as usize, height: rgb.height() as usize, rgb: rgb.into_raw() });
        }
    let work = dir.join(format!("build-{key}"));
    let _ = std::fs::remove_dir_all(&work);
    let vars = Vars { title: "Title".into(), author: "Author".into(), project: "preview".into() };
    let project = templates::instantiate(t, &work, &vars).map_err(|e| e.to_string())?;
    let job = project.compile_job().ok_or("template has no main file")?;
    let out = lf_tex::compile::run(&job, &lf_tex::compile::Cancel::default());
    let pdf = out.pdf.ok_or_else(|| out.launch_error.unwrap_or_else(|| "did not compile".into()))?;
    let info = lf_pdf::info(&pdf)?;
    let w = info.pages.first().map_or(595.0, |p| p[0]);
    let img = lf_pdf::render_page(&pdf, 1, 320.0 / w * 72.0)?;
    let mut ppm = format!("P6\n{} {}\n255\n", img.width, img.height).into_bytes();
    ppm.extend_from_slice(&img.rgb);
    let _ = std::fs::write(&cached, ppm);
    let _ = std::fs::remove_dir_all(&work);
    Ok(img)
}

impl Previews {
    pub fn new(ctx: &egui::Context) -> Previews {
        let (tx, wrx) = channel::<Template>();
        let (wtx, rx) = channel();
        let c = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(t) = wrx.recv() {
                let r = build_preview(&t);
                if wtx.send((t.id.clone(), r)).is_err() {
                    return;
                }
                c.request_repaint();
            }
        });
        Previews { tx, rx, textures: HashMap::new(), requested: Default::default() }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((id, r)) = self.rx.try_recv() {
            let v = r.map(|img| ctx.load_texture(format!("tpl-{id}"), egui::ColorImage::from_rgb([img.width, img.height], &img.rgb), TextureOptions::LINEAR));
            self.textures.insert(id, v);
        }
    }

    fn get(&mut self, t: &Template) -> Option<&Result<TextureHandle, String>> {
        let key = format!("{}#{}", t.id, preview_key(t));
        if self.requested.insert(key) {
            self.textures.remove(&t.id);
            let _ = self.tx.send(t.clone());
        }
        self.textures.get(&t.id)
    }

    /// Forces a rebuild (e.g. after editing a template).
    pub fn invalidate(&mut self, id: &str) {
        self.textures.remove(id);
        self.requested.retain(|k| !k.starts_with(&format!("{id}#")));
    }
}

fn card_image(ui: &mut egui::Ui, pal: &Palette, previews: &mut Previews, t: &Template, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, Color32::WHITE);
    match previews.get(t) {
        Some(Ok(tex)) => {
            let ts = tex.size_vec2();
            let scale = (size.x / ts.x).min(size.y / ts.y);
            let r = egui::Rect::from_min_size(rect.min, ts * scale);
            p.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        Some(Err(e)) => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "no preview", egui::FontId::proportional(12.0), Color32::GRAY);
            ui.interact(rect, ui.scope_id().with(("tpl-err", &t.id)), egui::Sense::hover()).on_hover_text(e.lines().next().unwrap_or(""));
        }
        None => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "rendering…", egui::FontId::proportional(12.0), Color32::GRAY);
        }
    }
    p.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, pal.border), egui::StrokeKind::Inside);
}

// ------------------------------------------------------------------ gallery

pub struct Gallery {
    templates: Vec<Template>,
    search: String,
    category: Option<String>,
    selected: Option<String>,
    pub name: String,
    pub location: PathBuf,
    pub title: String,
    pub author: String,
    error: Option<String>,
}

pub enum GalleryResult {
    Created(PathBuf),
    Closed,
}

impl Gallery {
    pub fn new(cfg: &lf_core::config::Config) -> Gallery {
        let templates = templates::list(&cfg.hidden_templates);
        let selected = templates.iter().find(|t| t.id == "builtin:article-en").or(templates.first()).map(|t| t.id.clone());
        Gallery {
            templates,
            search: String::new(),
            category: None,
            selected,
            name: String::new(),
            location: cfg.projects_dir(),
            title: String::new(),
            author: cfg.author.clone(),
            error: None,
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette, previews: &mut Previews) -> Option<GalleryResult> {
        previews.poll(ctx);
        let mut result = None;
        let screen = ctx.content_rect();
        let size = vec2((screen.width() - 80.0).clamp(500.0, 1100.0), (screen.height() - 80.0).clamp(400.0, 760.0));
        let modal = egui::Modal::new(egui::Id::new("gallery")).show(ctx, |ui| {
            ui.set_width(size.x);
            ui.set_height(size.y);
            ui.horizontal(|ui| {
                ui.heading("New Project");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("×").clicked() {
                        result = Some(GalleryResult::Closed);
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("🔍 Search templates").desired_width(200.0));
                });
            });
            ui.separator();
            let mut cats: Vec<String> = self.templates.iter().map(|t| t.meta.category.clone()).collect();
            cats.dedup();
            cats.sort();
            cats.dedup();
            egui::Panel::left("gallery-cats").resizable(false).default_size(150.0).frame(egui::Frame::new()).show(ui, |ui| {
                if ui.selectable_label(self.category.is_none(), "All templates").clicked() {
                    self.category = None;
                }
                for c in &cats {
                    if ui.selectable_label(self.category.as_ref() == Some(c), c).clicked() {
                        self.category = Some(c.clone());
                    }
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Your own templates live in\nTemplates › Manage Templates…").small().color(pal.dim));
            });
            egui::Panel::right("gallery-form").resizable(false).default_size(260.0).frame(egui::Frame::new().inner_margin(egui::Margin { left: 12, ..Default::default() })).show(ui, |ui| {
                let sel = self.selected.as_ref().and_then(|id| self.templates.iter().find(|t| &t.id == id)).cloned();
                if let Some(t) = &sel {
                    ui.label(RichText::new(&t.meta.name).strong().size(16.0));
                    ui.label(RichText::new(format!("{} · {}", t.meta.category, t.meta.engine.label())).small().color(pal.dim));
                    ui.add_space(4.0);
                    ui.label(&t.meta.description);
                    ui.add_space(10.0);
                }
                egui::Grid::new("new-project-form").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
                    ui.label("Project name");
                    ui.add(egui::TextEdit::singleline(&mut self.name).hint_text("my-paper").desired_width(160.0));
                    ui.end_row();
                    ui.label("Title");
                    ui.add(egui::TextEdit::singleline(&mut self.title).hint_text(if self.name.is_empty() { "Title" } else { self.name.as_str() }).desired_width(160.0));
                    ui.end_row();
                    ui.label("Author");
                    ui.add(egui::TextEdit::singleline(&mut self.author).desired_width(160.0));
                    ui.end_row();
                    ui.label("Location");
                    ui.horizontal(|ui| {
                        let shown = self.location.display().to_string();
                        ui.label(RichText::new(short_path(&shown, 18)).small()).on_hover_text(&shown);
                        if ui.small_button("…").clicked()
                            && let Some(p) = rfd::FileDialog::new().set_directory(&self.location).pick_folder() {
                                self.location = p;
                            }
                    });
                    ui.end_row();
                });
                ui.add_space(10.0);
                let bad_name = self.name.contains(['/', '\\']) || self.name.trim().starts_with('.');
                if bad_name {
                    ui.colored_label(pal.error, "The project name cannot contain / or start with a dot.");
                }
                let ok = sel.is_some() && !self.name.trim().is_empty() && !bad_name;
                if ui.add_enabled(ok, egui::Button::new(RichText::new("Create project").strong()).min_size(vec2(160.0, 30.0))).clicked() {
                    let t = sel.clone().unwrap();
                    let name = self.name.trim().to_string();
                    let dest = self.location.join(&name);
                    let title = if self.title.trim().is_empty() { name.clone() } else { self.title.clone() };
                    match templates::instantiate(&t, &dest, &Vars { title, author: self.author.clone(), project: name }) {
                        Ok(_) => result = Some(GalleryResult::Created(dest)),
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                if let Some(e) = &self.error {
                    ui.colored_label(pal.error, e);
                }
            });
            egui::CentralPanel::default().frame(egui::Frame::new()).show(ui, |ui| {
                let q = self.search.to_lowercase();
                let shown: Vec<Template> = self
                    .templates
                    .iter()
                    .filter(|t| self.category.as_ref().is_none_or(|c| &t.meta.category == c))
                    .filter(|t| q.is_empty() || t.meta.name.to_lowercase().contains(&q) || t.meta.description.to_lowercase().contains(&q))
                    .cloned()
                    .collect();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let card = vec2(150.0, 212.0);
                    let cols = ((ui.available_width() + 12.0) / (card.x + 12.0)).floor().max(1.0) as usize;
                    egui::Grid::new("gallery-grid").spacing([12.0, 12.0]).show(ui, |ui| {
                        for (i, t) in shown.iter().enumerate() {
                            let selected = self.selected.as_ref() == Some(&t.id);
                            let frame = egui::Frame::new()
                                .fill(if selected { pal.accent.gamma_multiply(0.18) } else { pal.panel2 })
                                .stroke(egui::Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { pal.accent } else { pal.border }))
                                .corner_radius(8)
                                .inner_margin(6);
                            let r = frame
                                .show(ui, |ui| {
                                    ui.set_width(card.x);
                                    ui.vertical(|ui| {
                                        card_image(ui, pal, previews, t, vec2(card.x, card.x * 1.2));
                                        ui.label(RichText::new(&t.meta.name).strong());
                                        let tag = if t.is_builtin() { "built-in" } else { "yours" };
                                        ui.label(RichText::new(format!("{} · {tag}", t.meta.category)).small().color(pal.dim));
                                    });
                                })
                                .response;
                            let r = ui.interact(r.rect, ui.scope_id().with(("card", &t.id)), egui::Sense::click());
                            if r.clicked() {
                                self.selected = Some(t.id.clone());
                            }
                            if (i + 1) % cols == 0 {
                                ui.end_row();
                            }
                        }
                    });
                    if shown.is_empty() {
                        ui.label(RichText::new("No template matches.").color(pal.dim));
                    }
                });
            });
        });
        if modal.should_close() && result.is_none() {
            result = Some(GalleryResult::Closed);
        }
        result
    }
}

fn short_path(s: &str, n: usize) -> String {
    let home = lf_core::paths::home_dir().display().to_string();
    let s = s.replacen(&home, "~", 1);
    if s.chars().count() <= n { s } else { format!("…{}", s.chars().skip(s.chars().count() - n).collect::<String>()) }
}

// ------------------------------------------------------------------ manage

pub struct Manager {
    templates: Vec<Template>,
    hidden: Vec<String>,
    selected: Option<String>,
    edit: Option<TemplateMeta>,
    status: Option<(bool, String)>,
    /// Folder of the open project (for "New from current project").
    project: Option<PathBuf>,
    new_name: String,
}

pub enum ManagerResult {
    Close,
    /// Hidden built-ins changed.
    Hidden(Vec<String>),
}

impl Manager {
    pub fn new(cfg: &lf_core::config::Config, project: Option<PathBuf>) -> Manager {
        let mut m = Manager { templates: Vec::new(), hidden: cfg.hidden_templates.clone(), selected: None, edit: None, status: None, project, new_name: String::new() };
        m.reload();
        m
    }

    fn reload(&mut self) {
        let mut all: Vec<Template> = templates::builtins().into_iter().chain(templates::user_templates()).collect();
        templates::sort(&mut all);
        self.templates = all;
        if let Some(id) = &self.selected
            && !self.templates.iter().any(|t| &t.id == id) {
                self.selected = None;
                self.edit = None;
            }
    }

    fn select(&mut self, id: &str) {
        self.selected = Some(id.to_string());
        self.edit = self.templates.iter().find(|t| t.id == id).map(|t| t.meta.clone());
        self.status = None;
    }

    pub fn ui(&mut self, ctx: &egui::Context, pal: &Palette, previews: &mut Previews) -> Option<ManagerResult> {
        previews.poll(ctx);
        let mut result = None;
        let screen = ctx.content_rect();
        let size = vec2((screen.width() - 80.0).clamp(500.0, 980.0), (screen.height() - 80.0).clamp(400.0, 700.0));
        let modal = egui::Modal::new(egui::Id::new("tpl-manager")).show(ctx, |ui| {
            ui.set_width(size.x);
            ui.set_height(size.y);
            ui.horizontal(|ui| {
                ui.heading("Manage Templates");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        result = Some(ManagerResult::Close);
                    }
                    if ui.button("📂 Templates folder").on_hover_text(templates::user_dir().display().to_string()).clicked() {
                        let d = templates::user_dir();
                        let _ = std::fs::create_dir_all(&d);
                        crate::app::open_external(&d);
                    }
                    if ui.button("⟳").on_hover_text("Reload (after editing templates on disk)").clicked() {
                        self.reload();
                    }
                });
            });
            ui.separator();
            // create
            ui.horizontal(|ui| {
                ui.label("New template:");
                ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("Name").desired_width(180.0));
                let ok = !self.new_name.trim().is_empty();
                let meta = TemplateMeta { name: self.new_name.trim().to_string(), category: "My Templates".into(), ..Default::default() };
                if let Some(p) = self.project.clone()
                    && ui.add_enabled(ok, egui::Button::new("from current project")).clicked() {
                        let mut m = meta.clone();
                        if let Ok(pr) = lf_core::project::Project::open(&p) {
                            m.main = pr.meta.main.clone().unwrap_or(m.main);
                            m.engine = pr.meta.engine;
                        }
                        self.finish_create(templates::create_from_folder(&p, &m));
                    }
                if ui.add_enabled(ok, egui::Button::new("from folder…")).clicked()
                    && let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        let mut m = meta.clone();
                        if let Some(main) = lf_core::project::detect_main(&dir) {
                            m.main = main;
                        }
                        self.finish_create(templates::create_from_folder(&dir, &m));
                    }
            });
            ui.add_space(6.0);
            egui::Panel::left("tpl-list").resizable(true).default_size(300.0).frame(egui::Frame::new()).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut last_cat = String::new();
                    let list = self.templates.clone();
                    for t in &list {
                        if t.meta.category != last_cat {
                            last_cat = t.meta.category.clone();
                            ui.add_space(4.0);
                            ui.label(RichText::new(&last_cat).small().strong().color(pal.dim));
                        }
                        ui.horizontal(|ui| {
                            if t.is_builtin() {
                                let mut shown = !self.hidden.contains(&t.id);
                                if ui.checkbox(&mut shown, "").on_hover_text("Show in the New Project gallery").changed() {
                                    if shown {
                                        self.hidden.retain(|h| h != &t.id);
                                    } else {
                                        self.hidden.push(t.id.clone());
                                    }
                                    result = Some(ManagerResult::Hidden(self.hidden.clone()));
                                }
                            } else {
                                ui.add_space(24.0);
                            }
                            let sel = self.selected.as_ref() == Some(&t.id);
                            let tag = if t.is_builtin() { " · built-in" } else { "" };
                            if ui.selectable_label(sel, format!("{}{tag}", t.meta.name)).clicked() {
                                self.select(&t.id);
                            }
                        });
                    }
                });
            });
            egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin { left: 12, ..Default::default() })).show(ui, |ui| {
                let Some(t) = self.selected.as_ref().and_then(|id| self.templates.iter().find(|t| &t.id == id)).cloned() else {
                    ui.label(RichText::new("Select a template on the left.").color(pal.dim));
                    ui.add_space(12.0);
                    help(ui, pal);
                    return;
                };
                ui.horizontal_top(|ui| {
                    card_image(ui, pal, previews, &t, vec2(170.0, 240.0));
                    ui.vertical(|ui| {
                        let editable = !t.is_builtin();
                        let mut edit_owned = self.edit.take().unwrap_or_else(|| t.meta.clone());
                        let edit = &mut edit_owned;
                        egui::Grid::new("tpl-edit").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                            ui.label("Name");
                            ui.add_enabled(editable, egui::TextEdit::singleline(&mut edit.name).desired_width(260.0));
                            ui.end_row();
                            ui.label("Category");
                            ui.add_enabled(editable, egui::TextEdit::singleline(&mut edit.category).desired_width(260.0));
                            ui.end_row();
                            ui.label("Description");
                            ui.add_enabled(editable, egui::TextEdit::multiline(&mut edit.description).desired_width(260.0).desired_rows(3));
                            ui.end_row();
                            ui.label("Main file");
                            ui.add_enabled(editable, egui::TextEdit::singleline(&mut edit.main).desired_width(260.0));
                            ui.end_row();
                            ui.label("Engine");
                            ui.add_enabled_ui(editable, |ui| {
                                egui::ComboBox::from_id_salt("tpl-engine").selected_text(edit.engine.label()).show_ui(ui, |ui| {
                                    for e in Engine::ALL {
                                        ui.selectable_value(&mut edit.engine, e, e.label());
                                    }
                                });
                            });
                            ui.end_row();
                            ui.label("Order");
                            ui.add_enabled(editable, egui::DragValue::new(&mut edit.order));
                            ui.end_row();
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if editable {
                                if ui.button("💾 Save").clicked() {
                                    let r = t.folder().map(|f| templates::write_meta(f, &*edit)).unwrap_or(Ok(()));
                                    self.status = Some(match r {
                                        Ok(()) => (true, "Saved.".into()),
                                        Err(e) => (false, e.to_string()),
                                    });
                                    previews.invalidate(&t.id);
                                    self.reload();
                                }
                                if ui.button("📂 Open folder").clicked()
                                    && let Some(f) = t.folder() {
                                        crate::app::open_external(f);
                                    }
                            }
                            if ui.button("Duplicate").on_hover_text("Copy into your templates to edit it").clicked() {
                                match templates::copy_to_user(&t) {
                                    Ok(n) => {
                                        let id = n.id.clone();
                                        self.reload();
                                        self.select(&id);
                                        self.status = Some((true, "Copied to your templates.".into()));
                                    }
                                    Err(e) => self.status = Some((false, e.to_string())),
                                }
                            }
                            if editable && ui.button(RichText::new("🗑 Delete").color(pal.error)).clicked() {
                                match templates::delete(&t) {
                                    Ok(()) => {
                                        self.reload();
                                        self.status = Some((true, "Moved to the trash.".into()));
                                    }
                                    Err(e) => self.status = Some((false, e.to_string())),
                                }
                            }
                            if ui.button("⟳ Preview").on_hover_text("Rebuild the preview image").clicked() {
                                let dir = lf_core::paths::cache_dir().join("previews");
                                let _ = std::fs::remove_file(dir.join(format!("{}.ppm", preview_key(&t))));
                                previews.invalidate(&t.id);
                            }
                        });
                        if let Some((ok, msg)) = &self.status {
                            ui.colored_label(if *ok { pal.ok } else { pal.error }, msg);
                        }
                        if t.is_builtin() {
                            ui.label(RichText::new("Built-in templates are read-only. Duplicate one to change it, or untick it to hide it.").small().color(pal.dim));
                        }
                        if self.edit.is_none() && self.selected.as_ref() == Some(&t.id) {
                            self.edit = Some(edit_owned);
                        }
                        ui.add_space(6.0);
                        ui.label(RichText::new("Files").small().strong());
                        for (p, d) in t.files().iter().take(30) {
                            ui.label(RichText::new(format!("{p}  ({} B)", d.len())).small().monospace().color(pal.dim));
                        }
                    });
                });
            });
        });
        if modal.should_close() && result.is_none() {
            result = Some(ManagerResult::Close);
        }
        result
    }

    fn finish_create(&mut self, r: std::io::Result<Template>) {
        match r {
            Ok(t) => {
                self.new_name.clear();
                self.reload();
                self.select(&t.id);
                self.status = Some((true, format!("Created in {}", t.folder().map(|f| f.display().to_string()).unwrap_or_default())));
            }
            Err(e) => self.status = Some((false, e.to_string())),
        }
    }
}

fn help(ui: &mut egui::Ui, pal: &Palette) {
    ui.label(RichText::new("Making templates by hand").strong());
    ui.label(
        RichText::new(format!(
            "Every folder in {} is a template. Put your files in it and add a template.toml:\n\n\
             name = \"My Report\"\ncategory = \"Reports\"\ndescription = \"…\"\nmain = \"main.tex\"\nengine = \"pdflatex\"\norder = 0\n\n\
             In text files, {{{{title}}}}, {{{{author}}}}, {{{{date}}}} and {{{{project}}}} are replaced when a project is created.\n\
             An optional preview.png is used as the gallery image; otherwise the first page is rendered.",
            templates::user_dir().display()
        ))
        .small()
        .color(pal.dim),
    );
}
