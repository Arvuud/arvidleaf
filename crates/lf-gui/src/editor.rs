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
use crate::vim::{self, VKey, Vim};

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
    /// Line to scroll to once the text has been laid out (a freshly opened
    /// file has no layout yet, and wrapped lines make `line * row height` wrong).
    scroll_pending: Option<u32>,
    goto: Option<(u32, Option<(usize, usize)>)>,
    pub cursor_line: u32,
    pub cursor_col: u32,
    completion: Option<Completion>,
    find: Option<Find>,
    /// Closing brackets inserted by auto-pairing, innermost last (typing the
    /// closer right there steps over it, Backspace removes an empty pair).
    closers: Vec<Closer>,
    /// Argument positions of the last inserted template (`\frac{|}{}`): Tab jumps there.
    stops: Option<Stops>,
    /// Vim emulation (when switched on in the Edit menu).
    vim: Option<Vim>,
    /// Cursor position last set by Vim; anything else came from the mouse or typing.
    vim_expected: Option<usize>,
    /// Commands and environments of this file, for ranking suggestions
    /// (refreshed at most once a second while completing).
    vocab: Option<(Instant, complete::Vocab)>,
    /// Packages the text now needs (e.g. `enumitem` after `\begin{enumerate}`);
    /// the app adds them to the preamble.
    pub needs_packages: Vec<String>,
    /// Completions the user picked (`kind:item`), for the usage statistics.
    pub picked: Vec<String>,
    /// Library keys cited through completion; the app copies them into the project `.bib`.
    pub needs_entries: Vec<String>,
    /// An input method is composing text (preedit shown); its commit then
    /// belongs to the text widget, which replaces the preedit.
    ime_composing: bool,
    /// Char index right behind a command name the completion inserted, so
    /// the next key does not count it a second time as typed by hand.
    completed_at: Option<usize>,
}

/// A closing bracket the editor inserted itself.
#[derive(Clone, Debug, PartialEq)]
struct Closer {
    /// char index of `close`
    pos: usize,
    /// the opening text right before the pair's inside (for Backspace)
    open: String,
    close: String,
}

/// Tab stops of an inserted template, as char indices in ascending order.
#[derive(Clone, Debug, PartialEq)]
struct Stops {
    /// where the template starts; moving the cursor before it ends the stops
    start: usize,
    at: Vec<usize>,
}

/// `\begin{name}` under the cursor with its matching `\end{name}`, recorded
/// before a frame's edits so a renamed `\begin` can rename its `\end`.
struct EnvLink {
    /// char index of the name in `\begin{`
    begin_name: usize,
    name: String,
    /// char index of the name in the matching `\end{`
    end_name: usize,
    /// char count of the whole text
    len: usize,
}

/// What Ctrl+Click on a command leads to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Label(String),
    Citation(String),
    File(String),
}

/// How `pre_input` dealt with an event.
enum Handled {
    /// done; the text widget must not see it
    Consumed,
    /// not ours and harmless (e.g. a letter key before its text event)
    Pass,
    /// edits or moves the cursor in a way only the widget knows: this and all
    /// later events of the frame go to the widget
    Stop,
}

struct Completion {
    items: Vec<String>,
    selected: usize,
    /// char index where the typed prefix starts
    start: usize,
    kind: Context,
    /// Per item: comes from the reference library (not yet in the project).
    library: Vec<bool>,
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
    /// Keys of the reference library (offered after the project's own keys).
    pub lib_keys: &'a [String],
    /// `Knuth (1984): The TeXbook` for project and library keys (lower-case keys).
    pub cite_info: &'a std::collections::HashMap<String, String>,
    pub files: &'a [String],
    pub font_size: f32,
    pub vim: bool,
    /// How often the user picked each completion (all projects).
    pub usage: &'a lf_core::usage::Usage,
}

pub enum EditorAction {
    SyncToPdf(u32, u32),
    /// `:w` in Vim mode
    Save,
    /// Ctrl+Click on `\ref`, `\cite`, `\input` …
    Follow(Link),
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
            scroll_pending: None,
            goto: None,
            cursor_line: 1,
            cursor_col: 1,
            completion: None,
            find: None,
            closers: Vec::new(),
            stops: None,
            vim: None,
            vim_expected: None,
            vocab: None,
            needs_packages: Vec::new(),
            picked: Vec::new(),
            needs_entries: Vec::new(),
            ime_composing: false,
            completed_at: None,
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

