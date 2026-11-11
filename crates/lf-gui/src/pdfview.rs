//! PDF preview: continuous scrolling, zoom, lazy page rendering, SyncTeX.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32, RichText, Sense, TextureHandle, TextureOptions, pos2, vec2};
use lf_pdf::{PdfInfo, Renderer, Request};

use crate::theme::Palette;

const GAP: f32 = 12.0;
const MARGIN: f32 = 12.0;
/// Rendered pages kept as textures (least recently used are dropped).
const MAX_TEXTURES: usize = 48;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fit {
    Width,
    Page,
    Free,
}

struct PageTex {
    generation: u64,
    dpi: f32,
    tex: TextureHandle,
    used: u64,
}

pub enum PdfAction {
    /// Inverse search: page (1-based) and position in PDF points from the top-left.
    Inverse(u32, f32, f32),
}

pub struct PdfView {
    /// Stable copy the renderer reads (the build output gets rewritten while compiling).
    shown: Option<PathBuf>,
    /// The build output (used for SyncTeX).
    pub source: Option<PathBuf>,
    pub generation: u64,
    info: Option<PdfInfo>,
    pub error: Option<String>,
    pub zoom: f32,
    pub fit: Fit,
    textures: HashMap<u32, PageTex>,
    renderer: Renderer,
    frame: u64,
    scroll_to: Option<(u32, f32)>,
    highlight: Option<(lf_tex::synctex::Hit, Instant)>,
    pub current_page: u32,
    pub loaded_at: Option<Instant>,
    /// Time from loading a new PDF until the first visible page was on screen.
    pub first_page_ms: Option<u128>,
}

