# arvidleaf

Ein lokaler LaTeX-Editor als Desktop-App, im Funktionsumfang an Overleaf
angelehnt und im Aussehen an arvidlab. Geschrieben in Rust (egui); kompiliert
wird mit der installierten TeX-Live-Distribution über `latexmk` – ohne Zeit-
oder Seitenlimits.

Version 0.1.0 – Prototyp. Änderungen: `CHANGELOG.md`. Plan und Anforderungen: `reqs/`.

## Voraussetzungen

| Werkzeug | Zweck | Arch-Paket |
|---|---|---|
| `latexmk`, `pdflatex` | Kompilierung | `texlive-binextra`, `texlive-basic` |
| `xelatex`, `lualatex` | weitere Engines (optional) | `texlive-xetex`, `texlive-luatex` |
| `pdftoppm`, `pdfinfo` | PDF-Vorschau | `poppler` |
| `synctex` | Sprung PDF ↔ Quelltext | `texlive-binextra` |
| `bibtex` / `biber` | Literatur | `texlive-bibtexextra` / `biber` |
| babel-Sprachen | z. B. Deutsch | `texlive-langgerman` |
| `pdftotext` | DOI aus importierten PDFs lesen (optional) | `poppler` |
| `curl` | Literatur per DOI, arXiv-ID oder ISBN holen (optional) | `curl` |

*Help › System Check* zeigt, was fehlt.

## Installieren und starten

Jedes Skript installiert die Werkzeuge (TeX, latexmk, biber, poppler, Rust ≥ 1.95), baut die
Release-Version und legt Programm, Icon und Starter an. `--update` baut nur neu und erneuert den
Starter, z. B. nach `git pull`.

**Linux** (pacman, apt, dnf, zypper): Programm in `~/.local`, Eintrag im App-Menü

```sh
./install.sh              # alles (sudo nur für die Systempakete)
./install.sh --update     # neu bauen und installieren, Starter erneuern
./install.sh --system     # für alle Nutzer nach /usr/local
./install.sh --dev        # App-Menü startet den Build dieses Quellordners
./install.sh --uninstall
```

