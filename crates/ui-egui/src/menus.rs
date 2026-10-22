//! The menu bar, built from the engine's command list rather than written by hand.
//!
//! Every command with a `menu` path such as `File/Open...` appears in the matching menu, so a
//! command added to the engine shows up here with no UI change. [`LAYOUTS`] orders a menu and
//! splits it into groups; a menu with no layout lists its commands in registry order.

use inbetween_engine::CommandSpec;

/// Menus in the order they appear; any other top-level name follows, in registry order.
const MENU_ORDER: &[&str] = &["File", "Edit", "Dots", "Image", "Sequence", "Effects"];

/// The order of a menu's entries, by menu path. `-` is a separator, `# Text` a small heading,
/// `> Name` a submenu, `!id` a shell action ([`crate::actions`]), anything else a command id,
/// optionally `id = Label` to show it under another name than its menu path. Commands of the
/// menu the layout leaves out follow at the end, so none can go missing.
const LAYOUTS: &[(&str, &[&str])] = &[
    (
        "File",
        &[
            "file.new",
            "project.new",
            "file.wizard",
            "-",
            "project.open",
            "> Open Recent",
            "-",
            "project.save",
            "project.saveAs",
            "-",
            "file.appendProject",
            "> Export",
            "-",
            "file.close",
            "!app.quit",
        ],
    ),
    (
        "Edit",
        &[
            "edit.undo",
            "edit.redo",
            "-",
            "edit.cut",
            "edit.copy",
            "edit.paste",
            "edit.delete",
            "-",
            "dots.selectAll",
            "dots.selectNone",
        ],
    ),
    (
        "File/Export",
        &[
            "# Whole movie",
            "export.video",
            "export.animation",
            "export.movie = Image Sequence...",
            "# This sequence",
            "export.sequence",
            "# This frame",
            "export.frame",
            "frame.exportEdit",
            "-",
            "export.pdf",
        ],
    ),
];

