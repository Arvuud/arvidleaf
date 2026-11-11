//! Text editor for LaTeX sources: highlighting, line numbers, error markers,
//! auto-closing brackets, `\begin`/`\end` completion, autocomplete, find/replace.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use eframe::egui::{self, Color32, FontId, Galley, Key, RichText, TextFormat, text::LayoutJob};
use egui::text::{CCursor, CCursorRange};
use lf_tex::complete::{self, Context};
use lf_tex::lexer::{self, Tok};
use lf_tex::log::Level;

use crate::theme::Palette;

pub struct EditorDoc {
    /// Path relative to the project root.
    pub rel: PathBuf,
    pub abs: PathBuf,
    pub text: String,
    pub saved: String,
    pub id: u64,
    pub last_edit: Option<Instant>,
    pub disk_mtime: Option<SystemTime>,
    /// Changed on disk while there were unsaved edits.
    pub conflict: bool,
    cache: Option<(u64, f32, Arc<Galley>)>,
    goto: Option<(u32, Option<(usize, usize)>)>,
    pub cursor_line: u32,
    pub cursor_col: u32,
    completion: Option<Completion>,
    find: Option<Find>,
    /// Char index of a closing bracket inserted by auto-closing (typing it
    /// right there steps over it instead of adding another one).
    auto_closed: Option<usize>,
}

struct Completion {
    items: Vec<String>,
    selected: usize,
    /// char index where the typed prefix starts
    start: usize,
    kind: Context,
}

#[derive(Default)]
struct Find {
    query: String,
    replace: String,
    case: bool,
    focus: bool,
    current: usize,
}

/// Data from the project the editor needs for markers and completion.
pub struct EditorEnv<'a> {
    pub problems: Vec<(u32, Level, String)>,
    pub labels: &'a [String],
    pub bib_keys: &'a [String],
    pub files: &'a [String],
    pub font_size: f32,
}

pub enum EditorAction {
    SyncToPdf(u32, u32),
}

impl EditorDoc {
    pub fn new(rel: PathBuf, abs: PathBuf, text: String, id: u64) -> EditorDoc {
        let disk_mtime = std::fs::metadata(&abs).and_then(|m| m.modified()).ok();
        EditorDoc {
            rel,
            abs,
            saved: text.clone(),
            text,
            id,
            last_edit: None,
            disk_mtime,
            conflict: false,
            cache: None,
            goto: None,
            cursor_line: 1,
            cursor_col: 1,
            completion: None,
            find: None,
            auto_closed: None,
        }
    }

    pub fn title(&self) -> String {
        self.rel.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        lf_core::write_atomic(&self.abs, self.text.as_bytes())?;
        self.saved = self.text.clone();
        self.disk_mtime = std::fs::metadata(&self.abs).and_then(|m| m.modified()).ok();
        self.last_edit = None;
        self.conflict = false;
        Ok(())
    }

    /// Reloads from disk (discarding unsaved edits).
    pub fn reload(&mut self) -> std::io::Result<()> {
        let t = std::fs::read_to_string(&self.abs)?;
        self.text = t.clone();
        self.saved = t;
        self.disk_mtime = std::fs::metadata(&self.abs).and_then(|m| m.modified()).ok();
        self.conflict = false;
        self.last_edit = None;
        Ok(())
    }

    /// Moves the cursor to a 1-based line and scrolls there.
    pub fn goto_line(&mut self, line: u32) {
        self.goto = Some((line.max(1), None));
    }

    fn id(&self) -> egui::Id {
        egui::Id::new(("editor", self.id))
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, pal: &Palette, env: &EditorEnv) -> Option<EditorAction> {
        let mut action = None;
        let ctx = ui.ctx().clone();
        let id = self.id();
        let font = FontId::monospace(env.font_size);
        let row_h = ui.fonts_mut(|f| f.row_height(&font));
        let has_focus = ui.memory(|m| m.has_focus(id));

        // ---- toolbar
        ui.horizontal(|ui| {
            ui.label(RichText::new(self.rel.display().to_string()).small().color(pal.dim));
            if self.conflict {
                ui.label(RichText::new("⚠ changed on disk").small().color(pal.warn));
                if ui.small_button("Reload").on_hover_text("Discard your edits and load the file from disk").clicked() {
                    let _ = self.reload();
                }
                if ui.small_button("Keep mine").clicked() {
                    self.conflict = false;
                    self.disk_mtime = std::fs::metadata(&self.abs).and_then(|m| m.modified()).ok();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🔍").on_hover_text("Find & replace (Ctrl+F)").clicked() {
                    self.find = if self.find.is_some() { None } else { Some(Find { focus: true, ..Default::default() }) };
                }
                if ui.small_button("➡ PDF").on_hover_text("Show this line in the PDF (Ctrl+J)").clicked() {
                    action = Some(EditorAction::SyncToPdf(self.cursor_line, self.cursor_col));
                }
            });
        });

