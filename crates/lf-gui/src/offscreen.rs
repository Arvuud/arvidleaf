//! Headless rendering: runs the UI without a window and rasterizes egui's
//! triangle meshes in software. Used for `arvidleaf --screenshot` (docs,
//! visual checks, CI) — independent of any display server.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, epaint::Primitive, Color32, TextureId};

use crate::app::App;

struct Tex {
    w: usize,
    h: usize,
    px: Vec<Color32>,
    linear: bool,
}

/// Simulated pointer input for a frame.
#[derive(Clone, Debug)]
pub enum Action {
    Click(f32, f32),
    DoubleClick(f32, f32),
    Drag(f32, f32, f32, f32),
    Key(egui::Key, egui::Modifiers),
    Text(String),
}

/// Key press action by egui key name ("Enter", "ArrowUp", "A", ...), optionally with
/// modifiers ("Ctrl+Plus", "Alt+1", "Shift+Tab").
pub fn key_action(name: &str) -> Option<Action> {
    let mut m = egui::Modifiers::NONE;
    let mut rest = name;
    while let Some((head, tail)) = rest.split_once('+').filter(|(_, t)| !t.is_empty()) {
        match head.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" => m = m | egui::Modifiers::COMMAND,
            "alt" => m = m | egui::Modifiers::ALT,
            "shift" => m = m | egui::Modifiers::SHIFT,
            _ => return None,
        }
        rest = tail;
    }
    egui::Key::from_name(rest).map(|k| Action::Key(k, m))
}

pub struct Options {
    pub width: usize,
    pub height: usize,
    pub frames: usize,
    pub open: Vec<PathBuf>,
    pub dark: bool,
    /// (frame number, action)
    pub actions: Vec<(usize, Action)>,
}

fn events_for(a: &Action, phase: usize) -> Vec<egui::Event> {
    let btn = |pos: egui::Pos2, pressed: bool| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    match a {
        Action::Click(x, y) => {
            let p = egui::pos2(*x, *y);
            vec![egui::Event::PointerMoved(p), btn(p, true), btn(p, false)]
        }
        Action::DoubleClick(x, y) => {
            let p = egui::pos2(*x, *y);
            vec![egui::Event::PointerMoved(p), btn(p, true), btn(p, false), btn(p, true), btn(p, false)]
        }
        Action::Drag(x0, y0, x1, y1) => {
            let a = egui::pos2(*x0, *y0);
            let b = egui::pos2(*x1, *y1);
            match phase {
                0 => vec![egui::Event::PointerMoved(a), btn(a, true)],
                1 => vec![egui::Event::PointerMoved(a.lerp(b, 0.5))],
                2 => vec![egui::Event::PointerMoved(b)],
                _ => vec![btn(b, false)],
            }
        }
        Action::Key(k, m) => vec![
            egui::Event::Key { key: *k, physical_key: None, pressed: true, repeat: false, modifiers: *m },
            egui::Event::Key { key: *k, physical_key: None, pressed: false, repeat: false, modifiers: *m },
        ],
        Action::Text(t) => vec![egui::Event::Text(t.clone())],
    }
}

/// Renders the application and returns RGBA8 pixels.
pub fn render(opts: Options) -> Vec<u8> {
    let ctx = egui::Context::default();
    let mut app = App::with_ctx(&ctx, opts.open.clone());
    if !opts.dark {
        crate::theme::apply(&ctx, false);
    }
    let mut textures: HashMap<TextureId, Tex> = HashMap::new();
    let mut last = None;
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(opts.width as f32, opts.height as f32));
    // drags span several frames
    let mut drag_phase: HashMap<usize, usize> = HashMap::new();
    let mut frame = 0;
    let mut extra = 0;
    while frame < opts.frames + extra {
        let mut input = egui::RawInput { screen_rect: Some(screen), time: Some(frame as f64 / 30.0), ..Default::default() };
        // modifiers of a key action are released one frame later
        if frame > 0 && opts.actions.iter().any(|(f, a)| *f + 1 == frame && matches!(a, Action::Key(..))) {
            input.events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        }
        for (i, (f, a)) in opts.actions.iter().enumerate() {
            if let Action::Drag(..) = a {
                let ph = drag_phase.entry(i).or_insert(0);
                if frame >= *f && *ph < 4 {
                    input.events.extend(events_for(a, *ph));
                    *ph += 1;
                }
            } else if *f == frame {
                if let Action::Key(_, m) = a {
                    // shortcuts read the global modifier state
                    input.events.push(egui::Event::ModifiersChanged(*m));
                }
                input.events.extend(events_for(a, 0));
            }
        }
        let mut out = ctx.run_ui(input, |ui| app.draw(ui));
        for (id, deltas) in &out.textures_delta.set {
            for d in deltas {
                let egui::ImageData::Color(img) = &d.image;
                let linear = d.options.magnification == egui::TextureFilter::Linear;
                match d.pos {
                    None => {
                        textures.insert(*id, Tex { w: img.size[0], h: img.size[1], px: img.pixels.clone(), linear });
                    }
                    Some([x0, y0]) => {
                        if let Some(t) = textures.get_mut(id) {
                            for y in 0..img.size[1] {
                                for x in 0..img.size[0] {
                                    t.px[(y0 + y) * t.w + x0 + x] = img.pixels[y * img.size[0] + x];
                                }
                            }
                        }
                    }
                }
            }
        }
        out.textures_delta.clear();
        last = Some(out);
        // give compile and render threads time to answer
        std::thread::sleep(Duration::from_millis(if app.is_busy() { 60 } else { 15 }));
        frame += 1;
        if frame == opts.frames + extra && app.is_busy() && extra < 400 {
            extra += 1;
        }
    }
    let out = last.unwrap();
    let prims = ctx.tessellate(out.shapes, out.pixels_per_point);
    let (w, h) = (opts.width, opts.height);
    let mut fb = vec![Color32::from_rgb(0, 0, 0); w * h];
    for cp in prims {
        let Primitive::Mesh(mesh) = cp.primitive else { continue };
        let Some(tex) = textures.get(&mesh.texture_id) else { continue };
        let clip = cp.clip_rect.intersect(screen);
        for tri in mesh.indices.chunks_exact(3) {
            let v = [mesh.vertices[tri[0] as usize], mesh.vertices[tri[1] as usize], mesh.vertices[tri[2] as usize]];
            raster(&mut fb, w, h, clip, &v, tex);
        }
    }
    let mut rgba = Vec::with_capacity(w * h * 4);
    for c in fb {
        rgba.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
    }
    rgba
}

