# Changelog

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