impl PdfView {
    pub fn new(ctx: &egui::Context) -> PdfView {
        let c = ctx.clone();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 6);
        PdfView {
            shown: None,
            source: None,
            generation: 0,
            info: None,
            error: None,
            zoom: 1.0,
            fit: Fit::Width,
            textures: HashMap::new(),
            renderer: Renderer::new(threads, move || c.request_repaint()),
            frame: 0,
            scroll_to: None,
            highlight: None,
            current_page: 1,
            loaded_at: None,
            first_page_ms: None,
        }
    }

    /// Pages are still being rendered.
    pub fn busy(&self) -> bool {
        self.renderer.pending() > 0
    }

    pub fn page_count(&self) -> usize {
        self.info.as_ref().map_or(0, |i| i.pages.len())
    }

    /// Shows a (re)built PDF. Pages of the previous build stay visible until
    /// the new ones are rendered, so recompiling never flickers.
    pub fn load(&mut self, pdf: &Path) {
        self.generation += 1;
        // alternate between two copies: renders of the previous build may still read the old one
        let copy = pdf.with_file_name(format!(".preview-{}.pdf", self.generation % 2));
        let shown = match std::fs::copy(pdf, &copy) {
            Ok(_) => copy,
            Err(_) => pdf.to_path_buf(),
        };
        match lf_pdf::info(&shown) {
            Ok(info) => {
                self.info = Some(info);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e);
            }
        }
        self.shown = Some(shown);
        self.source = Some(pdf.to_path_buf());
        let generation = self.generation;
        self.renderer.retain(|r| r.generation == generation);
        self.loaded_at = Some(Instant::now());
        self.first_page_ms = None;
    }

    pub fn clear(&mut self) {
        self.info = None;
        self.shown = None;
        self.source = None;
        self.textures.clear();
    }

    /// Scrolls to a SyncTeX hit and highlights it briefly.
    pub fn show_hit(&mut self, hit: lf_tex::synctex::Hit) {
        self.scroll_to = Some((hit.page, hit.y));
        self.highlight = Some((hit, Instant::now()));
    }

    pub fn zoom_by(&mut self, f: f32) {
        self.zoom = (self.zoom * f).clamp(0.2, 6.0);
        self.fit = Fit::Free;
    }

    fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(r) = self.renderer.rx.try_recv() {
            let Ok(img) = r.result else { continue };
            let page = r.request.page;
            // never replace newer pages with stale ones
            if let Some(t) = self.textures.get(&page)
                && (t.generation > r.request.generation || (t.generation == r.request.generation && (t.dpi - r.request.dpi).abs() < 0.5)) {
                    continue;
                }
            if r.request.generation != self.generation && self.textures.contains_key(&page) {
                continue;
            }
            let ci = egui::ColorImage::from_rgb([img.width, img.height], &img.rgb);
            let tex = ctx.load_texture(format!("pdf-page-{page}"), ci, TextureOptions::LINEAR);
            self.textures.insert(page, PageTex { generation: r.request.generation, dpi: r.request.dpi, tex, used: self.frame });
        }
        if self.textures.len() > MAX_TEXTURES {
            let mut v: Vec<(u32, u64)> = self.textures.iter().map(|(p, t)| (*p, t.used)).collect();
            v.sort_by_key(|x| x.1);
            for (p, _) in v.iter().take(self.textures.len() - MAX_TEXTURES) {
                self.textures.remove(p);
            }
        }
    }

    pub fn toolbar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        ui.horizontal(|ui| {
            if ui.small_button("−").on_hover_text("Zoom out (Ctrl+wheel)").clicked() {
                self.zoom_by(1.0 / 1.15);
            }
            ui.label(RichText::new(format!("{:.0} %", self.zoom * 100.0)).small());
            if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                self.zoom_by(1.15);
            }
            if ui.selectable_label(self.fit == Fit::Width, "↔").on_hover_text("Fit width").clicked() {
                self.fit = Fit::Width;
            }
            if ui.selectable_label(self.fit == Fit::Page, "⛶").on_hover_text("Fit page").clicked() {
                self.fit = Fit::Page;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(src) = &self.source
                    && ui.small_button("↗").on_hover_text("Open in the system PDF viewer").clicked() {
                        crate::app::open_external(src);
                    }
                let n = self.page_count();
                if n > 0 {
                    ui.label(RichText::new(format!("{} / {}", self.current_page, n)).small().color(pal.dim));
                }
            });
        });
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, pal: &Palette, compiling: bool) -> Option<PdfAction> {
        self.frame += 1;
        let ctx = ui.ctx().clone();
        self.receive(&ctx);
        let mut action = None;
        let Some(info) = self.info.clone().filter(|i| !i.pages.is_empty()) else {
            ui.centered_and_justified(|ui| {
                let msg = if compiling {
                    "Compiling…".to_string()
                } else if let Some(e) = &self.error {
                    format!("Could not open the PDF:\n{e}")
                } else {
                    "No PDF yet.\nCompile with Ctrl+Enter.".to_string()
                };
                ui.label(RichText::new(msg).color(pal.dim));
            });
            return None;
        };
        let ppp = ctx.pixels_per_point();
        let max_w = info.pages.iter().map(|p| p[0]).fold(1.0f32, f32::max);
        let avail = ui.available_size();
        match self.fit {
            Fit::Width => self.zoom = ((avail.x - 2.0 * MARGIN - 14.0) / max_w).clamp(0.1, 8.0),
            Fit::Page => {
                let p = info.pages[0];
                self.zoom = ((avail.x - 2.0 * MARGIN - 14.0) / p[0]).min((avail.y - 2.0 * MARGIN) / p[1]).clamp(0.1, 8.0);
            }
            Fit::Free => {}
        }
        let z = self.zoom;
        // page tops in screen units
        let mut tops = Vec::with_capacity(info.pages.len());
        let mut y = MARGIN;
        for p in &info.pages {
            tops.push(y);
            y += p[1] * z + GAP;
        }
        let total = vec2(max_w * z + 2.0 * MARGIN, y - GAP + MARGIN);
        // ctrl + wheel (and pinch) zooms
        let zoom = ui.input(|i| i.zoom_delta());
        if zoom != 1.0 && ui.rect_contains_pointer(ui.available_rect_before_wrap()) {
            self.zoom_by(zoom);
        }
        let mut sa = egui::ScrollArea::both().id_salt("pdf-scroll").auto_shrink([false, false]);
        if let Some((page, ypt)) = self.scroll_to.take()
            && let Some(top) = tops.get(page.saturating_sub(1) as usize) {
                sa = sa.vertical_scroll_offset((top + ypt * z - avail.y * 0.3).max(0.0));
            }
        let dpi = ((72.0 * z * ppp) / 6.0).ceil() * 6.0;
        let generation = self.generation;
        let shown = self.shown.clone();
        let frame = self.frame;
        sa.show_viewport(ui, |ui, viewport| {
            ui.set_min_size(total);
            let origin = ui.min_rect().min;
            let x0 = origin.x + ((ui.available_width().max(total.x) - max_w * z) / 2.0).max(MARGIN);
            let resp = ui.interact(egui::Rect::from_min_size(origin, total.max(viewport.size())), ui.scope_id().with("pdf-area"), Sense::click());
            let painter = ui.painter();
            let mut first_visible = None;
            let center = viewport.center().y;
            for (i, p) in info.pages.iter().enumerate() {
                let top = tops[i];
                let h = p[1] * z;
                let page_no = i as u32 + 1;
                if top <= center && center <= top + h + GAP {
                    self.current_page = page_no;
                }
                // render visible pages plus one page margin
                if top + h < viewport.min.y - h || top > viewport.max.y + h {
                    continue;
                }
                let visible = top + h >= viewport.min.y && top <= viewport.max.y;
                if visible && first_visible.is_none() {
                    first_visible = Some(page_no);
                }
                let px = x0 + (max_w - p[0]) * z / 2.0;
                let rect = egui::Rect::from_min_size(pos2(px, origin.y + top), vec2(p[0] * z, h));
                painter.rect_filled(rect.translate(vec2(0.0, 2.0)), 2.0, Color32::from_black_alpha(60));
                painter.rect_filled(rect, 0.0, Color32::WHITE);
                // visible page edge on the light theme too
                painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, pal.border), egui::StrokeKind::Outside);
                let mut have_current = false;
                if let Some(t) = self.textures.get_mut(&page_no) {
                    t.used = frame;
                    painter.image(t.tex.id(), rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    have_current = t.generation == generation && (t.dpi - dpi).abs() < 0.5;
                    if visible && t.generation == generation && self.first_page_ms.is_none() {
                        self.first_page_ms = self.loaded_at.map(|l| l.elapsed().as_millis());
                    }
                }
                if !have_current && !compiling
                    && let Some(path) = &shown {
                        self.renderer.request(Request { generation, path: path.clone(), page: page_no, dpi });
                    }
                if let Some((hit, at)) = &self.highlight
                    && hit.page == page_no {
                        let a = (1.0 - at.elapsed().as_secs_f32() / 2.5).max(0.0);
                        if a > 0.0 {
                            let hr = egui::Rect::from_min_size(
                                pos2(rect.min.x + hit.x * z - 4.0, rect.min.y + hit.y * z - 2.0),
                                vec2((hit.w * z).max(30.0) + 8.0, (hit.h * z).max(12.0) + 4.0),
                            );
                            painter.rect_filled(hr, 3.0, pal.accent.gamma_multiply(0.25 * a));
                            painter.rect_stroke(hr, 3.0, egui::Stroke::new(1.5, pal.accent.gamma_multiply(a)), egui::StrokeKind::Outside);
                            ui.ctx().request_repaint();
                        }
                    }
                // inverse search: double-click or Ctrl+click
                if let Some(pos) = resp.interact_pointer_pos() {
                    let ctrl_click = resp.clicked() && ui.input(|i| i.modifiers.command);
                    if (resp.double_clicked() || ctrl_click) && rect.contains(pos) {
                        let x = (pos.x - rect.min.x) / z;
                        let y = (pos.y - rect.min.y) / z;
                        action = Some(PdfAction::Inverse(page_no, x, y));
                    }
                }
            }
            let _ = (first_visible, resp);
        });
        // drop queued renders of other generations or far-away pages
        self.renderer.retain(|r| r.generation == generation);
        action
    }
}
