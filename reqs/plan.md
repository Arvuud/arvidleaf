# arvidleaf – Plan

> Phase: **PLAN** · Stand: 11.11.2026
> Ablauf: PLAN → IMPLEMENT → REVIEW → Testiterationen
> Grundlage: `reqs/overleafcapabilities.md`

arvidleaf ist ein **lokaler LaTeX-Editor als Desktop-App**, im Funktionsumfang an Overleaf angelehnt und im Aussehen an **arvidlab** (`~/Programs/arvidlab`).

---

## 1. Anforderungen

### 1.1 Vorgaben des Nutzers

| # | Anforderung | Umsetzung (Kurzfassung) |
|---|---|---|
| R1 | Läuft **lokal** | Kein Server, kein Account, kein Netz. Projekte sind normale Ordner auf der Platte. |
| R2 | **Desktop-App** | Native Rust-App mit egui/eframe (wie arvidlab), Menüeintrag und Icon |
| R3 | **Keine Echtzeit-Kollaboration** (vorerst) | Entfällt. Die Architektur verbaut sie aber nicht. |
| R4 | **Keine Seiten- oder Compile-Zeitlimits** | Lokales TeX Live ohne Timeout. Die Kompilierung läuft im Hintergrund-Thread und lässt sich jederzeit abbrechen. Der PDF-Viewer rendert nur sichtbare Seiten, damit auch 500+ Seiten flüssig bleiben. |
| R5 | **Imports funktionieren problemlos** | `\usepackage{…}` nutzt direkt die volle lokale TeX-Live-Installation. Eigene `.sty`/`.cls` im Projektordner werden gefunden. Bilder, `.bib`, `\input`/`\include` und Unterordner funktionieren. Projekt-Import aus ZIP oder Ordner. |
| R6 | **Eigenes Template-Menü** | Template-Galerie mit eingebauten und eigenen Vorlagen sowie „Projekt als Vorlage speichern“ |
| R7 | **Color-Coding im Editor** | Eigener LaTeX-Highlighter mit Farben aus der arvidlab-Palette |
| R8 | **GUI im Stil von arvidlab** | Dieselbe Palette (`theme.rs`), dieselben Fonts, Abstände, Rundungen, Menüleiste und Statusleiste sowie Dark/Light-Umschaltung |

### 1.2 Übernommene Overleaf-Funktionen (aus der Recherche)

| Priorität | Funktion |
|---|---|
| **Must** | Projektverwaltung (Start-Dashboard mit zuletzt geöffneten Projekten), Dateibaum, Editor-Tabs, Kompilierung mit pdfLaTeX/XeLaTeX/LuaLaTeX über latexmk, PDF-Vorschau, lesbare Fehler und Warnungen mit Sprung zur Zeile, Auto-Save, Template-Menü |
| **Should** | SyncTeX (Klick im PDF springt zur Quelle, Button springt von der Quelle ins PDF), Auto-Compile beim Speichern, Dokument-Outline, Autovervollständigung (Befehle, Umgebungen, `\ref`/`\label`, `\cite`-Keys), Suchen & Ersetzen (inkl. projektweit), ZIP-Import und -Export, Hinweis bei fehlenden Paketen |
| **Must (Prototyp)** | Lokale Versionshistorie (Snapshots mit Diff und Wiederherstellen) |
| **Could** | Rechtschreibprüfung, Symbol-Palette, Code-Folding, Vim-Keybindings |
| **Später** | Echtzeit-Kollaboration, visueller Editor, KI-Fehlererklärung, Zotero-Anbindung (→ ersetzt durch arvidref, Abschnitt 15) |

### 1.3 Nicht-funktionale Anforderungen
- **UI blockiert nie:** Kompilierung, PDF-Rendering und Dateisuche laufen in Worker-Threads, wie der Interpreter-Thread in arvidlab.
- **Startzeit** unter 1 s, Tippen ohne spürbare Latenz auch bei Dateien mit 10 000 Zeilen.
- **Keine Datenverluste:** Auto-Save (Debounce ca. 1 s), Wiederherstellung nach Absturz.
- **Lizenz** MIT OR Apache-2.0 wie arvidlab. Nur Abhängigkeiten mit kompatibler Lizenz linken. Externe Tools (TeX, poppler) laufen als separate Prozesse.

---

## 2. Systemvoraussetzungen (auf diesem Rechner geprüft)

| Komponente | Status | Zweck |
|---|---|---|
| TeX Live 2026 (`texlive-latexextra`, `-pictures`, `-science`, `-publishers`, `-xetex`, `-luatex` …) | ✅ installiert | Kompilierung, alle `\usepackage` |
| `latexmk` | ✅ | Steuerung der Mehrfachläufe (BibTeX, Index, Querverweise) |
| `synctex` | ✅ | PDF ↔ Quelltext |
| `pdftoppm` (poppler) | ✅ | PDF-Seiten rendern |
| `kpsewhich` | ✅ | Prüfen, ob ein Paket installiert ist |
| `bibtex` | ✅ | Literatur |
| `biber` | ❌ **fehlt** | Nötig für `biblatex` mit Biber-Backend → `sudo pacman -S biber` |
| Rust 1.99 (`~/.cargo/bin`) | ✅ (nicht im `PATH` der Shell) | Build |

arvidleaf prüft diese Tools beim Start und zeigt fehlende im Statusbereich mit Installationshinweis an.

---

## 3. Architektur

### 3.1 Tech-Stack (wie arvidlab)
- **Rust, Edition 2024**, Cargo-Workspace
- **eframe/egui 0.37** (glow, wayland, x11) als GUI
- **rfd** für Datei-Dialoge
- **notify** zum Beobachten von Dateiänderungen außerhalb der App
- **zip** für Import und Export
- **serde + toml/json** für Projekt- und Template-Metadaten

### 3.2 Crates

```
arvidleaf/
├── Cargo.toml                 Workspace
├── crates/
│   ├── lf-core/               Projektmodell, Dateibaum, Settings, Templates, Outline-Parser
│   ├── lf-tex/                LaTeX-Lexer (Highlighting, Outline, Autocomplete), Log-Parser,
│   │                          Compile-Runner (latexmk), SyncTeX-Anbindung, Paket-Prüfung
│   ├── lf-pdf/                PDF-Seitenanzahl/-größe, Rendering mit Cache
│   ├── lf-gui/                egui-Oberfläche (theme.rs aus arvidlab übernommen)
│   └── arvidleaf/             Programm (GUI, --screenshot, CLI-Compile)
├── templates/                 eingebaute Vorlagen (per include_dir eingebettet)
├── icons/  packaging/  tests/  reqs/
```