        // ---- shortcuts handled before the TextEdit sees the keys
        let (ctrl_f, ctrl_j, ctrl_slash, esc) = ui.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::F),
                i.modifiers.command && i.key_pressed(Key::J),
                i.modifiers.command && i.key_pressed(Key::Slash),
                i.key_pressed(Key::Escape),
            )
        });
        if has_focus && ctrl_f {
            let sel = self.selected_text(&ctx);
            let f = self.find.get_or_insert_with(Find::default);
            if let Some(s) = sel.filter(|s| !s.contains('\n') && !s.is_empty()) {
                f.query = s;
            }
            f.focus = true;
        }
        if has_focus && ctrl_j {
            action = Some(EditorAction::SyncToPdf(self.cursor_line, self.cursor_col));
        }
        if has_focus && ctrl_slash {
            self.toggle_comment(&ctx);
        }
        if esc && self.completion.is_none() && self.find.as_ref().is_some_and(|_| !has_focus) {
            self.find = None;
        }

        if self.find.is_some() {
            self.find_bar(ui, pal);
        }

        let mut typed: Option<String> = None;
        if has_focus {
            typed = self.pre_input(&ctx);
        }

        // ---- the editor (soft-wrapped, like Overleaf)
        let problems = &env.problems;
        let n_lines = self.text.split('\n').count().max(1);
        let mut scroll = egui::ScrollArea::vertical().id_salt(("editor_scroll", self.id)).auto_shrink([false, false]);
        if let Some((line, _)) = self.goto {
            // exact position from the last layout (lines may wrap)
            let y = match &self.cache {
                Some((_, _, g)) => g.pos_from_cursor(CCursor::new(char_index_of_line(&self.text, line))).min.y,
                None => (line as f32 - 1.0) * row_h,
            };
            let visible = ui.available_height();
            scroll = scroll.vertical_scroll_offset((y - visible * 0.35).max(0.0));
        }
        let mut output = None;
        egui::Frame::new().fill(pal.canvas).corner_radius(6).inner_margin(egui::Margin::symmetric(4, 6)).show(ui, |ui| {
            scroll.show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    // gutter: reserve the column now, paint after the text is laid out
                    let digits = n_lines.to_string().len().max(3) as f32;
                    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
                    let gw = digits * char_w + 18.0;
                    let (gutter, _) = ui.allocate_exact_size(egui::vec2(gw, row_h), egui::Sense::hover());
                    let err_lines: Vec<u32> = problems.iter().filter(|p| p.1 == Level::Error).map(|p| p.0).collect();
                    let cache = &mut self.cache;
                    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| -> Arc<Galley> {
                        let s = buf.as_str();
                        let h = hash(s) ^ hash(&err_lines) ^ (pal.text.to_array()[0] as u64) << 7 ^ (font.size.to_bits() as u64) << 20;
                        if let Some((hh, w, g)) = cache {
                            if *hh == h && *w == wrap {
                                return g.clone();
                            }
                        }
                        let mut job = highlight(s, &font, pal, &err_lines);
                        job.wrap.max_width = wrap;
                        let g = ui.fonts_mut(|f| f.layout_job(job));
                        *cache = Some((h, wrap, g.clone()));
                        g
                    };
                    let out = egui::TextEdit::multiline(&mut self.text)
                        .id(id)
                        .code_editor()
                        .frame(egui::Frame::NONE)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY)
                        .desired_rows(40)
                        .layouter(&mut layouter)
                        .show(ui);
                    // line numbers at the first row of every logical line
                    let painter = ui.painter();
                    let clip = ui.clip_rect();
                    let mut line = 1u32;
                    let mut new_line = true;
                    let mut line_y = std::collections::HashMap::new();
                    for row in &out.galley.rows {
                        let y = out.galley_pos.y + row.pos.y;
                        if new_line {
                            // only rows on screen are painted (large files stay fast)
                            if y + row_h >= clip.min.y && y <= clip.max.y {
                                painter.text(egui::pos2(gutter.max.x - 8.0, y), egui::Align2::RIGHT_TOP, line.to_string(), font.clone(), pal.dim);
                            }
                            line_y.insert(line, y);
                            line += 1;
                        }
                        new_line = row.ends_with_newline;
                    }
                    for (line, level, msg) in problems {
                        let Some(y) = line_y.get(line) else { continue };
                        let c = match level {
                            Level::Error => pal.error,
                            Level::Warning => pal.warn,
                            Level::BadBox => pal.dim,
                        };
                        let dot = egui::pos2(gutter.min.x + 5.0, y + row_h * 0.5);
                        painter.circle_filled(dot, 3.5, c);
                        let hover = egui::Rect::from_center_size(dot, egui::vec2(12.0, row_h));
                        ui.interact(hover, egui::Id::new(("gutter-tip", self.id, *line)), egui::Sense::hover()).on_hover_text(msg);
                    }
                    output = Some(out);
                });
            });
        });
        let out = output.unwrap();
        if out.response.changed() {
            self.last_edit = Some(Instant::now());
        }

        // ---- cursor position, pending goto
        if let Some((line, sel)) = self.goto.take() {
            let start = char_index_of_line(&self.text, line);
            let range = match sel {
                Some((a, b)) => CCursorRange::two(CCursor::new(a), CCursor::new(b)),
                None => CCursorRange::one(CCursor::new(start)),
            };
            self.set_cursor(&ctx, range);
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let cursor = out.cursor_range.map(|r| usize::from(r.primary.index));
        if let Some(c) = cursor {
            let byte = byte_of_char(&self.text, c);
            let before = &self.text[..byte];
            self.cursor_line = before.matches('\n').count() as u32 + 1;
            self.cursor_col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) as u32 + 1;
        }

        // ---- autocomplete
        if let (Some(c), true) = (cursor, has_focus) {
            if typed.is_some() {
                self.update_completion(c, env);
            }
            if self.completion.is_some() {
                let rect = out.galley.pos_from_cursor(CCursor::new(c));
                let pos = out.galley_pos + rect.left_bottom().to_vec2() + egui::vec2(0.0, 2.0);
                self.completion_popup(ui, pal, pos);
            }
        } else if !has_focus {
            self.completion = None;
        }

        // Ctrl+S / Ctrl+Enter are handled globally by the app
        action
    }

    fn selected_text(&self, ctx: &egui::Context) -> Option<String> {
        let st = egui::text_edit::TextEditState::load(ctx, self.id())?;
        let r = st.cursor.char_range()?;
        let (a, b) = sorted(r);
        if a == b {
            return None;
        }
        Some(self.text.chars().skip(a).take(b - a).collect())
    }

    fn cursor(&self, ctx: &egui::Context) -> Option<CCursorRange> {
        egui::text_edit::TextEditState::load(ctx, self.id())?.cursor.char_range()
    }

    fn set_cursor(&self, ctx: &egui::Context, range: CCursorRange) {
        let mut st = egui::text_edit::TextEditState::load(ctx, self.id()).unwrap_or_default();
        st.cursor.set_char_range(Some(range));
        st.store(ctx, self.id());
    }

    /// Replaces chars `a..b` with `s`, leaving the cursor at char index `cursor`.
    fn splice(&mut self, ctx: &egui::Context, a: usize, b: usize, s: &str, cursor: usize) {
        let ba = byte_of_char(&self.text, a);
        let bb = byte_of_char(&self.text, b);
        self.text.replace_range(ba..bb, s);
        self.set_cursor(ctx, CCursorRange::one(CCursor::new(cursor)));
        self.last_edit = Some(Instant::now());
        self.cache = None;
    }

    /// Intercepts keys for auto-closing, `\begin` completion and the
    /// completion popup. Returns typed text (if any) for completion updates.
    fn pre_input(&mut self, ctx: &egui::Context) -> Option<String> {
        let Some(range) = self.cursor(ctx) else { return None };
        let (a, b) = sorted(range);
        let mut typed = None;
        let events = ctx.input(|i| i.events.clone());
        let mut consumed = vec![false; events.len()];
        for (k, ev) in events.iter().enumerate() {
            match ev {
                egui::Event::Key { key, pressed: true, modifiers, .. } if self.completion.is_some() && !modifiers.command => match *key {
                    Key::ArrowDown => {
                        let c = self.completion.as_mut().unwrap();
                        c.selected = (c.selected + 1) % c.items.len().max(1);
                        consumed[k] = true;
                    }
                    Key::ArrowUp => {
                        let c = self.completion.as_mut().unwrap();
                        c.selected = (c.selected + c.items.len().max(1) - 1) % c.items.len().max(1);
                        consumed[k] = true;
                    }
                    Key::Tab | Key::Enter => {
                        self.accept_completion(ctx, a);
                        consumed[k] = true;
                    }
                    Key::Escape => {
                        self.completion = None;
                        consumed[k] = true;
                    }
                    _ => {}
                },
                egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if !modifiers.any() && a == b => {
                    // after `\begin{env}`: insert the matching `\end{env}`
                    let byte = byte_of_char(&self.text, a);
                    let line_start = self.text[..byte].rfind('\n').map_or(0, |p| p + 1);
                    let line = &self.text[line_start..byte];
                    let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                    if let Some(env) = trailing_begin(line) {
                        if !has_matching_end(&self.text[byte..], &env) {
                            let ins = format!("\n{indent}  \n{indent}\\end{{{env}}}");
                            let cur = a + 1 + indent.chars().count() + 2;
                            self.splice(ctx, a, b, &ins, cur);
                            consumed[k] = true;
                            continue;
                        }
                    }
                    // keep the indentation of the current line
                    if !indent.is_empty() {
                        let ins = format!("\n{indent}");
                        let cur = a + ins.chars().count();
                        self.splice(ctx, a, b, &ins, cur);
                        consumed[k] = true;
                    }
                }
                egui::Event::Text(t) => {
                    let t2 = t.clone();
                    let next = self.text.chars().nth(a);
                    let prev = if a > 0 { self.text.chars().nth(a - 1) } else { None };
                    if (t2 == "}" || t2 == "]") && a == b && next == t2.chars().next() && self.auto_closed == Some(a) {
                        // type over the closing bracket we inserted ourselves
                        self.set_cursor(ctx, CCursorRange::one(CCursor::new(a + 1)));
                        self.auto_closed = None;
                        consumed[k] = true;
                    } else if (t2 == "{" || t2 == "[") && prev != Some('\\') && next.is_none_or(|c| c.is_whitespace() || c == '}' || c == ']') {
                        // `\{` is a literal brace and stays unpaired
                        let close = if t2 == "{" { "}" } else { "]" };
                        if a == b {
                            self.splice(ctx, a, b, &format!("{t2}{close}"), a + 1);
                            self.auto_closed = Some(a + 1);
                        } else {
                            let sel: String = self.text.chars().skip(a).take(b - a).collect();
                            self.splice(ctx, a, b, &format!("{t2}{sel}{close}"), b + 2);
                        }
                        consumed[k] = true;
                    } else if self.auto_closed.is_some_and(|p| a != p || t2.contains(['\n', '}', ']'])) {
                        self.auto_closed = None;
                    } else if let Some(p) = self.auto_closed {
                        // typing inside the pair moves the closer along
                        self.auto_closed = Some(p + t2.chars().count());
                    }
                    typed = Some(t2);
                }
                egui::Event::Key { key: Key::Backspace, pressed: true, .. } => {
                    typed = Some(String::new());
                }
                _ => {}
            }
        }
        if consumed.iter().any(|c| *c) {
            ctx.input_mut(|i| {
                let mut k = 0;
                i.events.retain(|_| {
                    let keep = !consumed.get(k).copied().unwrap_or(false);
                    k += 1;
                    keep
                });
            });
        }
        typed
    }

    fn toggle_comment(&mut self, ctx: &egui::Context) {
        let Some(range) = self.cursor(ctx) else { return };
        let (a, b) = sorted(range);
        let ba = byte_of_char(&self.text, a);
        let bb = byte_of_char(&self.text, b);
        let start = self.text[..ba].rfind('\n').map_or(0, |p| p + 1);
        let end = self.text[bb..].find('\n').map_or(self.text.len(), |p| bb + p);
        let block = &self.text[start..end];
        let all_commented = block.lines().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with('%'));
        let new: Vec<String> = block
            .split('\n')
            .map(|l| {
                if all_commented {
                    let ws = l.len() - l.trim_start().len();
                    let rest = &l[ws..];
                    let rest = rest.strip_prefix("% ").or_else(|| rest.strip_prefix('%')).unwrap_or(rest);
                    format!("{}{}", &l[..ws], rest)
                } else if l.trim().is_empty() {
                    l.to_string()
                } else {
                    format!("% {l}")
                }
            })
            .collect();
        let new = new.join("\n");
        let ca = self.text[..start].chars().count();
        let cb = ca + block.chars().count();
        let cur = ca + new.chars().count();
        self.splice(ctx, ca, cb, &new, cur);
    }

    fn update_completion(&mut self, cursor: usize, env: &EditorEnv) {
        let byte = byte_of_char(&self.text, cursor);
        let ctx = complete::context(&self.text[..byte]);
        let Some(kind) = ctx else {
            self.completion = None;
            return;
        };
        let (prefix, items): (&str, Vec<String>) = match &kind {
            Context::Command(p) if p.len() >= 2 => (p, complete::rank(p, complete::COMMANDS.iter().copied(), 12).into_iter().map(String::from).collect()),
            Context::Environment(p) => (p, complete::rank(p, complete::ENVIRONMENTS.iter().copied(), 12).into_iter().map(String::from).collect()),
            Context::Label(p) => (p, complete::rank(p, env.labels.iter().map(String::as_str), 12).into_iter().map(String::from).collect()),
            Context::Citation(p) => (p, complete::rank(p, env.bib_keys.iter().map(String::as_str), 12).into_iter().map(String::from).collect()),
            Context::File(p) => (p, complete::rank(p, env.files.iter().map(String::as_str), 12).into_iter().map(String::from).collect()),
            _ => ("", Vec::new()),
        };
        // completing an empty argument shows everything (labels, keys, files)
        let items = if items.is_empty() && prefix.is_empty() {
            match &kind {
                Context::Label(_) => env.labels.iter().take(12).cloned().collect(),
                Context::Citation(_) => env.bib_keys.iter().take(12).cloned().collect(),
                Context::Environment(_) => complete::ENVIRONMENTS.iter().take(12).map(|s| s.to_string()).collect(),
                Context::File(_) => env.files.iter().take(12).cloned().collect(),
                _ => Vec::new(),
            }
        } else {
            items
        };
        if items.is_empty() {
            self.completion = None;
            return;
        }
        let start = cursor - prefix.chars().count();
        let selected = self.completion.as_ref().filter(|c| c.start == start).map_or(0, |c| c.selected.min(items.len() - 1));
        self.completion = Some(Completion { items, selected, start, kind });
    }

    fn accept_completion(&mut self, ctx: &egui::Context, cursor: usize) {
        let Some(c) = self.completion.take() else { return };
        let Some(item) = c.items.get(c.selected).cloned() else { return };
        match c.kind {
            Context::Command(_) => {
                // `section{}` → `section{|}`
                let (name, rest) = item.split_once('{').map_or((item.as_str(), ""), |(n, r)| (n, r));
                let args = if rest.is_empty() { String::new() } else { format!("{{{rest}") };
                let ins = format!("{name}{args}");
                let cur = c.start + name.chars().count() + usize::from(!args.is_empty());
                self.splice(ctx, c.start, cursor, &ins, cur);
            }
            Context::Environment(_) => {
                let byte = byte_of_char(&self.text, cursor);
                let line_start = self.text[..byte].rfind('\n').map_or(0, |p| p + 1);
                let is_begin = self.text[line_start..byte].contains("\\begin{");
                let indent: String = self.text[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let after_close = self.text[byte..].starts_with('}');
                let mut ins = item.clone();
                let mut end = cursor;
                if after_close {
                    end += 1;
                }
                ins.push('}');
                let mut cur = c.start + ins.chars().count();
                if is_begin && !has_matching_end(&self.text[byte..], &item) {
                    let body = format!("\n{indent}  \n{indent}\\end{{{item}}}");
                    cur += 1 + indent.chars().count() + 2;
                    ins.push_str(&body);
                }
                self.splice(ctx, c.start, end, &ins, cur);
            }
            _ => {
                let cur = c.start + item.chars().count();
                self.splice(ctx, c.start, cursor, &item, cur);
            }
        }
    }

    fn completion_popup(&mut self, ui: &mut egui::Ui, pal: &Palette, pos: egui::Pos2) {
        let Some(c) = &mut self.completion else { return };
        let mut clicked = None;
        egui::Area::new(egui::Id::new(("completion", self.id))).order(egui::Order::Foreground).fixed_pos(pos).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).inner_margin(4).show(ui, |ui| {
                ui.set_min_width(220.0);
                for (i, item) in c.items.iter().enumerate() {
                    let label = match c.kind {
                        Context::Command(_) => format!("\\{}", item.replace("{}", "{…}")),
                        _ => item.clone(),
                    };
                    let sel = i == c.selected;
                    let r = ui.selectable_label(sel, RichText::new(label).monospace().color(if sel { pal.text } else { pal.dim }));
                    if r.clicked() {
                        clicked = Some(i);
                    }
                }
                ui.label(RichText::new("Tab/Enter insert · Esc close").small().color(pal.dim));
            });
        });
        if let Some(i) = clicked {
            c.selected = i;
            let ctx = ui.ctx().clone();
            if let Some(r) = self.cursor(&ctx) {
                self.accept_completion(&ctx, usize::from(r.primary.index));
            }
        }
    }

    fn find_bar(&mut self, ui: &mut egui::Ui, pal: &Palette) {
        let ctx = ui.ctx().clone();
        let mut f = self.find.take().unwrap();
        let mut close = false;
        let mut go: Option<i32> = None;
        let mut replace_one = false;
        let mut replace_all = false;
        egui::Frame::new().fill(pal.panel2).corner_radius(6).inner_margin(6).show(ui, |ui| {
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut f.query).hint_text("Find").desired_width(200.0));
                if f.focus {
                    r.request_focus();
                    f.focus = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    go = Some(if ui.input(|i| i.modifiers.shift) { -1 } else { 1 });
                    r.request_focus();
                }
                if r.changed() {
                    f.current = 0;
                }
                let n = count_matches(&self.text, &f.query, f.case);
                ui.label(RichText::new(if f.query.is_empty() { String::new() } else { format!("{n} found") }).small().color(pal.dim));
                if ui.small_button("⏶").on_hover_text("Previous (Shift+Enter)").clicked() {
                    go = Some(-1);
                }
                if ui.small_button("⏷").on_hover_text("Next (Enter)").clicked() {
                    go = Some(1);
                }
                ui.toggle_value(&mut f.case, "Aa").on_hover_text("Match case");
                ui.add(egui::TextEdit::singleline(&mut f.replace).hint_text("Replace").desired_width(160.0));
                if ui.small_button("Replace").clicked() {
                    replace_one = true;
                }
                if ui.small_button("All").on_hover_text("Replace all").clicked() {
                    replace_all = true;
                }
                if ui.small_button("×").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
            });
        });
        ui.add_space(4.0);
        if replace_all && !f.query.is_empty() {
            let new = replace_matches(&self.text, &f.query, &f.replace, f.case, None);
            if new != self.text {
                self.text = new;
                self.last_edit = Some(Instant::now());
                self.cache = None;
            }
        }
        if replace_one && !f.query.is_empty() {
            let ms = match_positions(&self.text, &f.query, f.case);
            if !ms.is_empty() {
                let i = f.current.min(ms.len() - 1);
                self.text = replace_matches(&self.text, &f.query, &f.replace, f.case, Some(ms[i]));
                self.last_edit = Some(Instant::now());
                self.cache = None;
                go = Some(0);
            }
        }
        if let Some(dir) = go {
            let ms = match_positions(&self.text, &f.query, f.case);
            if !ms.is_empty() {
                let cur = self.cursor(&ctx).map(|r| sorted(r).0).unwrap_or(0);
                let cur_byte = byte_of_char(&self.text, cur);
                let i = match dir {
                    1 => ms.iter().position(|m| *m > cur_byte).unwrap_or(0),
                    -1 => ms.iter().rposition(|m| *m < cur_byte).unwrap_or(ms.len() - 1),
                    _ => ms.iter().position(|m| *m >= cur_byte).unwrap_or(0),
                };
                f.current = i;
                let b = ms[i];
                let a_char = self.text[..b].chars().count();
                let len = f.query.chars().count();
                let line = self.text[..b].matches('\n').count() as u32 + 1;
                self.goto = Some((line, Some((a_char, a_char + len))));
            }
        }
        if !close {
            self.find = Some(f);
        }
    }
}

