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

*Help › System Check* zeigt, was fehlt.

## Installieren und starten

```sh
./install.sh             # Abhängigkeiten (sudo), Build, Programm + Icon + App-Menü-Eintrag
./install.sh --no-deps   # ohne Systempakete
./install.sh --uninstall
```

Danach startet arvidleaf aus dem App-Menü oder mit `arvidleaf`. Direkt aus dem Quellbaum:

```sh
cargo build --release
./target/release/arvidleaf                 # Startseite
./target/release/arvidleaf ~/Docs/paper    # Projektordner öffnen
./target/release/arvidleaf paper.zip       # ZIP importieren (z. B. Overleaf-Download)
./target/release/arvidleaf --compile dir   # ohne GUI kompilieren, Probleme ausgeben
./target/release/arvidleaf --screenshot out.png [--size 1440x900] [--light] [--key 5:Ctrl+H] [dir]
```

Für die Entwicklung trägt `./install-desktop.sh` den Build ins App-Menü ein.
Ein installierbares Paket baut `packaging/build-package.sh`
(`dist/arvidleaf-<version>-linux-<arch>.tar.gz`, darin `install.sh`).

## Funktionen

- **Projekte** sind normale Ordner. Einstellungen, Build-Ausgabe und Historie liegen in
  `<projekt>/.arvidleaf/`; der Quellordner bleibt sauber.
- **Editor**: LaTeX-Farbhervorhebung (Befehle, Umgebungen, Mathematik, Kommentare,
  Argumente, Optionen, Sonderzeichen), Zeilenumbruch, Zeilennummern mit Fehler-/Warnungs-
  markern, Autovervollständigung (Befehle, Umgebungen, `\ref`-Labels, `\cite`-Schlüssel,
  Dateien), automatisches Schließen von `{}`/`[]` und `\begin…\end`, Suchen & Ersetzen,
  Kommentar umschalten, Auto-Save.
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

| Taste | Aktion |
|---|---|
| Strg+S | alles speichern und kompilieren |
| Strg+Enter / Strg+. | kompilieren / abbrechen |
| Strg+J, Doppelklick im PDF | SyncTeX vorwärts / rückwärts |
| Strg+F, Strg+Umschalt+F | suchen in Datei / im Projekt |
| Strg+P | Datei öffnen |
| Strg+/ | Kommentar umschalten |
| Strg+H | Historie |
| Strg+Umschalt+N, Strg+O | neues Projekt / Ordner öffnen |

## Aufbau

| Crate | Inhalt |
|---|---|
| `lf-tex` | LaTeX-Tokenizer, Log-Parser, Compile-Runner (latexmk), SyncTeX, Outline, Vervollständigung, Werkzeugprüfung |
| `lf-pdf` | Seitengrößen (`pdfinfo`), Seitenrendering (`pdftoppm`, PPM über stdout) mit Worker-Pool |
| `lf-core` | Projekte, Einstellungen, Templates, Versionshistorie (inhaltsadressiert, dedupliziert), ZIP |
| `lf-gui` | egui-Oberfläche; `theme.rs` stammt aus arvidlab |
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
