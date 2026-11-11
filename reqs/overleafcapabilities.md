# Overleaf – Funktionen & Vorteile

> Recherche-Grundlage für **arvidleaf** · Stand: 11.11.2026 · Phase: PLAN
>
> Zweck: Überblick, was Overleaf kann und warum Leute es nutzen. Daraus leiten wir später die Anforderungen für arvidleaf ab.

---

## 1. Was ist Overleaf?

Overleaf ist ein **browserbasierter, kollaborativer LaTeX-Editor**. Nutzer schreiben LaTeX-Quelltext im Browser, ein Server kompiliert ihn zu einem PDF, und mehrere Personen können gleichzeitig am selben Projekt arbeiten. Es gibt drei Varianten:

| Variante | Betrieb | Kosten |
|---|---|---|
| **overleaf.com** (Cloud) | SaaS | Free / Standard / Professional / Group / Campus |
| **Community Edition (CE)** | Self-hosted (Open Source, AGPL) | kostenlos |
| **Server Pro** | Self-hosted, Enterprise | kostenpflichtig |

Technischer Unterbau der Open-Source-Version: Node.js-Microservices, MongoDB, Redis, CodeMirror 6 als Editor, TeX Live (aktuell TeX Live 2026) für die Kompilierung.

---

## 2. Kernvorteile (warum Overleaf?)

1. **Keine lokale Installation:** Es ist keine TeX-Distribution nötig (mehrere GB). Ein Browser reicht.
2. **Echtzeit-Kollaboration:** Mehrere Autoren arbeiten gleichzeitig an einem Dokument, ähnlich wie bei Google Docs.
3. **Sofortiges Feedback:** Der Quelltext steht links, das kompilierte PDF rechts. Die Kompilierung ist automatisch oder auf Knopfdruck möglich.
4. **Niedrige Einstiegshürde:** Es gibt Vorlagen, einen visuellen Editor für LaTeX-Neulinge und eine große Lern-Dokumentation.
5. **Plattformunabhängig:** Projekte liegen zentral und sind von jedem Gerät erreichbar.
6. **Reproduzierbare Builds:** Alle nutzen dieselbe TeX-Live-Version, also gibt es kein „bei mir kompiliert's aber“.
7. **Wissenschafts-Workflow:** Literaturverwaltung, Journal-Vorlagen und eine direkte Einreichung bei Verlagen sind integriert.

---

## 3. Funktionsübersicht im Detail

### 3.1 Editor
- **Code-Editor** (CodeMirror 6) mit:
  - LaTeX-Syntax-Highlighting
  - Autovervollständigung für Befehle, Umgebungen, `\ref`/`\label`, `\cite`-Keys und Pakete
  - automatischem Schließen von Klammern und Umgebungen (`\begin{…}` → `\end{…}`)
  - Code-Folding sowie Suchen & Ersetzen (inkl. Regex und projektweiter Suche)
  - Linting und Syntaxprüfung direkt im Editor
  - Rechtschreibprüfung in mehreren Sprachen und einem eigenen Wörterbuch
  - wählbaren Tastenbelegungen (Default, **Vim**, **Emacs**) und Themes (hell/dunkel)
- **Visual Editor / Rich-Text-Modus:** WYSIWYM-Ansicht für Nutzer ohne LaTeX-Kenntnisse. Überschriften, Listen, Formeln und Abbildungen werden darin gerendert dargestellt.
- **Dokument-Outline:** Gliederung aus `\section`, `\subsection` usw., mit Klick-Navigation.
- **Symbol-Palette** und **Tabellen-Editor** (visuell) als Hilfen für Mathe-Symbole und Tabellen.

### 3.2 Kompilierung & PDF-Vorschau
- **Compiler:** pdfLaTeX, XeLaTeX, LuaLaTeX und LaTeX (DVI). Das Haupt-Dokument ist wählbar.
- **Bibliographie & Hilfstools:** BibTeX, Biber, makeindex und glossaries, automatisch über **latexmk**.
- **Auto-Compile** beim Tippen oder manuelle Kompilierung (Strg+S / Strg+Enter).
- **Wählbare TeX-Live-Version** pro Projekt.
- **Integrierter PDF-Viewer:** Zoom, Seitennavigation, Download.
- **SyncTeX:**
  - Klick im PDF springt zur passenden Stelle im Quelltext (*reverse search*).
  - Ein Button springt vom Quelltext zur passenden Stelle im PDF (*forward search*).
- **Fehler- und Warnungs-Logs:** Das rohe Log wird geparst und lesbar aufbereitet, mit Links zur Zeile und Hinweisen zur Behebung.
- **Draft-Modus / Fast-Compile** (Bilder werden nicht gerendert) für schnellere Vorschau.
- **Stop on first error** oder trotzdem weiter kompilieren.
- **Compile-Timeout** je nach Plan (Free: kurz, früher 10 s, heute „Basic“; Paid: ein Vielfaches davon, laut Overleaf das 24-Fache von Basic).
- **Barrierefreie PDFs** (Tagged PDF / Accessibility), seit 2026 produktiv nutzbar.