fn sorted(r: CCursorRange) -> (usize, usize) {
    let a = usize::from(r.primary.index);
    let b = usize::from(r.secondary.index);
    (a.min(b), a.max(b))
}

pub fn byte_of_char(s: &str, c: usize) -> usize {
    s.char_indices().nth(c).map_or(s.len(), |(b, _)| b)
}

fn char_index_of_line(s: &str, line: u32) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut n = 1;
    for (ci, ch) in s.chars().enumerate() {
        if ch == '\n' {
            n += 1;
            if n == line {
                return ci + 1;
            }
        }
    }
    s.chars().count()
}

/// `  \begin{itemize}` → `itemize` if the line ends with a `\begin{…}` (plus optional args).
fn trailing_begin(line: &str) -> Option<String> {
    let code = lf_tex::outline::strip_comment(line).trim_end();
    let p = code.rfind("\\begin{")?;
    let rest = &code[p + 7..];
    let close = rest.find('}')?;
    let env = &rest[..close];
    let tail = rest[close + 1..].trim();
    // allow [opts] and {args} after the environment name
    let ok = tail.is_empty() || tail.starts_with('[') || tail.starts_with('{');
    if ok && !env.is_empty() && env != "document" { Some(env.to_string()) } else { None }
}

/// Whether `\end{env}` follows before another `\begin{env}`.
fn has_matching_end(after: &str, env: &str) -> bool {
    let b = format!("\\begin{{{env}}}");
    let e = format!("\\end{{{env}}}");
    match (after.find(&b), after.find(&e)) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(pb), Some(pe)) => pe < pb,
    }
}

