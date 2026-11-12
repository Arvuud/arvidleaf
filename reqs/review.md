# arvidleaf – Review

> Phase: **REVIEW** · Stand: 11.11.2026 · Version 0.1.0
> Grundlage: `reqs/plan.md` (Abschnitt 6), Code-Durchsicht aller Crates, Tests, Screenshots

## 1. Befunde und Korrekturen

| # | Schwere | Befund | Korrektur | Absicherung |
|---|---|---|---|---|
| 1 | hoch | **Datenrennen in der Historie:** Timer-Snapshot, Snapshot nach dem Kompilieren und UI konnten gleichzeitig schreiben. Alle nutzten dieselbe Temp-Datei (`.x.arvidleaf-tmp`), ein Objekt hätte beschädigt werden können. Gleiche Snapshot-IDs waren möglich. | Temp-Dateien sind eindeutig (PID + Zähler). Snapshot, Restore, Prune und GC laufen unter einer Schreibsperre. | Test `concurrent_snapshots_do_not_corrupt` (8 Threads) |
| 2 | mittel | **Veraltete Compile-Nachricht:** Die Laufnummer zählte pro Projekt ab 0. Ein Ergebnis aus einem gerade geschlossenen Projekt hätte als Ergebnis des neuen Projekts gelten können (falsches PDF, falsche Fehler). | Laufnummern sind global eindeutig. | Code-Review |
| 3 | mittel | **Zeilennummern** wurden für jede Zeile gezeichnet, auch außerhalb des sichtbaren Bereichs. | Nur sichtbare Zeilen werden gezeichnet. | Messung, siehe 3. |
| 4 | mittel | **Hervorhebung** baute den Text Abschnitt für Abschnitt zusammen. | Text wird einmal kopiert, Abschnitte werden nur referenziert, Kapazität reserviert, Fehlerzeilen linear zugeordnet. | Tests `highlight_covers_text_and_marks_error_lines`, `large_file_relayout_is_fast` |
| 5 | niedrig | **Auto-Schließen:** `\{` bekam ein `}`. Beim Tippen von `}` wurde auch eine selbst geschriebene Klammer übersprungen. | Kein Paar nach `\`. Übersprungen wird nur die selbst eingefügte Klammer (Position wird verfolgt). | Code-Review, manuell zu prüfen (Abschnitt 4) |
| 6 | niedrig | **Verbatim:** In `verbatim`, `lstlisting`, `minted`, `\verb` wurde `%` als Kommentar und `$` als Mathe gefärbt. | Diese Bereiche werden wörtlich angezeigt, `comment`-Umgebung als Kommentar. | Test `verbatim_is_literal` |
| 7 | niedrig | **Projektname mit `/`** legte verschachtelte Ordner an. | Wird im Dialog abgewiesen. | Code-Review |
| 8 | kosmetisch | Im hellen Theme war die PDF-Seite kaum vom Hintergrund zu unterscheiden. | Seitenrahmen in `border`-Farbe. | Screenshot |

Aus der IMPLEMENT-Phase (dort gefunden und behoben, siehe CHANGELOG): BibTeX-Abbruch durch `max_print_line`, `\include` mit Build-Ordner, latexmk ohne Neulauf nach Fehlern, fehlende babel-Sprachen ohne Hinweis, nicht vorhandene Symbole im Font (mit Regressionstest).

**Geprüft ohne Befund:** ZIP-Import gegen „zip slip“ (`enclosed_name`), Dateioperationen nur innerhalb des Projekts (`safe_rel`), Shell-Escape standardmäßig aus, atomisches Speichern, Abbrechen beendet die ganze Prozessgruppe, keine blockierenden Aufrufe von TeX/poppler im UI-Thread.

## 2. Abgleich mit den Anforderungen

| # | Anforderung | Status | Nachweis |
|---|---|---|---|
| R1 | läuft lokal | ✅ erfüllt | kein Netzwerkzugriff; Projekte sind Ordner |
| R2 | Desktop-App | ✅ erfüllt | egui-App, `install.sh` mit `.desktop`-Datei und Icon im App-Menü |
| R3 | keine Echtzeit-Kollaboration | ✅ (bewusst weggelassen) | – |
| R4 | keine Seiten- oder Zeitlimits | ✅ erfüllt | Test `large_document_without_limits`: 520 Seiten mit TikZ, ohne Timeout; Seite 400 rendert direkt |
| R5 | Imports und `\usepackage` | ✅ erfüllt | Tests: eigene `.sty` im Unterordner, `\input`-Ketten, `\include`, Bilder, BibTeX, Umlaut-Pfade, Overleaf-ZIP, fehlendes Paket mit Hinweis |
| R6 | eigenes Template-Menü | ✅ erfüllt | Galerie, „Manage Templates…“, `template.toml` dokumentiert (README); Test: alle eingebauten Templates kompilieren |
| R7 | Color-Coding | ✅ erfüllt | 9 Token-Klassen, 9 Lexer-Tests |
| R8 | Stil wie arvidlab | ✅ erfüllt | `theme.rs` übernommen; Screenshot-Vergleich dark/light |
| – | Versionshistorie im Prototyp | ✅ erfüllt | Tests für Snapshot, Diff, Wiederherstellen, Deduplizierung, Ausdünnen, Nebenläufigkeit |

Should-Funktionen (Plan 1.2): SyncTeX ✅ (Roundtrip-Test), Auto-Compile ✅, Outline ✅, Autovervollständigung ✅, Suchen/Ersetzen ✅, ZIP ✅, Paket-Hinweise ✅.
Could: Rechtschreibprüfung ❌, Symbol-Palette ❌, Code-Folding ❌, Vim-Keybindings ❌, offen für später.

## 3. Nicht-funktionale Anforderungen

| Anforderung | Ergebnis |
|---|---|
| UI blockiert nie | ✅ Kompilieren, Rendern, SyncTeX, Vorschaubilder, Prune laufen in Threads |
| erste PDF-Seite < 300 ms | ✅ 60–65 ms (Statusleiste „preview … ms“), Test `pdf_first_page_renders_fast` |
| Tippen bei 10 000 Zeilen ohne spürbare Latenz | ⚠ **teilweise:** 40–50 ms pro Tastendruck bei 10 000 Zeilen *in einer Datei* (Release). Typische Dateien (< 3 000 Zeilen) liegen bei ca. 12–15 ms. Der Rest ist egui-Textlayout des ganzen Puffers. Verbesserung möglich durch Layout nur des sichtbaren Bereichs (eigenes Editor-Widget). |
| keine Datenverluste | ✅ Auto-Save nach 1 s, atomisches Schreiben, Sicherungs-Snapshot vor jedem Wiederherstellen und Löschen |
| Lizenz | ✅ MIT/Apache; TeX und poppler laufen als eigene Prozesse |

## 4. Offen für die Testiterationen

1. **Bedienung im echten Fenster:** Datei-Dialoge, Drag & Drop (Bilder → Figure-Snippet), Kontextmenüs, Umbenennen/Löschen, Wechsel zwischen Projekten.
2. **Editor-Verhalten beim Tippen:** Auto-Schließen, `\begin`→`\end` bei Enter, Completion mit Tab/Enter, Strg+/ (offscreen nur teilweise prüfbar).
3. **Externe Änderungen:** Datei in einem anderen Editor ändern, während sie in arvidleaf offen ist (mit und ohne ungespeicherte Änderungen).
4. **Echte Dokumente** des Nutzers (z. B. ein Overleaf-Projekt als ZIP). Erfahrung aus arvidlab: echte Dateien finden mehr als selbstgebaute Kataloge.
5. Nach `install.sh` (mit Abhängigkeiten): Artikel (Deutsch) kompiliert, `biber`/biblatex-Projekt.
6. ~~`clippy` installieren und Warnungen abarbeiten~~ → erledigt in Testiteration 1.

## 5. Testiteration 1 (11.11.2026)

Getestet: 12 neue Kompilier-Szenarien (`--compile`), Tippverhalten im Offscreen-Fenster
(Auto-Schließen, `\begin`→`\end`, Strg+/), CRLF/BOM-Dateien, Änderungen von außen
(Code-Durchsicht und Test), `clippy`. Dazu kam der Vim-Modus (Nutzerwunsch).

| # | Schwere | Unerwartetes Verhalten | Fix | Absicherung |
|---|---|---|---|---|
| T1 | hoch | **Auto-Save überschreibt Änderungen von außen:** Auto-Save (1 s) lief vor der Prüfung auf Änderungen von außen (alle 2 s). Hat ein anderes Programm die Datei geändert, während man tippte, wurde die fremde Änderung kommentarlos überschrieben. | Vor jedem Auto-Save wird der Zeitstempel auf der Platte geprüft; bei Abweichung erscheint der Konflikt-Hinweis („changed on disk“ mit Reload / Keep mine). | Test `detects_external_change` |
| T2 | mittel | **Mehrzeilige Fehlermeldungen abgeschnitten:** z. B. „Unicode character ∑ (U+2211)“ ohne „not set up for use with LaTeX“, fontspec-Meldung endet mit „requires either XeTeX or“. | Fortsetzungszeilen (`(paket)  …` und tief eingerückte Zeilen) werden angehängt, wie schon bei Warnungen. | Test `multi_line_errors_and_followups` |
| T3 | mittel | **Fehler in Systemdateien:** Bei `fontspec` unter pdfLaTeX zeigte der Fehler auf `/usr/share/…/fontspec.sty:101`; im Projekt war keine Zeile markiert, und es gab keinen Hinweis auf die Lösung. | Der Fehler zeigt auf die `\usepackage`-Zeile im Projekt („reported in fontspec.sty:101“ im Kontext). Dazu kommt der Hinweis „switch the engine to XeLaTeX or LuaLaTeX (Build › Engine)“. | Szenario `fontspec` |
| T4 | mittel | **Fehlendes `\end{document}`:** Gemeldet wurden nur „Emergency stop.“ und „==> Fatal error occurred“, ohne Datei und Zeile. Die eigentliche Ursache („no legal \end found“) fehlte. | Neue Meldung „Missing \end{document}: the file ended before the document was closed“, in der Hauptdatei an deren letzter Zeile. | Test `missing_end_document_and_runaway`; offscreen geprüft |
| T5 | mittel | **Fehlende `}`** („File ended while scanning use of \textbf“) ohne Zeile, dazu zwei Folgefehler. | Der Text aus „Runaway argument?“ wird im Quelltext gesucht, und der Fehler zeigt auf die Zeile, an der das Argument beginnt (nur bei eindeutigem Treffer). Dazu kommt der Hinweis „Probably a missing `}`“. | Test `missing_end_document_and_runaway`; Szenario `runaway` |
| T6 | niedrig | **Folgefehler** „Emergency stop“ und „==> Fatal error occurred“ wurden zusätzlich zum echten Fehler als eigene Fehler gezählt (3 statt 1). | Werden ausgeblendet, sobald es einen anderen Fehler gibt. | Tests T2/T4 |
| T7 | niedrig | **Strg+/** setzte `% ` in Spalte 0 vor die Einrückung (`%   \item`). | `% ` steht auf der kleinsten Einrückung des Blocks (`  % \item`). Entkommentieren funktioniert für beide Formen. | Test `comment_toggle_keeps_indentation` |
| T8 | – | **Vim-Modus: Escape nahm dem Editor den Fokus.** egui gibt den Fokus bei Escape ab, außer das Widget beansprucht die Taste. Escape kam deshalb nie beim Vim-Modus an. | Im Vim-Modus beansprucht der Editor Escape über den Fokus-Filter. | offscreen: `ifoo<Esc>jj` |
| T9 | Stil | `clippy`: 60 Hinweise (meist zusammenfassbare `if`). Keine echten Fehler. | Behoben bzw. begründet erlaubt (`large_enum_variant`, generierter Template-Typ). | `cargo clippy --workspace --all-targets` ohne Warnung |

**Ohne Befund:** biblatex + biber; `\input` mit Fehler im Unterordner (Datei und Zeile korrekt);
Leerzeichen in Ordner- und Dateinamen; `minted` (TeX Live 2026 braucht kein Shell-Escape mehr);
undefinierte Referenzen und Zitate (Warnungen mit Zeile); mehrere `\documentclass`-Dateien
(Hauptdatei richtig erkannt); `makeindex`; `latexmkrc` im Projekt (Build-Ordner bleibt
`.arvidleaf/build`); CRLF-Zeilenenden und UTF-8-BOM; Auto-Schließen von `{}`/`[]` und
Übertippen; `\{` ohne Paar; `\begin{itemize}` + Enter ergänzt `\end{itemize}` mit Einrückung.

### Vim-Modus

*Edit › Vim Mode* schaltet den Modus ein und aus (gespeichert in `config.toml`, `vim = true`).
Der Modus wird links über dem Editor angezeigt (`-- NORMAL --`, `-- INSERT --`, `-- VISUAL --`,
dazu noch nicht abgeschlossene Befehle oder die `:`-Zeile). Im Normalmodus erscheint ein
Block-Cursor.

- Modi: Normal, Insert, Visual (`v`), Visual-Line (`V`)
- Bewegungen mit Zähler: `h j k l w b e W B E 0 ^ $ gg G f F t T ; , % { } n N + - Enter`, Strg+D/U
- Operatoren: `d c y > <` mit Bewegung, Zeile (`dd`, `cc`, `yy`, `>>`) oder Textobjekt;
  `x X s S D C Y r J ~ p P o O i a I A u` Strg+R `.`
- Textobjekte: `iw aw iW aW i( i[ i{ i< i" i' ib iB` und für LaTeX **`i$`/`a$`** (Inline-Mathe)
  und **`ie`/`ae`** (`\begin…\end`-Umgebung, die innerste um den Cursor)
- Suche: `/` `?` `*` `#` (wörtlich, smartcase); Ex: `:w` (speichern + kompilieren wie Strg+S),
  `:<zeile>`, `:$`, `:s/a/b/[g]`, `:%s/a/b/g` (wörtlich, ohne reguläre Ausdrücke)
- Kopieren mit `y` geht zusätzlich in die Zwischenablage. Im Insert-Modus gelten weiter
  Auto-Schließen, `\begin`-Ergänzung und Autovervollständigung.
- Nicht enthalten: Makros (`q`), Marken (`m`), benannte Register (`"a`), Visual-Block
  (Strg+V), reguläre Ausdrücke.

Tests: 10 Unit-Tests (`vim::tests`, über 100 Tastenfolgen) und offscreen geprüft:
`ggjjwwdiwifoo<Esc>`, `vi$`, `Gkdd:w<CR>` speichert, Menü-Schalter.

## 6. Testiteration 2 (12.11.2026)

Getestet: alle 9 Vorlagen (`--compile`), 7 neue Fehlerszenarien (fehlendes Bild, fehlendes
`\input`, Umlaute in Ordner- und Dateinamen, falsches `\end`, unvollständige `align`-Formel,
unbekanntes Paket, Emoji unter pdfLaTeX), Installer `install.sh --no-deps` (neuer Build und
App-Menü-Eintrag), `desktop-file-validate`.

| # | Schwere | Unerwartetes Verhalten | Fix | Absicherung |
|---|---|---|---|---|
| T10 | mittel | **Fehlende Projektdateien als TeX-Paket gemeldet:** Bei `\includegraphics{bilder/fehlt.png}` oder `\input{kapitel/eins}` erschien „Missing TeX file … Install the TeX Live package that contains it“. Das führt in die falsche Richtung: die Datei fehlt im Projekt. | Als fehlendes Paket gelten nur Dateien der TeX-Distribution (`.sty`, `.cls`, `.ldf`, Schriften …) ohne Pfad. Die normale Fehlermeldung mit Zeile bleibt. | Test `missing_language_hides_followups_and_project_files_are_not_packages` |
| T11 | mittel | **Fehlende babel-Sprache: 7 Fehler statt 1.** Ohne `texlive-langgerman` meldete die deutsche Vorlage zusätzlich „Undefined control sequence“, „\begin{document} ended by \end{abstract}“ usw. | Nach „babel Error: Unknown option“ werden weitere Fehler ausgeblendet (alle sind Folgen). Der Installationshinweis bleibt. | gleicher Test; Vorlage `article-de` ohne Sprachpaket |
| T12 | niedrig | Vorlage **cv**: Schriftwarnung „Font shape T1/cmr/m/n in size <34> not available“. | `\usepackage{lmodern}` in der Vorlage. | `--compile`: 0 Warnungen |
| T13 | niedrig | `.desktop`: `desktop-file-validate` meldet mehrere Hauptkategorien (Eintrag kann doppelt im Menü erscheinen). | `Categories=Office;Publishing;` | `desktop-file-validate` ohne Hinweis |

**Ohne Befund:** übrige Vorlagen; Umlaute in Pfaden; `\begin{itemize}…\end{enumerate}`
(Zeile 5, Meldung nennt Zeile 3); unbekanntes Paket (Zeile der `\usepackage`); Emoji
(verständliche Unicode-Meldung).

**Bekannt, nicht geändert:**
- Vorlage *presentation* (metropolis) warnt unter pdfLaTeX, dass Fira-Schriften XeLaTeX/LuaLaTeX
  brauchen. Fira ist hier nicht installiert, ein Wechsel der Engine brächte nichts. Dazu kommt
  eine harmlose Overfull-\vbox-Meldung der Titelseite.
- Bei `\frac{1}{2` in `align` zeigt der Fehler auf den Anfang der Umgebung (Zeile 6), nicht auf
  Zeile 7. TeX liest die ganze Umgebung als Argument, genauer geht es aus dem Log nicht.
- Auf diesem Rechner fehlt `texlive-langgerman` (`sudo pacman -S texlive-langgerman`).

## 7. Testiteration 3 (12.11.2026)

Getestet im Testordner `~/Documents/arvidleaf/test`: die Testdatei des Nutzers (`main.tex`), eine
neu erzeugte Lorem-Ipsum-Arbeit (`lorem-thesis/`: `report`, 6 Kapitel per `\include`/`\input`,
biblatex + biber, cleveref, siunitx, TikZ, PNG-Bild, enumitem, 41 Seiten) und eine Kopie mit
eingebauten Fehlern (`lorem-errors/`). Dazu offscreen: Öffnen der Projekte und Klick auf einen
Fehler im Problems-Panel.

| # | Schwere | Unerwartetes Verhalten | Fix | Absicherung |
|---|---|---|---|---|
| T14 | mittel | **„Something's wrong--perhaps a missing \item“ ohne Erklärung** (Nutzer-Fund): `\begin{enumerate}[label=\alph*)]` ohne `enumitem`. Das ist kein Fehler von arvidleaf, die TeX-Meldung führt aber in die Irre. | Neues Modul `lf-tex/src/hints.rs`: Hinweise für typische Ursachen. Listen-Optionen ohne `enumitem`; ~40 Befehle (`\toprule`, `\SI`, `\cref`, `\href`, `\mathbb` …) und ~20 Umgebungen (`align`, `tikzpicture`, `lstlisting` …), deren Paket nicht geladen ist; „Missing $ inserted“. Ein Hinweis erscheint nur, wenn das Paket (oder ein Paket, das es mitlädt) wirklich fehlt. | Tests `hints::tests` |
| T15 | mittel | **Andere Dokumente im Ordner zählten mit:** Die Paketsuche (und die Zuordnung „fehlendes Paket → `\usepackage`-Zeile“) durchsuchte alle `.tex` im Projektordner, also auch Unterprojekte mit eigenem `\documentclass`. Im Testordner unterdrückte das `enumitem` aus `lorem-thesis/` den Hinweis für `main.tex`. | Gesucht wird nur in der Hauptdatei und in Dateien ohne eigenes `\documentclass`. | offscreen + `--compile` im Testordner |
| T16 | mittel | **Sprung zur Fehlerzeile landete daneben:** Ein Klick auf `03-method.tex:64` öffnete die Datei und setzte den Cursor auf Zeile 64. Sichtbar waren aber die Zeilen 36–56: Beim ersten Öffnen gibt es noch kein Layout, und die Schätzung „Zeile × Zeilenhöhe“ ignoriert umbrochene Zeilen. Betrifft auch Outline und Quick-Open bei frisch geöffneten Dateien. | Nach der Schätzung wird im nächsten Frame mit dem echten Layout exakt nachgescrollt. | offscreen: Klick auf Fehler, Zeile 64 mit Markern im Bild |

**Ohne Befund:** 41-seitige Arbeit kompiliert (erstes Mal 17,8 s mit biber; danach 3,2 s pro
Änderung, genauso schnell wie latexmk direkt). Fehler in `\include`-Kapiteln zeigen auf Kapiteldatei
und Zeile; fehlendes Bild, undefinierte Referenz und undefiniertes Zitat mit Zeile; Outline über
alle Kapitel.

**Bekannt, nicht geändert:** Fehlt ein Paket für eine Umgebung (z. B. `tikzpicture`), folgen
3–4 „Undefined control sequence“ aus ihrem Inhalt. Der erste Fehler trägt den Hinweis.