### 3.3 Kompilierung (R4, R5)
- Aufruf: `latexmk -<engine> -synctex=1 -interaction=nonstopmode -file-line-error -halt-on-error=off -outdir=.arvidleaf/build <main>.tex`
- **Kein Timeout.** Der Prozess läuft in einem Worker-Thread. Der Abbrechen-Button beendet ihn mit Kill auf die Prozessgruppe. Ein neuer Compile-Auftrag während eines laufenden Laufs wird zusammengefasst: Es folgt höchstens ein Folgelauf.
- Build-Artefakte liegen in `<projekt>/.arvidleaf/build/`, damit der Quellordner sauber bleibt. `TEXINPUTS`/`BIBINPUTS` werden so gesetzt, dass Projektordner und Unterordner durchsucht werden. Damit funktionieren eigene `.sty`/`.cls`/`.bst` per `\usepackage` ohne Pfade.
- Die Engine ist pro Projekt wählbar (pdfLaTeX / XeLaTeX / LuaLaTeX). Hauptdatei: automatisch erkannt (die Datei mit `\documentclass`) oder manuell gesetzt.
- Shell-Escape (für `minted`, `svg`) ist pro Projekt einschaltbar, standardmäßig aus.
- **Fehlende Pakete:** Der Log-Parser erkennt `! LaTeX Error: File 'xyz.sty' not found` und zeigt einen klaren Hinweis, welches Paket fehlt. Das passende Arch-Paket wird über `pacman -F` bzw. `tlmgr search` ermittelt, falls verfügbar.

### 3.4 PDF-Vorschau (R4)
- **Empfehlung: `pdftoppm` als Subprozess.** Es ist installiert, robust und läuft als eigener Prozess ohne Lizenzkonflikt. Gerendert wird nur der sichtbare Bereich (plus/minus 2 Seiten) in passender DPI für den aktuellen Zoom. Ein LRU-Cache hält die Texturen. Seitengrößen kommen einmalig aus `pdfinfo`.
- Alternative für später: `pdfium-render` (schneller, aber `libpdfium.so` muss mitgeliefert werden). Die Schnittstelle in `lf-pdf` wird als Trait gebaut, damit das Backend austauschbar ist.
- Funktionen: stufenloser Zoom, „Seitenbreite“ / „Ganze Seite“, kontinuierliches Scrollen, Seitenanzeige „12 / 340“, Scroll-Position bleibt nach dem Neukompilieren erhalten, Download/Öffnen in externem Viewer.
- **SyncTeX:** `synctex view` und `synctex edit` als Subprozess. Strg+Klick im PDF springt zur Quellzeile, Strg+J springt von der Quelle ins PDF.

### 3.5 Editor & Color-Coding (R7)
- Eigenes Editor-Widget nach dem Muster von `arvidlab/crates/al-gui/src/editor.rs`: `TextEdit` mit `layouter`, Zeilennummern-Gutter und Galley-Cache.
- **LaTeX-Lexer** (`lf-tex`) mit diesen Token-Klassen und Farben (Dark-Palette aus arvidlab):

| Token | Beispiel | Farbe (dark) |
|---|---|---|
| Befehl | `\section`, `\textbf` | `keyword` #c678dd |
| Umgebungsname | `\begin{`**`figure`**`}` | `func` #61afef |
| Mathe-Modus | `$…$`, `\[…\]`, `equation` | `number` #d19a66 |
| Kommentar | `% …` | `comment` #6a737d (kursiv) |
| Argument in `{}` von `\label`/`\ref`/`\cite` | `{fig:plot}` | `string` #98c379 |
| Optionale Argumente | `[htbp]` | `accent2` #2dd4bf |
| Klammern `{}` `[]` | | `dim` #8b939e |
| Sonderzeichen `&`, `\\`, `~` | | `warn` #ffb84d |
| Fehlerzeile (aus Log) | | Unterstreichung `error` #ff6b6b |

  Die Light-Palette ist entsprechend. Die Farben sind zentral in `Palette` (Erweiterung um `math`, `env`, `arg`).
- **Komfort:** Auto-Schließen von `{}`, `[]`, `$$` und `\begin{x}` → `\end{x}`. Tab-Einrückung in Umgebungen. Strg+/ kommentiert ein und aus. Klammer-Paar-Hervorhebung. Autovervollständigung aus einer Befehlsliste, den Labels des Projekts und den `.bib`-Keys.

### 3.6 Template-Menü (R6)
- **Menü „Templates“** in der Menüleiste und **„Neues Projekt…“-Dialog** als Galerie: Karten mit Vorschaubild (erste PDF-Seite, beim ersten Anzeigen gerendert und gecacht), Name, Kategorie, Beschreibung und Suchfeld.
- **Eingebaute Vorlagen** (in die Binary eingebettet):
  - Artikel (deutsch, `babel`/`ngerman`, `microtype`, `hyperref`)
  - Artikel (englisch)
  - Bericht / Abschlussarbeit (`scrbook`, Kapitel-Ordner, Titelseite, Literatur)
  - Präsentation (Beamer)
  - Brief (`scrlttr2`)
  - Lebenslauf
  - Übungsblatt / Mitschrift (`amsmath`, `amsthm`)
  - Poster (`tikzposter`)
  - Leeres Projekt
- **Eigene Vorlagen:** Sie liegen in `~/.local/share/arvidleaf/templates/<name>/` mit `template.toml` (Name, Kategorie, Beschreibung, Hauptdatei, Engine). „Datei → Als Vorlage speichern“ legt aus dem aktuellen Projekt eine an. Der Dialog erlaubt Bearbeiten, Löschen und Im-Dateimanager-Öffnen.
- Beim Anlegen fragt der Dialog nach dem Projektnamen und Zielordner und ersetzt Platzhalter wie `{{title}}`, `{{author}}` und `{{date}}`.

### 3.7 Projekte & Imports (R1, R5)
- Ein Projekt ist ein Ordner. Metadaten liegen in `<projekt>/.arvidleaf/project.toml` (Hauptdatei, Engine, Shell-Escape, geöffnete Tabs).
- **Start-Dashboard:** zuletzt geöffnete Projekte, „Neu aus Vorlage“, „Ordner öffnen“, „ZIP importieren“ (z. B. Overleaf-Export).
- **Dateibaum:** Neu, Umbenennen, Löschen (in den Papierkorb), Ordner, Drag & Drop aus dem Dateimanager (Bilder, `.bib`, `.sty`) und Bildvorschau.
- **Einfügen-Helfer:** Bild per Drag & Drop in den Editor fügt einen `\includegraphics`-Block ein.
- Externe Änderungen erkennt `notify` und lädt die Datei neu (mit Konfliktabfrage bei ungespeicherten Änderungen).
- **Export:** PDF speichern unter, Projekt als ZIP.