### 3.3 Projekt- & Dateiverwaltung
- **Dashboard** mit allen Projekten: Suchen, Tags/Ordner, Archivieren, Papierkorb, Kopieren.
- **Dateibaum** mit Ordnern sowie Upload per Drag & Drop für `.tex`, `.bib`, `.cls`, `.sty`, Bilder (PNG, JPG, PDF, EPS) und weitere Dateien.
- **Projekt-Import:** ZIP-Upload, Import aus GitHub, aus einer Vorlage oder aus einer URL.
- **Verknüpfte Dateien:** Dateien aus einer URL oder aus anderen Overleaf-Projekten, aktualisierbar.
- **Export:** PDF, Quellcode als ZIP, einzelne Dateien.
- **Bildvorschau** im Editor für Grafiken.

### 3.4 Kollaboration
- **Echtzeit-Mehrbenutzer-Editing** auf Basis von Operational Transformation (OT), mit farbigen Cursorn der anderen Nutzer.
- **Teilen:**
  - Einladung per E-Mail mit Rolle **Owner / Editor / Reviewer / Viewer**
  - Link-Sharing (Read-only- oder Edit-Link)
- **Kommentare:** Inline-Kommentare an Textstellen, Threads, Auflösen und Wiedereröffnen *(Cloud: alle Pläne; Self-hosted: nur Server Pro)*.
- **Track Changes / Review-Modus:** Änderungen werden vorgeschlagen und einzeln angenommen oder abgelehnt *(Premium bzw. Server Pro)*.
- **Projekt-Chat** im Editor.
- **Präsenzanzeige:** Wer ist gerade online, und wo im Dokument?
- **Kollaborateur-Limit** je Plan (Free: 1 pro Projekt; Standard: 10; Professional: unbegrenzt).

### 3.5 Versionierung & Historie
- **Automatische Versionshistorie** jeder Änderung, ohne manuelles Speichern.
- **Diff-Ansicht** zwischen Versionen, nach Autor eingefärbt.
- **Labels/Benennung** von Versionen (z. B. „Einreichung v1“).
- **Wiederherstellen** einzelner Dateien oder des ganzen Projekts auf einen früheren Stand.
- **Download** einer alten Version als ZIP.
- *(Free: nur kurzes Historien-Fenster von 24 h; Premium: volle Historie.)*

### 3.6 Integrationen
- **Git-Integration:** Das Projekt lässt sich als Git-Repo klonen, pushen und pullen *(Premium / Server Pro)*.
- **GitHub-Sync:** bidirektional *(Premium)*.
- **Dropbox-Sync** *(Premium)*.
- **Literaturverwaltung:** Import aus Zotero, Mendeley und Papers, mit automatischer Aktualisierung der `.bib` *(Premium)*.
- **Journal-Einreichung** direkt an Verlage und Repositorien (arXiv, Zenodo, F1000, IEEE …).
- **Citation Reviewer:** schlägt passende Paper aus Dimensions vor und hilft gegen „halluzinierte“ Referenzen.

### 3.7 Vorlagen & Lernressourcen
- **Template-Galerie** mit Tausenden Vorlagen: Journals, Konferenzen, Abschlussarbeiten, Lebensläufe, Präsentationen (Beamer), Poster, Briefe.
- **Eigene/private Vorlagen** für Organisationen *(Server Pro)*.
- **Umfangreiche Lern-Doku** („Learn LaTeX in 30 minutes“, Paket-Dokumentationen).

### 3.8 KI-Funktionen (Overleaf AI)
- Seit 07/2026 **in allen Plänen enthalten**, auch im Free-Plan, mit planabhängigen Nutzungslimits.
- **Error Assist:** erklärt Kompilierfehler und schlägt Fixes vor.
- **Schreibhilfe:** Formulierungen verbessern, LaTeX aus Beschreibungen generieren (z. B. Tabellen und Formeln).
- **Commons AI:** erweitertes Kontingent für Campus-Lizenzen.

### 3.9 Benutzer, Accounts & Administration
- **Login:** E-Mail/Passwort, Google, ORCID, institutionelles SSO.
- **Self-hosted Server Pro:** SSO via **SAML 2.0, LDAP, OpenID Connect**.
- **Admin-Panel:** Nutzerverwaltung, Projektübersicht, Systemnachrichten *(CE nur eingeschränkt)*.
- **Gruppen- und Campus-Lizenzen** mit zentraler Verwaltung.
- **Institutionelles Theming** (Server Pro 6.3).

### 3.10 Sicherheit & Betrieb
- **Sandboxed Compiles:** Jede Kompilierung läuft isoliert in einem eigenen Docker-Container *(Server Pro; CE bietet das laut offizieller Doku **nicht**, dort ist der Einsatz nur für vertrauenswürdige Nutzer gedacht)*.
- **`\write18` / Shell-Escape** kontrolliert, z. B. für `minted` und `pgfplots`-externalize.
- **Toolkit** (Docker-Compose-basiert) für die Installation und Upgrades bei Self-Hosting.
- **Upgrade CE → Server Pro** als Drop-in (`SERVER_PRO=true`).