fn sample(t: &Tex, u: f32, v: f32) -> [f32; 4] {
    let fetch = |x: i64, y: i64| {
        let x = x.clamp(0, t.w as i64 - 1) as usize;
        let y = y.clamp(0, t.h as i64 - 1) as usize;
        let c = t.px[y * t.w + x];
        [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32]
    };
    let fx = u * t.w as f32 - 0.5;
    let fy = v * t.h as f32 - 0.5;
    if !t.linear {
        return fetch(fx.round() as i64, fy.round() as i64);
    }
    let (x0, y0) = (fx.floor(), fy.floor());
    let (ax, ay) = (fx - x0, fy - y0);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let a = fetch(x0, y0);
    let b = fetch(x0 + 1, y0);
    let c = fetch(x0, y0 + 1);
    let d = fetch(x0 + 1, y0 + 1);
    let mut o = [0.0; 4];
    for i in 0..4 {
        o[i] = (a[i] * (1.0 - ax) + b[i] * ax) * (1.0 - ay) + (c[i] * (1.0 - ax) + d[i] * ax) * ay;
    }
    o
}

fn raster(fb: &mut [Color32], w: usize, h: usize, clip: egui::Rect, v: &[egui::epaint::Vertex; 3], tex: &Tex) {
    let (p0, p1, p2) = (v[0].pos, v[1].pos, v[2].pos);
    let area = (p1.x - p0.x) * (p2.y - p0.y) - (p2.x - p0.x) * (p1.y - p0.y);
    if area.abs() < 1e-8 {
        return;
    }
    let minx = p0.x.min(p1.x).min(p2.x).max(clip.min.x).max(0.0).floor() as i64;
    let maxx = p0.x.max(p1.x).max(p2.x).min(clip.max.x).min(w as f32).ceil() as i64;
    let miny = p0.y.min(p1.y).min(p2.y).max(clip.min.y).max(0.0).floor() as i64;
    let maxy = p0.y.max(p1.y).max(p2.y).min(clip.max.y).min(h as f32).ceil() as i64;
    for y in miny..maxy {
        for x in minx..maxx {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((p1.x - px) * (p2.y - py) - (p2.x - px) * (p1.y - py)) / area;
            let w1 = ((p2.x - px) * (p0.y - py) - (p0.x - px) * (p2.y - py)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
            let vv = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
            let col = |c: Color32| [c.r() as f32, c.g() as f32, c.b() as f32, c.a() as f32];
            let (c0, c1, c2) = (col(v[0].color), col(v[1].color), col(v[2].color));
            let t = sample(tex, u, vv);
            let mut s = [0.0f32; 4];
            for i in 0..4 {
                s[i] = (w0 * c0[i] + w1 * c1[i] + w2 * c2[i]) * t[i] / 255.0;
            }
            // premultiplied alpha blending
            let d = &mut fb[y as usize * w + x as usize];
            let ia = 1.0 - s[3] / 255.0;
            *d = Color32::from_rgb(
                (s[0] + d.r() as f32 * ia).clamp(0.0, 255.0) as u8,
                (s[1] + d.g() as f32 * ia).clamp(0.0, 255.0) as u8,
                (s[2] + d.b() as f32 * ia).clamp(0.0, 255.0) as u8,
            );
        }
    }
}

pub fn save_png(path: &std::path::Path, w: usize, h: usize, rgba: &[u8]) -> Result<(), String> {
    image::save_buffer(path, rgba, w as u32, h as u32, image::ColorType::Rgba8).map_err(|e| e.to_string())
}