/// Quiet hints at the right of a command, where a shortcut would sit.
const ASIDES: &[(&str, &str)] = &[
    ("export.video", "MP4, WebM, ProRes"),
    ("export.animation", "GIF, APNG, WebP"),
    ("export.movie", "PNG"),
    ("export.sequence", "PNG"),
    ("export.pdf", "PDF"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuNode {
    Menu {
        name: String,
        children: Vec<MenuNode>,
    },
    Command {
        id: &'static str,
        label: String,
        /// A hint shown at the right in place of a shortcut.
        aside: Option<&'static str>,
    },
    /// A shell action (Quit), not an engine command.
    Action {
        id: &'static str,
    },
    Heading(&'static str),
    Separator,
}

/// Group `specs` into menus. Commands with an empty menu path are left out (the shell calls
/// them from the canvas or from keys).
pub fn build(specs: &[CommandSpec]) -> Vec<MenuNode> {
    let mut roots: Vec<MenuNode> = Vec::new();
    for spec in specs.iter().filter(|s| !s.menu.is_empty()) {
        let parts: Vec<&str> = spec.menu.split('/').filter(|p| !p.is_empty()).collect();
        if let Some((leaf, path)) = parts.split_last() {
            insert(&mut roots, path, leaf, spec.id);
        }
    }
    roots.sort_by_key(|n| match n {
        MenuNode::Menu { name, .. } => MENU_ORDER
            .iter()
            .position(|m| m == name)
            .unwrap_or(MENU_ORDER.len()),
        _ => MENU_ORDER.len(),
    });
    roots
        .into_iter()
        .map(|n| match n {
            MenuNode::Menu { name, children } => MenuNode::Menu {
                children: arrange(children, &name),
                name,
            },
            other => other,
        })
        .collect()
}

/// Menu text as it is shown: a real ellipsis for the engine's three dots.
fn display(label: &str) -> String {
    label.replace("...", "\u{2026}")
}

fn insert(nodes: &mut Vec<MenuNode>, path: &[&str], leaf: &str, id: &'static str) {
    let Some((first, rest)) = path.split_first() else {
        nodes.push(MenuNode::Command {
            id,
            label: display(leaf),
            aside: ASIDES.iter().find(|(i, _)| *i == id).map(|(_, a)| *a),
        });
        return;
    };
    let existing = nodes
        .iter()
        .position(|n| matches!(n, MenuNode::Menu { name, .. } if name == first));
    let at = existing.unwrap_or_else(|| {
        nodes.push(MenuNode::Menu {
            name: (*first).to_string(),
            children: Vec::new(),
        });
        nodes.len() - 1
    });
    if let Some(MenuNode::Menu { children, .. }) = nodes.get_mut(at) {
        insert(children, rest, leaf, id);
    }
}

/// Put the entries of the menu at `path` (and its submenus) in their layout's order.
fn arrange(nodes: Vec<MenuNode>, path: &str) -> Vec<MenuNode> {
    let mut pool: Vec<MenuNode> = nodes
        .into_iter()
        .map(|n| match n {
            MenuNode::Menu { name, children } => MenuNode::Menu {
                children: arrange(children, &format!("{path}/{name}")),
                name,
            },
            other => other,
        })
        .collect();
    let Some((_, layout)) = LAYOUTS.iter().find(|(p, _)| *p == path) else {
        return pool;
    };
    let mut out = Vec::new();
    for entry in *layout {
        if *entry == "-" {
            out.push(MenuNode::Separator);
        } else if let Some(text) = entry.strip_prefix("# ") {
            out.push(MenuNode::Heading(text));
        } else if let Some(id) = entry.strip_prefix('!') {
            out.push(MenuNode::Action { id });
        } else if let Some(want) = entry.strip_prefix("> ") {
            let at = pool
                .iter()
                .position(|n| matches!(n, MenuNode::Menu { name, .. } if name == want));
            out.push(match at {
                Some(i) => pool.remove(i),
                // The shell fills some submenus itself (Open Recent).
                None => MenuNode::Menu {
                    name: want.to_string(),
                    children: Vec::new(),
                },
            });
        } else {
            let (want, rename) = match entry.split_once(" = ") {
                Some((id, label)) => (id, Some(label)),
                None => (*entry, None),
            };
            let at = pool
                .iter()
                .position(|n| matches!(n, MenuNode::Command { id, .. } if *id == want));
            if let Some(mut node) = at.map(|i| pool.remove(i)) {
                if let (MenuNode::Command { label, .. }, Some(text)) = (&mut node, rename) {
                    *label = display(text);
                }
                out.push(node);
            }
        }
    }
    if !pool.is_empty() {
        out.push(MenuNode::Separator);
        out.append(&mut pool);
    }
    tidy(out)
}

/// Drop separators at the ends or next to each other, and headings with nothing under them.
fn tidy(nodes: Vec<MenuNode>) -> Vec<MenuNode> {
    let mut out: Vec<MenuNode> = Vec::with_capacity(nodes.len());
    for node in nodes {
        let breaks = matches!(node, MenuNode::Separator | MenuNode::Heading(_));
        if breaks && matches!(out.last(), Some(MenuNode::Heading(_))) {
            out.pop();
        }
        if matches!(node, MenuNode::Separator)
            && matches!(out.last(), None | Some(MenuNode::Separator))
        {
            continue;
        }
        out.push(node);
    }
    while matches!(out.last(), Some(MenuNode::Separator | MenuNode::Heading(_))) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_engine::Registry;

    fn names(nodes: &[MenuNode]) -> Vec<&str> {
        nodes
            .iter()
            .map(|n| match n {
                MenuNode::Menu { name, .. } => name.as_str(),
                MenuNode::Command { label, .. } => label.as_str(),
                MenuNode::Action { id } => id,
                MenuNode::Heading(text) => text,
                MenuNode::Separator => "-",
            })
            .collect()
    }

    fn submenu<'a>(nodes: &'a [MenuNode], want: &str) -> &'a [MenuNode] {
        nodes
            .iter()
            .find_map(|n| match n {
                MenuNode::Menu { name, children } if name == want => Some(children.as_slice()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn standard_menus_come_in_order() {
        let menus = build(Registry::standard().specs());
        assert_eq!(&names(&menus)[..2], ["File", "Edit"]);
        assert!(names(&menus).contains(&"Dots") && names(&menus).contains(&"Sequence"));
    }

    #[test]
    fn the_edit_menu_follows_its_layout() {
        let menus = build(Registry::standard().specs());
        assert_eq!(
            names(submenu(&menus, "Edit")),
            [
                "Undo",
                "Redo",
                "-",
                "Cut",
                "Copy",
                "Paste",
                "Delete",
                "-",
                "Select All Dots",
                "Deselect All",
            ]
        );
    }

    #[test]
    fn the_file_menu_follows_its_layout() {
        let menus = build(Registry::standard().specs());
        let file = submenu(&menus, "File");
        assert_eq!(
            names(file),
            [
                "New Project",
                "New From Two Images\u{2026}",
                "New From Image List\u{2026}",
                "-",
                "Open\u{2026}",
                "Open Recent",
                "-",
                "Save",
                "Save As\u{2026}",
                "-",
                "Add Sequences From Project\u{2026}",
                "Export",
                "-",
                "Close Project",
                "app.quit",
            ]
        );
        assert_eq!(
            names(submenu(file, "Open Recent")),
            ["Browse Projects\u{2026}"]
        );
        let export = submenu(file, "Export");
        assert_eq!(
            names(export),
            [
                "Whole movie",
                "Video\u{2026}",
                "Animated Image\u{2026}",
                "Image Sequence\u{2026}",
                "This sequence",
                "Image Sequence\u{2026}",
                "This frame",
                "Image\u{2026}",
                "Image With Dots\u{2026}",
                "-",
                "Contact Sheet\u{2026}",
            ]
        );
        assert!(export.iter().any(|n| matches!(
            n,
            MenuNode::Command {
                id: "export.pdf",
                aside: Some("PDF"),
                ..
            }
        )));
    }

    #[test]
    fn a_layout_never_hides_a_command_and_tidies_its_breaks() {
        let cmd = |id| MenuNode::Command {
            id,
            label: id.to_string(),
            aside: None,
        };
        // "export.extra" is not in the layout; the sequence and frame commands are missing.
        let out = arrange(
            vec![cmd("export.extra"), cmd("export.video"), cmd("export.pdf")],
            "File/Export",
        );
        assert_eq!(
            names(&out),
            [
                "Whole movie",
                "export.video",
                "-",
                "export.pdf",
                "-",
                "export.extra"
            ]
        );
        assert_eq!(
            tidy(vec![
                MenuNode::Separator,
                MenuNode::Heading("a"),
                MenuNode::Separator,
                MenuNode::Separator,
                cmd("x"),
                MenuNode::Heading("b"),
            ]),
            vec![cmd("x")]
        );
    }

    #[test]
    fn every_command_with_a_menu_appears_exactly_once() {
        let registry = Registry::standard();
        let menus = build(registry.specs());
        fn count(nodes: &[MenuNode], id: &str) -> usize {
            nodes
                .iter()
                .map(|n| match n {
                    MenuNode::Command { id: i, .. } => usize::from(*i == id),
                    MenuNode::Menu { children, .. } => count(children, id),
                    _ => 0,
                })
                .sum()
        }
        for spec in registry.specs() {
            let expected = usize::from(!spec.menu.is_empty());
            assert_eq!(count(&menus, spec.id), expected, "{}", spec.id);
        }
    }

    #[test]
    fn every_layout_entry_names_a_real_command_or_action() {
        let registry = Registry::standard();
        for (_, layout) in LAYOUTS {
            for entry in *layout {
                if *entry == "-" || entry.starts_with("# ") || entry.starts_with("> ") {
                    continue;
                }
                if let Some(id) = entry.strip_prefix('!') {
                    assert!(crate::actions::find(id).is_some(), "{id}");
                    continue;
                }
                let id = entry.split(" = ").next().unwrap();
                assert!(registry.get(id).is_some(), "{id}");
            }
        }
    }

    #[test]
    fn canvas_commands_have_no_menu_entry() {
        let menus = build(Registry::standard().specs());
        fn has(nodes: &[MenuNode], id: &str) -> bool {
            nodes.iter().any(|n| match n {
                MenuNode::Command { id: i, .. } => *i == id,
                MenuNode::Menu { children, .. } => has(children, id),
                _ => false,
            })
        }
        for id in ["dots.add", "dots.move", "dots.delete", "edit.beginGroup"] {
            assert!(!has(&menus, id), "{id}");
        }
    }
}