---

## 4. Vergleich: Community Edition vs. Server Pro (Self-Hosted)

| Funktion | Community Edition | Server Pro |
|---|:---:|:---:|
| LaTeX-Editor + PDF-Vorschau | ✅ | ✅ |
| Echtzeit-Kollaboration | ✅ | ✅ |
| Volle Projekt-Historie | ✅ | ✅ |
| Kommentare | ❌ | ✅ |
| Track Changes | ❌ | ✅ |
| Git-Integration | ❌ | ✅ |
| Private Vorlagen | ❌ | ✅ |
| SSO (SAML / LDAP / OIDC) | ❌ | ✅ |
| Sandboxed Compiles | ❌ | ✅ |
| Volles Admin-Panel | eingeschränkt | ✅ |

---

## 5. Schwächen & Pain Points (Chancen für arvidleaf)

Wo Overleaf Nutzer häufig frustriert, kann arvidleaf sich abheben:

- **Paywall bei Kernfunktionen:** Track Changes, volle Historie, Git/GitHub-Sync und mehr als ein Kollaborateur kosten Geld.
- **Compile-Timeouts** im Free-Plan: Große Dokumente (Thesis, viele TikZ-Grafiken) brechen ab.
- **Online-Zwang:** Ohne Internet gibt es keinen sinnvollen Offline-Modus.
- **Performance:** Die Kompilierung großer Projekte läuft auf geteilten Servern oft langsam.
- **Community Edition ohne Sandboxing:** Sie ist für Multi-User-Umgebungen mit nicht vertrauenswürdigen Nutzern unsicher.
- **Begrenzte Erweiterbarkeit:** Es gibt kein Plugin-System für eigene Editor-Funktionen.
- **Datenhoheit:** In der Cloud liegen die Daten bei einem Drittanbieter (relevant für vertrauliche Forschung und Firmen).

---

## 6. Erste Priorisierung für arvidleaf (Vorschlag, in der PLAN-Phase zu verfeinern)

| Priorität | Funktionsbereich |
|---|---|
| **MVP (Must)** | Projekt-/Dateiverwaltung, Code-Editor mit LaTeX-Highlighting, serverseitige Kompilierung (latexmk, pdfLaTeX/XeLaTeX/LuaLaTeX, BibTeX/Biber), PDF-Vorschau, lesbare Fehlerlogs, Auto-Save |
| **Should** | SyncTeX (PDF ↔ Quelltext), Autovervollständigung, Dokument-Outline, Versionshistorie mit Diff, Vorlagen, ZIP-Import/-Export, sandboxed Compiles (Docker) |
| **Could** | Echtzeit-Kollaboration (OT/CRDT, z. B. Yjs), Kommentare, Track Changes, Git-Integration, Rechtschreibprüfung, Vim-Keybindings |
| **Later** | Visueller Editor, KI-Assistent (z. B. Fehlererklärung über die Claude API), Zotero-Integration, SSO, Admin-Panel |

> **Offene Fragen für die PLAN-Phase:** Single-User (lokal) oder Multi-User? Self-hosted oder Desktop-App? Gewünschter Tech-Stack? Wie wichtig ist Echtzeit-Kollaboration im ersten Wurf?

---

## Quellen

- [Overleaf Docs – Server Pro vs. Community Edition](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition)
- [Overleaf Docs – Choose Community Edition or Server Pro](https://docs.overleaf.com/on-premises/installation/using-the-toolkit/4.-choose-community-edition-or-server-pro)
- [Overleaf Docs – Premium features](https://docs.overleaf.com/getting-started/free-and-premium-plans/premium-features)
- [Overleaf – Features overview](https://cn.overleaf.com/about/features-overview)
- [Overleaf – Plans](https://www.overleaf.com/user/subscription/plans)
- [Overleaf – Plan limits](https://cn.overleaf.com/learn/how-to/Overleaf_plan_limits)
- [Overleaf – Premium features (Learn)](https://cn.overleaf.com/learn/how-to/Overleaf_premium_features)
- [Overleaf Blog (Product Updates: Overleaf AI, TeX Live 2026, Server Pro 6.3, Accessible PDFs)](https://www.overleaf.com/blog)
- [Overleaf for Enterprises – Cloud vs. Server](https://www.overleaf.com/for/enterprises/features)
- [GitHub – overleaf/overleaf](https://www.github.com/overleaf/overleaf/wiki)
- [G2 – Overleaf Features](https://www.g2.com/products/overleaf/features)
- [CASRAI – What is Overleaf](https://www.casrai.org/guides/what-is-overleaf)
- [dasroot.net – Overleaf Alternatives: Self-Hosted LaTeX Collaboration in 2026](https://dasroot.net/posts/2026/03/overleaf-alternatives-self-hosted-latex-collaboration/)