**Windows 10/11** (winget: MiKTeX, Strawberry Perl, Poppler, rustup, VS C++ Build Tools):
Programm in `%LOCALAPPDATA%\Programs\arvidleaf`, Startmenü, „Öffnen mit“ für `.tex/.bib/.md`,
Eintrag unter *Installierte Apps*

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1            # alles
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Update    # neu bauen
powershell -ExecutionPolicy Bypass -File .\install.ps1 -DesktopShortcut
powershell -ExecutionPolicy Bypass -File .\install.ps1 -Uninstall
```

**macOS 12+** (Homebrew: MacTeX ohne GUI-Apps oder mit `--small-tex` BasicTeX, poppler; rustup):
`arvidleaf.app` in `~/Applications`, Befehl `arvidleaf` in `~/.local/bin`

```sh
./install-macos.sh            # alles
./install-macos.sh --update   # neu bauen
./install-macos.sh --system   # nach /Applications
./install-macos.sh --uninstall
```

Die Windows- und macOS-Skripte sind bisher nur gegen den Code geprüft (Cross-Check des
Builds), nicht auf echten Geräten gelaufen.

Direkt aus dem Quellbaum:

```sh
cargo build --release
./target/release/arvidleaf                 # Startseite
./target/release/arvidleaf ~/Docs/paper    # Projektordner öffnen
./target/release/arvidleaf paper.zip       # ZIP importieren (z. B. Overleaf-Download)
./target/release/arvidleaf --screenshot out.png [--size 1440x900] [--light] [--key 5:Ctrl+H] [dir]
```

### Ohne Oberfläche kompilieren

```sh
arvidleaf --compile notizen.md              # → notizen.pdf daneben
arvidleaf --compile kapitel.tex -o out.pdf  # PDF an anderen Ort
arvidleaf --compile ~/Docs/paper            # ganzes Projekt (Hauptdatei automatisch)
```

Fehler und Warnungen kommen mit Datei und Zeile auf die Konsole. Exit-Code 0 bedeutet: PDF ohne Fehler.
Build-Dateien liegen in `.arvidleaf/` neben der Quelle.

Ein installierbares Linux-Paket baut `packaging/build-package.sh`
(`dist/arvidleaf-<version>-linux-<arch>.tar.gz`, darin `install.sh`).

## Funktionen

- **Projekte** sind normale Ordner. Einstellungen, Build-Ausgabe und Historie liegen in
  `<projekt>/.arvidleaf/`; der Quellordner bleibt sauber.
- **Editor**: LaTeX-Farbhervorhebung (Befehle, Umgebungen, Mathematik, Kommentare,
  Argumente, Optionen, Sonderzeichen), Zeilenumbruch, Zeilennummern mit Fehler-/Warnungs-
  markern, Autovervollständigung (Befehle, Umgebungen, `\ref`-Labels, `\cite`-Schlüssel,
  Dateien), automatisches Schließen von `{}`/`[]` und `\begin…\end`, Suchen & Ersetzen,
  Kommentar umschalten, Auto-Save, optional Vim-Modus.
- **Kompilieren**: pdfLaTeX, XeLaTeX oder LuaLaTeX über `latexmk`, im Hintergrund,
  abbrechbar, ohne Timeout. `\usepackage` nutzt die ganze TeX-Live-Installation; eigene
  `.sty`/`.cls`/`.bst`/`.bib` werden im ganzen Projekt (auch in Unterordnern) gefunden.
  Fehler und Warnungen erscheinen mit Datei und Zeile; fehlende Pakete mit Installationshinweis.
- **PDF-Vorschau**: Es werden nur sichtbare Seiten gerendert (auch 500+ Seiten bleiben
  flüssig), Zoom, Seitenbreite/ganze Seite, Doppelklick springt in den Quelltext, Strg+J
  vom Quelltext ins PDF (SyncTeX).
- **Templates**: Galerie mit Vorschaubildern, eingebaute Vorlagen und eigene Vorlagen
  (siehe unten).
- **Versionshistorie**: Snapshots nach jedem erfolgreichen Kompilieren, alle 10 Minuten
  und von Hand (mit Label); Diff je Datei; Datei oder ganzes Projekt wiederherstellen.
- **Import/Export**: ZIP (Overleaf-kompatibel), PDF exportieren, Dateien per Drag & Drop
  hinzufügen.
- **Literaturverwaltung (arvidref)**, Strg+Umschalt+R: eine zentrale Bibliothek für alle
  Projekte als normale BibLaTeX-Datei (`~/.local/share/arvidleaf/arvidref/library.bib`,
  verlegbar). Einträge kommen per DOI, arXiv-ID, ISBN, BibTeX, CSL-JSON (Zotero-Export) oder
  PDF per Drag & Drop hinein. Ordnen über Sammlungen und Tags, Suche, Editor mit
  Pflichtfeldern, PDF-Anhänge im eingebauten Viewer. Zitieren aus dem Editor mit
  Strg+Umschalt+C oder über die Vervollständigung in `\cite{`: Der Eintrag wird in die
  `.bib` des Projekts kopiert (fehlt sie, entsteht `refs.bib` samt biblatex-Einrichtung).
  Das Projekt bleibt damit ohne die Bibliothek kompilierbar.

## Eigene Templates

Jeder Ordner in `~/.local/share/arvidleaf/templates/` ist eine Vorlage. Anlegen geht über
*Templates › Manage Templates…* (aus dem aktuellen Projekt oder einem Ordner) oder von Hand:

```toml
# template.toml
name = "Mein Bericht"
category = "Berichte"
description = "Firmenlayout mit Logo"
main = "main.tex"
engine = "pdflatex"   # pdflatex | xelatex | lualatex
order = 0
```

In Textdateien werden `{{title}}`, `{{author}}`, `{{date}}` und `{{project}}` beim Anlegen
eines Projekts ersetzt. Ein optionales `preview.png` dient als Galeriebild, sonst wird die
erste Seite gerendert. Eingebaute Vorlagen (Ordner `templates/` im Quellcode, beim Bauen
eingebettet) lassen sich ausblenden oder als eigene Kopie bearbeiten.

## Tastenkürzel

Alle Kürzel zeigt die App unter **F1** (oder Strg+?, Help → Keyboard Shortcuts) mit Suchfeld.
Die Tabelle dafür steht in `crates/lf-gui/src/shortcuts.rs`.

| Taste | Aktion |
|---|---|
| F1, Strg+? | Übersicht aller Kürzel |
| Strg+S | alles speichern und kompilieren |
| Strg+Enter / Strg+. | kompilieren / abbrechen |
| Strg+J, Doppelklick im PDF | SyncTeX vorwärts / rückwärts |
| Strg+F, Strg+Umschalt+F | suchen in Datei / im Projekt |
| Strg+P | Datei öffnen |
| Strg+Klick | `\ref` → `\label`, `\cite` → `.bib`-Eintrag, `\input`/`\includegraphics` → Datei |
| Strg+/ | Kommentar umschalten |
| Strg+B / Strg+I / Strg+E | `\textbf{…}` / `\textit{…}` / `\emph{…}` (ohne Auswahl: leeres Paar) |
| Strg+M / Strg+Umschalt+M | `\(…\)` / `\[…\]` |
| Strg+Umschalt+E | Zeilen in `\begin{}…\end{}` setzen, Cursor im Namen |
| `(` `[` `{` `$` | erzeugen ein Paar (nicht direkt vor einem Wort; `$` nicht innerhalb von Mathe); das Schließzeichen tippen springt darüber, Backspace löscht ein leeres Paar |
| `\(` `\[` `\left(` | fügen `\)` `\]` `\right)` dazu |
| `$` `(` `[` `{` bei Auswahl | Auswahl umschließen: `x+y` → `$x+y$` |
| Name in `\begin{…}` ändern | das passende `\end{…}` ändert sich mit |
| Enter in einer Liste | neues `\item`; auf leerem `\item`: Liste beenden (Umschalt+Enter: normaler Umbruch) |
| Tab nach einer Vorlage | ins nächste Argument (`\frac{…}{…}`, `tabular`-Spalten), zuletzt dahinter |
| Strg+D, Alt+↑/↓, Strg+L | Zeile duplizieren / verschieben / markieren |
| Tab / Umschalt+Tab | markierte Zeilen ein- / ausrücken |
| Strg+H | Historie |
| Strg+Umschalt+N, Strg+O | neues Projekt / Ordner öffnen |
| Strg+Umschalt+C | Zitat aus der Bibliothek einfügen (im `\cite{…}`: Keys ergänzen) |
| Strg+Umschalt+R | Literaturverwaltung ein/aus; dort Strg+V fügt DOI/BibTeX aus der Zwischenablage ein, Entf/Strg+Z löschen/rückgängig |

Im Vim-Modus gelten die Editier-Kürzel im Insert-Modus.

### Automatische Pakete und Vorschläge

- Wer eine Umgebung oder einen Befehl einfügt, der ein Paket braucht, bekommt das `\usepackage`
  automatisch in die Präambel – z. B. `\begin{enumerate}` → `enumitem`, `align` → `amsmath`,
  `\toprule` → `booktabs`, `\includegraphics` → `graphicx`. Die Zeile kommt hinter das letzte
  `\usepackage` der Hauptdatei (oder der Datei mit `\documentclass`), auch wenn gerade ein
  Kapitel per `\input` bearbeitet wird. Bereits geladene Pakete (auch indirekt, z. B. `amsmath`
  durch `mathtools`, oder durch `beamer`) werden nicht doppelt eingetragen.
- Vorschläge erscheinen ab dem ersten Buchstaben nach `\`. Oben stehen Befehle und Umgebungen,
  die in der Datei schon vorkommen (auch eigene wie `\vect`), und solche, die man früher oft
  gewählt hat (`~/.config/arvidleaf/usage.json`). Ist der getippte Befehl schon vollständig
  (`\int`), bleibt Enter ein Zeilenumbruch.

### Vim-Modus

*Edit › Vim Mode* schaltet Vim-Tastenbelegung im Editor ein (bleibt gespeichert). Unterstützt
werden Normal-, Insert-, Visual- und Visual-Line-Modus, Zähler, Bewegungen (`w b e $ gg G f t %
{ }` …), Operatoren `d c y > <` mit Textobjekten, darunter für LaTeX `i$`/`a$` (Inline-Mathe)
und `ie`/`ae` (Umgebung), dazu `u`, Strg+R, `.`, `/ ? n N * #`, `:w` (speichern und
kompilieren), `:<zeile>`, `:%s/alt/neu/g` (wörtlich). Der aktuelle Modus steht über dem Editor.

## Aufbau

| Crate | Inhalt |
|---|---|
| `lf-tex` | LaTeX-Tokenizer, Log-Parser, Compile-Runner (latexmk), SyncTeX, Outline, Vervollständigung, Werkzeugprüfung |
| `lf-pdf` | Seitengrößen (`pdfinfo`), Seitenrendering (`pdftoppm`, PPM über stdout) mit Worker-Pool |
| `lf-ref` | arvidref ohne GUI: verlustfreier BibLaTeX-Parser, Bibliothek, Keys, Sammlungen, Import (BibTeX, CSL-JSON, PDF, DOI/arXiv/ISBN über curl) |
| `lf-core` | Projekte, Einstellungen, Templates, Versionshistorie (inhaltsadressiert, dedupliziert), ZIP |
| `lf-gui` | egui-Oberfläche (`vim.rs`: Vim-Emulation); `theme.rs` stammt aus arvidlab |
| `arvidleaf` | Programm (GUI, `--compile`, `--screenshot`) |

## Tests

```sh
cargo test --workspace                                         # Unit- und Integrationstests
cargo test -p lf-core --test integration -- --include-ignored  # inkl. 520-Seiten-Dokument
```

Die Integrationstests kompilieren echte Projekte mit TeX Live: alle Engines, eigene
Pakete in Unterordnern, BibTeX, Pfade mit Umlauten, fehlende Pakete, SyncTeX, alle
eingebauten Templates, Overleaf-ZIP-Import und die Render-Zeit der ersten Seite.

## Lizenz

MIT OR Apache-2.0