    /// Another program wrote the file since we loaded or saved it.
    pub fn changed_on_disk(&self) -> bool {
        let m = std::fs::metadata(&self.abs).and_then(|m| m.modified()).ok();
        m.is_some() && m != self.disk_mtime
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
        if env.vim != self.vim.is_some() {
            self.vim = env.vim.then(Vim::default);
            self.vim_expected = None;
        }
        ui.horizontal(|ui| {
            if let Some(v) = &self.vim {
                let c = if v.mode == vim::Mode::Insert { pal.accent2 } else { pal.accent };
                ui.label(RichText::new(v.status()).monospace().small().color(c));
            }
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
        // Vim sees the keys first; in normal/visual mode it uses all of them
        let mut vim_moved = false;
        if has_focus && self.vim.is_some() {
            let (a, moved) = self.vim_input(&ctx);
            action = a.or(action);
            vim_moved = moved;
            if moved {
                // Vim edited the text directly: positions of pairs and stops are unknown
                self.closers.clear();
                self.stops = None;
            }
        }
        // a `\begin{name}` being edited renames its `\end{name}` after this frame's edits
        let env_link = if has_focus { self.cursor(&ctx).and_then(|r| env_link_at(&self.text, sorted(r))) } else { None };
        let block_cursor = self.vim.as_ref().filter(|v| has_focus && v.mode != vim::Mode::Insert).map(|v| v.cursor);

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
        // what the widget is about to edit (keys `pre_input` left to it)
        let before_widget = (self.text.chars().count(), self.cursor(&ctx).map(sorted));

        // ---- the editor (soft-wrapped, like Overleaf)
        let problems = &env.problems;
        let n_lines = self.text.split('\n').count().max(1);
        let mut scroll = egui::ScrollArea::vertical().id_salt(("editor_scroll", self.id)).auto_shrink([false, false]);
        if let Some(line) = self.goto.map(|g| g.0).or(self.scroll_pending) {
            // exact position from the last layout (lines may wrap); without one,
            // estimate now and correct in the next frame
            let y = match &self.cache {
                Some((_, _, g)) => {
                    self.scroll_pending = None;
                    g.pos_from_cursor(CCursor::new(char_index_of_line(&self.text, line))).min.y
                }
                None => {
                    self.scroll_pending = Some(line);
                    ctx.request_repaint();
                    (line as f32 - 1.0) * row_h
                }
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
                        if let Some((hh, w, g)) = cache
                            && *hh == h && *w == wrap {
                                return g.clone();
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
                    if let Some(c) = block_cursor {
                        let r = out.galley.pos_from_cursor(CCursor::new(c)).translate(out.galley_pos.to_vec2());
                        let block = egui::Rect::from_min_size(r.min, egui::vec2(char_w, r.height().max(row_h)));
                        painter.rect_filled(block, 1.0, pal.accent.gamma_multiply(0.45));
                    }
                    if vim_moved {
                        let c = self.vim.as_ref().map_or(0, |v| v.cursor);
                        let r = out.galley.pos_from_cursor(CCursor::new(c)).translate(out.galley_pos.to_vec2());
                        ui.scroll_to_rect(r.expand(row_h), None);
                    }
                    output = Some(out);
                });
            });
        });
        let out = output.unwrap();
        if self.vim.is_some() {
            // egui drops the focus on Escape unless the widget claims the key;
            // in Vim mode Escape switches to normal mode instead
            let filter = egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true };
            ui.memory_mut(|m| m.set_focus_lock_filter(id, filter));
        }
        if out.response.changed() {
            self.last_edit = Some(Instant::now());
        }
        // keep pairs and tab stops on their text after the widget's own edits
        let len = self.text.chars().count();
        if let (old_len, Some((a, b))) = before_widget
            && len != old_len {
                let removed = b - a;
                if a != b {
                    self.shift_marks(a, b, (len + removed).saturating_sub(old_len));
                } else if len > old_len {
                    self.shift_marks(a, a, len - old_len);
                } else {
                    // Backspace (Delete is caught by the check in `validate_marks`)
                    self.shift_marks(a.saturating_sub(old_len - len), a, 0);
                }
            }
        if let Some(link) = env_link {
            self.sync_end(&ctx, link);
        }

        // ---- Ctrl+Click follows \ref, \cite, \input …
        let cmd_held = ui.input(|i| i.modifiers.command);
        // hovering a citation key shows the entry
        if !cmd_held
            && let Some(p) = out.response.hover_pos()
        {
            let c = out.galley.cursor_from_pos(p - out.galley_pos);
            if let Some(Link::Citation(k)) = link_at(&self.text, usize::from(c.index)) {
                let info = env.cite_info.get(&k.to_lowercase()).cloned().unwrap_or_else(|| format!("{k}: not in the project's .bib files or the library"));
                // at the pointer: below the widget would be below the whole editor
                egui::Response::clone(&out.response).on_hover_ui_at_pointer(|ui| {
                    ui.label(info);
                });
            }
        }
        if cmd_held && let Some(p) = out.response.hover_pos() {
            let c = out.galley.cursor_from_pos(p - out.galley_pos);
            if link_at(&self.text, usize::from(c.index)).is_some() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                if out.response.clicked() {
                    action = link_at(&self.text, usize::from(c.index)).map(EditorAction::Follow);
                }
            }
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
                self.completion_popup(ui, pal, pos, env.cite_info);
            }
        } else if !has_focus {
            self.completion = None;
        }

        // Ctrl+S / Ctrl+Enter are handled globally by the app
        action
    }

    /// Feeds this frame's keys to Vim and removes the ones it used.
    /// Returns an action (`:w`) and whether Vim handled any key.
    fn vim_input(&mut self, ctx: &egui::Context) -> (Option<EditorAction>, bool) {
        let widget_cursor = self.cursor(ctx).map(|r| usize::from(r.primary.index));
        let Some(v) = self.vim.as_mut() else { return (None, false) };
        if let Some(c) = widget_cursor
            && Some(c) != self.vim_expected {
                v.sync_cursor(&self.text, c);
            }
        let events = ctx.input(|i| i.events.clone());
        let leaves_insert = events.iter().any(|e| {
            matches!(e, egui::Event::Key { key: Key::Escape, pressed: true, .. }) || matches!(e, egui::Event::Key { key: Key::OpenBracket, pressed: true, modifiers, .. } if modifiers.ctrl)
        });
        // keys typed in insert mode go to the text widget (auto-closing, completion),
        // unless the mode changes within this frame
        v.self_insert = v.mode != vim::Mode::Insert || leaves_insert;
        let mut consumed = vec![false; events.len()];
        let mut action = None;
        let mut changed = false;
        let mut used = false;
        for (k, ev) in events.iter().enumerate() {
            let keys: Vec<VKey> = match ev {
                egui::Event::Text(t) => t.chars().map(VKey::Char).collect(),
                egui::Event::Key { key, pressed: true, modifiers, .. } => {
                    if modifiers.ctrl || modifiers.command {
                        let c = match key {
                            Key::R => 'r',
                            Key::D => 'd',
                            Key::U => 'u',
                            Key::N => 'n',
                            Key::P => 'p',
                            Key::OpenBracket => '[',
                            _ => continue,
                        };
                        if v.mode == vim::Mode::Insert && c != '[' {
                            continue;
                        }
                        vec![VKey::Ctrl(c)]
                    } else {
                        match key {
                            Key::Escape => vec![VKey::Esc],
                            Key::Enter => vec![VKey::Enter],
                            Key::Backspace => vec![VKey::Backspace],
                            Key::Delete => vec![VKey::Delete],
                            Key::Tab => vec![VKey::Tab],
                            Key::ArrowLeft => vec![VKey::Left],
                            Key::ArrowRight => vec![VKey::Right],
                            Key::ArrowUp => vec![VKey::Up],
                            Key::ArrowDown => vec![VKey::Down],
                            Key::Home => vec![VKey::Home],
                            Key::End => vec![VKey::End],
                            Key::PageUp => vec![VKey::PageUp],
                            Key::PageDown => vec![VKey::PageDown],
                            // letters arrive as Text events; keep the widget from acting on the key
                            _ => {
                                consumed[k] = v.mode != vim::Mode::Insert || v.self_insert;
                                continue;
                            }
                        }
                    }
                }
                // pasting in normal mode would bypass Vim
                egui::Event::Paste(_) | egui::Event::Cut if v.mode == vim::Mode::Normal => {
                    consumed[k] = true;
                    continue;
                }
                _ => continue,
            };
            for key in keys {
                let r = v.handle(&mut self.text, key);
                consumed[k] |= r.consumed;
                used |= r.consumed;
                changed |= r.changed;
                if r.save {
                    action = Some(EditorAction::Save);
                }
                if let Some(t) = r.copy {
                    ctx.copy_text(t);
                }
            }
        }
        if leaves_insert {
            self.completion = None;
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
        if changed {
            self.last_edit = Some(Instant::now());
            self.cache = None;
        }
        if used {
            let v = self.vim.as_ref().unwrap();
            let range = match v.selection(&self.text) {
                // the widget's selection shows the visual range; primary end = Vim's cursor side
                Some((a, b)) if v.cursor < v.anchor => CCursorRange::two(CCursor::new(b), CCursor::new(a)),
                Some((a, b)) => CCursorRange::two(CCursor::new(a), CCursor::new(b)),
                None => CCursorRange::one(CCursor::new(v.cursor)),
            };
            self.vim_expected = Some(usize::from(range.primary.index));
            self.set_cursor(ctx, range);
        }
        (action, used)
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
        self.shift_marks(a, b, s.chars().count());
        let ba = byte_of_char(&self.text, a);
        let bb = byte_of_char(&self.text, b);
        self.text.replace_range(ba..bb, s);
        self.set_cursor(ctx, CCursorRange::one(CCursor::new(cursor)));
        self.last_edit = Some(Instant::now());
        self.cache = None;
    }

    /// Like [`splice`](Self::splice), but selects `sel_a..sel_b` afterwards.
    fn splice_select(&mut self, ctx: &egui::Context, a: usize, b: usize, s: &str, sel_a: usize, sel_b: usize) {
        self.splice(ctx, a, b, s, sel_b);
        self.set_cursor(ctx, CCursorRange::two(CCursor::new(sel_a), CCursor::new(sel_b)));
    }

    /// Moves remembered closers and tab stops for an edit that replaces chars
    /// `a..b` with `ins` chars. Closers inside the replaced text are dropped.
    fn shift_marks(&mut self, a: usize, b: usize, ins: usize) {
        let moved = |p: usize| p - (b - a) + ins;
        self.closers.retain_mut(|c| {
            if c.pos >= b {
                c.pos = moved(c.pos);
                true
            } else {
                c.pos < a
            }
        });
        if let Some(s) = &mut self.stops {
            for p in s.at.iter_mut() {
                if *p >= b {
                    *p = moved(*p);
                } else if *p > a {
                    *p = a;
                }
            }
            if s.start >= b {
                s.start = moved(s.start);
            } else if s.start > a {
                s.start = a;
            }
        }
    }

    /// Forgets closers that are no longer right after the cursor's pair and
    /// tab stops the cursor has left behind.
    fn validate_marks(&mut self, cursor: usize) {
        let text = &self.text;
        self.closers.retain(|c| c.pos >= cursor && text[byte_of_char(text, c.pos)..].starts_with(&c.close));
        if self.stops.as_ref().is_some_and(|s| cursor < s.start || s.at.last().is_none_or(|l| cursor > *l)) {
            self.stops = None;
        }
    }

    fn inserting(&self) -> bool {
        self.vim.as_ref().is_none_or(|v| v.mode == vim::Mode::Insert)
    }

    /// Intercepts keys for auto-pairing, `\begin`/`\item` completion, editing
    /// shortcuts and the completion popup. Returns typed text (if any) for
    /// completion updates.
    fn pre_input(&mut self, ctx: &egui::Context) -> Option<String> {
        let range = self.cursor(ctx)?;
        self.validate_marks(sorted(range).0);
        let mut typed = None;
        let events = ctx.input(|i| i.events.clone());
        if debug_keys() {
            for ev in events.iter().filter(|e| matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Ime(_))) {
                eprintln!("[keys] {ev:?}");
            }
        }
        let mut consumed = vec![false; events.len()];
        for (k, ev) in events.iter().enumerate() {
            // the cursor as left by the previous event
            let Some(range) = self.cursor(ctx) else { break };
            let (a, b) = sorted(range);
            match self.handle_event(ctx, ev, a, b, &mut typed) {
                Handled::Consumed => consumed[k] = true,
                Handled::Pass => {}
                Handled::Stop => break,
            }
        }
        self.finish_pre_input(ctx, consumed, typed)
    }

    fn handle_event(&mut self, ctx: &egui::Context, ev: &egui::Event, a: usize, b: usize, typed: &mut Option<String>) -> Handled {
        use Handled::*;
        let inserting = self.inserting();
        match ev {
            egui::Event::Key { key, pressed: true, modifiers: m, .. } => {
                if self.completion.is_some() && !m.command {
                    match *key {
                        Key::ArrowDown => {
                            let c = self.completion.as_mut().unwrap();
                            c.selected = (c.selected + 1) % c.items.len().max(1);
                            return Consumed;
                        }
                        Key::ArrowUp => {
                            let c = self.completion.as_mut().unwrap();
                            c.selected = (c.selected + c.items.len().max(1) - 1) % c.items.len().max(1);
                            return Consumed;
                        }
                        Key::Tab | Key::Enter => {
                            self.accept_completion(ctx, a);
                            return Consumed;
                        }
                        Key::Escape => {
                            self.completion = None;
                            return Consumed;
                        }
                        _ => {}
                    }
                }
                let ctrl = m.command && !m.shift && !m.alt;
                let ctrl_shift = m.command && m.shift && !m.alt;
                let alt = m.alt && !m.command && !m.shift;
                if !inserting {
                    return if m.command || is_edit_key(*key) { Stop } else { Pass };
                }
                match *key {
                    Key::Enter if !m.any() && a == b => self.enter(ctx, a),
                    Key::Tab if !m.any() => {
                        if a != b && self.selected_lines(a, b) {
                            self.indent_lines(ctx, a, b, false);
                            Consumed
                        } else if self.jump_to_stop(ctx, b) {
                            Consumed
                        } else {
                            Stop
                        }
                    }
                    Key::Tab if m.shift && !m.command && !m.alt => {
                        self.indent_lines(ctx, a, b, true);
                        Consumed
                    }
                    Key::Backspace if !m.command && a == b => {
                        *typed = Some(String::new());
                        if self.delete_pair(ctx, a) { Consumed } else { Stop }
                    }
                    Key::B if ctrl => self.wrap_selection(ctx, "\\textbf{", "}"),
                    Key::I if ctrl => self.wrap_selection(ctx, "\\textit{", "}"),
                    Key::E if ctrl => self.wrap_selection(ctx, "\\emph{", "}"),
                    Key::M if ctrl => self.wrap_selection(ctx, "\\(", "\\)"),
                    Key::M if ctrl_shift => self.wrap_selection(ctx, "\\[", "\\]"),
                    Key::E if ctrl_shift => self.wrap_in_environment(ctx, a, b),
                    Key::D if ctrl => self.duplicate_lines(ctx, a, b),
                    Key::L if ctrl => self.select_lines(ctx, a, b),
                    Key::ArrowUp if alt => self.move_lines(ctx, a, b, true),
                    Key::ArrowDown if alt => self.move_lines(ctx, a, b, false),
                    Key::Backspace => {
                        *typed = Some(String::new());
                        Stop
                    }
                    // AltGr is reported as Ctrl+Alt on some systems: AltGr+7 = `{`
                    // is no command, its text event follows
                    k if (m.command && !m.alt) || is_edit_key(k) => Stop,
                    _ => Pass,
                }
            }
            egui::Event::Text(t) => {
                if !inserting {
                    return Stop;
                }
                *typed = Some(t.clone());
                self.type_text(ctx, t, a, b);
                Consumed
            }
            // with an input method (IBus, Fcitx, KWin's text input) typed
            // characters arrive as IME commits instead of text events
            // winit on Wayland sends an empty preedit after every cursor move
            // (the compositor's `done`); it changes nothing and must not end
            // our handling of the keys after it
            egui::Event::Ime(egui::ImeEvent::Preedit { text, .. }) if text.is_empty() => {
                self.ime_composing = false;
                Pass
            }
            egui::Event::Ime(egui::ImeEvent::Preedit { .. }) => {
                self.ime_composing = true;
                Stop
            }
            egui::Event::Ime(egui::ImeEvent::Enabled | egui::ImeEvent::Disabled) => Pass,
            egui::Event::Ime(egui::ImeEvent::Commit(t)) if !self.ime_composing && !t.is_empty() => {
                if !inserting {
                    return Stop;
                }
                *typed = Some(t.clone());
                self.type_text(ctx, t, a, b);
                Consumed
            }
            egui::Event::Ime(egui::ImeEvent::Commit(_)) => {
                self.ime_composing = false;
                Stop
            }
            egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Ime(_) => Stop,
            _ => Pass,
        }
    }

    /// Typed text: steps over a closer, wraps a selection, inserts a pair, or
    /// just inserts the text.
    fn type_text(&mut self, ctx: &egui::Context, t: &str, a: usize, b: usize) {
        if let Some(ch) = t.chars().next() {
            self.note_typed_command(a, ch);
        }
        let mut chars = t.chars();
        let single = match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        };
        if let Some(ch) = single {
            if a == b
                && let Some(i) = self.closers.iter().rposition(|c| c.pos == a && c.close == t) {
                    // type over the closer we inserted ourselves
                    self.closers.remove(i);
                    self.set_cursor(ctx, CCursorRange::one(CCursor::new(a + 1)));
                    return;
                }
            let before = &self.text[..byte_of_char(&self.text, a)];
            if a != b && !ends_escaped(before) {
                // a selection goes inside: `$x$`, `{x}`, `[x]`, `(x)`
                let close = match ch {
                    '$' => Some('$'),
                    '{' => Some('}'),
                    '[' => Some(']'),
                    '(' => Some(')'),
                    _ => None,
                };
                if let Some(close) = close {
                    let sel: String = self.text.chars().skip(a).take(b - a).collect();
                    self.splice(ctx, a, b, &format!("{ch}{sel}{close}"), b + 2);
                    return;
                }
            }
            let next = self.text.chars().nth(a);
            if a == b
                && let Some((open, close)) = pair_for(ch, before, next) {
                    self.splice(ctx, a, a, &format!("{ch}{close}"), a + 1);
                    self.closers.push(Closer { pos: a + 1, open, close });
                    return;
                }
        }
        self.splice(ctx, a, b, t, a + t.chars().count());
    }

    /// A command name typed by hand and finished with `ch` (space, `{`, `_` …)
    /// counts for the user's suggestions like a picked completion, so
    /// someone who always writes `\leqslant` gets it before `\leq`.
    fn note_typed_command(&mut self, cursor: usize, ch: char) {
        if ch.is_ascii_alphabetic() || self.completed_at.take() == Some(cursor) {
            return;
        }
        let before = &self.text[..byte_of_char(&self.text, cursor)];
        let name_len = before.bytes().rev().take_while(u8::is_ascii_alphabetic).count();
        let bs = before.len() - name_len;
        if name_len < 2 || !before[..bs].ends_with('\\') || ends_escaped(&before[..bs - 1]) {
            return;
        }
        let name = &before[bs..];
        let template = complete::COMMANDS.iter().find(|c| c.split('{').next() == Some(name)).map(|c| c.to_string());
        let template = template.unwrap_or_else(|| if ch == '{' { format!("{name}{{}}") } else { name.to_string() });
        self.picked.push(format!("cmd:{template}"));
    }

    /// Backspace between a pair the editor inserted (`(|)`) removes both parts.
    fn delete_pair(&mut self, ctx: &egui::Context, a: usize) -> bool {
        let Some(i) = self.closers.iter().rposition(|c| c.pos == a) else { return false };
        let c = self.closers[i].clone();
        if !self.text[..byte_of_char(&self.text, a)].ends_with(&c.open) {
            return false;
        }
        let start = a - c.open.chars().count();
        self.closers.remove(i);
        self.splice(ctx, start, a + c.close.chars().count(), "", start);
        true
    }

    /// Enter without a selection: `\end{…}` after `\begin{…}`, a new `\item` in
    /// lists, otherwise the line's indentation.
    fn enter(&mut self, ctx: &egui::Context, a: usize) -> Handled {
        self.note_typed_command(a, '\n');
        let byte = byte_of_char(&self.text, a);
        let line_start = self.text[..byte].rfind('\n').map_or(0, |p| p + 1);
        let line_end = self.text[byte..].find('\n').map_or(self.text.len(), |p| byte + p);
        let line = &self.text[line_start..byte];
        let indent: String = line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        if let Some(env) = trailing_begin(line)
            && !has_matching_end(&self.text[byte..], &env) {
                self.need_package(lf_tex::packages::for_env(&env));
                let item = if is_list(&env) { "\\item " } else { "" };
                let ins = format!("\n{indent}  {item}\n{indent}\\end{{{env}}}");
                let cur = a + 1 + indent.chars().count() + 2 + item.len();
                self.splice(ctx, a, a, &ins, cur);
                return Handled::Consumed;
            }
        let whole = &self.text[line_start..line_end];
        if is_item_line(whole) && open_env(&self.text[..line_start]).is_some_and(|e| is_list(&e)) {
            if whole.trim() == "\\item" {
                // Enter on an empty item ends the list
                self.end_list(ctx, line_start, line_end);
            } else {
                let ins = format!("\n{indent}\\item ");
                let n = ins.chars().count();
                self.splice(ctx, a, a, &ins, a + n);
            }
            return Handled::Consumed;
        }
        if indent.is_empty() {
            return Handled::Stop;
        }
        let ins = format!("\n{indent}");
        let n = ins.chars().count();
        self.splice(ctx, a, a, &ins, a + n);
        Handled::Consumed
    }

    /// Removes the empty `\item` line (bytes `ls..le`) and continues after
    /// the list's `\end{…}` if it follows directly.
    fn end_list(&mut self, ctx: &egui::Context, ls: usize, le: usize) {
        let ca = self.text[..ls].chars().count();
        let ce = ca + self.text[ls..le].chars().count();
        let next_end = self.text[le..].strip_prefix('\n').map(|rest| {
            let l = rest.split('\n').next().unwrap_or("");
            (l.to_string(), l.trim_start().starts_with("\\end{"))
        });
        match next_end {
            Some((end_line, true)) if ca > 0 => {
                // `\n<item line>\n<end line>` → `\n<end line>\n<indent>`
                let end_indent: String = end_line.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let ins = format!("\n{end_line}\n{end_indent}");
                let stop = ce + 1 + end_line.chars().count();
                let cur = ca - 1 + ins.chars().count();
                self.splice(ctx, ca - 1, stop, &ins, cur);
            }
            _ => {
                let indent: String = self.text[ls..le].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let outer: String = indent.chars().skip(2).collect();
                let n = outer.chars().count();
                self.splice(ctx, ca, ce, &outer, ca + n);
            }
        }
    }

    /// Removes the events `pre_input` handled itself. Events after an early
    /// stop stay for the text widget.
    fn finish_pre_input(&mut self, ctx: &egui::Context, consumed: Vec<bool>, typed: Option<String>) -> Option<String> {
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

    /// Wraps the selection in `open … close` (`\textbf{…}`), or inserts the
    /// pair with the cursor in between (Tab then jumps behind it).
    fn wrap_selection(&mut self, ctx: &egui::Context, open: &str, close: &str) -> Handled {
        let Some(range) = self.cursor(ctx) else { return Handled::Consumed };
        let (a, b) = sorted(range);
        let sel: String = self.text.chars().skip(a).take(b - a).collect();
        let (no, nc) = (open.chars().count(), close.chars().count());
        let cur = if a == b { a + no } else { b + no + nc };
        self.splice(ctx, a, b, &format!("{open}{sel}{close}"), cur);
        if a == b {
            self.stops = Some(Stops { start: a, at: vec![a + no + nc] });
            self.closers.push(Closer { pos: a + no, open: open.to_string(), close: close.to_string() });
        }
        self.completion = None;
        Handled::Consumed
    }

    /// Ctrl+Shift+E: the selected lines (or the current one) go into
    /// `\begin{}…\end{}`, indented, with the cursor in the name.
    fn wrap_in_environment(&mut self, ctx: &egui::Context, a: usize, b: usize) -> Handled {
        let (ls, le) = line_bounds(&self.text, a, b);
        let block: String = self.text.chars().skip(ls).take(le - ls).collect();
        let indent: String = block.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let body = if block.trim().is_empty() {
            format!("{indent}  ")
        } else {
            block.split('\n').map(|l| if l.trim().is_empty() { l.to_string() } else { format!("  {l}") }).collect::<Vec<_>>().join("\n")
        };
        let ins = format!("{indent}\\begin{{}}\n{body}\n{indent}\\end{{}}");
        let ni = indent.chars().count();
        let cur = ls + ni + "\\begin{".len();
        // Tab: from the name to the start of the body
        let body_start = ls + ni + "\\begin{}\n".len() + ni + 2;
        self.splice(ctx, ls, le, &ins, cur);
        self.stops = Some(Stops { start: cur, at: vec![body_start] });
        self.completion = None;
        Handled::Consumed
    }

    /// Whether `a..b` covers more than one line.
    fn selected_lines(&self, a: usize, b: usize) -> bool {
        self.text.chars().skip(a).take(b - a).any(|c| c == '\n')
    }

    /// Tab / Shift+Tab on the lines of `a..b`: two spaces more or less.
    fn indent_lines(&mut self, ctx: &egui::Context, a: usize, b: usize, out: bool) {
        let (ls, le) = line_bounds(&self.text, a, b);
        let block: String = self.text.chars().skip(ls).take(le - ls).collect();
        let mut first_shift: isize = 0;
        let new: Vec<String> = block
            .split('\n')
            .enumerate()
            .map(|(i, l)| {
                let (nl, d) = if out {
                    let n = if l.starts_with('\t') { 1 } else { l.chars().take(2).take_while(|c| *c == ' ').count() };
                    (l.chars().skip(n).collect(), -(n as isize))
                } else if l.trim().is_empty() {
                    (l.to_string(), 0)
                } else {
                    (format!("  {l}"), 2)
                };
                if i == 0 {
                    first_shift = d;
                }
                nl
            })
            .collect();
        let new = new.join("\n");
        let new_le = ls + new.chars().count();
        if a == b {
            let cur = (a as isize + first_shift).max(ls as isize) as usize;
            self.splice(ctx, ls, le, &new, cur);
        } else {
            self.splice_select(ctx, ls, le, &new, ls, new_le);
        }
    }

    /// Ctrl+D: copies the lines of the selection below them; the selection follows the copy.
    fn duplicate_lines(&mut self, ctx: &egui::Context, a: usize, b: usize) -> Handled {
        let (ls, le) = line_bounds(&self.text, a, b);
        let block: String = self.text.chars().skip(ls).take(le - ls).collect();
        let d = le - ls + 1;
        self.splice_select(ctx, le, le, &format!("\n{block}"), a + d, b + d);
        Handled::Consumed
    }

    /// Ctrl+L: selects the current line; on a selection of whole lines, adds the next one.
    fn select_lines(&mut self, ctx: &egui::Context, a: usize, b: usize) -> Handled {
        let n = self.text.chars().count();
        let (ls, le) = line_bounds(&self.text, a, b);
        let whole = a == ls && a != b && (b == n || self.text.chars().nth(b - 1) == Some('\n'));
        let end = if whole && b < n {
            // extend over the next line
            let (_, next_end) = line_bounds(&self.text, b, b);
            next_end
        } else {
            le
        };
        let end = (end + 1).min(n);
        self.set_cursor(ctx, CCursorRange::two(CCursor::new(ls), CCursor::new(end)));
        Handled::Consumed
    }

    /// Alt+↑/↓: swaps the lines of the selection with the line above or below.
    fn move_lines(&mut self, ctx: &egui::Context, a: usize, b: usize, up: bool) -> Handled {
        let (ls, le) = line_bounds(&self.text, a, b);
        let n = self.text.chars().count();
        let block: String = self.text.chars().skip(ls).take(le - ls).collect();
        if up {
            if ls == 0 {
                return Handled::Consumed;
            }
            let (ps, _) = line_bounds(&self.text, ls - 1, ls - 1);
            let prev: String = self.text.chars().skip(ps).take(ls - 1 - ps).collect();
            let d = ls - ps;
            self.splice_select(ctx, ps, le, &format!("{block}\n{prev}"), a - d, b - d);
        } else {
            if le >= n {
                return Handled::Consumed;
            }
            let (_, ne) = line_bounds(&self.text, le + 1, le + 1);
            let next: String = self.text.chars().skip(le + 1).take(ne - le - 1).collect();
            let d = ne - le;
            self.splice_select(ctx, ls, ne, &format!("{next}\n{block}"), a + d, b + d);
        }
        Handled::Consumed
    }

    /// Tab after a template: to the next argument. False if there is none.
    fn jump_to_stop(&mut self, ctx: &egui::Context, cursor: usize) -> bool {
        let Some(s) = &mut self.stops else { return false };
        s.at.retain(|p| *p > cursor);
        let Some(&next) = s.at.first() else {
            self.stops = None;
            return false;
        };
        s.at.remove(0);
        if s.at.is_empty() {
            self.stops = None;
        }
        // stepping out of a pair leaves its closer behind
        self.closers.retain(|c| c.pos >= next);
        self.set_cursor(ctx, CCursorRange::one(CCursor::new(next)));
        self.completion = None;
        true
    }

    /// Renames the `\end{…}` of a `\begin{…}` whose name was edited this frame.
    fn sync_end(&mut self, ctx: &egui::Context, link: EnvLink) {
        let len = self.text.chars().count();
        let delta = len as isize - link.len as isize;
        let nb = byte_of_char(&self.text, link.begin_name);
        if !self.text[..nb].ends_with("\\begin{") {
            return;
        }
        let Some(close) = self.text[nb..].find(['}', '\n']).filter(|p| self.text[nb + p..].starts_with('}')) else { return };
        let name = self.text[nb..nb + close].to_string();
        let old_n = link.name.chars().count();
        // only the name changed, nothing else in the text
        if name == link.name || name.contains(['\\', '{']) || delta != name.chars().count() as isize - old_n as isize {
            return;
        }
        let end = (link.end_name as isize + delta) as usize;
        let eb = byte_of_char(&self.text, end);
        if !(self.text[..eb].ends_with("\\end{") && self.text[eb..].starts_with(&format!("{}}}", link.name))) {
            return;
        }
        let cur = self.cursor(ctx);
        self.shift_marks(end, end + old_n, name.chars().count());
        self.text.replace_range(eb..eb + link.name.len(), &name);
        // the cursor is in `\begin`, before the change
        if let Some(r) = cur {
            self.set_cursor(ctx, r);
        }
        self.cache = None;
        self.last_edit = Some(Instant::now());
    }

    fn need_package(&mut self, pkg: Option<&str>) {
        if let Some(p) = pkg
            && !self.needs_packages.iter().any(|x| x == p) {
                self.needs_packages.push(p.to_string());
            }
    }

    /// Inserts `s` at byte `at` (from the app, e.g. a `\usepackage` line),
    /// keeping the cursor on the same text.
    pub fn insert_at(&mut self, ctx: &egui::Context, at: usize, s: &str) {
        let at_char = self.text[..at].chars().count();
        let n = s.chars().count();
        let range = self.cursor(ctx);
        self.shift_marks(at_char, at_char, n);
        self.text.insert_str(at, s);
        if let Some(r) = range {
            let shift = |c: CCursor| {
                let i = usize::from(c.index);
                CCursor::new(if i >= at_char { i + n } else { i })
            };
            self.set_cursor(ctx, CCursorRange::two(shift(r.secondary), shift(r.primary)));
        }
        if let Some(c) = &mut self.completion
            && c.start >= at_char {
                c.start += n;
            }
        self.last_edit = Some(Instant::now());
        self.cache = None;
    }

    fn toggle_comment(&mut self, ctx: &egui::Context) {
        let Some(range) = self.cursor(ctx) else { return };
        let (a, b) = sorted(range);
        let ba = byte_of_char(&self.text, a);
        let bb = byte_of_char(&self.text, b);
        let start = self.text[..ba].rfind('\n').map_or(0, |p| p + 1);
        let end = self.text[bb..].find('\n').map_or(self.text.len(), |p| bb + p);
        let block = &self.text[start..end];
        let new = toggle_comment_block(block);
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
        let prefix = match &kind {
            Context::Command(p) | Context::Environment(p) | Context::Label(p) | Context::Citation(p) | Context::File(p) => p.clone(),
        };
        // the file's own commands and environments count, and so do earlier picks
        if matches!(kind, Context::Command(_) | Context::Environment(_)) && self.vocab.as_ref().is_none_or(|v| v.0.elapsed().as_secs_f32() > 1.0) {
            let skip = matches!(kind, Context::Command(_)).then(|| byte - prefix.len());
            self.vocab = Some((Instant::now(), complete::vocabulary(&self.text, skip)));
        }
        let empty = complete::Vocab::default();
        let vocab = self.vocab.as_ref().map_or(&empty, |v| &v.1);
        let key = usage_kind(&kind);
        let math = matches!(kind, Context::Command(_)) && complete::in_math(&self.text[..byte - prefix.len()]);
        let score = |item: &str| {
            let user = env.usage.get(&format!("{key}:{item}"));
            match kind {
                // general frequency, math or text mode, the document, the user's habits
                Context::Command(_) => complete::command_score(item, user, vocab.commands.get(item).copied().unwrap_or(0), math),
                Context::Environment(_) => 3 * user + vocab.envs.get(item).copied().unwrap_or(0),
                _ => 3 * user,
            }
        };
        const N: usize = 12;
        let mut lib_from = usize::MAX;
        let items: Vec<String> = match &kind {
            Context::Command(_) => {
                let all = complete::COMMANDS.iter().copied().chain(vocab.commands.keys().map(String::as_str));
                complete::rank_by(&prefix, all, score, N).into_iter().map(String::from).collect()
            }
            Context::Environment(_) => {
                let all = complete::ENVIRONMENTS.iter().copied().chain(vocab.envs.keys().map(String::as_str));
                if prefix.is_empty() {
                    // `\begin{`: the most used first
                    let mut v: Vec<&str> = all.collect::<std::collections::BTreeSet<_>>().into_iter().collect();
                    v.sort_by_key(|e| std::cmp::Reverse(score(e)));
                    v.into_iter().take(N).map(String::from).collect()
                } else {
                    complete::rank_by(&prefix, all, score, N).into_iter().map(String::from).collect()
                }
            }
            Context::Label(_) => complete::rank_by(&prefix, env.labels.iter().map(String::as_str), score, N).into_iter().map(String::from).collect(),
            Context::Citation(_) => {
                // the project's keys first, then library keys it does not have yet
                let mut items: Vec<String> = complete::rank_by(&prefix, env.bib_keys.iter().map(String::as_str), score, N).into_iter().map(String::from).collect();
                let room = N.saturating_sub(items.len()).max(if prefix.is_empty() { 0 } else { 4 });
                let in_project = |k: &str| env.bib_keys.iter().any(|p| p.eq_ignore_ascii_case(k));
                lib_from = items.len();
                items.extend(complete::rank_by(&prefix, env.lib_keys.iter().map(String::as_str).filter(|k| !in_project(k)), score, room).into_iter().map(String::from));
                items
            }
            Context::File(_) => complete::rank_by(&prefix, env.files.iter().map(String::as_str), score, N).into_iter().map(String::from).collect(),
        };
        if items.is_empty() {
            self.completion = None;
            return;
        }
        let start = cursor - prefix.chars().count();
        let selected = self.completion.as_ref().filter(|c| c.start == start).map_or(0, |c| c.selected.min(items.len() - 1));
        let library = (0..items.len()).map(|i| i >= lib_from).collect();
        self.completion = Some(Completion { items, selected, start, kind, library });
    }

    fn accept_completion(&mut self, ctx: &egui::Context, cursor: usize) {
        let Some(c) = self.completion.take() else { return };
        let Some(item) = c.items.get(c.selected).cloned() else { return };
        self.picked.push(format!("{}:{item}", usage_kind(&c.kind)));
        self.vocab = None;
        match c.kind {
            Context::Command(_) => {
                // `section{}` → `section{|}`
                let (name, rest) = item.split_once('{').map_or((item.as_str(), ""), |(n, r)| (n, r));
                let args = if rest.is_empty() { String::new() } else { format!("{{{rest}") };
                let ins = format!("{name}{args}");
                let cur = c.start + name.chars().count() + usize::from(!args.is_empty());
                self.completed_at = Some(c.start + name.chars().count());
                self.need_package(lf_tex::packages::for_command(name));
                self.splice(ctx, c.start, cursor, &ins, cur);
                if !args.is_empty() {
                    // Tab: into each further argument, then behind the command
                    let base = c.start + name.chars().count();
                    let mut at: Vec<usize> = args.char_indices().skip(1).filter(|(_, ch)| *ch == '{' || *ch == '[').map(|(i, _)| base + i + 1).collect();
                    at.push(c.start + ins.chars().count());
                    self.stops = Some(Stops { start: c.start, at });
                    self.closers.push(Closer { pos: cur, open: "{".into(), close: "}".into() });
                }
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
                if is_begin {
                    self.need_package(lf_tex::packages::for_env(&item));
                }
                // an `\end{…}` for the name typed so far is renamed by `sync_end`
                let typed_name: String = self.text.chars().skip(c.start).take(cursor - c.start).collect();
                let has_end = has_matching_end(&self.text[byte..], &item) || (after_close && has_matching_end(&self.text[byte + 1..], &typed_name));
                let mut stops = Vec::new();
                if is_begin && !has_end {
                    let args = env_args(&item);
                    let item_ins = if is_list(&item) { "\\item " } else { "" };
                    let body_cur = cur + args.chars().count() + 1 + indent.chars().count() + 2 + item_ins.len();
                    // empty arguments get the cursor, then Tab stops, then the body
                    let base = cur;
                    let mut holes = args.match_indices("{}").map(move |(i, _)| base + i + 1);
                    if let Some(first) = holes.next() {
                        stops.extend(holes);
                        stops.push(body_cur);
                        cur = first;
                    } else {
                        cur = body_cur;
                    }
                    ins.push_str(&format!("{args}\n{indent}  {item_ins}\n{indent}\\end{{{item}}}"));
                }
                self.splice(ctx, c.start, end, &ins, cur);
                if !stops.is_empty() {
                    self.stops = Some(Stops { start: c.start, at: stops });
                }
            }
            _ => {
                if c.library.get(c.selected).copied().unwrap_or(false) {
                    self.needs_entries.push(item.clone());
                }
                let cur = c.start + item.chars().count();
                self.splice(ctx, c.start, cursor, &item, cur);
            }
        }
    }

    /// Replaces the whole text (from the app, e.g. an updated `.bib`); the cursor keeps its
    /// position as far as the new text allows. Undo works as for any edit.
    pub fn replace_text(&mut self, ctx: &egui::Context, new: &str) {
        let n = new.chars().count();
        let cur = self.cursor(ctx).map_or(0, |r| usize::from(r.primary.index)).min(n);
        let len = self.text.chars().count();
        self.splice(ctx, 0, len, new, cur);
    }

    /// Inserts several texts at byte offsets (ascending), keeping the cursor on its text.
    pub fn insert_all(&mut self, ctx: &egui::Context, ins: &[(usize, String)]) {
        for (at, s) in ins.iter().rev() {
            self.insert_at(ctx, *at, s);
        }
    }

    /// Cites `keys` at the cursor: added to the citation the cursor is in, else a new
    /// `\cmd{…}` (`[@…]` in Markdown) is inserted. Returns false without a cursor.
    pub fn insert_citation(&mut self, ctx: &egui::Context, cmd: &str, keys: &[String]) -> bool {
        let Some(r) = self.cursor(ctx) else { return false };
        let (a, b) = sorted(r);
        let markdown = self.rel.extension().is_some_and(|e| e == "md");
        let byte = byte_of_char(&self.text, b);
        if a == b
            && let Some(span) = lf_ref::project::cite_at(&self.text, byte)
        {
            let add = span.addition(keys);
            if !add.is_empty() {
                let at = self.text[..span.close].chars().count();
                let cur = at + add.chars().count();
                self.splice(ctx, at, at, &add, cur);
            }
            return true;
        }
        let mut ins = lf_ref::project::cite_text(cmd, keys, markdown);
        // `word~\cite{…}` keeps the reference on the line of its word (not for `\textcite`,
        // which starts a phrase)
        let prev = self.text[..byte_of_char(&self.text, a)].chars().next_back();
        if !markdown && !matches!(cmd, "textcite" | "Textcite" | "citet") && prev.is_some_and(|c| c.is_alphanumeric() || matches!(c, '.' | ',' | ')' | '}')) {
            ins.insert(0, '~');
        }
        let cur = a + ins.chars().count();
        self.splice(ctx, a, b, &ins, cur);
        ctx.memory_mut(|m| m.request_focus(self.id()));
        true
    }

    fn completion_popup(&mut self, ui: &mut egui::Ui, pal: &Palette, pos: egui::Pos2, cite_info: &std::collections::HashMap<String, String>) {
        let Some(c) = &mut self.completion else { return };
        let mut clicked = None;
        egui::Area::new(egui::Id::new(("completion", self.id))).order(egui::Order::Foreground).fade_in(false).fixed_pos(pos).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).inner_margin(4).show(ui, |ui| {
                ui.set_min_width(220.0);
                for (i, item) in c.items.iter().enumerate() {
                    let label = match c.kind {
                        Context::Command(_) => format!("\\{}", item.replace("{}", "{…}")),
                        _ => item.clone(),
                    };
                    let sel = i == c.selected;
                    let mut job = egui::text::LayoutJob::default();
                    job.append(&label, 0.0, TextFormat::simple(FontId::monospace(13.0), if sel { pal.text } else { pal.dim }));
                    if let Context::Citation(_) = c.kind {
                        if let Some(info) = cite_info.get(&item.to_lowercase()) {
                            let short: String = info.chars().take(48).collect();
                            let dots = if short.len() < info.len() { "…" } else { "" };
                            job.append(&format!("{short}{dots}"), 10.0, TextFormat::simple(FontId::proportional(11.5), pal.dim));
                        }
                        if c.library.get(i).copied().unwrap_or(false) {
                            job.append("library", 10.0, TextFormat::simple(FontId::proportional(11.0), pal.accent2));
                        }
                    }
                    job.wrap = egui::text::TextWrapping { max_width: 560.0, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
                    let r = ui.selectable_label(sel, job);
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

/// Comments or uncomments lines. The `% ` goes at the smallest indentation
/// of the block, so indented code stays aligned (like Overleaf).
fn toggle_comment_block(block: &str) -> String {
    let filled = || block.split('\n').filter(|l| !l.trim().is_empty());
    let all_commented = filled().all(|l| l.trim_start().starts_with('%')) && filled().next().is_some();
    let indent = filled().map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    block
        .split('\n')
        .map(|l| {
            if l.trim().is_empty() {
                l.to_string()
            } else if all_commented {
                let ws = l.len() - l.trim_start().len();
                let rest = &l[ws..];
                let rest = rest.strip_prefix("% ").or_else(|| rest.strip_prefix('%')).unwrap_or(rest);
                format!("{}{}", &l[..ws], rest)
            } else {
                format!("{}% {}", &l[..indent], &l[indent..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Prefix of the usage statistics keys per completion kind.
fn usage_kind(k: &Context) -> &'static str {
    match k {
        Context::Command(_) => "cmd",
        Context::Environment(_) => "env",
        Context::Label(_) => "label",
        Context::Citation(_) => "cite",
        Context::File(_) => "file",
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

/// Byte offset of the `\end{env}` that closes an environment opened just
/// before `after`, counting nested `\begin{env}`.
fn matching_end(after: &str, env: &str) -> Option<usize> {
    let b = format!("\\begin{{{env}}}");
    let e = format!("\\end{{{env}}}");
    let mut depth = 0;
    let mut i = 0;
    loop {
        let nb = after[i..].find(&b).map(|p| i + p);
        let ne = after[i..].find(&e).map(|p| i + p)?;
        match nb {
            Some(pb) if pb < ne => {
                depth += 1;
                i = pb + b.len();
            }
            _ if depth == 0 => return Some(ne),
            _ => {
                depth -= 1;
                i = ne + e.len();
            }
        }
    }
}

/// The `\begin{name}` whose name holds the cursor (`a..b`), if it has a matching `\end`.
fn env_link_at(text: &str, (a, b): (usize, usize)) -> Option<EnvLink> {
    let ba = byte_of_char(text, a);
    let bb = byte_of_char(text, b);
    let ls = text[..ba].rfind('\n').map_or(0, |p| p + 1);
    let p = text[ls..ba].rfind("\\begin{").map(|p| ls + p)?;
    let ns = p + "\\begin{".len();
    let close = ns + text[ns..].find(['}', '\n'])?;
    if !text[close..].starts_with('}') || bb > close {
        return None;
    }
    let name = &text[ns..close];
    let end = close + 1 + matching_end(&text[close + 1..], name)? + "\\end{".len();
    let begin_name = text[..ns].chars().count();
    let end_name = begin_name + text[ns..end].chars().count();
    Some(EnvLink { begin_name, name: name.to_string(), end_name, len: text.chars().count() })
}

/// Arguments a fresh `\begin{env}` gets right away; empty ones are filled in
/// first (Tab moves on).
fn env_args(env: &str) -> &'static str {
    match env {
        "tabular" | "array" | "minipage" | "column" | "subfigure" | "multicols" => "{}",
        "tabularx" => "{\\linewidth}{}",
        "wrapfigure" => "{r}{}",
        _ => "",
    }
}

fn is_list(env: &str) -> bool {
    matches!(env, "itemize" | "enumerate" | "description")
}

/// `  \item text` (not `\itemsep`)
fn is_item_line(line: &str) -> bool {
    line.trim_start().strip_prefix("\\item").is_some_and(|r| !r.starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// The innermost environment still open at the end of `before`.
fn open_env(before: &str) -> Option<String> {
    let mut stack: Vec<&str> = Vec::new();
    for line in before.lines() {
        let code = lf_tex::outline::strip_comment(line);
        let mut i = 0;
        while let Some(p) = code[i..].find('\\') {
            let at = i + p;
            let rest = &code[at + 1..];
            let (begin, skip) = if rest.starts_with("begin{") {
                (true, 6)
            } else if rest.starts_with("end{") {
                (false, 4)
            } else {
                i = at + 1 + rest.chars().next().map_or(0, char::len_utf8);
                continue;
            };
            let ns = at + 1 + skip;
            let Some(close) = code[ns..].find('}') else { break };
            let name = &code[ns..ns + close];
            if begin {
                stack.push(name);
            } else if let Some(k) = stack.iter().rposition(|n| *n == name) {
                stack.truncate(k);
            }
            i = ns + close + 1;
        }
    }
    stack.last().map(|s| s.to_string())
}

/// Whether the text ends in an odd number of backslashes (`\{` is a literal brace).
fn ends_escaped(before: &str) -> bool {
    before.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// Whether the cursor is inside `$…$`: an odd number of unescaped `$` in the paragraph before it.
fn in_inline_math(before: &str) -> bool {
    let para = before.rfind("\n\n").map_or(before, |p| &before[p..]);
    let mut n = 0;
    let mut backslashes = 0;
    for ch in para.chars() {
        if ch == '$' && backslashes % 2 == 0 {
            n += 1;
        }
        backslashes = if ch == '\\' { backslashes + 1 } else { 0 };
    }
    n % 2 == 1
}

/// The pair typing `ch` should create: (opening text before the cursor for
/// Backspace, closer). `None` types `ch` alone.
fn pair_for(ch: char, before: &str, next: Option<char>) -> Option<(String, String)> {
    if next.is_some_and(|c| c.is_alphanumeric() || c == '\\') {
        return None;
    }
    if before.ends_with("\\left") {
        let close = match ch {
            '(' => ")",
            '[' => "]",
            '|' => "|",
            _ => return None,
        };
        return Some((ch.to_string(), format!("\\right{close}")));
    }
    if before.ends_with("\\left\\") && ch == '{' {
        return Some(("\\{".into(), "\\right\\}".into()));
    }
    if ends_escaped(before) {
        // `\(` and `\[` open math; `\{`, `\$` are literal characters
        return match ch {
            '(' => Some(("\\(".into(), "\\)".into())),
            '[' => Some(("\\[".into(), "\\]".into())),
            _ => None,
        };
    }
    let close = match ch {
        '(' => ")",
        '[' => "]",
        '{' => "}",
        '$' if !in_inline_math(before) => "$",
        _ => return None,
    };
    Some((ch.to_string(), close.to_string()))
}

/// `ARVIDLEAF_DEBUG_KEYS=1` prints the editor's key, text and IME events.
fn debug_keys() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("ARVIDLEAF_DEBUG_KEYS").is_some_and(|v| !v.is_empty() && v != "0"))
}

/// Keys the text widget uses to edit or move the cursor.
fn is_edit_key(k: Key) -> bool {
    matches!(
        k,
        Key::Enter | Key::Backspace | Key::Delete | Key::Tab | Key::Escape | Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::PageUp | Key::PageDown
    )
}

/// Char range `start..end` of the lines touched by the selection `a..b`
/// (without the final newline; a selection ending at a line start leaves that line out).
fn line_bounds(text: &str, a: usize, b: usize) -> (usize, usize) {
    let ba = byte_of_char(text, a);
    let mut bb = byte_of_char(text, b);
    if b > a && text[..bb].ends_with('\n') {
        bb -= 1;
    }
    let ls = text[..ba].rfind('\n').map_or(0, |p| p + 1);
    let le = text[bb..].find('\n').map_or(text.len(), |p| bb + p);
    (text[..ls].chars().count(), text[..le].chars().count())
}

/// The link under char index `c`: `\ref{x}` → label, `\cite{a,b}` → the key
/// under the cursor, `\input{f}` / `\includegraphics{f}` → file.
pub fn link_at(text: &str, c: usize) -> Option<Link> {
    let at = byte_of_char(text, c);
    let ls = text[..at].rfind('\n').map_or(0, |p| p + 1);
    let le = text[at..].find('\n').map_or(text.len(), |p| at + p);
    let line = lf_tex::outline::strip_comment(&text[ls..le]);
    let pos = at - ls;
    let mut i = 0;
    while let Some(p) = line[i..].find('\\') {
        let bs = i + p;
        let name: String = line[bs + 1..].chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        i = bs + 1;
        if name.is_empty() {
            continue;
        }
        // `\cite*[p. 3]{key}`
        let mut j = bs + 1 + name.len();
        let b = line.as_bytes();
        if b.get(j) == Some(&b'*') {
            j += 1;
        }
        while b.get(j) == Some(&b'[') {
            match line[j..].find(']') {
                Some(e) => j += e + 1,
                None => break,
            }
        }
        if b.get(j) != Some(&b'{') {
            continue;
        }
        let Some(close) = line[j..].find('}').map(|e| j + e) else { continue };
        if pos < bs || pos > close {
            continue;
        }
        let arg = &line[j + 1..close];
        let link = match name.as_str() {
            "ref" | "eqref" | "pageref" | "autoref" | "cref" | "Cref" | "nameref" | "vref" => Some(Link::Label(arg.trim().to_string())),
            n if n.contains("cite") => {
                // the key under the cursor, or the first one
                let mut start = j + 1;
                let mut key = None;
                for k in arg.split(',') {
                    if key.is_none() || (pos >= start && pos <= start + k.len()) {
                        key = Some(k.trim());
                    }
                    start += k.len() + 1;
                }
                key.filter(|k| !k.is_empty()).map(|k| Link::Citation(k.to_string()))
            }
            "input" | "include" | "subfile" | "includegraphics" | "lstinputlisting" | "addbibresource" | "bibliography" | "includepdf" => {
                arg.split(',').next().map(|f| Link::File(f.trim().to_string())).filter(|l| *l != Link::File(String::new()))
            }
            // e.g. `\textbf{see \ref{x}}`: look at the inner commands
            _ => None,
        };
        if link.is_some() {
            return link;
        }
    }
    None
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
        // an empty line has nothing to mark
        err_ranges.retain(|(s, e)| s < e);
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
        // split the token at error-line boundaries; every arm moves `pos` or `k` forward
        let mut pos = a;
        let mut k = er;
        while pos < b {
            match err_ranges.get(k) {
                Some(&(_, e)) if e <= pos => k += 1,
                Some(&(s, e)) if s <= pos => {
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

    /// An error on an empty line (its text was just deleted) used to loop
    /// forever and fill the memory until the program aborted.
    #[test]
    fn highlight_survives_errors_on_empty_lines() {
        let pal = crate::theme::dark();
        let font = FontId::monospace(13.0);
        for (src, lines) in [
            ("\\title{t}\n\n\\author{a}\n", &[2u32][..]),
            ("a\n\n\n$$\n", &[2, 3, 4]),
            ("x\n", &[2]),
            ("\n", &[1, 2]),
            ("", &[1]),
            ("\\begin{document}\n\n\\end{document}", &[2, 3, 9]),
        ] {
            let job = highlight(src, &font, &pal, lines);
            let covered: usize = job.sections.iter().map(|s| usize::from(s.byte_range.end) - usize::from(s.byte_range.start)).sum();
            assert_eq!(covered, src.len(), "{src:?}");
            assert!(job.sections.len() <= src.len() + 1, "{src:?}: {} sections", job.sections.len());
        }
    }

    #[test]
    fn comment_toggle_keeps_indentation() {
        let b = "  \\item a\n\n    \\item b";
        let c = toggle_comment_block(b);
        assert_eq!(c, "  % \\item a\n\n  %   \\item b");
        assert_eq!(toggle_comment_block(&c), b);
        assert_eq!(toggle_comment_block("x"), "% x");
        assert_eq!(toggle_comment_block("%x"), "x");
        assert_eq!(toggle_comment_block(""), "");
    }

    #[test]
    fn detects_external_change() {
        let dir = std::env::temp_dir().join(format!("arvidleaf-ext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.tex");
        std::fs::write(&f, "one").unwrap();
        let mut d = EditorDoc::new("a.tex".into(), f.clone(), "one".into(), 1);
        assert!(!d.changed_on_disk());
        d.text.push_str(" mine");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&f, "theirs").unwrap();
        assert!(d.changed_on_disk(), "external write must be noticed before saving");
        d.save().unwrap();
        assert!(!d.changed_on_disk());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runs one frame of the editor with the given input events.
    fn frame(ctx: &egui::Context, d: &mut EditorDoc, usage: &lf_core::usage::Usage, events: Vec<egui::Event>) {
        let pal = crate::theme::dark();
        // like the real backend: held modifiers are announced before the key
        let modifiers = events.iter().find_map(|e| if let egui::Event::Key { modifiers, .. } = e { Some(*modifiers) } else { None }).unwrap_or_default();
        let events = std::iter::once(egui::Event::ModifiersChanged(modifiers)).chain(events).collect();
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let info = std::collections::HashMap::new();
            let env = EditorEnv { problems: Vec::new(), labels: &[], bib_keys: &[], lib_keys: &[], cite_info: &info, files: &[], font_size: 13.0, vim: false, usage };
            d.ui(ui, &pal, &env);
        });
        out.textures_delta.clear();
    }

    /// A focused editor with the selection `a..b` (char indices).
    fn editor(text: &str, a: usize, b: usize) -> (egui::Context, EditorDoc) {
        let ctx = egui::Context::default();
        let mut d = EditorDoc::new("main.tex".into(), "/nonexistent/main.tex".into(), text.into(), 7);
        let usage = lf_core::usage::Usage::default();
        frame(&ctx, &mut d, &usage, Vec::new());
        ctx.memory_mut(|m| m.request_focus(d.id()));
        d.set_cursor(&ctx, CCursorRange::two(CCursor::new(a), CCursor::new(b)));
        frame(&ctx, &mut d, &usage, Vec::new());
        (ctx, d)
    }

    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn dollar_wraps_selection_in_math() {
        let (ctx, mut d) = editor("a x+y b", 2, 5);
        frame(&ctx, &mut d, &Default::default(), vec![egui::Event::Text("$".into())]);
        assert_eq!(d.text, "a $x+y$ b");
        assert_eq!(d.cursor(&ctx).map(sorted), Some((7, 7)));
        // outside math `$` opens a pair
        frame(&ctx, &mut d, &Default::default(), vec![egui::Event::Text("$".into())]);
        assert_eq!(d.text, "a $x+y$$$ b");
    }

    #[test]
    fn brace_and_ctrl_b_wrap_selection() {
        let (ctx, mut d) = editor("word next", 0, 4);
        frame(&ctx, &mut d, &Default::default(), vec![egui::Event::Text("{".into())]);
        assert_eq!(d.text, "{word} next", "a selection followed by text is wrapped too");
        let (ctx, mut d) = editor("bold", 0, 4);
        frame(&ctx, &mut d, &Default::default(), vec![key(Key::B, egui::Modifiers::COMMAND)]);
        assert_eq!(d.text, "\\textbf{bold}");
        let (ctx, mut d) = editor("x ", 2, 2);
        frame(&ctx, &mut d, &Default::default(), vec![key(Key::I, egui::Modifiers::COMMAND)]);
        assert_eq!(d.text, "x \\textit{}");
        assert_eq!(d.cursor(&ctx).map(sorted), Some((10, 10)), "cursor inside the braces");
    }

    #[test]
    fn begin_enumerate_asks_for_enumitem() {
        let src = "\\documentclass{article}\n\\begin{document}\n\\begin{enumerate}\n\\end{document}";
        let end = src.find("{enumerate}").unwrap() + "{enumerate}".len();
        let (ctx, mut d) = editor(src, end, end);
        frame(&ctx, &mut d, &Default::default(), vec![key(Key::Enter, egui::Modifiers::NONE)]);
        assert!(d.text.contains("\\begin{enumerate}\n  \\item \n\\end{enumerate}"), "{}", d.text);
        assert_eq!(d.needs_packages, vec!["enumitem".to_string()]);
    }

    #[test]
    fn suggestions_follow_the_users_commands() {
        let src = "$\\vect{a} \\vect{b}$ ";
        let n = src.chars().count();
        let (ctx, mut d) = editor(src, n, n);
        // one letter is enough for the popup
        frame(&ctx, &mut d, &Default::default(), vec![egui::Event::Text("\\".into())]);
        frame(&ctx, &mut d, &Default::default(), vec![egui::Event::Text("v".into())]);
        let c = d.completion.as_ref().expect("popup after one letter");
        assert_eq!(c.items[0], "vect{}", "the document's own command first: {:?}", c.items);
        // earlier picks rank above the alphabet
        let mut usage = lf_core::usage::Usage::default();
        for _ in 0..3 {
            usage.record("cmd:varphi");
        }
        frame(&ctx, &mut d, &usage, vec![egui::Event::Text("a".into())]);
        assert_eq!(d.completion.as_ref().unwrap().items[0], "varphi");
        frame(&ctx, &mut d, &usage, vec![key(Key::Tab, egui::Modifiers::NONE)]);
        assert!(d.text.ends_with("\\varphi"), "{}", d.text);
        assert_eq!(d.picked, vec!["cmd:varphi".to_string()]);
    }

    fn text(t: &str) -> egui::Event {
        egui::Event::Text(t.into())
    }

    /// Types `s` one character per frame, like a person.
    fn type_str(ctx: &egui::Context, d: &mut EditorDoc, s: &str) {
        for ch in s.chars() {
            frame(ctx, d, &Default::default(), vec![text(&ch.to_string())]);
        }
    }

    fn press(ctx: &egui::Context, d: &mut EditorDoc, k: Key, m: egui::Modifiers) {
        frame(ctx, d, &Default::default(), vec![key(k, m)]);
    }

    fn cur(ctx: &egui::Context, d: &EditorDoc) -> (usize, usize) {
        d.cursor(ctx).map(sorted).unwrap()
    }

    #[test]
    fn brackets_and_dollars_come_in_pairs() {
        let (ctx, mut d) = editor("", 0, 0);
        type_str(&ctx, &mut d, "(x)");
        assert_eq!(d.text, "(x)", "the typed closer steps over the inserted one");
        assert_eq!(cur(&ctx, &d), (3, 3));
        type_str(&ctx, &mut d, " {[");
        assert_eq!(d.text, "(x) {[]}");
        type_str(&ctx, &mut d, "]}");
        assert_eq!(d.text, "(x) {[]}");
        assert_eq!(cur(&ctx, &d), (8, 8));

        let (ctx, mut d) = editor("a ", 2, 2);
        type_str(&ctx, &mut d, "$x$");
        assert_eq!(d.text, "a $x$");
        // inside math a `$` closes instead of pairing
        let (ctx, mut d) = editor("a $x ", 5, 5);
        type_str(&ctx, &mut d, "$");
        assert_eq!(d.text, "a $x $");
        // no pair right before a word, and `\{` / `\$` stay single
        let (ctx, mut d) = editor("word", 0, 0);
        type_str(&ctx, &mut d, "(");
        assert_eq!(d.text, "(word");
        let (ctx, mut d) = editor("", 0, 0);
        type_str(&ctx, &mut d, "\\{ \\$");
        assert_eq!(d.text, "\\{ \\$");
    }

    /// Real keyboards send the key before its text, with the held modifiers.
    #[test]
    fn brackets_pair_with_real_key_sequences() {
        let shift = egui::Modifiers::SHIFT;
        let altgr = egui::Modifiers { alt: true, ctrl: true, command: true, ..Default::default() };
        let (ctx, mut d) = editor("", 0, 0);
        // German layout: Shift+8 = `(`, AltGr+7 = `{` (on some systems reported as Ctrl+Alt)
        frame(&ctx, &mut d, &Default::default(), vec![key(Key::OpenBracket, shift), text("(")]);
        assert_eq!(d.text, "()");
        frame(&ctx, &mut d, &Default::default(), vec![key(Key::OpenCurlyBracket, altgr), text("{")]);
        assert_eq!(d.text, "({})");
        assert_eq!(cur(&ctx, &d), (2, 2));
    }

    #[test]
    fn brackets_pair_with_an_input_method() {
        let commit = |t: &str| egui::Event::Ime(egui::ImeEvent::Commit(t.into()));
        let (ctx, mut d) = editor("", 0, 0);
        frame(&ctx, &mut d, &Default::default(), vec![commit("[")]);
        assert_eq!(d.text, "[]");
        frame(&ctx, &mut d, &Default::default(), vec![commit("x"), commit("]")]);
        assert_eq!(d.text, "[x]");
        assert_eq!(cur(&ctx, &d), (3, 3));
    }

    /// KDE Wayland: winit sends an empty preedit after each cursor move, often
    /// in the same frame as the next key.
    #[test]
    fn empty_preedit_does_not_disable_shortcuts() {
        let clear = || egui::Event::Ime(egui::ImeEvent::Preedit { text: String::new(), active_range_chars: None });
        let (ctx, mut d) = editor("", 0, 0);
        frame(&ctx, &mut d, &Default::default(), vec![clear(), key(Key::OpenBracket, egui::Modifiers::SHIFT), text("(")]);
        assert_eq!(d.text, "()");
        let (ctx, mut d) = editor("$x$ ", 4, 4);
        frame(&ctx, &mut d, &Default::default(), vec![clear(), text("\\")]);
        frame(&ctx, &mut d, &Default::default(), vec![clear(), text("v")]);
        assert!(d.completion.is_some(), "popup after one letter");
        frame(&ctx, &mut d, &Default::default(), vec![clear(), key(Key::Tab, egui::Modifiers::NONE)]);
        assert!(d.text.len() > "$x$ \\v".len(), "Tab accepts the suggestion: {}", d.text);
        assert!(d.completion.is_none());
    }

    /// The key sequence logged on KDE Wayland with a German layout
    /// (`ARVIDLEAF_DEBUG_KEYS=1`): `(`, `\ve`, two Backspaces, `be`, Enter.
    #[test]
    fn replay_logged_german_keys() {
        let n = egui::Modifiers::NONE;
        let sh = egui::Modifiers::SHIFT;
        let up = |k: Key, m| egui::Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: m };
        let (ctx, mut d) = editor("", 0, 0);
        let frames: Vec<Vec<egui::Event>> = vec![
            vec![key(Key::Num8, sh), text("(")],
            vec![up(Key::Num8, sh)],
            vec![key(Key::Backslash, n), text("\\")],
            vec![key(Key::V, n), text("v")],
            vec![key(Key::E, n), text("e")],
            vec![key(Key::Backspace, n)],
            vec![key(Key::Backspace, n)],
            vec![key(Key::B, n), text("b")],
            vec![key(Key::E, n), text("e")],
        ];
        for f in frames {
            frame(&ctx, &mut d, &Default::default(), f);
        }
        assert_eq!(d.text, "(\\be)");
        let c = d.completion.as_ref().expect("suggestions for \\be");
        let first = c.items[0].split('{').next().unwrap().to_string();
        frame(&ctx, &mut d, &Default::default(), vec![key(Key::Enter, n)]);
        assert!(d.text.starts_with(&format!("(\\{first}")), "Enter takes the suggestion: {:?}", d.text);
        assert!(d.completion.is_none());
    }

    #[test]
    fn hand_typed_commands_count_once() {
        let (ctx, mut d) = editor("", 0, 0);
        type_str(&ctx, &mut d, "$\\leqslant x \\myop{a} \\\\ ");
        assert_eq!(d.picked, vec!["cmd:leqslant".to_string(), "cmd:myop{}".to_string()]);
        // a completed command is counted as the pick only
        let (ctx, mut d) = editor("$x ", 3, 3);
        type_str(&ctx, &mut d, "\\alph");
        frame(&ctx, &mut d, &Default::default(), vec![key(Key::Tab, egui::Modifiers::NONE)]);
        type_str(&ctx, &mut d, " ");
        assert_eq!(d.picked, vec!["cmd:alpha".to_string()]);
    }

    #[test]
    fn math_delimiters_and_left_right() {
        let (ctx, mut d) = editor("", 0, 0);
        type_str(&ctx, &mut d, "\\(");
        assert_eq!(d.text, "\\(\\)");
        assert_eq!(cur(&ctx, &d), (2, 2));
        let (ctx, mut d) = editor("", 0, 0);
        type_str(&ctx, &mut d, "\\[");
        assert_eq!(d.text, "\\[\\]");
        let (ctx, mut d) = editor("$\\left $", 6, 6);
        type_str(&ctx, &mut d, "(");
        assert_eq!(d.text, "$\\left(\\right) $");
        assert_eq!(cur(&ctx, &d), (7, 7));
        // `\\[2pt]` after a line break is an ordinary bracket pair
        let (ctx, mut d) = editor("a \\\\", 4, 4);
        type_str(&ctx, &mut d, "[");
        assert_eq!(d.text, "a \\\\[]");
    }

    #[test]
    fn backspace_removes_an_empty_pair() {
        let (ctx, mut d) = editor("x ", 2, 2);
        type_str(&ctx, &mut d, "(");
        press(&ctx, &mut d, Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(d.text, "x ");
        type_str(&ctx, &mut d, "\\(");
        press(&ctx, &mut d, Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(d.text, "x ");
        // a pair the user typed in full is not ours: Backspace deletes one character
        let (ctx, mut d) = editor("()", 1, 1);
        press(&ctx, &mut d, Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(d.text, ")");
    }

    #[test]
    fn formatting_shortcuts() {
        let shift = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        for (k, m, want) in [
            (Key::E, egui::Modifiers::COMMAND, "a \\emph{word}"),
            (Key::M, egui::Modifiers::COMMAND, "a \\(word\\)"),
            (Key::M, shift, "a \\[word\\]"),
            (Key::B, egui::Modifiers::COMMAND, "a \\textbf{word}"),
        ] {
            let (ctx, mut d) = editor("a word", 2, 6);
            press(&ctx, &mut d, k, m);
            assert_eq!(d.text, want);
        }
        // without a selection: cursor inside, Tab jumps behind the pair
        let (ctx, mut d) = editor("", 0, 0);
        press(&ctx, &mut d, Key::E, egui::Modifiers::COMMAND);
        type_str(&ctx, &mut d, "x");
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::NONE);
        assert_eq!(d.text, "\\emph{x}");
        assert_eq!(cur(&ctx, &d), (8, 8));
    }

    #[test]
    fn wrap_in_environment_and_rename_end() {
        let (ctx, mut d) = editor("a\n\nb\nc", 0, 6);
        press(&ctx, &mut d, Key::E, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
        assert_eq!(d.text, "\\begin{}\n  a\n\n  b\n  c\n\\end{}");
        assert_eq!(cur(&ctx, &d), (7, 7));
        type_str(&ctx, &mut d, "zz");
        assert_eq!(d.text, "\\begin{zz}\n  a\n\n  b\n  c\n\\end{zz}", "the \\end follows the typed name");
        // Tab: from the name into the body
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::NONE);
        assert_eq!(cur(&ctx, &d), (13, 13));

        // picking a suggestion renames the \end too, without a second body
        let (ctx, mut d) = editor("\\begin{}\n  x\n\\end{}", 7, 7);
        type_str(&ctx, &mut d, "fi");
        assert!(d.completion.is_some());
        press(&ctx, &mut d, Key::Enter, egui::Modifiers::NONE);
        assert_eq!(d.text, "\\begin{figure}\n  x\n\\end{figure}");

        // renaming an existing environment, with a nested one of the same name
        let src = "\\begin{itemize}\n\\begin{itemize}\\end{itemize}\n\\end{itemize}\nx";
        let (ctx, mut d) = editor(src, 7, 14);
        type_str(&ctx, &mut d, "enumerate");
        assert_eq!(d.text, "\\begin{enumerate}\n\\begin{itemize}\\end{itemize}\n\\end{enumerate}\nx");
    }

    #[test]
    fn enter_continues_and_ends_lists() {
        let src = "\\begin{itemize}\n  \\item a\n\\end{itemize}";
        let p = src.find("a\n").unwrap() + 1;
        let (ctx, mut d) = editor(src, p, p);
        press(&ctx, &mut d, Key::Enter, egui::Modifiers::NONE);
        assert_eq!(d.text, "\\begin{itemize}\n  \\item a\n  \\item \n\\end{itemize}");
        assert_eq!(cur(&ctx, &d), (p + 9, p + 9));
        // Enter on the empty item: it goes, the cursor continues after the list
        press(&ctx, &mut d, Key::Enter, egui::Modifiers::NONE);
        assert_eq!(d.text, "\\begin{itemize}\n  \\item a\n\\end{itemize}\n");
        let n = d.text.chars().count();
        assert_eq!(cur(&ctx, &d), (n, n));
        // outside a list `\item` lines are not continued
        let (ctx, mut d) = editor("\\item x", 7, 7);
        press(&ctx, &mut d, Key::Enter, egui::Modifiers::NONE);
        assert_eq!(d.text, "\\item x\n");
    }

    #[test]
    fn line_shortcuts() {
        let (ctx, mut d) = editor("a\nb", 0, 0);
        press(&ctx, &mut d, Key::D, egui::Modifiers::COMMAND);
        assert_eq!(d.text, "a\na\nb");
        assert_eq!(cur(&ctx, &d), (2, 2));
        press(&ctx, &mut d, Key::ArrowDown, egui::Modifiers::ALT);
        assert_eq!(d.text, "a\nb\na");
        assert_eq!(cur(&ctx, &d), (4, 4));
        press(&ctx, &mut d, Key::ArrowUp, egui::Modifiers::ALT);
        press(&ctx, &mut d, Key::ArrowUp, egui::Modifiers::ALT);
        assert_eq!(d.text, "a\na\nb");
        assert_eq!(cur(&ctx, &d), (0, 0));
        press(&ctx, &mut d, Key::ArrowUp, egui::Modifiers::ALT);
        assert_eq!(d.text, "a\na\nb", "first line stays");

        let (ctx, mut d) = editor("one\ntwo\nthree", 1, 1);
        press(&ctx, &mut d, Key::L, egui::Modifiers::COMMAND);
        assert_eq!(cur(&ctx, &d), (0, 4));
        press(&ctx, &mut d, Key::L, egui::Modifiers::COMMAND);
        assert_eq!(cur(&ctx, &d), (0, 8));

        let (ctx, mut d) = editor("a\n\nb", 0, 4);
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::NONE);
        assert_eq!(d.text, "  a\n\n  b");
        assert_eq!(cur(&ctx, &d), (0, 8));
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::SHIFT);
        assert_eq!(d.text, "a\n\nb");
        // Shift+Tab without a selection outdents the current line
        let (ctx, mut d) = editor("    x", 5, 5);
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::SHIFT);
        assert_eq!(d.text, "  x");
        assert_eq!(cur(&ctx, &d), (3, 3));
    }

    #[test]
    fn tab_moves_through_template_arguments() {
        let (ctx, mut d) = editor("$ $", 1, 1);
        type_str(&ctx, &mut d, "\\fra");
        assert_eq!(d.completion.as_ref().map(|c| c.items[0].as_str()), Some("frac{}{}"));
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::NONE);
        assert_eq!(d.text, "$\\frac{}{} $");
        type_str(&ctx, &mut d, "1");
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::NONE);
        type_str(&ctx, &mut d, "2");
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::NONE);
        assert_eq!(d.text, "$\\frac{1}{2} $");
        assert_eq!(cur(&ctx, &d), (12, 12));
        // tabular: column spec first, then the body
        let (ctx, mut d) = editor("", 0, 0);
        type_str(&ctx, &mut d, "\\begin{tabula");
        press(&ctx, &mut d, Key::Enter, egui::Modifiers::NONE);
        assert_eq!(d.text, "\\begin{tabular}{}\n  \n\\end{tabular}");
        assert_eq!(cur(&ctx, &d), (16, 16));
        type_str(&ctx, &mut d, "lr");
        press(&ctx, &mut d, Key::Tab, egui::Modifiers::NONE);
        assert_eq!(cur(&ctx, &d), (22, 22));
    }

    #[test]
    fn links_under_the_cursor() {
        let t = "see \\ref{fig:a} and \\cite[p.~3]{knuth, lamport} % \\ref{no}\n\\input{chapters/one}";
        assert_eq!(link_at(t, 10), Some(Link::Label("fig:a".into())));
        assert_eq!(link_at(t, 5), Some(Link::Label("fig:a".into())), "on the command name");
        assert_eq!(link_at(t, 2), None);
        let k = t.find("lamport").unwrap() + 2;
        assert_eq!(link_at(t, k), Some(Link::Citation("lamport".into())));
        assert_eq!(link_at(t, t.find("\\cite").unwrap() + 1), Some(Link::Citation("knuth".into())));
        assert_eq!(link_at(t, t.find("{no}").unwrap() + 1), None, "comments do not link");
        let n = t.chars().count() - 2;
        assert_eq!(link_at(t, n), Some(Link::File("chapters/one".into())));
        assert_eq!(link_at("\\textbf{see \\ref{x}}", 14), Some(Link::Label("x".into())));
    }

    #[test]
    fn environment_helpers() {
        assert_eq!(matching_end("\\begin{a}\\end{a} x \\end{a}", "a"), Some(19));
        assert_eq!(matching_end("\\end{b}", "a"), None);
        assert_eq!(open_env("\\begin{itemize}\n\\begin{equation}\\end{equation}\n"), Some("itemize".into()));
        assert_eq!(open_env("\\begin{itemize}\\end{itemize}"), None);
        assert_eq!(open_env("% \\begin{itemize}"), None);
        assert!(in_inline_math("a $x"));
        assert!(!in_inline_math("a $x$ \\$ b"));
        assert!(!in_inline_math("$x\n\nb"));
    }

    #[test]
    fn insert_at_keeps_cursor_on_text() {
        let (ctx, mut d) = editor("ab\ncd", 4, 4);
        d.insert_at(&ctx, 1, "XY");
        assert_eq!(d.text, "aXYb\ncd");
        assert_eq!(d.cursor(&ctx).map(sorted), Some((6, 6)));
    }

    #[test]
    fn citations_are_inserted_or_extended() {
        let keys = |k: &[&str]| k.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // after a word: `~\cite{…}`, cursor behind it
        let (ctx, mut d) = editor("Transformers changed", 12, 12);
        assert!(d.insert_citation(&ctx, "parencite", &keys(&["vaswani2017"])));
        assert_eq!(d.text, "Transformers~\\parencite{vaswani2017} changed");
        assert_eq!(d.cursor(&ctx).map(sorted), Some((36, 36)));
        // \textcite starts a phrase, no tie
        let (ctx, mut d) = editor("As ", 3, 3);
        d.insert_citation(&ctx, "textcite", &keys(&["knuth"]));
        assert_eq!(d.text, "As \\textcite{knuth}");
        // inside an existing citation: only new keys are added
        let (ctx, mut d) = editor("see \\cite{a, b} here", 12, 12);
        d.insert_citation(&ctx, "cite", &keys(&["B", "c"]));
        assert_eq!(d.text, "see \\cite{a, b,c} here");
        // Markdown
        let (ctx, mut d) = editor("x", 1, 1);
        d.rel = "notes.md".into();
        d.insert_citation(&ctx, "cite", &keys(&["a", "b"]));
        assert_eq!(d.text, "x[@a; @b]");
        let (ctx, mut d) = editor("as [@a] says", 6, 6);
        d.rel = "notes.md".into();
        d.insert_citation(&ctx, "cite", &keys(&["z"]));
        assert_eq!(d.text, "as [@a; @z] says");
    }

    #[test]
    fn find_replace() {
        assert_eq!(match_positions("Ab ab AB", "ab", false), vec![0, 3, 6]);
        assert_eq!(replace_matches("a.a.a", "a", "b", true, None), "b.b.b");
        assert_eq!(replace_matches("a.a.a", "a", "b", true, Some(2)), "a.b.a");
        assert_eq!(char_index_of_line("ä\nb\nc", 3), 4);
    }
}