---

## 4. GUI-Layout (R8, angelehnt an arvidlab)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 🍃 arvidleaf  File  Edit  View  Templates  Build  Help │ ▶ Compile ▾ │ ⏹ │ … │  ← Menüleiste (panel)
├──────────────┬───────────────────────────────┬──────────────────────────────┤
│ PROJECT      │ main.tex ×  intro.tex ×        │ ⊖ 110 % ⊕  ⇔  ⤓   12 / 48   │
│ ▾ chapters   │  1 \documentclass{scrbook}     │ ┌──────────────────────────┐ │
│    intro.tex │  2 \usepackage{amsmath}        │ │                          │ │
│ ▸ figures    │  3 \begin{document}            │ │        PDF-Seite         │ │
│   main.tex ● │  4 \chapter{Einleitung}        │ │                          │ │
│   refs.bib   │  5 Text mit $E = mc^2$ …       │ │                          │ │
│──────────────│                                │ └──────────────────────────┘ │
│ OUTLINE      │                                │                              │
│ 1 Einleitung │                                │                              │
│  1.1 Motiv.  │                                │                              │
├──────────────┴───────────────────────────────┴──────────────────────────────┤
│ ⚠ 2 Warnungen  ✖ 1 Fehler   main.tex:42  Undefined control sequence \foo     │  ← Log-Panel (einklappbar)
├─────────────────────────────────────────────────────────────────────────────┤
│ ● Kompiliert in 1,8 s · pdfLaTeX · 48 Seiten           Zeile 5, Spalte 12   │  ← Statusleiste (bg, Punkt-Indikator)
└─────────────────────────────────────────────────────────────────────────────┘
```

- **Theme:** `arvidlab/crates/al-gui/src/theme.rs` wird übernommen: Palette dark/light, `install_fonts` (Inter / JetBrains Mono), `apply` (Rundung 6/8/10, Abstände, Schriftgrößen 14/13,5/18). Standard ist dark, Umschaltung über View → Dark theme.
- **Panels** wie bei arvidlab: `Panel::top` (Menüleiste, `pal.panel`), `Panel::bottom` (Statusleiste, `pal.bg`, farbiger Punkt: grün = OK, gelb = Warnungen, rot = Fehler, blau pulsierend = kompiliert), `Panel::left` (Dateien + Outline, verstellbar, 260 px), Mitte geteilt in Editor und PDF (verstellbares Verhältnis).
- **Logo:** Variante des arvidlab-Dreiecks in denselben Teal-Tönen (#0b5e5d … #15c6b6), als Blatt-Form. App-Name klein geschrieben: „arvidleaf“.
- **Tastenkürzel:** Strg+S speichern + kompilieren, Strg+Enter kompilieren, Strg+. abbrechen, Strg+J zur PDF-Stelle springen, Strg+P Datei schnell öffnen, Strg+Umschalt+F projektweit suchen, F11 nur PDF anzeigen.
- **UI-Sprache: Englisch** (entschieden), die Doku ist deutsch.

---

## 5. IMPLEMENT-Phase: Meilensteine

| # | Meilenstein | Inhalt | Ergebnis |
|---|---|---|---|
| M0 | Gerüst | Workspace, Crates, eframe-Fenster, Theme aus arvidlab, Fonts, Menü- und Statusleiste, Icon | App startet im arvidlab-Look |
| M1 | Projekt & Editor | Ordner öffnen, Dateibaum, Tabs, Editor mit Zeilennummern, Speichern, Auto-Save, Projektmetadaten | Dateien bearbeiten |
| M2 | Color-Coding | LaTeX-Lexer, Highlighting, Klammer-Paare, Auto-Schließen | R7 erfüllt |
| M3 | Kompilierung | latexmk-Runner im Thread, Engine-Wahl, Abbrechen, Log-Parser, Log-Panel mit Sprung zur Zeile, Fehlende-Pakete-Hinweis | R4/R5 (Kompilierung) erfüllt |
| M4 | PDF-Viewer | pdftoppm-Backend, Lazy Rendering, Zoom, Scroll-Erhalt, SyncTeX in beide Richtungen | Vollständige Vorschau |
| M5 | Templates | Eingebaute Vorlagen, Galerie mit Vorschau, eigene Vorlagen, „Templates → Manage…“ zum manuellen Anlegen und Bearbeiten, Platzhalter | R6 erfüllt |
| M5b | Versionshistorie | Snapshots, History-Panel, Diff, Wiederherstellen (siehe Abschnitt 9) | Historie im Prototyp |
| M6 | Imports & Dashboard | Start-Dashboard, ZIP-Import/-Export, Drag & Drop, Dateiüberwachung | R5 (Imports) erfüllt |
| M7 | Komfort | Outline, Autovervollständigung, Suchen/Ersetzen, Quick-Open, Kommentar-Toggle | Should-Funktionen |
| M8 | Packaging | `--screenshot`, `install-desktop.sh`, `packaging/` wie arvidlab, README, CHANGELOG | Installierbar |

---

## 6. REVIEW-Phase
- Code-Review jedes Meilensteins (Korrektheit, Thread-Sicherheit, Fehlerbehandlung bei fehlenden Tools oder Pfaden mit Leerzeichen und Umlauten).
- Abgleich gegen die Anforderungstabelle 1.1/1.2: Jede Anforderung bekommt Nachweis „erfüllt / teilweise / offen“.
- UI-Review per Screenshot-Vergleich mit arvidlab (gleiche Fenstergröße, dark und light).
- `cargo clippy -- -D warnings`, `cargo fmt --check`.

## 7. Testiterationen
- **Unit-Tests:** Lexer (Token-Grenzen, verschachtelte Mathe-Modi, `\%` ist kein Kommentar), Log-Parser (echte Logs als Fixtures), Template-Platzhalter, Outline.
- **Integrationstests** (`tests/`): echte Kompilierung von Testprojekten:
  - Minimaldokument mit allen drei Engines
  - **großes Dokument** (500+ Seiten, viele TikZ-Grafiken), das ohne Timeout durchlaufen muss (R4)
  - Projekt mit eigener `.sty` in Unterordner, `\input`-Ketten, Bildern, BibTeX (R5)
  - Projekt mit fehlendem Paket, das eine korrekte Fehlermeldung ergeben muss
  - alle eingebauten Templates kompilieren fehlerfrei (R6)
  - Overleaf-ZIP-Export importieren und kompilieren
- **GUI-Tests:** Offscreen-Screenshots (Mechanismus aus arvidlab `offscreen.rs`) für Dashboard, Editor, Template-Galerie und Fehleransicht, mit Sichtprüfung.
- **Performance:** Tipp-Latenz bei 10 000 Zeilen, Scrollen durch ein PDF mit 500 Seiten.
- Jede Iteration: Tests ausführen, Fehler beheben, im CHANGELOG festhalten.

---

## 8. Risiken

| Risiko | Gegenmaßnahme |
|---|---|
| egui-`TextEdit` wird bei sehr großen Dateien langsam | Galley-Cache wie in arvidlab. Falls nötig, später nur sichtbare Zeilen layouten. |
| `pdftoppm` ist bei hohem Zoom oder vielen Seiten langsam | Nur sichtbare Seiten rendern, Cache, Backend-Trait für späteren Wechsel zu pdfium |
| `biber` fehlt | Beim Start prüfen und Installationshinweis anzeigen |
| Pfade mit Leerzeichen und Umlauten in TeX | Kompilieren mit relativem Pfad im Projektordner als Arbeitsverzeichnis, Tests dafür |

---

## 9. Entscheidungen (11.11.2026)

| Frage | Entscheidung |
|---|---|
| UI-Sprache | **Englisch** (wie arvidlab). Die Doku bleibt deutsch. |
| PDF-Backend | `pdftoppm` ist ok. Bedingung: **Rust und schnell**. Deshalb gilt: nur sichtbare Seiten rendern, aggressiver Cache, die erste Seite nach dem Kompilieren in < 300 ms sichtbar. Wird das verfehlt, wechseln wir über den Backend-Trait zu pdfium. |
| Templates | Die eingebaute Liste ist nur ein Startpunkt. **Wichtiger ist, dass Vorlagen manuell angelegt werden können.** Deshalb gibt es „Templates → Manage…“: neue Vorlage aus Projekt/Ordner, Metadaten bearbeiten, Vorschau neu erzeugen, löschen, Ordner öffnen. Das Format (`template.toml` + Dateien) ist dokumentiert, damit Vorlagen auch von Hand im Dateisystem entstehen können. |
| Versionshistorie | **Kommt in den ersten Prototyp** (siehe M5b). |
| Git-Repo | Angelegt: `~/Programs/arvidleaf`, Branch `main`. |

### M5b – Lokale Versionshistorie (neu im MVP)
- Snapshots in `<projekt>/.arvidleaf/history/` als inhaltsadressierter Speicher: Datei-Blobs nach Hash, gleiche Inhalte nur einmal, mit zstd komprimiert.
- Automatischer Snapshot bei jedem erfolgreichen Kompilieren und spätestens alle 10 Minuten bei Änderungen. Dazu kommen manuelle Snapshots mit Label (z. B. „Abgabe v1“).
- Panel „History“: Zeitleiste der Snapshots, Diff-Ansicht pro Datei (Zeilen-Diff mit Farben aus der Palette), einzelne Datei oder ganzes Projekt wiederherstellen. Vor dem Wiederherstellen wird automatisch ein Snapshot angelegt, damit nichts verloren geht.
- Aufräumen: Ältere automatische Snapshots werden ausgedünnt (stündlich ab 1 Tag, täglich ab 1 Woche). Snapshots mit Label bleiben immer erhalten.
- Tests: Snapshot → Ändern → Diff → Wiederherstellen ergibt den exakten Ausgangszustand. Deduplizierung wird ebenfalls geprüft.

---

## 10. Stand IMPLEMENT-Phase (11.11.2026)

| Meilenstein | Stand |
|---|---|
| M0 Gerüst, Theme | erledigt (Theme aus arvidlab, Leaf-Logo) |
| M1 Projekt & Editor | erledigt |
| M2 Color-Coding | erledigt (Lexer mit 15 Unit-Tests) |
| M3 Kompilierung | erledigt (latexmk, kein Timeout, Abbrechen, Log-Parser, Paket-Hinweise) |
| M4 PDF-Viewer | erledigt (erste Seite ~60 ms, SyncTeX beide Richtungen) |
| M5 Templates | erledigt (9 eingebaute, Galerie, Manage-Dialog, `template.toml`) |
| M5b Versionshistorie | erledigt |
| M6 Imports & Dashboard | erledigt |
| M7 Komfort | erledigt (Outline, Autocomplete, Suchen/Ersetzen, Quick-Open) |
| M8 Packaging | erledigt (`packaging/`, `install-desktop.sh`, README, CHANGELOG) |

Tests: 35 Unit-/Integrationstests grün, zusätzlich der 520-Seiten-Test (`--include-ignored`).
Offen für REVIEW: Test im echten Fenster (Dialoge, Drag & Drop), `clippy` installieren und laufen lassen.

---

## 11. PLAN: Markdown-Dokumente mit LaTeX-Mathe (Entwurf 12.11.2026, nicht freigegeben)

**Ziel:** `.md`-Dateien in arvidleaf schreiben und als PDF kompilieren, mit echter LaTeX-Mathe
(`$…$`, `$$…$$`, `align`, eigene Makros, beliebige Pakete). Vorschau, Fehlerliste, SyncTeX,
Historie und Auto-Compile funktionieren wie bei `.tex`.

### 11.1 Ansatz
Markdown → LaTeX → bestehende latexmk-Kette.

- Neues Crate **`lf-md`**: Es liest Markdown mit `pulldown-cmark` (Rust, MIT, mit Mathe-Erweiterung
  `ENABLE_MATH`) und schreibt `.arvidleaf/build/<name>.md.tex`. Die Quelldatei bleibt unverändert.
- Mathe wird **wörtlich** an LaTeX weitergegeben. Was in LaTeX kompiliert, kompiliert auch in Markdown.
- Rohes LaTeX im Markdown (z. B. `\newpage`, `\begin{figure}`) wird ebenfalls durchgereicht.
  Das gilt auch für Blöcke in ```` ```latex ````.
