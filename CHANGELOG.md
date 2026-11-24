# Changelog

## Unveröffentlicht – arvidref (Literaturverwaltung), Meilenstein RF1

- **Neue Crate `lf-ref`** (Grundlage für arvidref, siehe `reqs/plan.md` Abschnitt 15):
  - BibLaTeX-Parser, der nichts verliert: Unveränderte Dateien werden Byte für Byte
    zurückgeschrieben (geprüft an allen 83 `.bib`-Dateien von TeX Live, 17 270 Einträge,
    13,7 MB in 0,3 s). Geänderte Einträge werden einzeln neu formatiert, der Rest bleibt unberührt.
  - Fehlertolerant: Ein kaputter Eintrag wird mit Zeile und Grund gemeldet und als Text
    erhalten. Die folgenden Einträge werden trotzdem gelesen. `@string`, `#`, `(…)`-Einträge,
    `%`-Kommentare, `@comment`, `@preamble` und CRLF werden unterstützt.
  - LaTeX ↔ Unicode (`M{\"u}ller` → Müller, `--` → –), Umschrift nach ASCII für Keys
    (Müller → Mueller) und BibTeX-Namenslisten (`von Last, Jr, First`, `and others`).
- `\cite`-Vervollständigung und Strg+Klick auf einen Zitier-Key nutzen den neuen Parser.
  Keys in `@comment` oder in kaputten Einträgen werden nicht mehr vorgeschlagen.

## Unveröffentlicht – Installation auf Linux, Windows und macOS

- **Klammer-Paare und Vorschlagsauswahl unter KDE Wayland repariert:** winit schickte nach
  jeder Cursorbewegung ein leeres IME-Ereignis, das die Editor-Kürzel aushebelte und die App
  ständig neu zeichnen ließ. Die Ereignisse werden jetzt herausgefiltert. Zeichen über eine
  Eingabemethode und AltGr als Strg+Alt werden ebenfalls gepaart.
  `ARVIDLEAF_DEBUG_KEYS=1` gibt die Tastenereignisse des Editors aus.
- **Vorschläge nach Häufigkeit und Gewohnheit:** Die Reihenfolge berücksichtigt, wie verbreitet
  ein Befehl ist, ob der Cursor in Mathe oder im Text steht (`\be` → `\begin` im Text,
  `\beta` in Mathe), die Befehle des Dokuments und vor allem das eigene Verhalten über alle
  Projekte. Auch von Hand getippte Befehle zählen (wer `\leqslant` schreibt, bekommt es vor
  `\leq`). Alte Gewohnheiten verblassen mit der Zeit. Weitere Mathe-Befehle sind hinzugekommen.
- **Absturz behoben:** Stand ein LaTeX-Fehler auf einer leeren Zeile (z. B. nachdem man den
  Inhalt der Fehlerzeile gelöscht hatte), lief die Fehlermarkierung im Editor in eine
  Endlosschleife, bis der Speicher voll war und das Programm abbrach.
- `install.sh` überarbeitet: `--update`, `--system`, `--dev` (ersetzt `install-desktop.sh`), zypper,
  Rust ≥ 1.95 über rustup, falls die Distribution ein älteres hat, Hinweis bei laufender App.
- Starter (`.desktop`) aus einer gemeinsamen Vorlage (`packaging/desktop-entry.sh`): GenericName,
  Suchwörter, `.bib`/`.md`, StartupNotify.
- Neu: `install.ps1` (Windows, winget) und `install-macos.sh` (macOS, Homebrew, `arvidleaf.app`).
- Portabilität: Pfade nach Systemkonvention (`%APPDATA%`, `~/Library`), `.exe`-Suche, Öffnen mit
  `open`/`explorer`, Abbrechen per `taskkill`, keine Konsolenfenster unter Windows, TeX-Pfade
  für aus dem Finder gestartete Apps.
- `arvidleaf --compile datei.md` (oder `.tex`) kompiliert eine einzelne Datei; das PDF landet
  daneben oder mit `-o` an einem anderen Ort.

## Unveröffentlicht – Editor-Kürzel

Neu (Teil 2):

- Übersicht aller Tastenkürzel mit Suchfeld: F1, Strg+?, Help/Edit-Menü.
- `(`, `[`, `{`, `$` schließen automatisch; Schließzeichen überspringen, Backspace löscht leere Paare.
  `\(`, `\[`, `\left(` bekommen ihr Gegenstück.
- Strg+E `\emph`, Strg+M `\(…\)`, Strg+Umschalt+M `\[…\]`, Strg+Umschalt+E Umgebung um die Zeilen.
- `\end{…}` folgt dem geänderten Namen in `\begin{…}`.
- Listen: Enter setzt `\item`, Enter auf leerem `\item` beendet die Liste.
- Zeilen: Strg+D duplizieren, Alt+↑/↓ verschieben, Strg+L markieren, Tab/Umschalt+Tab einrücken.
- Tab-Stopps in Vorlagen (`\frac{}{}`, `tabular`-Spaltenangabe).
- Strg+Klick auf `\ref`, `\cite`, `\input`, `\includegraphics` öffnet Ziel.

Neu:

- `\begin{enumerate}` trägt `\usepackage{enumitem}` in die Präambel ein; ebenso weitere Pakete
  für Umgebungen und Befehle (`amsmath`, `booktabs`, `graphicx`, `tikz`, …), nur wenn sie fehlen.
