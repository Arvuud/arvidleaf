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
6. `clippy` installieren (`rustup component add clippy`) und Warnungen abarbeiten.