- **Kein pandoc:** Das wäre eine große zusätzliche Abhängigkeit, die hier nicht installiert ist.
  Außerdem ließen sich Zeilen schlecht zurück auf die `.md` abbilden.

### 11.2 Umfang (MVP)
| Markdown | LaTeX |
|---|---|
| `#`–`######` | `\section` … `\subparagraph` (bzw. `\chapter` bei `class: report/book`) |
| *kursiv*, **fett**, `code`, ~~durch~~ | `\emph`, `\textbf`, `\texttt` (escaped), `\sout` |
| Listen, verschachtelt, Aufgabenlisten | `itemize`/`enumerate` (Startnummer), `☐/☑` |
| Code-Blöcke mit Sprache | `listings` (Farben aus der Palette des PDF-Themes) |
| Tabellen (GFM) mit Ausrichtung | `tabular` + `booktabs` |
| Bilder `![Titel](pfad)` | `figure` + `\includegraphics` + `\caption` |
| Links, Fußnoten, Zitate (`>`) | `\href`, `\footnote`, `quote` |
| `[@knuth1984]` | `\cite{knuth1984}` (mit `bibliography:` im Front Matter) |
| `$…$`, `$$…$$` | `\(…\)`, `\[…\]` bzw. wörtlich bei Umgebungen |

