# arvidref – Stand der Umsetzung (26.11.2026)

Diese Notiz dient als Einstieg, wenn später an arvidref weitergearbeitet wird. Plan,
Entscheidungen und Review stehen in `reqs/plan.md`, Abschnitt 15 (15.8 Stand, 15.9
Entscheidungen, 15.10 Review). Hier steht, *wie* die Umsetzung gebaut ist und was offen ist.

Stand: RF1–RF7 und REVIEW sind erledigt, auf `main` gemergt (`da9c167`).

## Commits

| Commit | Inhalt |
|---|---|
| `c148c61` | Plan (Abschnitt 15) |
| `b3b5881` | RF1 Parser/Writer, LaTeX-Zeichen, Namen |
| `11d29bb` | RF2 Bibliothek, Suche, Dubletten, Keys, Sammlungen |
| `18ed249` | RF3 Import (BibTeX, CSL-JSON, DOI, arXiv, ISBN, PDF) |
| `763a9ef` | RF4 References-Ansicht |
| `2f7c499` | RF5 Zitieren aus dem Editor |
| `ba0dc5e` | RF6 Abgleich Projekt ↔ Bibliothek |
| `19cce67` | RF7 Import aus `zotero.sqlite` |
| `d13aebf` | Installer: curl, sqlite3 |
| `da9c167` | Review-Korrekturen |

## Aufbau

### Crate `lf-ref` (keine GUI-Abhängigkeit)

| Datei | Inhalt |
|---|---|
| `bib/parse.rs`, `bib/write.rs`, `bib/mod.rs` | Verlustfreier BibLaTeX-Parser. `Bib` besteht aus `Block`s: `raw: Some(text)` heißt unverändert, das wird Byte für Byte zurückgeschrieben; `None` heißt geändert, das wird formatiert. Kaputte Einträge werden zu `Kind::Broken` (Rohtext), der Parser setzt beim nächsten `@` am Zeilenanfang wieder auf. `Bib::source/find/replace_entry/push_entry/remove_entry`. |
| `latex.rs` | `decode` (LaTeX → Unicode für Anzeige und Suche, verwirft Formatierung), `encode` (Text → sicheres LaTeX), `fold_ascii` (ä → ae, für Keys), `strip_accents` (ä → a, für die Suche) |
| `names.rs` | BibTeX-Namen (First/von/Last/Jr) |
| `key.rs` | Key-Muster `[auth][year][title]` usw., Kollisionen mit a…z, aa… |
| `collections.rs` | `collections.toml`, Pfade mit `/` für Unterordner |
| `library.rs` | `Library`: `open/reload/save(force)`. Speichern ist atomar (tmp + fsync + rename). Konflikte erkennt `save` über einen Stempel aus Änderungszeit und Größe (`SaveError::Conflict`). Pro Tag eine Sicherung in `backups/`, 30 bleiben. Index (`EntryView`), `search` (gewichtet, `tag:`/`type:`), `find_duplicate` (DOI/arXiv/Titel+Jahr), `add/add_anyway/update/remove`. Anhänge: `attach/detach/files`, Ordner `files_dir_in()` mit unter Windows gültigem Namen. Feld `file` als `;`-Liste, `:` escaped (JabRef-kompatibel). Zeitstempel `x-arvidref-added/-modified`. |
| `types.rs` | Pflicht- und optionale Felder je Eintragstyp (biblatex-Handbuch) |
| `net.rs` | HTTP über das Programm `curl` (Abbruch per `AtomicBool`, Wiederholungen, Fallback für curl < 7.71) |
| `import/mod.rs` | `detect` (DOI/arXiv/ISBN), `fetch`, `read_text` (CSL-JSON, BibTeX oder Kennungen), `add_all`, `html_to_latex`, `protect_case` |
| `import/csl.rs` | CSL-JSON → BibLaTeX. Auch für doi.org und als Zwischenformat für Zotero. Freie Datumsangaben werden zum Jahr. |
| `import/arxiv.rs`, `isbn.rs`, `bibtex.rs`, `pdf.rs` | arXiv-Atom, OpenLibrary (Crossref als Ausweichquelle), BibTeX mit `@string`, DOI aus PDFs (`pdftotext` Seite 1, `pdfinfo -meta`) |
| `import/zotero.rs` | Kopie von `zotero.sqlite` → `sqlite3 -readonly -json` → `item_to_csl` (Zoteros CSL-Zuordnung, typspezifische Felder → Basisfelder) → `csl::to_entry`. Sammlungen, Notizen → `annotation`, Anhänge (`storage:`-Pfade), `add_to_library`, `copy_file` (für den Worker-Thread). |
| `project.rs` | Bibliographie des Projekts: `bib_resources`, `bib_setup` (mehrere Dateien), `entries_to_append`, `export_entry` (ohne `file`, `annotation`, Zeitstempel), `cite_at`/`cite_text` (auch Markdown `[@a; @b]`), `undefined_citation`, `cited_keys`. Abgleich: `sync` → `SyncItem` (Changed, KeyConflict, ProjectOnly, SameWork, Missing), `replace_entry`, `merge_into_library`. |