- `$` auf einer Auswahl setzt sie in Mathe-Modus, statt sie zu ersetzen.
- Strg+B / Strg+I für `\textbf{…}` / `\textit{…}`.
- Vorschläge ab dem ersten Buchstaben, ohne Einblend-Animation, sortiert nach den Befehlen der
  Datei und nach früheren Auswahlen. Eigene Befehle (`\newcommand`) und Umgebungen werden vorgeschlagen.

Behoben:

- Enter nach einem vollständigen Befehl wie `\int` fügte `\iint` ein statt einer neuen Zeile.
- `{`/`[` auf einer Auswahl umschlossen sie nur, wenn danach ein Leerzeichen stand.

## Unveröffentlicht – Testiteration 2

Behoben (Details: `reqs/review.md` Abschnitt 6):

- Fehlende Bilder und `\input`-Dateien wurden als fehlendes TeX-Paket gemeldet.
- Fehlt eine babel-Sprache, erscheint nur noch dieser Fehler statt sechs Folgefehlern.
- Vorlage *cv* ohne Schriftwarnung; `.desktop` mit gültigen Kategorien.
- Neu: Hinweise zu typischen Ursachen, z. B. Listen-Optionen ohne `enumitem`, `\toprule` ohne
  `booktabs`, `align` ohne `amsmath`, „Missing $ inserted“ (`reqs/review.md` Abschnitt 7).
- Sprung zu einer Fehlerzeile in einer frisch geöffneten Datei landete bei umbrochenen Zeilen zu weit oben.
- Paketprüfung ignoriert andere Dokumente im selben Ordner.

## Unveröffentlicht – Testiteration 1

Neu:

- **Vim-Modus** (*Edit › Vim Mode*): Normal-, Insert- und Visual-Modus, Zähler, Operatoren,
  Textobjekte inkl. LaTeX `i$`/`ie`, Undo/Redo, `.`, Suche, `:w`, `:s`. Details: `reqs/review.md` Abschnitt 5.

Behoben (Details und Tests: `reqs/review.md` Abschnitt 5):

- Auto-Save konnte Änderungen eines anderen Programms überschreiben; jetzt erscheint der Konflikt-Hinweis.
- Mehrzeilige TeX-Fehlermeldungen wurden abgeschnitten.
- Fehler aus Paketdateien (z. B. fontspec unter pdfLaTeX) zeigen auf die `\usepackage`-Zeile;
  Hinweis zum Wechsel der Engine.
- Fehlendes `\end{document}` und fehlende `}` werden verständlich gemeldet, mit Zeile.
  Folgefehler („Emergency stop“) sind ausgeblendet.
- Strg+/ kommentiert auf Höhe der Einrückung.
- `clippy` ohne Warnungen.

## 0.1.0 – 11.11.2026 (Prototyp)

Erste Version nach `reqs/plan.md` (Meilensteine M0–M8).

- Desktop-App in Rust/egui im Stil von arvidlab (Palette, Schriften, Menü- und Statusleiste, Dark/Light).
- LaTeX-Editor mit Farbhervorhebung, Zeilenumbruch, Fehlermarkern, Autovervollständigung,
  `\begin`/`\end`-Ergänzung, Suchen & Ersetzen, Auto-Save.
- Kompilieren über `latexmk` (pdfLaTeX/XeLaTeX/LuaLaTeX) ohne Zeit- und Seitenlimits, abbrechbar.
- PDF-Vorschau mit Lazy Rendering (erste Seite ~60 ms nach dem Kompilieren), Zoom, SyncTeX.
- Template-Galerie mit 9 eingebauten Vorlagen und eigenen Vorlagen (`template.toml`).
- Lokale Versionshistorie mit Diff und Wiederherstellen.
- ZIP-Import/-Export, Projektsuche, Quick-Open, Outline, Dateibaum mit Kontextmenü.

Beim Testen gefunden und behoben:

- BibTeX bricht mit „3 is a bad bad“ ab, wenn `error_line`/`half_error_line` oder
  `max_print_line` > 10000 gesetzt sind.
- `\include{chapters/x}` scheitert mit separatem Build-Ordner („I can't write on file“):
  die Ordnerstruktur wird jetzt gespiegelt.
- latexmk wiederholt nach einem Fehlschlag nichts („gave an error in previous invocation“),
  auch wenn die Ursache behoben ist: nach Fehlern wird mit `-g` neu kompiliert.
- Fehlende babel-Sprachen (z. B. `ngerman`) werden als fehlendes Paket mit Arch-Paketnamen
  gemeldet und der `\usepackage`-Zeile zugeordnet.

Review (siehe `reqs/review.md`):

- Historie: nebenläufige Snapshots konnten dieselbe Temp-Datei beschreiben (eindeutige
  Temp-Namen, Schreibsperre).
- Ergebnis eines geschlossenen Projekts konnte als aktuelles Ergebnis gelten (globale Laufnummern).
- Zeilennummern nur für sichtbare Zeilen; schnellere Hervorhebung.
- Auto-Schließen: kein Paar nach `\`, nur selbst eingefügte Klammern werden übertippt.
- `verbatim`, `lstlisting`, `minted`, `\verb` werden wörtlich dargestellt.
- Projektnamen mit `/` werden abgewiesen; Seitenrahmen im hellen Theme.
- Installer `install.sh` mit Abhängigkeiten (pacman/apt/dnf), Programm, Icon und App-Menü-Eintrag.

Bekannt / offen:

- Tippen in einer einzelnen Datei mit 10 000 Zeilen: 40–50 ms pro Tastendruck.
- Datei-Dialoge und Drag & Drop sind nur im echten Fenster testbar (nicht offscreen).
