# Changelog

## Unveröffentlicht – arvidref (Literaturverwaltung), Meilensteine RF1–RF7

- **Review-Korrekturen:**
  - „Cite“ in der Literaturverwaltung (Werkzeugleiste, Detail, Kontextmenü) fügt das Zitat ins
    offene Dokument ein.
  - Lädt eine eingebundene Präambeldatei biblatex, kommt `\addbibresource` dorthin, statt
    biblatex ein zweites Mal zu laden.
  - Anhangsordner sind auch unter Windows gültig, wenn der Key `:` oder `/` enthält.
  - Online-Abfragen funktionieren auch mit älterem curl (ohne `--retry-all-errors`).
  - Unter Windows öffnet der Zotero-Import keine Konsolenfenster.
  - Projekt-`.bib`-Dateien werden nur nach Änderungen neu gelesen.
  - Verschwindet `library.bib` kurz (Sync-Programm), bleiben die Einträge geladen.
  - Speicherkonflikte werden auch bei verborgener Ansicht gemeldet.

- **Import direkt aus Zotero** (RF7, *References › Import from Zotero…*, Ordner mit
  `zotero.sqlite`, Standard `~/Zotero`):
  - Gelesen wird eine Kopie der Datenbank über das Programm `sqlite3`. Zotero darf dabei
    laufen und wird nie verändert.
  - Einträge der eigenen Bibliothek mit Autoren/Herausgebern/Übersetzern, Tags, Notizen
    (Absätze bleiben erhalten), Datum „hinzugefügt“ und Zitierschlüssel (Feld `citationKey`
    oder `Citation Key:` im Feld Extra). Die Umsetzung folgt Zoteros eigener CSL-Zuordnung,
    einschließlich typspezifischer Felder wie `bookTitle`, `university` und `thesisType`.
  - Sammlungen samt Unterordnern. PDF- und andere Anhänge werden im Hintergrund nach
    `files/<key>/` kopiert, Web-Schnappschüsse (HTML) übersprungen.
  - Übersprungen werden außerdem: Papierkorb, Gruppenbibliotheken, Notizen und Dateien
    ohne Eintrag. Fehlende Dateien (nicht synchronisiert, verknüpft) werden gezählt und
    gemeldet. Schon vorhandene Werke werden nicht doppelt angelegt, bekommen aber die
    Sammlungen und, falls sie noch keinen Anhang haben, die PDFs.
  - Freie Datumsangaben wie „Sommer 2018“ werden zum Jahr, weil biblatex ISO-Daten
    braucht. Das gilt auch für CSL-JSON-Importe.
  - Gegenprobe: Testdatenbank (`tests/fixtures/zotero/zotero.sql`, Tabellen wie in Zoteros
    `userdata.sql`) → Bibliothek → biber ohne Warnung
    (`~/Documents/arvidleaf/test/arvidref-rf7`).
- Die Werkzeugübersicht zeigt jetzt auch `sqlite3`.

- **Abgleich Projekt ↔ Bibliothek** (RF6, *References › Sync Project .bib…*): Liste der
  Unterschiede mit Diff (Projekt − / Bibliothek +).
  - Geänderte Einträge: Bibliotheksfassung ins Projekt übernehmen (die übrigen Einträge der
    Datei bleiben Byte für Byte gleich) oder die Bibliothek aus dem Projekt aktualisieren
    (Anhänge und Notizen der Bibliothek bleiben).
  - Einträge nur im Projekt: in die Bibliothek aufnehmen, einzeln oder alle.
  - Zitiert, aber nicht in der Projekt-`.bib`, jedoch in der Bibliothek: ins Projekt holen.
  - Key-Konflikt (gleicher Key, anderes Werk): Das Projekt behält seinen Eintrag, die
    Bibliothek nimmt ihn unter einem neuen Key auf. Danach wird er als „dasselbe Werk unter
    anderem Key“ angezeigt statt weiter als Konflikt.
  - Gleiche Arbeit unter anderem Key: nur ein Hinweis, beide Keys bleiben.
  - Unterschiede nur in Klammern/Anführungszeichen, Leerraum oder den Bibliotheksfeldern
    (`file`, `annotation`, Zeitstempel) zählen nicht.