### GUI (`lf-gui`)

- `refs_ui.rs` – `RefsView`:
  - Aufbau: Seitenleiste (Sammlungen, Tags), virtualisierte Tabelle, Detail-Editor (`Draft`, wird nach 0,8 s angewendet), BibLaTeX-Quelle, PDF-Reader (`PdfView::open`).
  - Hintergrundarbeit läuft auf Worker-Threads: `Job` → `Done` über einen mpsc-Kanal (Online-Abfragen, PDFs, Zotero, Dateikopien). Die Bibliothek gehört nur dem UI-Thread.
  - Nach außen: `poll()` arbeitet auch bei verborgener Ansicht; `version()` dient als Cache-Schlüssel für die App; `can_cite`/`cite` bilden die Brücke zum Editor; `with_library()` ändert die Bibliothek und speichert.
- `app.rs`:
  - Menü *References*, Strg+Umschalt+R (Ansicht), Strg+Umschalt+C (Zitier-Dialog).
  - Dialoge `Cite`, `BibSetup` und `Sync`.
  - `add_to_project_bib`, `project_bib` (alle `.tex`-Dateien), `set_up_bibliography`, `insert_into`/`write_project_text` (ändert den Editorpuffer, falls die Datei offen ist, sonst die Platte).
  - `update_cite_cache` (Bibliotheks-Keys und Beschreibungen für Completion und Hover), `bib_cache` (Projekt-`.bib` nach Inhalts-Hash).
  - Problems-Panel: „Add from library“.
- `editor.rs`: `EditorEnv.lib_keys/cite_info`, Completion mit Markierung „library“ (→ `needs_entries`), Hover am Zeiger, `insert_citation` (mit `~`, Ergänzen bestehender Zitate), `replace_text`, `insert_all`.
- `lf-core::config`: `library_dir`, `key_pattern`, `ref_mailto`. `ProjectMeta.cite_command`.

### Daten auf der Platte

```
~/.local/share/arvidleaf/arvidref/   (verlegbar)
├── library.bib
├── collections.toml
├── files/<key ohne : / …>/…
└── backups/library-JJJJ-MM-TT.bib
```

## Entscheidungen, die man kennen sollte

- **Externe Programme statt Bibliotheken:** `curl` (Netz), `pdftotext`/`pdfinfo` (poppler), `sqlite3` (Zotero). Es gibt kein TLS und keinen C-Code im Build. Alle drei sind optional, ohne sie fällt nur die jeweilige Funktion aus.
- **Einträge werden ins Projekt kopiert, nicht verlinkt.** Das Projekt kompiliert ohne die Bibliothek. Aktuell gehalten wird es über *Sync Project .bib*.
- **Felder werden als LaTeX-Quelle bearbeitet,** weil `decode` Formatierung verwirft.
- **Beobachten durch Abfrage alle 2 s** statt `notify`, wie bei den Projektdateien.
- **Keine Import-Vorschau:** Der Eintrag landet direkt in der Bibliothek, wird ausgewählt und ist sofort änderbar. Löschen lässt sich mit Strg+Z rückgängig machen.

## Tests

- `cargo test -p lf-ref`:
  - Unit-Tests;
  - `tests/bib.rs` (Roundtrip, unsaubere Dateien);
  - `tests/library.rs` (u. a. 10 000 Einträge, Anhänge);
  - `tests/import.rs` (gespeicherte echte Antworten in `fixtures/net/`);
  - `tests/zotero.rs` (Testdatenbank aus `fixtures/zotero/zotero.sql`, braucht `sqlite3`).
- Online-Test: `cargo test -p lf-ref --test import -- --ignored online`.
- Bibliothek für eine biber-Gegenprobe erzeugen:
  - aus den Import-Fixtures: `ARVIDREF_DUMP_DIR=… cargo test -p lf-ref --test import dump -- --ignored`;
  - aus der Zotero-Testdatenbank: dasselbe mit `--test zotero`.
