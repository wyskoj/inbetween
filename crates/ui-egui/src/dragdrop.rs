//! Dropping files onto the window. Deciding what each dropped file means is a pure function
//! ([`route_drop`]) so it can be tested without a window; the app carries the result out
//! through engine commands.

use crate::state::{EffectScope, Panel};

/// Where on the window a drop landed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DropZone {
    #[default]
    Elsewhere,
    Panel(Panel),
    /// An effect's thumbnail slot in the Effects window: the effect command and its scope.
    EffectSlot {
        effect: &'static str,
        scope: EffectScope,
    },
}

impl From<Option<Panel>> for DropZone {
    fn from(p: Option<Panel>) -> Self {
        p.map_or(Self::Elsewhere, Self::Panel)
    }
}

impl DropZone {
    fn panel(self) -> Option<Panel> {
        match self {
            Self::Panel(p) => Some(p),
            _ => None,
        }
    }
}

/// One dropped file. A platform that hands over only the bytes (a browser) gives no path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dropped {
    pub path: Option<String>,
    /// A name to show when there is no path.
    pub name: String,
}

/// What to do with a drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropAction {
    /// Put an image into slot 1 or 2 of the current sequence.
    SetImage { image: u8, path: String },
    /// Put an image into an effect's slot (the effect is switched on).
    SetEffectImage {
        effect: &'static str,
        scope: EffectScope,
        path: String,
    },
    /// Use a WAV or MP3 file as the movie's sound.
    SetSound(String),
    /// Open a project (asking first about unsaved changes).
    OpenProject(String),
    /// Replace the current sequence's dots.
    ImportDots(String),
    /// Several images: open the wizard with them filled in, in drop order.
    Wizard(Vec<String>),
    /// Something that cannot be used; the message says why.
    Unsupported(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Project,
    Dots,
    Image,
    Sound,
    Other,
}

fn kind_of(path: &str) -> Kind {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "inbetween" => Kind::Project,
        "inbetweendots" => Kind::Dots,
        "wav" | "mp3" => Kind::Sound,
        e if inbetween_codecs::IMAGE_EXTENSIONS.contains(&e) => Kind::Image,
        _ => Kind::Other,
    }
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// Decide what a drop means.
///
/// - A project file opens (and wins over anything dropped with it).
/// - One image goes to the image panel it was dropped on; dropped anywhere else it fills
///   Image 1 if that is empty, otherwise Image 2.
/// - Two or more images open the wizard, in drop order.
/// - A dot file replaces the current sequence's dots.
/// - Anything else, and files with no path, are reported as unsupported.
pub fn route_drop(
    files: &[Dropped],
    over: impl Into<DropZone>,
    image1_empty: bool,
) -> Vec<DropAction> {
    let zone: DropZone = over.into();
    let over = zone.panel();
    let mut out = Vec::new();
    let mut projects = Vec::new();
    let mut dots = Vec::new();
    let mut images = Vec::new();
    let mut sounds = Vec::new();
    for f in files {
        let Some(path) = &f.path else {
            out.push(DropAction::Unsupported(format!(
                "{}: dropped data without a file path is not supported here",
                f.name
            )));
            continue;
        };
        match kind_of(path) {
            Kind::Project => projects.push(path.clone()),
            Kind::Dots => dots.push(path.clone()),
            Kind::Image => images.push(path.clone()),
            Kind::Sound => sounds.push(path.clone()),
            Kind::Other => out.push(DropAction::Unsupported(format!(
                "{}: not an image, project or dot file",
                file_name(path)
            ))),
        }
    }
    if let Some(first) = projects.first() {
        // Opening replaces everything, so the rest of the drop is not acted on.
        let mut only = vec![DropAction::OpenProject(first.clone())];
        if files.len() > 1 {
            only.push(DropAction::Unsupported(
                "A project was dropped, so the other files were ignored".into(),
            ));
        }
        return only;
    }
    match images.len() {
        0 => {}
        1 => match zone {
            DropZone::EffectSlot {
                effect: "effect.sound",
                ..
            } => out.push(DropAction::Unsupported(
                "The Sound slot takes a WAV or MP3 file".into(),
            )),
            DropZone::EffectSlot { effect, scope } => out.push(DropAction::SetEffectImage {
                effect,
                scope,
                path: images.remove(0),
            }),
            _ => {
                let image = match over {
                    Some(Panel::Source1) => 1,
                    Some(Panel::Source2) => 2,
                    _ if image1_empty => 1,
                    _ => 2,
                };
                out.push(DropAction::SetImage {
                    image,
                    path: images.remove(0),
                });
            }
        },
        _ => out.push(DropAction::Wizard(images)),
    }
    if let Some(first) = sounds.first() {
        out.push(DropAction::SetSound(first.clone()));
        if sounds.len() > 1 {
            out.push(DropAction::Unsupported(
                "Only the first sound file was used".into(),
            ));
        }
    }
    if let Some(first) = dots.first() {
        out.push(DropAction::ImportDots(first.clone()));
        if dots.len() > 1 {
            out.push(DropAction::Unsupported(
                "Only the first dot file was used".into(),
            ));
        }
    }
    out
}

