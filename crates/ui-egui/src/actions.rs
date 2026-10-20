//! Shell-only view actions: zoom, layouts, fullscreen, preview scope. They change view state,
//! never the project, so they are not engine commands. One table drives the View menu and the
//! keyboard, so a key and its menu entry cannot drift apart.

/// One view action. `id` follows the spec's command ids (`view.*`).
#[derive(Clone, Copy, Debug)]
pub struct ShellAction {
    pub id: &'static str,
    pub label: &'static str,
    /// Text like `Ctrl+0`, parsed by [`crate::shortcuts::parse`].
    pub shortcut: Option<&'static str>,
    /// A blank line goes before this entry in the menu.
    pub gap_before: bool,
}

const fn action(
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
    gap_before: bool,
) -> ShellAction {
    ShellAction {
        id,
        label,
        shortcut,
        gap_before,
    }
}

pub const ACTIONS: &[ShellAction] = &[
    action("view.zoomIn", "Zoom In", Some("Ctrl+Equals"), false),
    action("view.zoomOut", "Zoom Out", Some("Ctrl+Minus"), false),
    action("view.fit", "Best Fit", Some("Ctrl+0"), false),
    action(
        "view.actualSize",
        "Actual Size (100%)",
        Some("Alt+Ctrl+0"),
        false,
    ),
    action("view.fitWidth", "Fit Width", Some("Alt+ArrowRight"), false),
    action("view.fitHeight", "Fit Height", Some("Alt+ArrowDown"), false),
    action("view.syncSources", "Link the Two Image Views", None, true),
    action("view.syncPreview", "Link the Preview View", None, false),
    action("view.mode.full", "Full Layout", Some("F5"), true),
    action("view.mode.edit", "Edit Layout", Some("F6"), false),
    action("view.mode.preview", "Preview Layout", Some("F7"), false),
    action("view.mode.frames", "Frame Sequence", Some("F8"), false),
    action("view.mode.dual", "Dual Window", Some("F9"), false),
    action("view.fullscreen", "Full Screen", Some("F11"), false),
    action("view.sequenceList", "Sequence List", Some("F2"), true),
    action("view.previewScope", "Preview the Whole Movie", None, false),
    action(
        "view.toggle.checkerboard",
        "Checkerboard Behind Transparency",
        None,
        false,
    ),
    action("view.toggle.numbers", "Dot Numbers", None, false),
    action("view.toggle.mute", "Mute Sound", None, false),
    action("view.help", "Help", None, true),
];

/// Extra spellings of the same actions that the menu does not list.
pub const ALIASES: &[(&str, &str)] = &[
    ("Alt+Enter", "view.fullscreen"),
    ("Ctrl+Period", "view.zoomIn"),
    ("Ctrl+Comma", "view.zoomOut"),
    ("Ctrl+Slash", "view.fit"),
];

pub fn find(id: &str) -> Option<&'static ShellAction> {
    ACTIONS.iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts;

    #[test]
    fn ids_are_unique() {
        for (i, a) in ACTIONS.iter().enumerate() {
            assert!(ACTIONS[i + 1..].iter().all(|b| b.id != a.id), "{}", a.id);
        }
    }

    #[test]
    fn every_shortcut_parses_and_none_clashes_with_a_command_or_another_action() {
        let registry = inbetween_engine::Registry::standard();
        let mut seen = Vec::new();
        let texts = ACTIONS
            .iter()
            .filter_map(|a| a.shortcut.map(|s| (a.id, s)))
            .chain(ALIASES.iter().map(|(s, id)| (*id, *s)))
            .chain(
                registry
                    .specs()
                    .iter()
                    .filter_map(|c| c.shortcut.map(|s| (c.id, s))),
            );
        for (id, text) in texts {
            let parsed = shortcuts::parse(text)
                .unwrap_or_else(|| panic!("{id} has a shortcut that does not parse: {text}"));
            assert!(!seen.contains(&parsed), "{text} ({id}) is used twice");
            seen.push(parsed);
        }
    }

    #[test]
    fn aliases_point_at_real_actions() {
        for (_, id) in ALIASES {
            assert!(find(id).is_some(), "{id}");
        }
    }
}
