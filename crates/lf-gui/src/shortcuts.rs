//! All keyboard shortcuts in one table: the overview dialog (F1) shows it,
//! and the README lists the same keys.

/// (group, [(keys, what they do)])
pub const GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "General",
        &[
            ("F1, Ctrl+?", "This overview"),
            ("Ctrl+S", "Save all and compile"),
            ("Ctrl+Enter", "Compile"),
            ("Ctrl+.", "Stop compiling"),
            ("Ctrl+P", "Go to file"),
            ("Ctrl+Shift+F", "Find in project"),
            ("Ctrl+W", "Close tab"),
            ("Ctrl+H", "History"),
            ("Ctrl+Shift+N", "New project from template"),
            ("Ctrl+O", "Open project folder"),
            ("Ctrl+Shift+R", "References (library) on / off"),
        ],
    ),
    (
        "References",
        &[
            ("Ctrl+V in the list", "Add the DOI, arXiv ID, ISBN or BibTeX on the clipboard"),
            ("↑ / ↓, Ctrl/Shift+Click", "Select entries"),
            ("Enter, double-click", "Open the attached PDF"),
            ("Del, Ctrl+Z", "Delete the selection, undo"),
            ("Drag onto a collection", "File the entries"),
            ("Esc", "Back from the PDF to the list"),
        ],
    ),
    (
        "PDF",
        &[
            ("Ctrl+J", "Show the cursor position in the PDF"),
            ("Double-click PDF", "Jump to the source"),
            ("Ctrl+wheel, Ctrl +/−", "Zoom PDF"),
        ],
    ),
    (
        "Find & navigate",
        &[
            ("Ctrl+F", "Find & replace in file"),
            ("Enter / Shift+Enter", "Next / previous match (in the find bar)"),
            ("Ctrl+Click", "Follow \\ref, \\cite, \\input, \\includegraphics"),
            ("Ctrl+Shift+C", "Insert a citation from the library"),
            ("Tab", "Next argument after a template (\\frac{…}{…}), then behind it"),
        ],
    ),
    (
        "LaTeX",
        &[
            ("Ctrl+B", "Bold: \\textbf{…}"),
            ("Ctrl+I", "Italic: \\textit{…}"),
            ("Ctrl+E", "Emphasis: \\emph{…}"),
            ("Ctrl+M", "Inline math: \\(…\\)"),
            ("Ctrl+Shift+M", "Display math: \\[…\\]"),
            ("Ctrl+Shift+E", "Put the lines into \\begin{…} … \\end{…}"),
            ("$ ( [ { on a selection", "Wrap the selection: $x$, (x), [x], {x}"),
            ("( [ { $", "Insert the pair; typing the closer steps over it"),
            ("Backspace", "Removes an empty pair: (|) → |"),
            ("\\( \\[ \\left(", "Insert \\) \\] \\right) as well"),
            ("Enter after \\begin{…}", "Insert the matching \\end{…}"),
            ("Edit \\begin{name}", "The matching \\end{name} follows"),
            ("Enter in a list", "New \\item; on an empty \\item: end the list"),
            ("Shift+Enter", "Plain line break"),
            ("Ctrl+/", "Toggle comment"),
            ("Tab / Enter, Esc", "Accept / close suggestions"),
        ],
    ),
    (
        "Lines",
        &[
            ("Ctrl+D", "Duplicate line(s)"),
            ("Alt+↑ / Alt+↓", "Move line(s) up / down"),
            ("Ctrl+L", "Select line (again: next line too)"),
            ("Tab / Shift+Tab", "Indent / outdent the selected lines"),
        ],
    ),
    (
        "Vim mode (Edit menu)",
        &[
            ("Esc, Ctrl+[", "Normal mode"),
            ("i, a, o", "Insert mode – the editor shortcuts above work here"),
            ("v", "Visual mode"),
            (":w", "Save and compile"),
        ],
    ),
];

/// Groups with the entries matching `query` (keys or description, any case).
pub fn filtered(query: &str) -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    let q = query.trim().to_lowercase();
    GROUPS
        .iter()
        .map(|(g, items)| {
            let hit = |s: &str| s.to_lowercase().contains(&q);
            let items = items.iter().copied().filter(|(k, d)| q.is_empty() || hit(k) || hit(d) || hit(g)).collect::<Vec<_>>();
            (*g, items)
        })
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_finds_keys_and_descriptions() {
        assert_eq!(filtered("").len(), GROUPS.len());
        let r = filtered("emph");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].1[0].0, "Ctrl+E");
        let r = filtered("ctrl+d");
        assert!(r.iter().any(|(_, i)| i.iter().any(|(k, _)| *k == "Ctrl+D")));
        assert!(filtered("nothing like this").is_empty());
    }
}