/// Words for the overlay shown while files hover over the window.
pub fn hover_hint(files: &[Dropped], over: impl Into<DropZone>, image1_empty: bool) -> String {
    let actions = route_drop(files, over, image1_empty);
    let Some(first) = actions.first() else {
        return "Drop files here".into();
    };
    match first {
        DropAction::SetImage { image, .. } => format!("Drop to set Image {image}"),
        DropAction::SetEffectImage { effect, scope, .. } => {
            let name = effect.strip_prefix("effect.").unwrap_or(effect);
            format!("Drop to set the {} {name} image", scope.id())
        }
        DropAction::SetSound(_) => "Drop to use as the movie's sound".into(),
        DropAction::OpenProject(_) => "Drop to open the project".into(),
        DropAction::ImportDots(_) => "Drop to import the dots".into(),
        DropAction::Wizard(paths) => {
            format!("Drop to start the wizard with {} images", paths.len())
        }
        DropAction::Unsupported(m) => m.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drop(paths: &[&str]) -> Vec<Dropped> {
        paths
            .iter()
            .map(|p| Dropped {
                path: Some((*p).to_string()),
                name: (*p).to_string(),
            })
            .collect()
    }

    fn set(image: u8, path: &str) -> DropAction {
        DropAction::SetImage {
            image,
            path: path.into(),
        }
    }

    #[test]
    fn one_image_goes_to_the_panel_it_lands_on() {
        let f = drop(&["a.png"]);
        assert_eq!(
            route_drop(&f, Some(Panel::Source1), false),
            [set(1, "a.png")]
        );
        assert_eq!(
            route_drop(&f, Some(Panel::Source2), true),
            [set(2, "a.png")]
        );
    }

    #[test]
    fn one_image_elsewhere_fills_the_first_empty_slot() {
        let f = drop(&["a.PNG"]);
        for over in [None, Some(Panel::Preview)] {
            assert_eq!(route_drop(&f, over, true), [set(1, "a.PNG")]);
            assert_eq!(route_drop(&f, over, false), [set(2, "a.PNG")]);
        }
    }

    #[test]
    fn several_images_open_the_wizard_in_drop_order() {
        let f = drop(&["c.png", "a.bmp", "b.psd"]);
        assert_eq!(
            route_drop(&f, Some(Panel::Source1), false),
            [DropAction::Wizard(vec![
                "c.png".into(),
                "a.bmp".into(),
                "b.psd".into()
            ])]
        );
    }

    #[test]
    fn every_image_format_counts_as_an_image() {
        for ext in inbetween_codecs::IMAGE_EXTENSIONS {
            let f = drop(&[&format!("x.{ext}")]);
            assert!(matches!(
                route_drop(&f, None, true)[0],
                DropAction::SetImage { .. }
            ));
        }
    }

    #[test]
    fn projects_and_dot_files_have_their_own_targets() {
        assert_eq!(
            route_drop(&drop(&["m.inbetween"]), None, true),
            [DropAction::OpenProject("m.inbetween".into())]
        );
        assert_eq!(
            route_drop(&drop(&["d.INBETWEENDOTS"]), Some(Panel::Source2), true),
            [DropAction::ImportDots("d.INBETWEENDOTS".into())]
        );
        // A project wins over everything dropped with it.
        let mixed = route_drop(&drop(&["a.png", "m.inbetween", "b.png"]), None, true);
        assert_eq!(mixed[0], DropAction::OpenProject("m.inbetween".into()));
        assert!(matches!(mixed[1], DropAction::Unsupported(_)));
        assert_eq!(mixed.len(), 2);
        // An image and a dot file together do both.
        let both = route_drop(&drop(&["a.png", "d.inbetweendots"]), None, true);
        assert_eq!(
            both,
            [
                set(1, "a.png"),
                DropAction::ImportDots("d.inbetweendots".into())
            ]
        );
        // Two dot files: the first is used and the person is told.
        let two = route_drop(&drop(&["1.inbetweendots", "2.inbetweendots"]), None, true);
        assert_eq!(two[0], DropAction::ImportDots("1.inbetweendots".into()));
        assert!(matches!(two[1], DropAction::Unsupported(_)));
    }

    #[test]
    fn unsupported_files_and_missing_paths_are_reported_not_acted_on() {
        let f = drop(&["notes.txt", "movie.mp4", "noext"]);
        let actions = route_drop(&f, None, true);
        assert_eq!(actions.len(), 3);
        assert!(actions
            .iter()
            .all(|a| matches!(a, DropAction::Unsupported(_))));
        let bytes_only = [Dropped {
            path: None,
            name: "photo.png".into(),
        }];
        let actions = route_drop(&bytes_only, None, true);
        assert!(matches!(&actions[..], [DropAction::Unsupported(m)] if m.contains("photo.png")));
        assert!(route_drop(&[], None, true).is_empty());
        // A real image next to junk still works.
        let mixed = route_drop(&drop(&["a.png", "x.txt"]), None, true);
        assert_eq!(mixed.len(), 2);
        assert_eq!(
            mixed[0],
            DropAction::Unsupported("x.txt: not an image, project or dot file".into())
        );
        assert_eq!(mixed[1], set(1, "a.png"));
    }

    #[test]
    fn hover_hints_describe_the_result() {
        assert_eq!(
            hover_hint(&drop(&["a.png"]), Some(Panel::Source2), true),
            "Drop to set Image 2"
        );
        assert_eq!(
            hover_hint(&drop(&["a.png", "b.png"]), None, true),
            "Drop to start the wizard with 2 images"
        );
        assert_eq!(hover_hint(&[], None, true), "Drop files here");
    }
}