fn match_positions(text: &str, q: &str, case: bool) -> Vec<usize> {
    if q.is_empty() {
        return Vec::new();
    }
    if case {
        text.match_indices(q).map(|(i, _)| i).collect()
    } else {
        // ASCII case folding keeps byte offsets valid
        let t = text.to_ascii_lowercase();
        let q = q.to_ascii_lowercase();
        t.match_indices(&q).map(|(i, _)| i).collect()
    }
}

fn count_matches(text: &str, q: &str, case: bool) -> usize {
    match_positions(text, q, case).len()
}

fn replace_matches(text: &str, q: &str, with: &str, case: bool, only: Option<usize>) -> String {
    let ms = match_positions(text, q, case);
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for m in ms {
        if only.is_some_and(|o| o != m) {
            continue;
        }
        out.push_str(&text[last..m]);
        out.push_str(with);
        last = m + q.len();
    }
    out.push_str(&text[last..]);
    out
}

fn hash<T: std::hash::Hash + ?Sized>(s: &T) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

/// LaTeX highlighting into a LayoutJob; lines with errors are underlined.
///
/// Called on every keystroke for the whole buffer, so the job is built
/// directly: the text is copied once and each token only adds a section.
pub fn highlight(src: &str, font: &FontId, pal: &Palette, err_lines: &[u32]) -> LayoutJob {
    const TOKS: [Tok; 9] = [Tok::Text, Tok::Command, Tok::Env, Tok::Math, Tok::Comment, Tok::Arg, Tok::OptArg, Tok::Brace, Tok::Special];
    let color = |t: Tok| -> Color32 {
        match t {
            Tok::Text => pal.text,
            Tok::Command => pal.keyword,
            Tok::Env => pal.func,
            Tok::Math => pal.math,
            Tok::Comment => pal.comment,
            Tok::Arg => pal.string,
            Tok::OptArg => pal.opt,
            Tok::Brace => pal.dim,
            Tok::Special => pal.special,
        }
    };
    // one prepared format per token class, plain and on an error line
    let formats: Vec<(TextFormat, TextFormat)> = TOKS
        .iter()
        .map(|t| {
            let f = TextFormat { font_id: font.clone(), color: color(*t), italics: *t == Tok::Comment, ..Default::default() };
            let mut e = f.clone();
            e.underline = egui::Stroke::new(1.5, pal.error);
            e.background = pal.error.gamma_multiply(0.12);
            (f, e)
        })
        .collect();
    let fmt = |t: Tok, err: bool| {
        let i = TOKS.iter().position(|x| *x == t).unwrap_or(0);
        if err { formats[i].1.clone() } else { formats[i].0.clone() }
    };
    // byte ranges of error lines, in order
    let mut err_ranges: Vec<(usize, usize)> = Vec::new();
    if !err_lines.is_empty() {
        let mut line = 1u32;
        let mut start = 0;
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' {
                if err_lines.contains(&line) {
                    err_ranges.push((start, i));
                }
                line += 1;
                start = i + 1;
            }
        }
        if err_lines.contains(&line) {
            err_ranges.push((start, src.len()));
        }
    }
    let tokens = lexer::tokenize(src);
    let mut job = LayoutJob { text: src.to_owned(), ..Default::default() };
    job.sections.reserve(tokens.len() + 2 * err_ranges.len() + 1);
    let mut section = |a: usize, b: usize, format: TextFormat| {
        job.sections.push(egui::text::LayoutSection { leading_space: 0.0, byte_range: a.into()..b.into(), format });
    };
    let mut er = 0; // next error range that may overlap
    for tok in tokens {
        let (a, b) = (tok.range.start, tok.range.end);
        while er < err_ranges.len() && err_ranges[er].1 <= a {
            er += 1;
        }
        if er >= err_ranges.len() || err_ranges[er].0 >= b {
            section(a, b, fmt(tok.tok, false));
            continue;
        }
        // split the token at error-line boundaries
        let mut pos = a;
        let mut k = er;
        while pos < b {
            match err_ranges.get(k) {
                Some(&(s, e)) if pos >= s && pos < e => {
                    let end = e.min(b);
                    section(pos, end, fmt(tok.tok, true));
                    pos = end;
                    k += 1;
                }
                Some(&(s, _)) if s < b => {
                    section(pos, s, fmt(tok.tok, false));
                    pos = s;
                }
                _ => {
                    section(pos, b, fmt(tok.tok, false));
                    pos = b;
                }
            }
        }
    }
    if src.is_empty() {
        section(0, 0, fmt(Tok::Text, false));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_detection() {
        assert_eq!(trailing_begin("  \\begin{itemize}"), Some("itemize".into()));
        assert_eq!(trailing_begin("\\begin{figure}[htbp]"), Some("figure".into()));
        assert_eq!(trailing_begin("\\begin{tabular}{lr}"), Some("tabular".into()));
        assert_eq!(trailing_begin("\\begin{document}"), None);
        assert_eq!(trailing_begin("\\begin{x} text"), None);
        assert!(has_matching_end("\n a \\end{x}", "x"));
        assert!(!has_matching_end("\n\\begin{x}\\end{x}", "x"));
    }

    /// Plan: no noticeable typing latency with 10 000 lines. Every keystroke
    /// re-highlights and re-lays out the whole buffer.
    #[test]
    fn large_file_relayout_is_fast() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut o = ctx.run_ui(Default::default(), |_| {});
        o.textures_delta.clear();
        let pal = crate::theme::dark();
        let font = FontId::monospace(13.5);
        let mut src = String::new();
        for i in 0..10_000 {
            src.push_str(&format!("\\section{{Part {i}}} Text with $x_{{{i}}}^2$ and \\cite{{key{i}}} % note\n"));
        }
        // warm up the glyph cache
        let _ = ctx.fonts_mut(|f| f.layout_job(highlight(&src, &font, &pal, &[])));
        let t = Instant::now();
        let runs = 5;
        for k in 0..runs {
            src.push_str(&k.to_string());
            let job = highlight(&src, &font, &pal, &[3, 500]);
            let _ = ctx.fonts_mut(|f| f.layout_job(job));
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        let t2 = Instant::now();
        for _ in 0..runs {
            std::hint::black_box(highlight(&src, &font, &pal, &[3, 500]));
        }
        let hl = t2.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        let t3 = Instant::now();
        for _ in 0..runs {
            std::hint::black_box(lexer::tokenize(&src));
        }
        let lx = t3.elapsed().as_secs_f64() * 1000.0 / runs as f64;
        eprintln!("10 000 lines: {ms:.1} ms per keystroke (highlighting {hl:.1} ms, of that tokenizing {lx:.1} ms)");
        assert!(ms < 120.0, "{ms:.1} ms per keystroke");
    }

    #[test]
    fn highlight_covers_text_and_marks_error_lines() {
        let pal = crate::theme::dark();
        let font = FontId::monospace(13.0);
        let src = "\\section{A}\nbad \\foo line\n$x$ ok\n";
        let job = highlight(src, &font, &pal, &[2]);
        let mut pos = 0;
        for s in &job.sections {
            assert_eq!(usize::from(s.byte_range.start), pos);
            pos = usize::from(s.byte_range.end);
            let text = &src[usize::from(s.byte_range.start)..usize::from(s.byte_range.end)];
            let on_line_2 = usize::from(s.byte_range.start) >= 12 && usize::from(s.byte_range.end) <= 25;
            assert_eq!(s.format.underline.width > 0.0, on_line_2 && !text.is_empty(), "{text:?}");
        }
        assert_eq!(pos, src.len());
    }

    #[test]
    fn find_replace() {
        assert_eq!(match_positions("Ab ab AB", "ab", false), vec![0, 3, 6]);
        assert_eq!(replace_matches("a.a.a", "a", "b", true, None), "b.b.b");
        assert_eq!(replace_matches("a.a.a", "a", "b", true, Some(2)), "a.b.a");
        assert_eq!(char_index_of_line("ä\nb\nc", 3), 4);
    }
}