- README: Abschnitt „Literaturverwaltung (arvidref)“.

- **Zitieren aus dem Editor** (RF5):
  - Zitier-Dialog (Strg+Umschalt+C, *Edit › Insert Citation…*): Suche über die Bibliothek
    und die `.bib`-Dateien des Projekts. Enter zitiert. Mehrere Einträge wählt man über die
    Kästchen oder Strg+Leertaste. Der Befehl (`\cite`, `\parencite`, `\textcite`,
    `\autocite`, `\footcite`, `\citep`, `\citet`) wird pro Projekt gemerkt. In Markdown
    entsteht `[@a; @b]`.
  - Steht der Cursor in einem `\cite{…}` oder `[@…]`, werden nur die neuen Keys dort
    ergänzt. Nach einem Wort wird `~\cite{…}` eingefügt (außer bei `\textcite`/`\citet`).
  - Vervollständigung in `\cite{`: erst die Keys des Projekts, dann die der Bibliothek
    (markiert mit „library“), jeweils mit Autor, Jahr und Titel.
  - Zitierte Einträge aus der Bibliothek werden in die `.bib` des Projekts kopiert, ohne
    Anhangspfade, Notizen und Zeitstempel: in die erste Datei aus
    `\addbibresource`/`\bibliography` bzw. `bibliography:` im Markdown-Front-Matter, sonst in
    eine neue `refs.bib`. Im letzten Fall fragt arvidleaf, ob `\usepackage{biblatex}`,
    `\addbibresource` und `\printbibliography` eingefügt werden sollen (bei vorhandenem
    `\bibliographystyle` stattdessen `\bibliography{…}`). Ist die Datei offen, wird der
    Editorpuffer geändert, der Cursor bleibt stehen.
  - Hover über einem Key zeigt Autor, Jahr und Titel.
  - Nach dem Kompilieren zeigt das Problems-Panel bei undefinierten Zitaten, die in der
    Bibliothek stehen, „Add from library“, einzeln und für alle zusammen. Danach wird neu
    kompiliert.
  - Gegenprobe mit biber in `~/Documents/arvidleaf/test/arvidref-rf5`: Zitier-Dialog und
    Vervollständigung bis zum fertigen Literaturverzeichnis, ohne Warnung.
- `--screenshot` kennt `--move F:X,Y` (Zeiger bewegen, z. B. für Tooltips).

- **References-Ansicht** (RF4, *View › References* oder Strg+Umschalt+R, auch ohne offenes
  Projekt):
  - Links „All entries“, „Unfiled“, Sammlungen als Baum (anlegen, umbenennen, löschen,
    Einträge per Drag & Drop einsortieren) und Tags als Filter.
  - Tabelle mit Autor, Jahr, Titel, Zeitschrift/Verlag und Anhang-Symbol. Sortierbar per
    Klick auf die Spaltenköpfe, Suche bei jedem Tastendruck. Gezeichnet werden nur die
    sichtbaren Zeilen: 10 000 Einträge filtern und sortieren in 12–16 ms (Release).
  - Mehrfachauswahl mit Strg/Umschalt+Klick, ↑/↓, Strg+A. Kontextmenü: `\cite` / Key /
    BibLaTeX kopieren, PDF öffnen, DOI öffnen, zu einer Sammlung hinzufügen, löschen.
    Entf löscht, Strg+Z oder „Undo delete“ stellt wieder her.
  - Editor rechts: Eintragstyp, Key (mit Neuerzeugung nach Muster), Felder, wobei fehlende
    Pflichtfelder des Typs markiert sind, Felder hinzufügen und entfernen, Tags, Notizen
    (`annotation`) und Anhänge. Änderungen werden nach 0,8 s Pause gespeichert. Ein Key, den
    ein anderer Eintrag schon hat, wird abgelehnt. Umschaltbar auf die BibLaTeX-Quelle mit
    Hervorhebung.
  - Import über das Eingabefeld oben (DOI, arXiv-ID, ISBN oder BibTeX, auch mehrere),
    Strg+V in der Liste, *Import…* oder Drag & Drop von PDFs, `.bib`- und CSL-JSON-Dateien.
    Online-Abfragen laufen im Hintergrund und lassen sich abbrechen. Hat ein PDF keine
    erkennbare Kennung oder schlägt die Abfrage fehl, wird es mit dem Dateinamen als Titel
    übernommen (eine gefundene DOI bleibt erhalten). Ein PDF zu einem vorhandenen Eintrag
    ohne Anhang wird diesem angehängt.
  - Anhänge werden nach `files/<key>/` kopiert und öffnen im eingebauten PDF-Viewer
    (Esc zurück zur Liste). Wechselt der Key, ziehen Ordner und Pfade mit.
  - Ändert ein anderes Programm `library.bib`, lädt die Ansicht sie neu. Hat man selbst
    ungespeicherte Änderungen, fragt sie: „Reload from disk“ oder „Keep mine“.
  - *References*-Menü: Bibliotheksordner öffnen oder verlegen (z. B. in einen Sync- oder
    git-Ordner), Key-Muster, optionale Kontaktadresse für Crossref.