- `lf-gui`: Tests der Ansicht in `refs_ui.rs` (Einfügen, Bearbeiten, Rückgängig, PDF ohne DOI, 10 000 Zeilen, Zotero mit Kopie im Hintergrund) und `editor.rs` (`citations_are_inserted_or_extended`).
- Offscreen: `arvidleaf --screenshot out.png --key 2:Ctrl+Shift+R --click F:X,Y --move F:X,Y …` mit `ARVIDLEAF_HOME=<testordner>/home`, damit die echte Bibliothek und Config unberührt bleiben.
- Testordner: `~/Documents/arvidleaf/test/arvidref-rf1` … `arvidref-rf7`.

## Offen (auf später verschoben, 26.11.2026)

### 1. Zotero mit einer echten Installation

Bisher nur gegen eine Testdatenbank getestet, deren Tabellen Zoteros `resource/schema/userdata.sql` nachbilden.

Vorgehen:
1. Zotero installieren oder einen vorhandenen `~/Zotero`-Ordner nehmen, am besten mit
   Einträgen aller Typen, Sammlungen, Notizen, PDFs und Better BibTeX.
2. *References › Import from Zotero…* ausführen und mit Zotero vergleichen:
   - Anzahl der Einträge, Felder je Typ, Keys (`citationKey` bzw. `Citation Key:` in Extra);
   - Sammlungen, Anhänge, Notizen.
3. Gegenprobe mit biber: alle Einträge per `\nocite{*}` kompilieren, ohne Warnungen.
4. Bekannte Lücken:
   - Anhänge mit relativem Basisordner (`attachments:` + Zotero-Einstellung „Basisverzeichnis“) werden als fehlend gemeldet. Den Basisordner könnte man aus `prefs.js` des Zotero-Profils lesen (`extensions.zotero.baseAttachmentPath`).
   - Gruppenbibliotheken werden übersprungen.
   - Creator-Typen außer author/editor/translator gehen verloren (z. B. `bookAuthor`).
   - Der Better-BibTeX-Key aus `better-bibtex.sqlite` wird nicht gelesen. Nur `citationKey`/Extra zählen; ältere BBT-Versionen speichern den Key nur dort.
5. Kleinere Befunde gleich als Fixture in `tests/fixtures/zotero/zotero.sql` nachbauen.

### 2. Installer für Windows und macOS

`install.ps1` und `install-macos.sh` sind angepasst, aber noch nicht auf den Systemen gelaufen.

- Windows:
  - winget `SQLite.SQLite` (portabel, Fallback-Suche nach `sqlite3.exe` wie bei Poppler);
  - curl liegt in Windows bei; die Prüfung sucht `curl.exe`, weil `curl` in PowerShell 5.1 ein Alias ist;
  - prüfen: Erscheint `sqlite3` nach der Installation im PATH? Kein Konsolenfenster bei Online-Import und Zotero-Import? Online-Import auch mit dem curl von Windows?
- macOS:
  - `pdftotext` kommt mit Homebrew-poppler, curl und sqlite3 liegen im System;
  - prüfen: Abschlussprüfung, PDF-Drop, Zotero-Import (Zotero-Ordner `~/Zotero`).
- Linux-Paket in `dist/` ist noch auf dem Stand 0.1.0; für ein Release mit `packaging/build-package.sh` neu bauen.

### 3. `cargo fmt`

rustfmt ist nicht installiert, und es gibt keine `rustfmt.toml`. Der Code hat über 1000 Zeilen mit mehr als 100 Zeichen.

Vorschlag:
1. `rustup component add rustfmt`.
2. `rustfmt.toml` mit `max_width = 200` (und ggf. `use_small_heuristics = "Max"`) anlegen.
3. Den Diff von `cargo fmt` ansehen, bevor er übernommen wird. Danach `cargo fmt --check` in die Review-Checkliste aufnehmen.

### Weitere Ideen (nicht geplant)

- Markdown: Vervollständigung nach `[@`, Hover über `@key`.
- Import-Vorschau vor dem Speichern (15.7).
- Abgleich mehrerer `.bib`-Dateien pro Projekt (zurzeit nur die erste).
- „Metadaten aktualisieren“ für Einträge mit DOI (neu abfragen und als Diff zeigen, wie beim Abgleich).
- PDF-Annotationen (15.9: später).
