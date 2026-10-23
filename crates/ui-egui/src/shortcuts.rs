//! Turning the engine's shortcut text ("Ctrl+Shift+Z") into egui shortcuts.

use egui::{Key, KeyboardShortcut, Modifiers};

/// Parse text like `Ctrl+O`, `Ctrl+Shift+Z` or `Delete`. Returns `None` for anything we do not
/// recognise, so a typo in a command's shortcut disables it instead of crashing.
pub fn parse(text: &str) -> Option<KeyboardShortcut> {
    let mut modifiers = Modifiers::NONE;
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "command" => modifiers.command = true,
            "shift" => modifiers.shift = true,
            "alt" => modifiers.alt = true,
            other => {
                if key.is_some() {
                    return None;
                }
                key = Key::from_name(part).or_else(|| Key::from_name(&capitalise(other)));
            }
        }
    }
    key.map(|k| KeyboardShortcut::new(modifiers, k))
}

fn capitalise(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// How to show a shortcut in a menu: short key names and symbols ("Del", "=", arrows), so a key
/// does not read like a label.
pub fn label(text: &str) -> String {
    text.split('+')
        .map(|key| match key {
            "Delete" => "Del",
            "Equals" => "=",
            "Minus" => "\u{2212}",
            "ArrowLeft" => "\u{2190}",
            "ArrowUp" => "\u{2191}",
            "ArrowRight" => "\u{2192}",
            "ArrowDown" => "\u{2193}",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shortcuts_the_engine_uses() {
        let s = parse("Ctrl+Z").unwrap();
        assert!(s.modifiers.command && !s.modifiers.shift);
        assert_eq!(s.logical_key, Key::Z);
        assert_eq!(
            parse("Ctrl+Shift+Z").unwrap().modifiers,
            Modifiers::COMMAND | Modifiers::SHIFT
        );
        assert_eq!(
            parse("Delete").unwrap(),
            KeyboardShortcut::new(Modifiers::NONE, Key::Delete)
        );
        assert_eq!(parse("Ctrl+S").unwrap().logical_key, Key::S);
        assert_eq!(parse("ctrl+n").unwrap().logical_key, Key::N);
    }

    #[test]
    fn menus_show_short_key_names() {
        assert_eq!(label("Delete"), "Del");
        assert_eq!(label("Ctrl+Shift+A"), "Ctrl+Shift+A");
        assert_eq!(label("Ctrl+Equals"), "Ctrl+=");
        assert_eq!(label("Alt+ArrowRight"), "Alt+\u{2192}");
    }

    #[test]
    fn rejects_nonsense() {
        for bad in ["", "Ctrl+", "Ctrl+Banana", "Z+X", "+", "Ctrl++Z+"] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn every_shortcut_in_the_registry_parses_and_is_unique() {
        let registry = inbetween_engine::Registry::standard();
        let mut seen = Vec::new();
        for spec in registry.specs() {
            if let Some(text) = spec.shortcut {
                let parsed =
                    parse(text).unwrap_or_else(|| panic!("{} has bad shortcut {text}", spec.id));
                assert!(!seen.contains(&parsed), "{text} is used twice");
                seen.push(parsed);
            }
        }
        assert!(!seen.is_empty());
    }
}
