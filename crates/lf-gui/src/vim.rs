//! Vim emulation for the editor: normal/insert/visual modes, counts, motions,
//! operators with text objects (including LaTeX `i$`/`ie`), registers, undo,
//! dot-repeat, search and a few ex commands (`:w`, `:<line>`, `:s`).
//!
//! Works on char indices of the document text and knows nothing about egui:
//! the editor feeds keys and shows the cursor/selection it reports.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VKey {
    Char(char),
    Esc,
    Enter,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// Ctrl + letter
    Ctrl(char),
}

#[derive(Default, Debug)]
pub struct Response {
    /// The key was used by Vim (the text widget must not see it).
    pub consumed: bool,
    pub changed: bool,
    /// `:w` – save
    pub save: bool,
    /// Yanked text for the system clipboard.
    pub copy: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Exclusive,
    Inclusive,
    Linewise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordFwd(bool),
    WordBack(bool),
    WordEnd(bool),
    LineStart,
    FirstNonBlank,
    LineEnd,
    LastLine,
    FirstLine,
    Find(char, char),
    RepeatFind(bool),
    Match,
    ParaFwd,
    ParaBack,
    SearchNext(bool),
    NextLine,
    PrevLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Motion(Option<usize>, Motion),
    Line,
    Object(bool, char),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cmd {
    Move(Motion),
    Op(char, Target),
    Act(char),
    Replace(char),
    VisualObject(bool, char),
    Redo,
    HalfPage(bool),
    Nop,
}

enum Parse {
    More,
    Bad,
    Done(Option<usize>, Cmd),
}

const UNDO_LIMIT: usize = 100;
const HALF_PAGE: usize = 15;

pub struct Vim {
    pub mode: Mode,
    pub cursor: usize,
    /// Start of the visual selection.
    pub anchor: usize,
    pending: Vec<VKey>,
    /// `:` or `/`/`?` command line being typed.
    cmdline: Option<(char, String)>,
    register: String,
    register_lines: bool,
    undo: Vec<(String, usize)>,
    redo: Vec<(String, usize)>,
    search: Option<(String, bool)>,
    last_find: Option<(char, char)>,
    /// Column kept when moving up/down (`usize::MAX` after `$`).
    want_col: Option<usize>,
    /// The current command had a count (`5G` vs `G`).
    count_given: bool,
    pub message: String,
    last_change: Vec<VKey>,
    recording: Option<Vec<VKey>>,
    replaying: bool,
    /// Insert-mode keys are applied by Vim itself, not by the text widget
    /// (dot-repeat, or `ifoo<Esc>` arriving within one frame).
    pub self_insert: bool,
}

impl Default for Vim {
    fn default() -> Self {
        Vim {
            mode: Mode::Normal,
            cursor: 0,
            anchor: 0,
            pending: Vec::new(),
            cmdline: None,
            register: String::new(),
            register_lines: false,
            undo: Vec::new(),
            redo: Vec::new(),
            search: None,
            last_find: None,
            want_col: None,
            count_given: false,
            message: String::new(),
            last_change: Vec::new(),
            recording: None,
            replaying: false,
            self_insert: true,
        }
    }
}

// ---------------------------------------------------------------- text helpers

fn ls(b: &[char], i: usize) -> usize {
    let mut i = i.min(b.len());
    while i > 0 && b[i - 1] != '\n' {
        i -= 1;
    }
    i
}

fn le(b: &[char], i: usize) -> usize {
    let mut i = i.min(b.len());
    while i < b.len() && b[i] != '\n' {
        i += 1;
    }
    i
}

fn blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn fnb(b: &[char], i: usize) -> usize {
    let mut p = ls(b, i);
    let e = le(b, i);
    while p < e && blank(b[p]) {
        p += 1;
    }
    p
}

fn line_of(b: &[char], i: usize) -> usize {
    b[..i.min(b.len())].iter().filter(|c| **c == '\n').count()
}

fn line_start(b: &[char], n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut seen = 0;
    for (i, c) in b.iter().enumerate() {
        if *c == '\n' {
            seen += 1;
            if seen == n {
                return i + 1;
            }
        }
    }
    ls(b, b.len())
}

fn class(c: char, big: bool) -> u8 {
    if c.is_whitespace() {
        0
    } else if big || c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

fn escaped(b: &[char], i: usize) -> bool {
    i > 0 && b[i - 1] == '\\' && !(i > 1 && b[i - 2] == '\\')
}

/// Normal mode never rests on a line break (except on an empty line).
fn clamp(b: &[char], i: usize) -> usize {
    let i = i.min(b.len());
    if i > ls(b, i) && (i == b.len() || b[i] == '\n') { i - 1 } else { i }
}

fn word_fwd(b: &[char], mut i: usize, big: bool) -> usize {
    if i >= b.len() {
        return b.len();
    }
    let c0 = class(b[i], big);
    if c0 != 0 {
        while i < b.len() && class(b[i], big) == c0 {
            i += 1;
        }
    }
    while i < b.len() {
        if b[i] == '\n' {
            // an empty line counts as a word
            if i + 1 < b.len() && b[i + 1] == '\n' {
                return i + 1;
            }
            i += 1;
        } else if blank(b[i]) {
            i += 1;
        } else {
            break;
        }
    }
    i
}

fn word_end(b: &[char], mut i: usize, big: bool) -> usize {
    i += 1;
    while i < b.len() && b[i].is_whitespace() {
        i += 1;
    }
    if i >= b.len() {
        return b.len().saturating_sub(1);
    }
    let c = class(b[i], big);
    while i + 1 < b.len() && class(b[i + 1], big) == c {
        i += 1;
    }
    i
}

fn word_back(b: &[char], mut i: usize, big: bool) -> usize {
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && b[i].is_whitespace() {
        if b[i] == '\n' && b[i - 1] == '\n' {
            return i;
        }
        i -= 1;
    }
    let c = class(b[i], big);
    while i > 0 && class(b[i - 1], big) == c && c != 0 {
        i -= 1;
    }
    i
}

fn starts_with(b: &[char], i: usize, s: &str) -> bool {
    s.chars().enumerate().all(|(k, c)| b.get(i + k) == Some(&c))
}

/// Matching bracket for the bracket at `i`.
fn match_bracket(b: &[char], i: usize) -> Option<usize> {
    let (open, close, fwd) = match b[i] {
        '(' => ('(', ')', true),
        '[' => ('[', ']', true),
        '{' => ('{', '}', true),
        ')' => ('(', ')', false),
        ']' => ('[', ']', false),
        '}' => ('{', '}', false),
        _ => return None,
    };
    let mut depth = 0i32;
    if fwd {
        for k in i..b.len() {
            if escaped(b, k) {
                continue;
            }
            if b[k] == open {
                depth += 1;
            } else if b[k] == close {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
        }
    } else {
        for k in (0..=i).rev() {
            if escaped(b, k) {
                continue;
            }
            if b[k] == close {
                depth += 1;
            } else if b[k] == open {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
        }
    }
    None
}

/// Innermost `open … close` pair around `i`.
fn enclosing(b: &[char], i: usize, open: char, close: char) -> Option<(usize, usize)> {
    let mut depth = 0;
    let mut s = None;
    let start = if b.get(i) == Some(&close) && !escaped(b, i) { i.checked_sub(1)? } else { i.min(b.len().saturating_sub(1)) };
    if b.get(i) == Some(&open) && !escaped(b, i) {
        s = Some(i);
    } else {
        for k in (0..=start).rev() {
            if escaped(b, k) {
                continue;
            }
            if b[k] == close {
                depth += 1;
            } else if b[k] == open {
                if depth == 0 {
                    s = Some(k);
                    break;
                }
                depth -= 1;
            }
        }
    }
    let s = s?;
    let e = match_bracket(b, s)?;
    Some((s, e))
}

/// `\begin{x} … \end{x}` around `i`: (begin start, after begin's `}`, end start, after end's `}`).
fn environment(b: &[char], i: usize) -> Option<(usize, usize, usize, usize)> {
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut best = None;
    let mut k = 0;
    while k < b.len() {
        let is_begin = starts_with(b, k, "\\begin{");
        if (is_begin || starts_with(b, k, "\\end{")) && !escaped(b, k) {
            let brace = k + if is_begin { 6 } else { 4 };
            let Some(close) = match_bracket(b, brace) else {
                k += 1;
                continue;
            };
            if is_begin {
                stack.push((k, close + 1));
            } else if let Some((bs, be)) = stack.pop()
                && bs <= i && i < close + 1 {
                    // the first closed pair around i is the innermost
                    best = Some((bs, be, k, close + 1));
                    break;
                }
            k = close + 1;
            continue;
        }
        k += 1;
    }
    best
}

fn text_object(b: &[char], c: usize, inner: bool, ch: char) -> Option<(usize, usize)> {
    match ch {
        'w' | 'W' => {
            let big = ch == 'W';
            if c >= b.len() || b[c] == '\n' {
                return None;
            }
            let (l0, l1) = (ls(b, c), le(b, c));
            let cl = class(b[c], big);
            let (mut s, mut e) = (c, c + 1);
            while s > l0 && class(b[s - 1], big) == cl {
                s -= 1;
            }
            while e < l1 && class(b[e], big) == cl {
                e += 1;
            }
            if !inner {
                let mut e2 = e;
                while e2 < l1 && blank(b[e2]) {
                    e2 += 1;
                }
                if e2 > e {
                    e = e2;
                } else {
                    while s > l0 && blank(b[s - 1]) {
                        s -= 1;
                    }
                }
            }
            Some((s, e))
        }
        '(' | ')' | 'b' | '[' | ']' | '{' | '}' | 'B' | '<' | '>' => {
            let (o, cl) = match ch {
                '(' | ')' | 'b' => ('(', ')'),
                '[' | ']' => ('[', ']'),
                '<' | '>' => ('<', '>'),
                _ => ('{', '}'),
            };
            let (s, e) = enclosing(b, c, o, cl)?;
            if inner { Some((s + 1, e)) } else { Some((s, e + 1)) }
        }
        '"' | '\'' | '`' | '$' => {
            let (l0, l1) = (ls(b, c), le(b, c));
            let qs: Vec<usize> = (l0..l1).filter(|&k| b[k] == ch && !escaped(b, k)).collect();
            let pair = qs.chunks(2).filter(|p| p.len() == 2).find(|p| c <= p[1]).map(|p| (p[0], p[1]))?;
            if inner { Some((pair.0 + 1, pair.1)) } else { Some((pair.0, pair.1 + 1)) }
        }
        'e' => {
            let (bs, be, es, ee) = environment(b, c)?;
            if inner {
                let s = if b.get(be) == Some(&'\n') { be + 1 } else { be };
                // `\end` alone on its line: keep the line break before it
                let l = ls(b, es);
                let e = if b[l..es].iter().all(|c| blank(*c)) && l > s { l } else { es };
                Some((s, e.max(s)))
            } else {
                Some((bs, ee))
            }
        }
        _ => None,
    }
}

fn smart_eq(a: char, b: char, fold: bool) -> bool {
    if fold { a.to_lowercase().eq(b.to_lowercase()) } else { a == b }
}

/// Literal search with smartcase, wrapping around the end of the text.
fn find(b: &[char], pat: &[char], from: usize, forward: bool) -> Option<usize> {
    if pat.is_empty() || pat.len() > b.len() {
        return None;
    }
    let fold = !pat.iter().any(|c| c.is_uppercase());
    let n = b.len() - pat.len() + 1;
    let at = |k: usize| pat.iter().enumerate().all(|(j, p)| smart_eq(b[k + j], *p, fold));
    if forward {
        (from + 1..n).chain(0..(from + 1).min(n)).find(|&k| at(k))
    } else {
        (0..from.min(n)).rev().chain((from.min(n)..n).rev()).find(|&k| at(k))
    }
}

fn parse_count(keys: &[VKey], i: &mut usize) -> Option<usize> {
    let mut count: Option<usize> = None;
    while let Some(VKey::Char(c)) = keys.get(*i) {
        match c.to_digit(10) {
            Some(d) if d > 0 || count.is_some() => {
                count = Some(count.unwrap_or(0).saturating_mul(10).saturating_add(d as usize).min(100_000));
                *i += 1;
            }
            _ => break,
        }
    }
    count
}

enum MP {
    More,
    Bad,
    No,
    Got(Motion),
}

fn parse_motion(keys: &[VKey], i: &mut usize) -> MP {
    let Some(k) = keys.get(*i) else { return MP::More };
    *i += 1;
    let m = match *k {
        VKey::Char('h') | VKey::Left | VKey::Backspace => Motion::Left,
        VKey::Char('l') | VKey::Right | VKey::Char(' ') => Motion::Right,
        VKey::Char('j') | VKey::Down | VKey::Ctrl('n') => Motion::Down,
        VKey::Char('k') | VKey::Up | VKey::Ctrl('p') => Motion::Up,
        VKey::Char('w') => Motion::WordFwd(false),
        VKey::Char('W') => Motion::WordFwd(true),
        VKey::Char('b') => Motion::WordBack(false),
        VKey::Char('B') => Motion::WordBack(true),
        VKey::Char('e') => Motion::WordEnd(false),
        VKey::Char('E') => Motion::WordEnd(true),
        VKey::Char('0') | VKey::Home => Motion::LineStart,
        VKey::Char('^') => Motion::FirstNonBlank,
        VKey::Char('$') | VKey::End => Motion::LineEnd,
        VKey::Char('G') => Motion::LastLine,
        VKey::Char('%') => Motion::Match,
        VKey::Char('}') => Motion::ParaFwd,
        VKey::Char('{') => Motion::ParaBack,
        VKey::Char('n') => Motion::SearchNext(false),
        VKey::Char('N') => Motion::SearchNext(true),
        VKey::Char(';') => Motion::RepeatFind(false),
        VKey::Char(',') => Motion::RepeatFind(true),
        VKey::Enter | VKey::Char('+') => Motion::NextLine,
        VKey::Char('-') => Motion::PrevLine,
        VKey::Char('g') => match keys.get(*i) {
            None => return MP::More,
            Some(VKey::Char('g')) => {
                *i += 1;
                Motion::FirstLine
            }
            Some(_) => return MP::Bad,
        },
        VKey::Char(f @ ('f' | 't' | 'F' | 'T')) => match keys.get(*i) {
            None => return MP::More,
            Some(VKey::Char(c)) => {
                *i += 1;
                Motion::Find(f, *c)
            }
            Some(_) => return MP::Bad,
        },
        _ => {
            *i -= 1;
            return MP::No;
        }
    };
    MP::Got(m)
}

fn parse(keys: &[VKey], visual: bool) -> Parse {
    let mut i = 0;
    let count = parse_count(keys, &mut i);
    match parse_motion(keys, &mut i) {
        MP::More => return Parse::More,
        MP::Bad => return Parse::Bad,
        MP::Got(m) => return Parse::Done(count, Cmd::Move(m)),
        MP::No => {}
    }
    let k = keys[i];
    i += 1;
    let next_char = |i: usize| match keys.get(i) {
        None => Err(true),
        Some(VKey::Char(c)) => Ok(*c),
        Some(_) => Err(false),
    };
    let cmd = match k {
        VKey::Char(op @ ('d' | 'c' | 'y' | '>' | '<')) if !visual => {
            let c2 = parse_count(keys, &mut i);
            match keys.get(i) {
                None => return Parse::More,
                Some(VKey::Char(o)) if *o == op => Cmd::Op(op, Target::Line),
                Some(VKey::Char(ia @ ('i' | 'a'))) => match next_char(i + 1) {
                    Ok(c) => Cmd::Op(op, Target::Object(*ia == 'i', c)),
                    Err(true) => return Parse::More,
                    Err(false) => return Parse::Bad,
                },
                Some(_) => match parse_motion(keys, &mut i) {
                    MP::More => return Parse::More,
                    MP::Got(m) => Cmd::Op(op, Target::Motion(c2, m)),
                    _ => return Parse::Bad,
                },
            }
        }
        VKey::Char(ia @ ('i' | 'a')) if visual => match next_char(i) {
            Ok(c) => Cmd::VisualObject(ia == 'i', c),
            Err(true) => return Parse::More,
            Err(false) => return Parse::Bad,
        },
        VKey::Char('r') => match next_char(i) {
            Ok(c) => Cmd::Replace(c),
            Err(true) => return Parse::More,
            Err(false) => return Parse::Bad,
        },
        VKey::Char('z') => match keys.get(i) {
            None => return Parse::More,
            Some(_) => Cmd::Nop,
        },
        VKey::Char(c) if "xXsSDCYpPJu~iaIAoOvV.*#:/?dcy><U".contains(c) => Cmd::Act(c),
        VKey::Ctrl('r') => Cmd::Redo,
        VKey::Ctrl('d') | VKey::PageDown => Cmd::HalfPage(true),
        VKey::Ctrl('u') | VKey::PageUp => Cmd::HalfPage(false),
        VKey::Esc | VKey::Ctrl('[') | VKey::Ctrl('c') => Cmd::Act('\x1b'),
        VKey::Delete => Cmd::Act('x'),
        _ => return Parse::Bad,
    };
    Parse::Done(count, cmd)
}

/// Keys that change the text (recorded for `.`).
fn is_change(cmd: &Cmd) -> bool {
    match cmd {
        Cmd::Op(op, _) => *op != 'y',
        Cmd::Replace(_) => true,
        Cmd::Act(c) => "xXsSDCpPJ~iaIAoO".contains(*c),
        _ => false,
    }
}

impl Vim {
    /// Short state for the status line: mode, pending keys, command line, message.
    pub fn status(&self) -> String {
        if let Some((k, s)) = &self.cmdline {
            return format!("{k}{s}");
        }
        let mode = match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        };
        let pending: String = self
            .pending
            .iter()
            .map(|k| match k {
                VKey::Char(c) => c.to_string(),
                VKey::Ctrl(c) => format!("^{}", c.to_ascii_uppercase()),
                _ => String::new(),
            })
            .collect();
        let mut s = format!("-- {mode} --");
        if !pending.is_empty() {
            s.push_str("  ");
            s.push_str(&pending);
        }
        if !self.message.is_empty() {
            s.push_str("  ");
            s.push_str(&self.message);
        }
        s
    }

    /// Visual selection as a char range `[a, b)`.
    pub fn selection(&self, text: &str) -> Option<(usize, usize)> {
        match self.mode {
            Mode::Visual => {
                let n = text.chars().count();
                let (a, b) = (self.anchor.min(self.cursor), self.anchor.max(self.cursor));
                Some((a, (b + 1).min(n)))
            }
            Mode::VisualLine => {
                let b: Vec<char> = text.chars().collect();
                let (a, z) = (self.anchor.min(self.cursor), self.anchor.max(self.cursor));
                Some((ls(&b, a), le(&b, z)))
            }
            _ => None,
        }
    }

    /// The text widget moved the cursor (mouse click, or typing in insert mode).
    pub fn sync_cursor(&mut self, text: &str, c: usize) {
        self.cursor = c;
        if self.mode != Mode::Insert {
            if self.mode != Mode::Normal {
                self.mode = Mode::Normal;
            }
            let b: Vec<char> = text.chars().collect();
            self.cursor = clamp(&b, c);
        }
    }

    pub fn handle(&mut self, text: &mut String, key: VKey) -> Response {
        let mut r = Response::default();
        let leaving = matches!(key, VKey::Esc | VKey::Ctrl('['));
        if self.mode == Mode::Insert && !self.self_insert && !leaving {
            if let Some(rec) = &mut self.recording {
                rec.push(key);
            }
            return r;
        }
        let mut b: Vec<char> = text.chars().collect();
        self.cursor = self.cursor.min(b.len());
        self.anchor = self.anchor.min(b.len());
        self.key(&mut b, key, &mut r);
        if r.changed {
            *text = b.iter().collect();
        }
        r
    }

    fn key(&mut self, b: &mut Vec<char>, key: VKey, r: &mut Response) {
        r.consumed = true;
        if self.mode == Mode::Insert {
            self.insert_key(b, key, r);
            return;
        }
        if self.cmdline.is_some() {
            self.cmdline_key(b, key, r);
            return;
        }
        self.message.clear();
        self.pending.push(key);
        let visual = matches!(self.mode, Mode::Visual | Mode::VisualLine);
        match parse(&self.pending, visual) {
            Parse::More => {}
            Parse::Bad => self.pending.clear(),
            Parse::Done(count, cmd) => {
                let keys = std::mem::take(&mut self.pending);
                let was_undo = self.undo.len();
                if visual {
                    self.visual_cmd(b, count, cmd, r);
                } else {
                    self.normal_cmd(b, count, cmd, r);
                }
                if !self.replaying && !visual && is_change(&cmd) {
                    if self.mode == Mode::Insert {
                        self.recording = Some(keys);
                    } else if self.undo.len() != was_undo || r.changed {
                        self.last_change = keys;
                    }
                }
                if self.mode == Mode::Normal {
                    self.cursor = clamp(b, self.cursor);
                }
            }
        }
    }

    fn snapshot(&mut self, b: &[char]) {
        self.undo.push((b.iter().collect(), self.cursor));
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn insert_key(&mut self, b: &mut Vec<char>, key: VKey, r: &mut Response) {
        if matches!(key, VKey::Esc | VKey::Ctrl('[')) {
            if let Some(mut rec) = self.recording.take() {
                rec.push(VKey::Esc);
                if !self.replaying {
                    self.last_change = rec;
                }
            }
            self.mode = Mode::Normal;
            if self.cursor > ls(b, self.cursor) {
                self.cursor -= 1;
            }
            self.cursor = clamp(b, self.cursor);
            self.want_col = None;
            return;
        }
        if let Some(rec) = &mut self.recording {
            rec.push(key);
        }
        let c = self.cursor;
        match key {
            VKey::Char(ch) => {
                b.insert(c, ch);
                self.cursor += 1;
            }
            VKey::Tab => {
                b.insert(c, '\t');
                self.cursor += 1;
            }
            VKey::Enter => {
                let indent: Vec<char> = b[ls(b, c)..].iter().take_while(|c| blank(**c)).copied().collect();
                b.insert(c, '\n');
                for (k, ch) in indent.iter().enumerate() {
                    b.insert(c + 1 + k, *ch);
                }
                self.cursor = c + 1 + indent.len();
            }
            VKey::Backspace if c > 0 => {
                b.remove(c - 1);
                self.cursor -= 1;
            }
            VKey::Delete if c < b.len() => {
                b.remove(c);
            }
            VKey::Left => self.cursor = c.saturating_sub(1).max(ls(b, c)),
            VKey::Right => self.cursor = (c + 1).min(le(b, c)),
            _ => return,
        }
        r.changed = !matches!(key, VKey::Left | VKey::Right);
    }

    fn cmdline_key(&mut self, b: &mut Vec<char>, key: VKey, r: &mut Response) {
        let Some((kind, s)) = &mut self.cmdline else { return };
        match key {
            VKey::Esc | VKey::Ctrl('[') | VKey::Ctrl('c') => self.cmdline = None,
            VKey::Backspace => {
                if s.pop().is_none() {
                    self.cmdline = None;
                }
            }
            VKey::Char(c) => s.push(c),
            VKey::Enter => {
                let (kind, s) = (*kind, std::mem::take(s));
                self.cmdline = None;
                if kind == ':' {
                    self.ex(b, s.trim(), r);
                } else if !s.is_empty() || self.search.is_some() {
                    let pat = if s.is_empty() { self.search.as_ref().unwrap().0.clone() } else { s };
                    self.search = Some((pat, kind == '/'));
                    if let Some((t, _)) = self.motion(b, Motion::SearchNext(false), 1) {
                        self.cursor = t;
                    }
                }
            }
            _ => {}
        }
    }

    fn ex(&mut self, b: &mut Vec<char>, cmd: &str, r: &mut Response) {
        match cmd {
            "" => {}
            "w" | "w!" | "up" | "update" => r.save = true,
            "wq" | "wq!" | "x" | "wa" | "wqa" | "xa" => r.save = true,
            "q" | "q!" | "qa" | "qa!" => self.message = "close the file with Ctrl+W".into(),
            "noh" | "nohlsearch" => {}
            "$" => self.cursor = fnb(b, line_start(b, line_of(b, b.len()))),
            _ if cmd.chars().all(|c| c.is_ascii_digit()) => {
                let n: usize = cmd.parse().unwrap_or(1);
                self.cursor = fnb(b, line_start(b, n.saturating_sub(1)));
            }
            _ if cmd.starts_with("s") || cmd.starts_with("%s") => self.substitute(b, cmd, r),
            _ => self.message = format!("not an editor command: {cmd}"),
        }
    }

    /// `:s/old/new/g` and `:%s/…` (literal text, no regular expressions).
    fn substitute(&mut self, b: &mut Vec<char>, cmd: &str, r: &mut Response) {
        let all = cmd.starts_with('%');
        let rest = &cmd[if all { 2 } else { 1 }..];
        let Some(delim) = rest.chars().next().filter(|c| !c.is_alphanumeric() && !c.is_whitespace()) else {
            self.message = format!("not an editor command: {cmd}");
            return;
        };
        let parts: Vec<&str> = rest[delim.len_utf8()..].split(delim).collect();
        let (pat, rep) = (parts.first().copied().unwrap_or(""), parts.get(1).copied().unwrap_or(""));
        let global = parts.get(2).is_some_and(|f| f.contains('g'));
        if pat.is_empty() {
            return;
        }
        let text: String = b.iter().collect();
        let cur_line = line_of(b, self.cursor);
        let mut n = 0;
        let lines: Vec<String> = text
            .split('\n')
            .enumerate()
            .map(|(i, l)| {
                if !(all || i == cur_line) || !l.contains(pat) {
                    return l.to_string();
                }
                if global {
                    n += l.matches(pat).count();
                    l.replace(pat, rep)
                } else {
                    n += 1;
                    l.replacen(pat, rep, 1)
                }
            })
            .collect();
        if n == 0 {
            self.message = format!("pattern not found: {pat}");
            return;
        }
        self.snapshot(b);
        *b = lines.join("\n").chars().collect();
        r.changed = true;
        self.message = format!("{n} substitution{}", if n == 1 { "" } else { "s" });
        self.cursor = clamp(b, self.cursor);
    }

    /// Target of a motion and how an operator treats it.
    fn motion(&mut self, b: &[char], m: Motion, n: usize) -> Option<(usize, Kind)> {
        let c = self.cursor;
        let len = b.len();
        let vertical = |want: Option<usize>, down: bool| {
            let col = want.unwrap_or(c - ls(b, c));
            let line = line_of(b, c);
            let last = line_of(b, len);
            let t = if down { (line + n).min(last) } else { line.saturating_sub(n) };
            if t == line {
                return None;
            }
            let s = line_start(b, t);
            Some((s + col.min(le(b, s) - s), Kind::Linewise))
        };
        let r = match m {
            Motion::Left => (c - n.min(c - ls(b, c)), Kind::Exclusive),
            Motion::Right => ((c + n).min(le(b, c)), Kind::Exclusive),
            Motion::Down => return vertical(self.want_col, true),
            Motion::Up => return vertical(self.want_col, false),
            Motion::WordFwd(big) => ((0..n).fold(c, |i, _| word_fwd(b, i, big)), Kind::Exclusive),
            Motion::WordBack(big) => ((0..n).fold(c, |i, _| word_back(b, i, big)), Kind::Exclusive),
            Motion::WordEnd(big) => ((0..n).fold(c, |i, _| word_end(b, i, big)), Kind::Inclusive),
            Motion::LineStart => (ls(b, c), Kind::Exclusive),
            Motion::FirstNonBlank => (fnb(b, c), Kind::Exclusive),
            Motion::LineEnd => {
                let t = if n > 1 { line_start(b, line_of(b, c) + n - 1) } else { c };
                (le(b, t), Kind::Exclusive)
            }
            Motion::LastLine | Motion::FirstLine => {
                let line = match (m, self.pending_count(n)) {
                    (_, Some(k)) => k - 1,
                    (Motion::LastLine, None) => line_of(b, len),
                    _ => 0,
                };
                (fnb(b, line_start(b, line)), Kind::Linewise)
            }
            Motion::NextLine | Motion::PrevLine => {
                let line = line_of(b, c);
                let t = if m == Motion::NextLine { (line + n).min(line_of(b, len)) } else { line.saturating_sub(n) };
                (fnb(b, line_start(b, t)), Kind::Linewise)
            }
            Motion::Find(kind, ch) => {
                self.last_find = Some((kind, ch));
                return self.find_char(b, kind, ch, n);
            }
            Motion::RepeatFind(rev) => {
                let (kind, ch) = self.last_find?;
                let kind = if rev {
                    match kind {
                        'f' => 'F',
                        'F' => 'f',
                        't' => 'T',
                        _ => 't',
                    }
                } else {
                    kind
                };
                return self.find_char(b, kind, ch, n);
            }
            Motion::Match => {
                let e = le(b, c);
                let p = (c..e).find(|&k| "()[]{}".contains(b[k]) && !escaped(b, k))?;
                (match_bracket(b, p)?, Kind::Inclusive)
            }
            Motion::ParaFwd | Motion::ParaBack => {
                let fwd = m == Motion::ParaFwd;
                let mut i = c;
                for _ in 0..n {
                    // skip empty lines, then find the next empty line
                    let empty = |k: usize| ls(b, k) == le(b, k);
                    if fwd {
                        while i < len && empty(i) {
                            i = le(b, i) + 1;
                        }
                        while i < len && !empty(i) {
                            i = le(b, i) + 1;
                        }
                        i = i.min(len);
                    } else {
                        i = ls(b, i);
                        while i > 0 && empty(i) {
                            i = ls(b, i - 1);
                        }
                        while i > 0 && !empty(i) {
                            i = ls(b, i - 1);
                        }
                    }
                }
                (i, Kind::Exclusive)
            }
            Motion::SearchNext(rev) => {
                let Some((pat, fwd)) = self.search.clone() else {
                    self.message = "no previous search".into();
                    return None;
                };
                let pat: Vec<char> = pat.chars().collect();
                let mut i = c;
                for _ in 0..n {
                    match find(b, &pat, i, fwd != rev) {
                        Some(p) => i = p,
                        None => {
                            self.message = format!("pattern not found: {}", pat.iter().collect::<String>());
                            return None;
                        }
                    }
                }
                (i, Kind::Exclusive)
            }
        };
        Some(r)
    }

    /// `G`/`gg` take the count as a line number; `n` is 1 without a count.
    fn pending_count(&self, n: usize) -> Option<usize> {
        self.count_given.then_some(n)
    }

    fn find_char(&self, b: &[char], kind: char, ch: char, n: usize) -> Option<(usize, Kind)> {
        let c = self.cursor;
        let (l0, l1) = (ls(b, c), le(b, c));
        match kind {
            'f' | 't' => {
                let start = if kind == 't' { c + 2 } else { c + 1 };
                let p = (start.min(l1)..l1).filter(|&k| b[k] == ch).nth(n - 1)?;
                Some((if kind == 't' { p - 1 } else { p }, Kind::Inclusive))
            }
            _ => {
                let end = if kind == 'T' { c.saturating_sub(1) } else { c };
                let p = (l0..end.max(l0)).rev().filter(|&k| b[k] == ch).nth(n - 1)?;
                Some((if kind == 'T' { p + 1 } else { p }, Kind::Exclusive))
            }
        }
    }

    /// Char range `[a, b)` an operator acts on, and whether it is linewise.
    fn op_range(&mut self, b: &[char], op: char, target: Target, n: usize) -> Option<(usize, usize, bool)> {
        let c = self.cursor;
        match target {
            Target::Line => {
                let last = line_start(b, (line_of(b, c) + n - 1).min(line_of(b, b.len())));
                Some((ls(b, c), le(b, last), true))
            }
            Target::Object(inner, ch) => {
                let (a, z) = text_object(b, c, inner, ch)?;
                Some((a, z, false))
            }
            Target::Motion(n2, m) => {
                let total = n * n2.unwrap_or(1);
                if n2.is_some() {
                    self.count_given = true;
                }
                // `cw` on a word changes to its end, like `ce`
                let m = match m {
                    Motion::WordFwd(big) if op == 'c' && c < b.len() && !b[c].is_whitespace() => Motion::WordEnd(big),
                    m => m,
                };
                let (mut t, kind) = self.motion(b, m, total)?;
                // `dw` on the last word of a line stops at the line end
                if matches!(m, Motion::WordFwd(_)) && t > le(b, c) && ls(b, t) > 0 {
                    t = ls(b, t) - 1;
                }
                let (a, z) = (c.min(t), c.max(t));
                Some(match kind {
                    Kind::Exclusive => (a, z, false),
                    Kind::Inclusive => (a, (z + 1).min(b.len()), false),
                    Kind::Linewise => (ls(b, a), le(b, z), true),
                })
            }
        }
    }

    /// Applies an operator to `[a, z)` (for linewise: whole lines `a..z`, `z` at a line end).
    fn apply_op(&mut self, b: &mut Vec<char>, op: char, a: usize, z: usize, lines: bool, r: &mut Response) {
        let yank: String = if lines { b[ls(b, a)..le(b, z)].iter().collect::<String>() + "\n" } else { b[a..z].iter().collect() };
        match op {
            'y' => {
                self.register = yank.clone();
                self.register_lines = lines;
                r.copy = Some(yank);
                if !lines {
                    self.cursor = a;
                }
            }
            'd' | 'c' => {
                self.snapshot(b);
                self.register = yank;
                self.register_lines = lines;
                if lines && op == 'c' {
                    let (s, e) = (ls(b, a), le(b, z));
                    let indent: Vec<char> = b[s..].iter().take_while(|c| blank(**c)).copied().collect();
                    b.splice(s..e, indent.iter().copied());
                    self.cursor = s + indent.len();
                } else if lines {
                    let (s, e) = (ls(b, a), le(b, z));
                    let (s, e) = if e < b.len() { (s, e + 1) } else { (s.saturating_sub(1), e) };
                    b.drain(s..e);
                    self.cursor = fnb(b, s.min(b.len()));
                } else {
                    b.drain(a..z);
                    self.cursor = a;
                }
                r.changed = true;
                if op == 'c' {
                    self.mode = Mode::Insert;
                }
            }
            '>' | '<' => {
                self.snapshot(b);
                let (l0, l1) = (line_of(b, a), line_of(b, z.max(a)));
                for line in (l0..=l1).rev() {
                    let s = line_start(b, line);
                    if op == '>' {
                        if le(b, s) > s {
                            b.splice(s..s, "  ".chars());
                        }
                    } else {
                        let k = if b.get(s) == Some(&'\t') { 1 } else { b[s..].iter().take(2).take_while(|c| **c == ' ').count() };
                        b.drain(s..s + k);
                    }
                }
                self.cursor = fnb(b, line_start(b, l0));
                r.changed = true;
            }
            '~' | 'u' | 'U' => {
                self.snapshot(b);
                for ch in &mut b[a..z] {
                    let up = match op {
                        'u' => false,
                        'U' => true,
                        _ => ch.is_lowercase(),
                    };
                    let conv: Vec<char> = if up { ch.to_uppercase().collect() } else { ch.to_lowercase().collect() };
                    if conv.len() == 1 {
                        *ch = conv[0];
                    }
                }
                self.cursor = a;
                r.changed = true;
            }
            _ => {}
        }
    }

    fn paste(&mut self, b: &mut Vec<char>, before: bool, n: usize, r: &mut Response) {
        if self.register.is_empty() {
            return;
        }
        self.snapshot(b);
        let c = self.cursor;
        let reg: Vec<char> = self.register.repeat(n).chars().collect();
        if self.register_lines {
            let at = if before {
                ls(b, c)
            } else if le(b, c) < b.len() {
                le(b, c) + 1
            } else {
                // last line without a line break
                let e = b.len();
                b.insert(e, '\n');
                let body = &reg[..reg.len() - 1];
                b.splice(e + 1..e + 1, body.iter().copied());
                self.cursor = fnb(b, e + 1);
                r.changed = true;
                return;
            };
            b.splice(at..at, reg.iter().copied());
            self.cursor = fnb(b, at);
        } else {
            let at = if before || c >= b.len() || b[c] == '\n' { c } else { c + 1 };
            b.splice(at..at, reg.iter().copied());
            self.cursor = at + reg.len() - 1;
        }
        r.changed = true;
    }

    fn join(&mut self, b: &mut Vec<char>, n: usize, r: &mut Response) {
        self.snapshot(b);
        for _ in 0..n.max(2) - 1 {
            let e = le(b, self.cursor);
            if e >= b.len() {
                break;
            }
            let mut k = e + 1;
            while k < b.len() && blank(b[k]) {
                k += 1;
            }
            let sep = if e > ls(b, e) && !blank(b[e - 1]) && k < b.len() && b[k] != '\n' && b[k] != ')' { " " } else { "" };
            b.splice(e..k, sep.chars());
            self.cursor = e;
            r.changed = true;
        }
    }

    fn normal_cmd(&mut self, b: &mut Vec<char>, count: Option<usize>, cmd: Cmd, r: &mut Response) {
        let n = count.unwrap_or(1);
        self.count_given = count.is_some();
        let c = self.cursor;
        match cmd {
            Cmd::Nop => {}
            Cmd::Move(m) => {
                if let Some((t, _)) = self.motion(b, m, n) {
                    self.cursor = t;
                    match m {
                        Motion::Up | Motion::Down => {
                            self.want_col.get_or_insert(c - ls(b, c));
                        }
                        Motion::LineEnd => self.want_col = Some(usize::MAX),
                        _ => self.want_col = None,
                    }
                }
            }
            Cmd::Op(op, target) => {
                if let Some((a, z, lines)) = self.op_range(b, op, target, n) {
                    self.apply_op(b, op, a, z, lines, r);
                }
                self.want_col = None;
            }
            Cmd::Replace(ch) => {
                let e = le(b, c);
                if c + n <= e {
                    self.snapshot(b);
                    b[c..c + n].fill(ch);
                    self.cursor = c + n - 1;
                    r.changed = true;
                }
            }
            Cmd::Redo => {
                for _ in 0..n {
                    if let Some((t, cur)) = self.redo.pop() {
                        self.undo.push((b.iter().collect(), self.cursor));
                        *b = t.chars().collect();
                        self.cursor = cur;
                        r.changed = true;
                    }
                }
            }
            Cmd::HalfPage(down) => {
                let m = if down { Motion::Down } else { Motion::Up };
                if let Some((t, _)) = self.motion(b, m, HALF_PAGE * n) {
                    self.cursor = t;
                }
            }
            Cmd::VisualObject(..) => {}
            Cmd::Act(a) => self.normal_act(b, a, count, r),
        }
    }

    fn normal_act(&mut self, b: &mut Vec<char>, a: char, count: Option<usize>, r: &mut Response) {
        let n = count.unwrap_or(1);
        let c = self.cursor;
        let e = le(b, c);
        match a {
            '\x1b' => self.message.clear(),
            'x' | 's' if c < e => {
                let z = (c + n).min(e);
                self.apply_op(b, if a == 'x' { 'd' } else { 'c' }, c, z, false, r);
            }
            's' => {
                self.snapshot(b);
                self.mode = Mode::Insert;
            }
            'X' if c > ls(b, c) => {
                let s = c - n.min(c - ls(b, c));
                self.apply_op(b, 'd', s, c, false, r);
            }
            'D' | 'C' => {
                let t = if n > 1 { le(b, line_start(b, (line_of(b, c) + n - 1).min(line_of(b, b.len())))) } else { e };
                self.apply_op(b, if a == 'D' { 'd' } else { 'c' }, c, t, false, r);
                if a == 'D' {
                    self.cursor = clamp(b, c);
                }
            }
            'S' => self.normal_cmd(b, count, Cmd::Op('c', Target::Line), r),
            'Y' => self.normal_cmd(b, count, Cmd::Op('y', Target::Line), r),
            'p' | 'P' => self.paste(b, a == 'P', n, r),
            'J' => self.join(b, n, r),
            'u' => {
                for _ in 0..n {
                    if let Some((t, cur)) = self.undo.pop() {
                        self.redo.push((b.iter().collect(), self.cursor));
                        *b = t.chars().collect();
                        self.cursor = cur.min(b.len());
                        r.changed = true;
                    } else {
                        self.message = "already at oldest change".into();
                    }
                }
            }
            '~' if c < e => {
                let z = (c + n).min(e);
                self.apply_op(b, '~', c, z, false, r);
                self.cursor = z;
            }
            'i' | 'a' | 'I' | 'A' | 'o' | 'O' => {
                self.snapshot(b);
                self.mode = Mode::Insert;
                match a {
                    'a' if c < e => self.cursor = c + 1,
                    'I' => self.cursor = fnb(b, c),
                    'A' => self.cursor = e,
                    'o' | 'O' => {
                        let indent: Vec<char> = b[ls(b, c)..].iter().take_while(|c| blank(**c)).copied().collect();
                        let at = if a == 'o' { e } else { ls(b, c) };
                        let mut ins = indent.clone();
                        if a == 'o' {
                            ins.insert(0, '\n');
                        } else {
                            ins.push('\n');
                        }
                        b.splice(at..at, ins);
                        self.cursor = if a == 'o' { at + 1 + indent.len() } else { at + indent.len() };
                        r.changed = true;
                    }
                    _ => {}
                }
            }
            'v' | 'V' => {
                self.mode = if a == 'v' { Mode::Visual } else { Mode::VisualLine };
                self.anchor = c;
            }
            '.' => {
                let keys = self.last_change.clone();
                if keys.is_empty() {
                    return;
                }
                let saved = self.self_insert;
                self.self_insert = true;
                self.replaying = true;
                for k in keys {
                    self.key(b, k, r);
                }
                self.replaying = false;
                self.self_insert = saved;
            }
            '*' | '#' => {
                if let Some((s, z)) = text_object(b, c, true, 'w').filter(|(s, _)| class(b[*s], false) == 1) {
                    let w: String = b[s..z].iter().collect();
                    self.search = Some((w, a == '*'));
                    self.cursor = s;
                    if let Some((t, _)) = self.motion(b, Motion::SearchNext(false), n) {
                        self.cursor = t;
                    }
                }
            }
            ':' | '/' | '?' => self.cmdline = Some((a, String::new())),
            _ => {}
        }
    }

    fn visual_cmd(&mut self, b: &mut Vec<char>, count: Option<usize>, cmd: Cmd, r: &mut Response) {
        let n = count.unwrap_or(1);
        self.count_given = count.is_some();
        let lines = self.mode == Mode::VisualLine;
        let (a, z) = (self.anchor.min(self.cursor), self.anchor.max(self.cursor));
        let end = (z + 1).min(b.len());
        match cmd {
            Cmd::Move(m) => {
                if let Some((t, _)) = self.motion(b, m, n) {
                    self.cursor = t.min(b.len().saturating_sub(1));
                }
            }
            Cmd::VisualObject(inner, ch) => {
                if let Some((s, e)) = text_object(b, self.cursor, inner, ch)
                    && e > s {
                        self.anchor = s;
                        self.cursor = e - 1;
                    }
            }
            Cmd::HalfPage(down) => {
                if let Some((t, _)) = self.motion(b, if down { Motion::Down } else { Motion::Up }, HALF_PAGE * n) {
                    self.cursor = t;
                }
            }
            Cmd::Replace(ch) => {
                self.snapshot(b);
                for x in b[a..end].iter_mut().filter(|x| **x != '\n') {
                    *x = ch;
                }
                self.mode = Mode::Normal;
                self.cursor = a;
                r.changed = true;
            }
            Cmd::Act(act) => {
                let leave = |v: &mut Vim| v.mode = Mode::Normal;
                match act {
                    '\x1b' => leave(self),
                    'v' | 'V' => {
                        let m = if act == 'v' { Mode::Visual } else { Mode::VisualLine };
                        self.mode = if self.mode == m { Mode::Normal } else { m };
                    }
                    'o' => std::mem::swap(&mut self.anchor, &mut self.cursor),
                    'd' | 'x' | 'X' | 'D' | 'y' | 'Y' | 'c' | 's' | 'C' | 'S' | 'R' | '>' | '<' | '~' | 'u' | 'U' => {
                        let op = match act {
                            'x' | 'X' | 'D' => 'd',
                            'Y' => 'y',
                            's' | 'C' | 'S' | 'R' => 'c',
                            o => o,
                        };
                        let lines = lines || matches!(act, 'X' | 'D' | 'Y' | 'C' | 'S' | 'R');
                        leave(self);
                        if lines { self.apply_op(b, op, a, z, true, r) } else { self.apply_op(b, op, a, end, false, r) }
                        if op == 'y' {
                            self.cursor = a;
                        }
                    }
                    'J' => {
                        leave(self);
                        self.cursor = a;
                        let k = line_of(b, z) - line_of(b, a) + 1;
                        self.join(b, k, r);
                    }
                    'p' | 'P' => {
                        // replace the selection with the register (one undo step)
                        let (reg, reg_lines) = (self.register.clone(), self.register_lines);
                        leave(self);
                        if lines { self.apply_op(b, 'd', a, z, true, r) } else { self.apply_op(b, 'd', a, end, false, r) }
                        (self.register, self.register_lines) = (reg, reg_lines);
                        let steps = self.undo.len();
                        self.paste(b, true, 1, r);
                        if self.undo.len() > steps {
                            self.undo.pop();
                        }
                    }
                    ':' | '/' | '?' => {
                        leave(self);
                        self.cmdline = Some((act, String::new()));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses `"dw<Esc>:w<CR>"`-style key notation.
    fn keys(s: &str) -> Vec<VKey> {
        let mut v = Vec::new();
        let mut rest = s;
        while let Some(c) = rest.chars().next() {
            if c == '<'
                && let Some(end) = rest.find('>') {
                    let name = &rest[1..end];
                    let k = match name {
                        "Esc" => Some(VKey::Esc),
                        "CR" => Some(VKey::Enter),
                        "BS" => Some(VKey::Backspace),
                        "Tab" => Some(VKey::Tab),
                        _ if name.starts_with("C-") => name[2..].chars().next().map(VKey::Ctrl),
                        _ => None,
                    };
                    if let Some(k) = k {
                        v.push(k);
                        rest = &rest[end + 1..];
                        continue;
                    }
                }
            v.push(VKey::Char(c));
            rest = &rest[c.len_utf8()..];
        }
        v
    }

    /// Runs keys on a text; `|` marks the cursor. Returns the text with `|` at the cursor.
    fn run(text: &str, input: &str) -> String {
        let (v, s) = run_vim(text, input);
        let mut out: Vec<char> = s.chars().collect();
        out.insert(v.cursor.min(out.len()), '|');
        out.into_iter().collect()
    }

    fn run_vim(text: &str, input: &str) -> (Vim, String) {
        let cursor = text.chars().position(|c| c == '|').expect("cursor mark");
        let mut s: String = text.replacen('|', "", 1);
        let mut v = Vim { cursor, ..Default::default() };
        for k in keys(input) {
            v.handle(&mut s, k);
        }
        (v, s)
    }

    #[test]
    fn motions() {
        assert_eq!(run("|hello world foo", "w"), "hello |world foo");
        assert_eq!(run("|hello world foo", "2w"), "hello world |foo");
        assert_eq!(run("hello world |foo", "b"), "hello |world foo");
        assert_eq!(run("|hello world", "e"), "hell|o world");
        assert_eq!(run("|\\section{Intro}", "w"), "\\|section{Intro}");
        assert_eq!(run("  |abc def", "$"), "  abc de|f");
        assert_eq!(run("  abc |def", "0"), "|  abc def");
        assert_eq!(run("  abc |def", "^"), "  |abc def");
        assert_eq!(run("|a(b[c]d)e", "%"), "a(b[c]d|)e");
        assert_eq!(run("a(b[c]d|)e", "%"), "a|(b[c]d)e");
        assert_eq!(run("|one, two, three", "f,;"), "one, two|, three");
        assert_eq!(run("|one, two", "t,"), "on|e, two");
        assert_eq!(run("one\n|two\nthree", "G"), "one\ntwo\n|three");
        assert_eq!(run("one\ntwo\n|three", "gg"), "|one\ntwo\nthree");
        assert_eq!(run("|one\ntwo\nthree", "2G"), "one\n|two\nthree");
        assert_eq!(run("p1\np1\n\np2|", "{"), "p1\np1\n|\np2");
    }

    #[test]
    fn vertical_motion_keeps_column() {
        assert_eq!(run("abc|def\nx\nabcdef", "jj"), "abcdef\nx\nabc|def");
        assert_eq!(run("a|b\nlonger line", "$j"), "ab\nlonger lin|e");
        assert_eq!(run("one\n|two", "k"), "|one\ntwo");
    }

    #[test]
    fn delete_change_yank() {
        assert_eq!(run("|hello world", "dw"), "|world");
        assert_eq!(run("hello |world\nnext", "dw"), "hello| \nnext");
        assert_eq!(run("|hello world", "cwbye<Esc>"), "by|e world");
        assert_eq!(run("hel|lo world", "ciwbye<Esc>"), "by|e world");
        assert_eq!(run("hel|lo world", "daw"), "|world");
        assert_eq!(run("a\n|b\nc", "dd"), "a\n|c");
        assert_eq!(run("a\nb\n|c", "dd"), "a\n|b");
        assert_eq!(run("|a\nb\nc", "2dd"), "|c");
        assert_eq!(run("a|bc def", "D"), "|a");
        assert_eq!(run("a|bc def", "d$"), "|a");
        assert_eq!(run("abc |def", "C!<Esc>"), "abc |!");
        assert_eq!(run("|abc", "3x"), "|");
        assert_eq!(run("ab|c", "X"), "a|c");
        assert_eq!(run("|one\ntwo\nthree", "dG"), "|");
        assert_eq!(run("one\n|two\nthree", "dj"), "|one");
        assert_eq!(run("|abc", "rx"), "|xbc");
        assert_eq!(run("|abc", "~~"), "AB|c");
    }

    #[test]
    fn latex_text_objects() {
        assert_eq!(run("\\textbf{bo|ld text}", "di{"), "\\textbf{|}");
        assert_eq!(run("\\textbf{bo|ld text}", "da{"), "\\textb|f");
        assert_eq!(run("f(a, |b)", "ci(x<Esc>"), "f(|x)");
        assert_eq!(run("a $x+|y$ b", "di$"), "a $|$ b");
        assert_eq!(run("a $x+|y$ b", "da$"), "a | b");
        assert_eq!(run("say \"h|i\" now", "ci\"yo<Esc>"), "say \"y|o\" now");
        let env = "\\begin{itemize}\n  \\item |a\n  \\item b\n\\end{itemize}\nafter";
        assert_eq!(run(env, "die"), "\\begin{itemize}\n|\\end{itemize}\nafter");
        assert_eq!(run(env, "dae"), "|\nafter");
        // nested: the innermost environment
        let nested = "\\begin{a}\n\\begin{b}\nx|y\n\\end{b}\n\\end{a}";
        assert_eq!(run(nested, "dae"), "\\begin{a}\n|\n\\end{a}");
        // escaped braces are not brackets
        assert_eq!(run("\\set{a \\{ |b}", "di{"), "\\set{|}");
    }

    #[test]
    fn paste_undo_redo_repeat() {
        assert_eq!(run("|one\ntwo", "yyp"), "one\n|one\ntwo");
        assert_eq!(run("one\n|two", "yyp"), "one\ntwo\n|two");
        assert_eq!(run("one\n|two", "yyP"), "one\n|two\ntwo");
        assert_eq!(run("|ab", "xp"), "b|a");
        assert_eq!(run("|abc", "xxu"), "|bc");
        assert_eq!(run("|abc", "xxuu<C-r>"), "|bc");
        assert_eq!(run("|a b c", "x."), "|b c");
        assert_eq!(run("|a\nb\nc", "A;<Esc>j.j."), "a;\nb;\nc|;");
        assert_eq!(run("|word word", "cwx<Esc>w."), "x |x");
        assert_eq!(run("|a\nb", "dd."), "|");
        assert_eq!(run("|one", "ihi <Esc>u"), "|one");
    }

    #[test]
    fn insert_commands() {
        assert_eq!(run("  |a", "onew<Esc>"), "  a\n  ne|w");
        assert_eq!(run("  |a", "Onew<Esc>"), "  ne|w\n  a");
        assert_eq!(run("a|b", "ax<Esc>"), "ab|x");
        assert_eq!(run("  a|b", "Ix<Esc>"), "  |xab");
        assert_eq!(run("a|b\nc", "J"), "ab| c");
        assert_eq!(run("|a\nb", ">>"), "  |a\nb");
        assert_eq!(run("  |a", "<<"), "|a");
        assert_eq!(run("|ab", "ix<BS>y<Esc>"), "|yab");
    }

    #[test]
    fn visual_mode() {
        assert_eq!(run("|hello world", "vey"), "|hello world");
        assert_eq!(run("|hello world", "ved"), "| world");
        assert_eq!(run("|hello world", "veU"), "|HELLO world");
        assert_eq!(run("a\n|b\nc\nd", "Vjd"), "a\n|d");
        assert_eq!(run("\\emph{a |b}", "vi{c!<Esc>"), "\\emph{|!}");
        assert_eq!(run("|one two", "yiwwviwp"), "one on|e");
        assert_eq!(run("|a\nb", "Vj>"), "  |a\n  b");
    }

    #[test]
    fn search_and_ex() {
        assert_eq!(run("|foo bar foo baz", "/foo<CR>"), "foo bar |foo baz");
        assert_eq!(run("|foo bar foo baz", "/foo<CR>n"), "|foo bar foo baz");
        assert_eq!(run("|foo bar Foo", "/Foo<CR>"), "foo bar |Foo");
        assert_eq!(run("|foo bar foo", "*"), "foo bar |foo");
        assert_eq!(run("|a\nb\nc", ":3<CR>"), "a\nb\n|c");
        assert_eq!(run("|a a\na", ":%s/a/b/g<CR>"), "|b b\nb");
        assert_eq!(run("|a a\na", ":s/a/b/<CR>"), "|b a\na");
        let (v, _) = run_vim("|x", ":w<CR>");
        assert_eq!(v.mode, Mode::Normal);
        let mut s = "x".to_string();
        let mut v = Vim::default();
        for k in keys(":w") {
            v.handle(&mut s, k);
        }
        assert!(v.handle(&mut s, VKey::Enter).save);
        let (v, _) = run_vim("|x", ":nonsense<CR>");
        assert!(v.message.contains("not an editor command"));
    }

    #[test]
    fn insert_mode_keys_go_to_the_widget() {
        let mut s = "ab".to_string();
        let mut v = Vim::default();
        v.handle(&mut s, VKey::Char('i'));
        v.self_insert = false;
        let r = v.handle(&mut s, VKey::Char('x'));
        assert!(!r.consumed, "the text widget types in insert mode");
        assert_eq!(s, "ab");
        let r = v.handle(&mut s, VKey::Esc);
        assert!(r.consumed);
        assert_eq!(v.mode, Mode::Normal);
    }

    #[test]
    fn unicode_and_edges() {
        assert_eq!(run("|Grüße Welt", "dw"), "|Welt");
        assert_eq!(run("|", "dd"), "|");
        assert_eq!(run("|", "x"), "|");
        assert_eq!(run("|", "p"), "|");
        assert_eq!(run("|a", "jjkk"), "|a");
        assert_eq!(run("|abc", "d"), "|abc");
        assert_eq!(run("|\n\nx", "dd"), "|\nx");
    }
}
