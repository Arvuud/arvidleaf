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
| **Später** | Echtzeit-Kollaboration, visueller Editor, KI-Fehlererklärung, Zotero-Anbindung |

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
