//! Shell-only actions: zoom, layouts, fullscreen, preview scope, and Quit. They change view
//! state or the window, never the project, so they are not engine commands. One table drives
//! the menus and the keyboard, so a key and its menu entry cannot drift apart. The menu
//! layouts (`menus::LAYOUTS`) place them: most in View, Help in Help, Quit in File.

/// One shell action. `id` follows the spec's command ids (`view.*`, `app.*`).
#[derive(Clone, Copy, Debug)]
pub struct ShellAction {
    pub id: &'static str,
    pub label: &'static str,
    /// Text like `Ctrl+0`, parsed by [`crate::shortcuts::parse`].
    pub shortcut: Option<&'static str>,
}

const fn action(
    id: &'static str,
    label: &'static str,
    shortcut: Option<&'static str>,
) -> ShellAction {
    ShellAction {
        id,
        label,
        shortcut,
    }
}

pub const ACTIONS: &[ShellAction] = &[
    action("view.zoomIn", "Zoom In", Some("Ctrl+Equals")),
    action("view.zoomOut", "Zoom Out", Some("Ctrl+Minus")),
    action("view.fit", "Best Fit", Some("Ctrl+0")),
    action("view.actualSize", "Actual Size", Some("Alt+Ctrl+0")),
    action("view.fitWidth", "Fit Width", Some("Alt+ArrowRight")),
    action("view.fitHeight", "Fit Height", Some("Alt+ArrowDown")),
    action("view.syncSources", "Link Image Views", None),
    action("view.syncPreview", "Link Preview View", None),
    action("view.mode.full", "Full Layout", Some("F5")),
    action("view.mode.edit", "Edit Layout", Some("F6")),
    action("view.mode.preview", "Preview Layout", Some("F7")),
    action("view.mode.frames", "Frame Sequence", Some("F8")),
    action("view.mode.dual", "Dual Window", Some("F9")),
    action("view.fullscreen", "Full Screen", Some("F11")),
    action("view.sequenceList", "Sequence List", Some("F2")),
    action("view.previewScope", "Preview Whole Movie", None),
    action(
        "view.toggle.checkerboard",
        "Transparency Checkerboard",
        None,
    ),
    action("view.toggle.numbers", "Dot Numbers", None),
    action("view.toggle.mute", "Mute Sound", None),
    action("view.help", "Inbetween Help", Some("F1")),
    action("app.quit", "Quit", Some("Ctrl+Q")),
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