**Front Matter** (YAML-Kopf, optional):
```yaml
---
title: Mein Dokument
author: arvid
class: article          # article | report | scrartcl …
lang: ngerman
packages: [siunitx, tikz]
preamble: |
  \newcommand{\R}{\mathbb{R}}
bibliography: refs.bib
engine: pdflatex
---
```

### 11.3 Integration
- **Projekte:** Eine `.md` mit Front Matter oder als gewählte Hauptdatei kann Hauptdatei sein.
  Gemischte Projekte (`.tex` bindet `\input{kapitel.md}` ein) kommen später.
- **Fehler und SyncTeX:** Der Konverter schreibt eine Zeilentabelle (`.tex`-Zeile → `.md`-Zeile).
  Fehler, Warnungen und Klicks im PDF zeigen auf die Markdown-Zeile.
- **Editor:** Hervorhebung für Markdown (Überschriften, Hervorhebung, Listen, Links, Code). Innerhalb
  von `$…$`/`$$…$$` gilt die LaTeX-Hervorhebung. Die Outline zeigt Überschriften, die Completion
  arbeitet in Mathe-Bereichen.
- **Templates:** Neue Vorlage „Markdown Note“ (Front Matter, Mathe-Beispiele).
- **Escaping:** Text-Sonderzeichen (`# % & _ { } ~ ^ \`) werden außerhalb von Mathe und Raw-LaTeX
  maskiert. Umlaute und Unicode laufen über `inputenc`/XeLaTeX wie bei `.tex`.

### 11.4 Meilensteine
| | Inhalt | Test |
|---|---|---|
| MD1 | `lf-md`: Konverter + Zeilentabelle, Front Matter | Golden-Tests (Markdown → erwartetes LaTeX), Escaping-Fälle |
| MD2 | Kompilieren über latexmk, Fehler/Warnungen auf `.md`-Zeilen | `--compile` mit absichtlichen Fehlern in Mathe und Text |
| MD3 | SyncTeX beide Richtungen über die Zeilentabelle | offscreen: Klick im PDF → richtige `.md`-Zeile |
| MD4 | Editor: Hervorhebung, Outline, Completion in Mathe | Lexer-Tests |
| MD5 | Vorlage, README, CHANGELOG | alle Vorlagen kompilieren |

### 11.5 Offene Fragen an den Nutzer
1. Soll die Hauptdatei eines Projekts eine `.md` sein dürfen, oder reicht zunächst „einzelne `.md` öffnen
   und kompilieren“?
2. Live-Vorschau gerenderter Formeln *im Editor* (z. B. beim Hovern über `$…$`): jetzt oder später?
   (Mein Vorschlag: später. Das PDF zeigt die Formeln ohnehin nach ~1 s.)
3. Export auch als `.tex` (Konverter-Ergebnis „herauslösen“, um in LaTeX weiterzuarbeiten)?

---

## 12. Editor-Kürzel (Wunsch 19.11.2026, umgesetzt)

Wunsch: `\begin{enumerate}` importiert direkt `enumitem`; `$` auf einer Auswahl setzt sie in den
Mathe-Modus; Vorschläge richten sich nach den bisherigen Eingaben und erscheinen schneller.

Entscheidungen:
- **Pakete automatisch:** Tabelle Umgebung/Befehl → Paket in `lf-tex/src/packages.rs`. Ausgelöst
  beim Einfügen über die Completion und bei Enter nach `\begin{…}` (nicht bei jedem Tastendruck).
  Geprüft werden alle `.tex`/`.sty` des Projekts inkl. indirekt geladener Pakete; `enumitem`,
  `geometry`, `hyperref` nie bei `beamer`. Ziel: Datei mit `\documentclass`, sonst Hauptdatei,
  sonst eine Präambel-Datei mit `\usepackage`. Offene Datei → Puffer, sonst direkt auf die Platte.
- **Auswahl umschließen:** `$` → `$…$`, `{`/`[` immer bei Auswahl, Strg+B/Strg+I → `\textbf`/`\textit`.
- **Vorschläge:** ab 1 Buchstabe; Rang = 3 × frühere Auswahl (`~/.config/arvidleaf/usage.json`,
  max. 2000 Einträge) + Häufigkeit in der Datei; eigene Befehle/Umgebungen der Datei kommen dazu.
  Popup ohne Fade-in. Ist der Präfix schon ein vollständiger Befehl, nur Verlängerungen zeigen
  (Enter bleibt Zeilenumbruch).
- **Tests:** Unit-Tests in `packages.rs`, `complete.rs`, `usage.rs`, Editor-Tests mit echten
  egui-Eingaben (`editor.rs`), offscreen-Durchlauf im Testordner `shortcuts/`.

---

## 13. Weitere Editor-Kürzel und Kürzel-Übersicht (Wunsch 19.11.2026, umgesetzt)

Wunsch: alle Vorschläge aus der Besprechung umsetzen, dazu eine Übersicht aller Kürzel.

Entscheidungen:
- **Auto-Pairing:** `(`, `[`, `{`, `$` erzeugen ein Paar, außer direkt danach steht ein Buchstabe,
  eine Ziffer oder `\`. `$` nur außerhalb von Mathe (gerade Anzahl unmaskierter `$` im Absatz
  davor), sonst schließt es. `\{` und `\$` bleiben einzeln. `\(` → `\(\)`, `\[` → `\[\]`,
  `\left(` / `\left[` / `\left\{` / `\left|` → passendes `\right…`. Auswahl + `(` umschließt.
- **Überschreiben/Backspace:** Selbst eingefügte Schließer werden als Stapel gemerkt; das gleiche
  Zeichen tippen springt darüber, Backspace auf einem leeren Paar löscht beide Teile.
- **Formatieren:** Strg+E `\emph{…}`, Strg+M `\(…\)`, Strg+Umschalt+M `\[…\]`.
- **Umgebung:** Strg+Umschalt+E setzt die Zeilen der Auswahl (eingerückt) in `\begin{}…\end{}`,
  Cursor im Namen. Wird der Name in `\begin{…}` geändert, folgt das passende `\end{…}`
  (verschachtelungssicher; nur wenn sich ausschließlich der Name geändert hat).
- **Listen:** Enter in einer `\item`-Zeile innerhalb `itemize`/`enumerate`/`description` beginnt
  `\item `; Enter auf einem leeren `\item` entfernt es und setzt den Cursor hinter `\end{…}`.
  Umschalt+Enter = normaler Zeilenumbruch. Neue Listen starten mit `\item `.
- **Zeilen:** Strg+D dupliziert, Alt+↑/↓ verschiebt, Strg+L markiert Zeile(n) (wiederholt: erweitern).
  Tab / Umschalt+Tab rücken mehrzeilige Auswahl um 2 Leerzeichen ein/aus (Umschalt+Tab auch ohne
  Auswahl für die aktuelle Zeile).
- **Tab-Stopps:** Nach einer Vorlage mit mehreren Argumenten (`\frac{}{}`, `\href{}{}`, Umgebungen
  mit Argumenten wie `tabular`, `minipage`) springt Tab zum nächsten Argument und zuletzt hinter
  das Ganze.
- **Strg+Klick:** auf `\ref`-artige Befehle → `\label`, auf `\input`/`\include`/`\includegraphics`
  → Datei, auf `\cite` → Eintrag in der `.bib`. Bei gedrückter Strg-Taste Hand-Cursor über Links.
- **Übersicht:** Eine Tabelle (`lf-gui/src/shortcuts.rs`) ist die einzige Quelle; Dialog mit
  Suchfeld und Gruppen über F1, Strg+? und Help/Edit-Menü. Vim-Modus: die Editier-Kürzel gelten
  nur im Insert-Modus.
- **Tests:** Editor-Tests mit echten egui-Eingaben je Kürzel, offscreen-Screenshot der Übersicht,
  Testprojekt `shortcuts/` im Testordner.

---

## 14. Installation auf Linux, Windows, macOS; Datei-Kompilierung per CLI (Wunsch 19.11.2026, umgesetzt)

- **Linux:** `install.sh` mit `--update`, `--system`, `--dev`, zypper; Rust ≥ 1.95 per rustup.
  Ein gemeinsamer Generator `packaging/desktop-entry.sh` liefert den Starter für Paket und Dev-Build.
- **Windows:** `install.ps1` (PowerShell 5.1, ASCII). winget installiert MiKTeX (mit AutoInstall
  fehlender Pakete), Strawberry Perl (für latexmk), Poppler (bin-Ordner wird bei Bedarf in den PATH
  eingetragen), rustup und die VS C++ Build Tools. Installiert wird pro Nutzer ohne Admin:
  Startmenü, .ico aus dem PNG, „Öffnen mit“, Deinstallation über *Installierte Apps*.
- **macOS:** `install-macos.sh` mit Homebrew (MacTeX ohne GUI-Apps oder BasicTeX + tlmgr), baut
  `arvidleaf.app` (Info.plist, icns via sips/iconutil, Ad-hoc-Signatur), dazu einen Symlink nach
  `~/.local/bin`.
- **Code:** systemübliche Pfade, `.exe`-Suche, `open`/`explorer`, `taskkill`, CREATE_NO_WINDOW,
  `windows_subsystem` mit AttachConsole für CLI-Modi, PATH-Ergänzung unter macOS.
  Geprüft per `cargo check --target x86_64-pc-windows-gnu` und `aarch64-apple-darwin`.
- **CLI:** `--compile <datei.md|datei.tex>`, PDF daneben oder per `-o`.
- **Offen:** Test auf echten Windows- und macOS-Rechnern.

---

## 15. PLAN: arvidref – Literaturverwaltung in arvidleaf (Entwurf 24.11.2026, Entscheidungen 15.8 getroffen)

**Ziel:** Quellen verwalten wie mit Zotero, aber direkt im Editor. Eine Quelle wird per DOI geholt,
mit einem Kürzel zitiert und landet automatisch in der `.bib` des Projekts. Den Programmwechsel
und das Kopieren von BibTeX gibt es dann nicht mehr.

**Bewusst nicht im Umfang** (das, was Zotero groß macht):
- **Seiten-Übersetzer und Browser-Connector:** Tausende Scraper, eine Daueraufgabe. Ersatz: Import per Kennung (DOI, arXiv, ISBN).
- **Cloud-Sync und Gruppen:** R1 (lokal). Wer synchronisieren will, legt die Bibliothek in einen eigenen Sync- oder git-Ordner.
- **Word/LibreOffice-Plugins und CSL-Formatierung:** Bei LaTeX formatiert biblatex/biber.

### 15.1 Anforderungen

| # | Anforderung | Umsetzung (Kurzfassung) |
|---|---|---|
| Q1 | **Eine zentrale Bibliothek** für alle Projekte | `<data_dir>/arvidref/library.bib` (über `lf_core::paths::data_dir()`), Ort in den Settings änderbar |
| Q2 | **Offenes Format** | Die Bibliothek *ist* eine BibLaTeX-Datei. Lesbar, mit git versionierbar, mit JabRef/Zotero austauschbar |
| Q3 | **Zitieren aus dem Editor** | Zitier-Dialog (Strg+Umschalt+C) und `\cite`-Completion über die Bibliothek. Der Eintrag wird in die Projekt-`.bib` übernommen |
| Q4 | **Import per Kennung (online)** | DOI, arXiv-ID, ISBN; Netz nur bei diesem Schritt (vom Nutzer freigegeben 24.11.2026) |
| Q5 | **Import offline** | BibTeX/BibLaTeX einfügen oder Datei öffnen, CSL-JSON (Zotero-Export), PDF per Drag & Drop (DOI aus dem Text) |
| Q6 | **PDF-Anhänge** | Pro Eintrag beliebig viele Dateien, Öffnen im eingebauten PDF-Viewer |
| Q7 | **Ordnen** | Tags, Sammlungen, Volltextsuche über Autor/Titel/Jahr/Key/Journal/Notiz |
| Q8 | **Später eigenständig** | Eigene Crate ohne GUI-Abhängigkeit, damit ein `arvidref`-Programm ohne Umbau möglich ist |

### 15.2 Architektur

Neue Crate **`lf-ref`** (keine egui-Abhängigkeit):

```
crates/lf-ref/src/
├── bib/
│   ├── parse.rs       BibLaTeX-Parser mit Byte-Spannen je Eintrag (verlustfreies Zurückschreiben)
│   ├── write.rs       Formatierer für neue/geänderte Einträge
│   └── latex.rs       {\"a} ↔ ä, --, ~ … für Anzeige und Suche
├── model.rs           Entry { key, kind, fields: IndexMap }, Person-Namen (von/Last/Jr/First)
├── library.rs         Laden, Speichern (atomar: tmp + rename), Index, Suche, Dubletten
├── key.rs             Key-Erzeugung und Konfliktauflösung
├── import/
│   ├── doi.rs         doi.org mit Content-Negotiation → CSL-JSON
│   ├── arxiv.rs       export.arxiv.org Atom-API
│   ├── isbn.rs        OpenLibrary
│   ├── csl.rs         CSL-JSON → Entry (auch für Zotero-Exporte)
│   └── pdf.rs         pdftotext (Seite 1–2) → DOI/arXiv-ID per Regex
└── project.rs         Projekt-.bib finden, Einträge übernehmen/abgleichen
```

`lf-gui` bekommt `refs_ui.rs` (Bibliotheksansicht, Zitier-Dialog). `lf-tex::outline::bib_keys`
wird durch den neuen Parser ersetzt, damit Projekt und Bibliothek dieselbe Logik nutzen.

**Netz:** `ureq` (synchron, rustls, MIT/Apache) in einem Worker-Thread wie der Compile-Runner,
Timeout 10 s, Abbrechen möglich. User-Agent `arvidleaf/<version>` **ohne E-Mail-Adresse**
(Crossrefs „polite pool“ wünscht eine; optional in den Settings eintragbar, Standard leer).
Ohne Netz funktioniert alles außer Q4. Der Fehler wird klar angezeigt („Offline – DOI-Import nicht möglich“).

**Warum CSL-JSON per doi.org:** Ein Aufruf mit `Accept: application/vnd.citationstyles.csl+json`
deckt Crossref, DataCite (Datensätze, Zenodo) und mEDRA ab. Die Umsetzung in BibLaTeX liegt
bei uns, statt das oft fehlerhafte BibTeX der Anbieter zu übernehmen (z. B. Titel in Großbuchstaben, `&amp;`).

### 15.3 Datenmodell und Speicherung

```
<data_dir>/arvidref/
├── library.bib            alle Einträge (Quelle der Wahrheit)
├── collections.toml       Sammlungen: Name → Liste von Keys (Bäume über "Eltern/Kind")
└── files/<key>/…pdf       Anhänge (kopiert, nicht verlinkt)
```

- **Tags** im Standardfeld `keywords`, **Anhänge** im Feld `file` (Format wie JabRef/Better BibTeX).
  **Notizen** in `annotation`. **Zeitstempel** in `x-arvidref-added`/`x-arvidref-modified`
  (biber ignoriert unbekannte Felder).
- **Verlustfrei:** Unveränderte Einträge werden Byte für Byte zurückgeschrieben. Kommentare,
  `@string`, `@preamble` und die Reihenfolge bleiben erhalten. Damit erzeugt die Bibliothek
  unter git nur echte Diffs.
- **Key-Schema** (Standard, in den Settings änderbar): `<nachname><jahr><erstes Titelwort>`,
  kleingeschrieben, ASCII (`müller2021neuronale`). Bei Konflikten werden `a`, `b` … angehängt.
  Stoppwörter (the, a, der, die, …) werden übersprungen.
- **Dubletten:** gleiche DOI, gleiche arXiv-ID oder (normalisierter Titel + Jahr). Beim Import gibt es den Hinweis
  „Already in library as `knuth1984texbook`“ statt eines zweiten Eintrags.
- Änderungen von außen (Editor, git pull) werden über `notify` erkannt und neu geladen.

### 15.4 Zitieren im Projekt

- **Projekt-`.bib`:** Gemeint ist die erste Datei aus `\addbibresource`/`\bibliography` bzw. `bibliography:` im
  Markdown-Front-Matter. Gibt es keine, wird beim ersten Zitat `refs.bib` angelegt. Dann folgt eine
  Frage, ob `\addbibresource{refs.bib}` + `\printbibliography` eingefügt werden soll.
- **Einträge werden kopiert, nicht verlinkt.** Das Projekt bleibt eigenständig (ZIP-Export,
  Overleaf, Koautoren) und kompiliert ohne die Bibliothek.
- **Zitier-Dialog** (Strg+Umschalt+C, auch Menü *Insert → Citation…*): unscharfe Suche, Liste mit
  Autor · Jahr · Titel · Journal, Mehrfachauswahl. Enter fügt `\cite{a,b}` ein, im Markdown `[@a; @b]`.
  Steht der Cursor in einem bestehenden `\cite{…}`, werden die Keys dort ergänzt. Der Befehl
  (`\cite`, `\parencite`, `\textcite`, `\autocite`) ist im Dialog wählbar und wird pro Projekt gemerkt.
- **Completion:** In `\cite{` erscheinen zuerst die Projekt-Keys, darunter die Bibliotheks-Keys
  (markiert „library“). Die Auswahl eines Bibliotheks-Keys übernimmt den Eintrag ins Projekt.
- **Hover** über einem Key zeigt Autor, Jahr und Titel. Der bestehende Strg+Klick-Sprung zur `.bib` bleibt.
- **Nach dem Kompilieren:** Bei „Citation `x` undefined“ zeigt das Log-Panel einen Knopf
  „Add from library“, wenn `x` in der Bibliothek existiert.
- **Abgleich** (*References → Sync project .bib*): Projekt-Einträge, die in der Bibliothek geändert
  wurden, werden als Diff angezeigt (vorhandene Diff-Ansicht der History) und auf Wunsch übernommen.
  Umgekehrt lassen sich Einträge, die nur im Projekt stehen, in die Bibliothek übernehmen.

### 15.5 GUI

Eigene Ansicht **References** (Menü *View → References*, Strg+Umschalt+R). Sie ersetzt den
Mittelbereich wie das Dashboard, die Menü- und Statusleiste bleiben.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 🍃 arvidleaf  File Edit View References …   [ 10.1145/359545.359563   ⏎ Add ] │
├──────────────┬───────────────────────────────────────┬──────────────────────┤
│ LIBRARY  412 │ 🔍 knuth                                │ Article              │
│ ▾ Thesis     │ Author        Year  Title               │ Key   knuth1984lit…  │
│    Kapitel 2 │ Knuth, D. E.  1984  Literate Programm…  │ Title Literate Pro…  │
│ ▸ Lehre      │ Knuth, D. E.  1986  The TeXbook       📎│ Author Knuth, Don…   │
│ Unfiled   37 │ Lamport, L.   1994  LaTeX: A Document…  │ Year  1984           │
│──────────────│                                        │ DOI   10.1093/…  ↗   │
│ TAGS         │                                        │ ── Files ──          │
│ #ml #survey  │                                        │ 📎 knuth1984.pdf     │
│              │                                        │ [Cite] [Copy BibTeX] │
├──────────────┴───────────────────────────────────────┴──────────────────────┤
│ ● 412 entries · library.bib                       Imported 1 entry (Crossref) │
└─────────────────────────────────────────────────────────────────────────────┘
```

- Spalten sortierbar, Tabelle virtualisiert (nur sichtbare Zeilen), auch für 10 000 Einträge flüssig.
- Detailansicht: Felder je nach Eintragstyp (Pflichtfelder hervorgehoben), Rohansicht als BibLaTeX
  mit dem vorhandenen Highlighter umschaltbar. Änderungen speichern sofort (Auto-Save wie im Editor).
- Das Eingabefeld oben erkennt DOI (auch als URL `https://doi.org/…`), arXiv-ID/-URL und ISBN.
  Eingefügtes BibTeX wird direkt importiert. Drag & Drop von PDFs oder `.bib`/`.json`-Dateien auf die Ansicht importiert sie.
- Kontextmenü: Cite, Copy key, Copy BibTeX, Open PDF, Open DOI, Add to collection, Delete
  (mit Rückgängig über einen Snapshot der `library.bib`, analog zur Versionshistorie).
- Stil: Palette und Abstände aus `theme.rs`, UI-Sprache Englisch.

### 15.6 Meilensteine

| | Inhalt | Test |
|---|---|---|
| RF1 | `lf-ref`: Parser/Writer, Modell, LaTeX-Zeichen, Namen | Roundtrip: echte `.bib`-Dateien (TeX-Live-`biblatex-examples.bib`, Zotero/BBT-Export, unsaubere Dateien mit `@string`, `#`, verschachtelten Klammern, `(…)`-Einträgen) bleiben Byte für Byte gleich |
| RF2 | Bibliothek: Laden/Speichern atomar, Index, Suche, Keys, Dubletten, Sammlungen | Unit-Tests, 10 000 Einträge laden + suchen < 100 ms |
| RF3 | Import: BibTeX, CSL-JSON, PDF (pdftotext), DOI/arXiv/ISBN online | Offline-Fixtures (gespeicherte Antworten) für die Umsetzung; ein mit `#[ignore]` markierter Online-Test gegen echte DOIs |
| RF4 | GUI: References-Ansicht, Detail-Editor, Drag & Drop, Anhänge im PDF-Viewer | Offscreen-Screenshots dark/light |
| RF5 | Editor-Integration: Zitier-Dialog, Completion, Hover, Übernahme in Projekt-`.bib`, „Add from library“ im Log | Testprojekt in `~/Documents/arvidleaf/test`: zitieren → kompilieren → Literaturverzeichnis enthält die Einträge (biber) |
| RF6 | Abgleich Projekt ↔ Bibliothek, README, CHANGELOG | Diff-Fälle: geändert, nur Projekt, nur Bibliothek, Key-Konflikt |

### 15.7 Risiken

| Risiko | Gegenmaßnahme |
|---|---|
| `.bib`-Dateien aus der Praxis sind unsauber | Fehlertoleranter Parser: Ein kaputter Eintrag wird als Rohtext erhalten und markiert, der Rest lädt. Fixture-Sammlung. |
| Metadaten der Anbieter sind lückenhaft oder falsch | Vor dem Speichern Vorschau mit editierbaren Feldern. Fehlende Pflichtfelder werden markiert. |
| Gleichzeitige Änderung durch arvidleaf und externen Editor | `notify`, beim Speichern Prüfung der Änderungszeit, bei Konflikt Nachfrage statt Überschreiben |
| Datenverlust in der zentralen Bibliothek | Atomares Speichern, Snapshots der `library.bib` über den vorhandenen History-Speicher |
| Rate-Limits/Ausfälle der APIs | Klare Fehlermeldung, kein automatisches Wiederholen in Schleifen |

### 15.8 Stand IMPLEMENT-Phase

| Meilenstein | Stand |
|---|---|
| RF1 Parser/Writer, LaTeX-Zeichen, Namen | erledigt (24.11.2026): 18 Tests, Roundtrip über den TeX-Live-Bestand (83 Dateien, 17 270 Einträge, 0,3 s). Die 2 dort gemeldeten Fehler sind echte Fehler in den Dateien (`frankenstein.bib`: unbalancierte Klammer, `publ-imp-test.bib`: fehlendes Komma). Biber-Gegenprobe: `~/Documents/arvidleaf/test/arvidref-rf1`. `lf_tex::outline::bib_keys` ersetzt. |
| RF2 Bibliothek | erledigt (25.11.2026): atomares Speichern mit Konflikterkennung, tägliche Sicherung, Suche, Dubletten (DOI/arXiv/Titel+Jahr), Keys nach Muster, Tags, Sammlungen. 10 000 Einträge öffnen in 72 ms (Release, Index auf mehreren Threads), Suche < 15 ms. Nicht in RF2: Beobachten per `notify` (kommt mit der GUI in RF4, `changed_on_disk()` ist vorbereitet), Ort der Bibliothek in den Settings (RF4). |
| RF3–RF7 | offen |

### 15.9 Entscheidungen (24.11.2026)

Der Nutzer hat die offenen Fragen delegiert („Mach, was du für richtig hältst“). Entschieden wurde:

| Frage | Entscheidung | Begründung |
|---|---|---|
| Projekt-`.bib` | **Einträge werden ins Projekt kopiert**, kein Verweis auf die zentrale Bibliothek | Projekte bleiben eigenständig (ZIP-Export, Overleaf, Koautoren). Aktualität über *Sync project .bib* (15.4). |
| Key-Schema | **`müller2021neuronale` → `mueller2021neuronale`** (Nachname + Jahr + erstes Titelwort, klein, ASCII, Umlaute als ae/oe/ue/ss). In den Settings als Muster änderbar | Kollidiert seltener als `Mueller2021`, bleibt lesbar. Die Umschrift von ä zu ae entspricht Better BibTeX. |
| Ansicht | **Eigene Vollansicht** wie das Dashboard, dazu der Zitier-Dialog als Overlay im Editor | Tabelle und Detailansicht brauchen Breite. Zum Zitieren muss die Ansicht nicht gewechselt werden. |
| Zotero-Bestand | **Import über Zotero-Export** (BibTeX/BBT oder CSL-JSON) in RF3. Ein direkter Import aus `zotero.sqlite` (mit PDFs und Sammlungen) folgt als **RF7, nach RF1–RF6** | Das Schema von `zotero.sqlite` ändert sich zwischen Versionen. Der Export reicht für den Umstieg. |
| PDF-Annotationen | **Später**, nicht in RF1–RF6 | Braucht Text-/Koordinatenauswahl im PDF-Viewer, ein eigenes Projekt. Notizen pro Eintrag (`annotation`) gibt es ab RF4. |

| | Inhalt | Test |
|---|---|---|
| RF7 (nach RF6) | Import direkt aus `zotero.sqlite` (nur lesend, Kopie der DB), Sammlungen und PDF-Anhänge | Fixture-DB aus einer Test-Zotero-Installation |
