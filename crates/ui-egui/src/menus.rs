//! The menu bar, built from the engine's command list rather than written by hand.
//!
//! Every command with a `menu` path such as `File/Open...` appears in the matching menu in the
//! order the registry lists it, so a command added to the engine shows up here with no UI change.

use inbetween_engine::CommandSpec;

/// Menus in the order they appear; any other top-level name follows, in registry order.
const MENU_ORDER: &[&str] = &["File", "Edit", "Dots", "Image", "Sequence", "Effects"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuNode {
    Menu {
        name: String,
        children: Vec<MenuNode>,
    },
    Command {
        id: &'static str,
        label: String,
    },
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
        MenuNode::Command { .. } => MENU_ORDER.len(),
    });
    roots
}

fn insert(nodes: &mut Vec<MenuNode>, path: &[&str], leaf: &str, id: &'static str) {
    let Some((first, rest)) = path.split_first() else {
        nodes.push(MenuNode::Command {
            id,
            label: leaf.to_string(),
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
            })
            .collect()
    }

    #[test]
    fn standard_menus_come_in_order() {
        let menus = build(Registry::standard().specs());
        assert_eq!(&names(&menus)[..2], ["File", "Edit"]);
        assert!(names(&menus).contains(&"Dots") && names(&menus).contains(&"Sequence"));
    }

    #[test]
    fn nested_menus_are_grouped() {
        let menus = build(Registry::standard().specs());
        let file = menus
            .iter()
            .find_map(|n| match n {
                MenuNode::Menu { name, children } if name == "File" => Some(children),
                _ => None,
            })
            .unwrap();
        let export = file.iter().find_map(|n| match n {
            MenuNode::Menu { name, children } if name == "Export" => Some(children),
            _ => None,
        });
        let export = export.expect("File/Export submenu");
        assert!(export.iter().any(|n| matches!(
            n,
            MenuNode::Command {
                id: "export.frame",
                ..
            }
        )));
        assert!(export.iter().any(|n| matches!(
            n,
            MenuNode::Command {
                id: "export.sequence",
                ..
            }
        )));
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
                })
                .sum()
        }
        for spec in registry.specs() {
            let expected = usize::from(!spec.menu.is_empty());
            assert_eq!(count(&menus, spec.id), expected, "{}", spec.id);
        }
    }

    #[test]
    fn canvas_commands_have_no_menu_entry() {
        let menus = build(Registry::standard().specs());
        fn has(nodes: &[MenuNode], id: &str) -> bool {
            nodes.iter().any(|n| match n {
                MenuNode::Command { id: i, .. } => *i == id,
                MenuNode::Menu { children, .. } => has(children, id),
            })
        }
        for id in ["dots.add", "dots.move", "dots.delete", "edit.beginGroup"] {
            assert!(!has(&menus, id), "{id}");
        }
    }
}