- **Import** (`lf_ref::import`, RF3):
  - Einfügen oder Datei: BibTeX/BibLaTeX (`@string`-Makros der Quelle werden aufgelöst),
    CSL-JSON (Zotero-Export „CSL JSON“, Keys von Better BibTeX bleiben erhalten) oder eine
    Liste von Kennungen. Dubletten werden übersprungen und gemeldet.
  - Erkennung von DOI (auch als Link oder `doi:`), arXiv-ID (auch als Link, `arXiv:` oder
    arXiv-DOI) und ISBN-10/13 mit Prüfziffer.
  - Online auf Anfrage: DOI über doi.org (Crossref, DataCite, mEDRA), arXiv über die arXiv-API.
    Hat ein Preprint eine Zeitschriften-DOI, wird der Zeitschrifteneintrag übernommen und um
    die eprint-Felder ergänzt. ISBN über OpenLibrary, sonst über Crossref.
  - Netzzugriff über das Programm `curl`: kein TLS im Build, abbrechbar, mit Wiederholung bei
    Verbindungsabbrüchen. Gesendet wird nur die Kennung, der User-Agent enthält keine Adresse.
  - PDF: Die DOI oder arXiv-ID wird aus den Metadaten oder der ersten Seite gelesen (`pdftotext`).
  - HTML/JATS in Metadaten wird zu LaTeX (`<i>` → `\emph`, `<sub>` → `\textsubscript`).
    Wörter mit Großbuchstaben im Inneren (`LaTeX`, `DNA`) werden mit Klammern geschützt.
  - Gegenprobe: 14 importierte Einträge kompilieren mit biber ohne Warnung
    (`~/Documents/arvidleaf/test/arvidref-rf3`).
- Die Werkzeugübersicht zeigt jetzt auch `curl` und `pdftotext`.

- **Zentrale Bibliothek** (`lf_ref::Library`, RF2): `library.bib` plus `collections.toml`
  und `files/<key>/` für Anhänge.
  - Speichern ist atomar. Hat ein anderes Programm die Datei inzwischen geändert, wird sie
    nicht überschrieben, sondern ein Konflikt gemeldet. Vor dem ersten Speichern des Tages
    entsteht eine Sicherung in `backups/`, die letzten 30 bleiben erhalten.
  - Suche über Key, Autoren und Herausgeber, Jahr, Titel, Zeitschrift, Tags, Notizen und DOI.
    Groß-/Kleinschreibung und Akzente zählen nicht (`muller` und `mueller` finden Müller),
    dazu `tag:` und `type:` als Filter. 10 000 Einträge: Öffnen 72 ms, Suchen < 15 ms (Release).
  - Dubletten über DOI (in jeder Schreibweise), arXiv-ID (eprint, DOI oder URL, ohne Version)
    oder Titel + Jahr.
  - Keys nach Muster, Standard `[auth][year][title]` (`mueller2021neuronale`), bei Konflikten
    mit `a`, `b` … Wird ein Key geändert, ziehen Sammlungen und Anhang-Ordner mit.
  - Tags im Feld `keywords`, verschachtelte Sammlungen (`Thesis/Kapitel 2`).
- Beim Löschen eines Eintrags bleibt der Abstand zwischen den übrigen Einträgen erhalten.
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
