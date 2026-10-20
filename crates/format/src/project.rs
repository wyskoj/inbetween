use crate::{check_header, check_image_path, decode_text, parse_error, FormatError};
use base64::Engine as _;
use inbetween_doc::Project;
use serde::{Deserialize, Serialize};

/// Marker written into every project file.
pub const PROJECT_FORMAT: &str = "inbetween-project";
/// The newest project-file version this build reads and the one it writes.
pub const PROJECT_VERSION: u32 = 1;
/// Largest project file the reader accepts, in bytes.
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Most sequences a project may hold.
pub const MAX_SEQUENCES: usize = 10_000;

/// Longest side of an embedded thumbnail, in pixels.
pub const MAX_THUMBNAIL_PIXELS: u32 = 512;
/// Largest embedded thumbnail PNG, in bytes once decoded.
pub const MAX_THUMBNAIL_PNG_BYTES: usize = 1024 * 1024;

const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// A small preview of the project, embedded in the file so a file browser can show it
/// without rendering. `png` is the base64 text of a PNG.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub png: String,
}

impl Thumbnail {
    /// Wrap PNG bytes, refusing a thumbnail the reader would refuse.
    pub fn from_png(width: u32, height: u32, png: &[u8]) -> Result<Self, FormatError> {
        let t = Self {
            width,
            height,
            png: base64::engine::general_purpose::STANDARD.encode(png),
        };
        t.decode()?;
        Ok(t)
    }

    /// The PNG bytes, after checking every limit.
    pub fn decode(&self) -> Result<Vec<u8>, FormatError> {
        let bad = |m: &str| FormatError::Parse(format!("bad thumbnail: {m}"));
        let side = 1..=MAX_THUMBNAIL_PIXELS;
        if !side.contains(&self.width) || !side.contains(&self.height) {
            return Err(bad("size is out of range"));
        }
        // Base64 grows by 4/3; check before decoding so a huge string is never expanded.
        if self.png.len() > MAX_THUMBNAIL_PNG_BYTES / 3 * 4 + 8 {
            return Err(bad("image is too large"));
        }
        let png = base64::engine::general_purpose::STANDARD
            .decode(self.png.as_bytes())
            .map_err(|_| bad("not valid base64"))?;
        if png.len() > MAX_THUMBNAIL_PNG_BYTES {
            return Err(bad("image is too large"));
        }
        if !png.starts_with(PNG_SIGNATURE) {
            return Err(bad("not a PNG"));
        }
        Ok(png)
    }
}

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumbnail: Option<&'a Thumbnail>,
    project: &'a Project,
}

#[derive(Deserialize, Default)]
struct ThumbnailIn {
    #[serde(default)]
    thumbnail: Option<Thumbnail>,
}

/// Read only the embedded thumbnail of project-file bytes: `Ok(None)` when the file has
/// none, an error when it has one that breaks a limit. Returns the size and the PNG bytes.
pub fn thumbnail_from_bytes(bytes: &[u8]) -> Result<Option<(u32, u32, Vec<u8>)>, FormatError> {
    let text = decode_text(bytes, MAX_FILE_BYTES)?;
    check_header(text, PROJECT_FORMAT, PROJECT_VERSION)?;
    let env: ThumbnailIn = serde_json::from_str(text).map_err(parse_error)?;
    match env.thumbnail {
        None => Ok(None),
        Some(t) => {
            let png = t.decode()?;
            Ok(Some((t.width, t.height, png)))
        }
    }
}

#[derive(Deserialize)]
struct EnvelopeIn {
    #[serde(default)]
    project: Project,
}

/// Serialise to pretty JSON. Fails if the project breaks a model limit or holds an
/// unusable image path, so a saved file always loads back.
pub fn project_to_string(project: &Project) -> Result<String, FormatError> {
    project_to_string_with_thumbnail(project, None)
}

/// Like [`project_to_string`], embedding a preview thumbnail.
pub fn project_to_string_with_thumbnail(
    project: &Project,
    thumbnail: Option<&Thumbnail>,
) -> Result<String, FormatError> {
    validate(project)?;
    if let Some(t) = thumbnail {
        t.decode()?;
    }
    let env = EnvelopeOut {
        format: PROJECT_FORMAT,
        version: PROJECT_VERSION,
        thumbnail,
        project,
    };
    let mut text = serde_json::to_string_pretty(&env)
        .map_err(|e| FormatError::Parse(format!("could not serialise: {e}")))?;
    text.push('\n');
    Ok(text)
}

/// Parse project-file text.
pub fn project_from_str(text: &str) -> Result<Project, FormatError> {
    project_from_bytes(text.as_bytes())
}

/// Parse project-file bytes (UTF-8, with or without a byte-order mark).
pub fn project_from_bytes(bytes: &[u8]) -> Result<Project, FormatError> {
    project_from_bytes_limited(bytes, MAX_FILE_BYTES)
}

pub(crate) fn project_from_bytes_limited(
    bytes: &[u8],
    limit: usize,
) -> Result<Project, FormatError> {
    let text = decode_text(bytes, limit)?;
    check_header(text, PROJECT_FORMAT, PROJECT_VERSION)?;
    let env: EnvelopeIn = serde_json::from_str(text).map_err(parse_error)?;
    validate(&env.project)?;
    Ok(env.project)
}

fn validate(project: &Project) -> Result<(), FormatError> {
    if project.sequences.len() > MAX_SEQUENCES {
        return Err(FormatError::TooManySequences {
            count: project.sequences.len(),
            max: MAX_SEQUENCES,
        });
    }
    project.validate()?;
    for image in project.all_paths() {
        check_image_path(&image.0)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{
        AdjustSettings, Align, BackColor, Camera, CameraKey, Caption, Crop, Curve, CurvePoint,
        DotPair, DotSet, Effects, Filter, FitMode, ImageEdit, ImageLayer, ImageRef, Light, Mask,
        MeshPolicy, MorphType, OutputSettings, ProjectType, Radial, Sequence, Shadow, Track,
        TrackPoint, Trail, Transition,
    };
    use inbetween_geom::Point;

    pub(crate) fn sample() -> Project {
        let dots = DotSet::new(vec![
            DotPair::new(Point::new(10.5, 20.25), Point::new(30.0, 40.125)),
            DotPair::fixed(Point::new(-5.0, 1e-7)),
            DotPair::new(
                Point::new(1.0 / 3.0, 2.0 / 3.0),
                Point::new(0.1 + 0.2, 1e21),
            ),
        ]);
        Project {
            kind: ProjectType::Layer,
            output: OutputSettings {
                width: 640,
                height: 480,
            },
            fps: 29.97,
            sequences: vec![
                Sequence {
                    name: "first \"quoted\" \u{1F600}".into(),
                    image1: ImageRef::new("images/a.png"),
                    image2: ImageRef::new("../shared/b.jpg"),
                    dots,
                    frames: 24,
                    hold_start: 3,
                    hold_end: 5,
                    morph: MorphType::Warp2,
                    fit: FitMode::Cover,
                    mesh: MeshPolicy::PerFrame,
                    drop_corner_tris: true,
                    feature_curve: Curve {
                        points: vec![
                            CurvePoint::new(0.0, 0.0),
                            CurvePoint::new(0.1 + 0.2, 1.0 / 3.0),
                            CurvePoint::new(1.0, 1.0),
                        ],
                        linear: true,
                    },
                    shape_curve: Curve::default(),
                    visible: false,
                    effects: Effects {
                        back_color: BackColor {
                            enabled: true,
                            rgba: [1, 2, 3, 4],
                        },
                        background: ImageLayer {
                            enabled: true,
                            image: ImageRef::new("bg/sky.png"),
                            opacity: 12.5,
                        },
                        mask: Mask {
                            enabled: true,
                            image: ImageRef::new("m.png"),
                            intensity: -33.3,
                            invert: true,
                        },
                        foreground: ImageLayer::default(),
                        transition: Transition {
                            enabled: true,
                            image: ImageRef::new("t/wipe.png"),
                            softness: 7.5,
                            invert: true,
                        },
                        light: Light {
                            enabled: true,
                            image: ImageRef::new("l.png"),
                            bright: 0.1 + 0.2,
                            dark: 99.0,
                        },
                        trail: Trail {
                            enabled: true,
                            overlay: false,
                            rgba: [9, 8, 7, 6],
                            intensity: 33.3,
                        },
                        radial: Radial {
                            enabled: true,
                            overlay: false,
                            rgba: [1, 1, 1, 1],
                            intensity: 1.0 / 3.0,
                        },
                    },
                    camera: Camera {
                        enabled: true,
                        keys: vec![
                            CameraKey::identity(0.0),
                            CameraKey {
                                t: 0.1 + 0.2,
                                x: -0.25,
                                y: 1.0 / 3.0,
                                scale_x: -2.0,
                                scale_y: 1.5,
                                rotation: 45.5,
                            },
                        ],
                        easing: Curve {
                            points: vec![
                                CurvePoint::new(0.0, 0.0),
                                CurvePoint::new(0.5, 0.25),
                                CurvePoint::new(1.0, 1.0),
                            ],
                            linear: false,
                        },
                        aspect: inbetween_doc::CameraAspect::Locked,
                    },
                    master_curve: Curve {
                        points: vec![CurvePoint::new(0.0, 0.25), CurvePoint::new(1.0, 0.0)],
                        linear: true,
                    },
                    track: Track {
                        enabled: true,
                        points: vec![
                            TrackPoint { u: 0.0, v: 0.0 },
                            TrackPoint { u: 0.4, v: -0.3 },
                            TrackPoint { u: 1.0, v: 0.0 },
                        ],
                        tension: 2.5,
                        flip_h: true,
                        flip_v: false,
                    },
                    edit1: ImageEdit {
                        quarter_turns: 3,
                        flip_h: true,
                        flip_v: false,
                        crop: Some(Crop {
                            x: -2.5,
                            y: 1.0 / 3.0,
                            w: 100.25,
                            h: 50.5,
                        }),
                        angle: 12.5,
                        expand: true,
                        adjust: AdjustSettings {
                            brightness: 10.5,
                            contrast: -20.0,
                            red: 1.0,
                            green: 2.0,
                            blue: 3.0,
                            hue: -45.5,
                            saturation: 33.3,
                            negative: true,
                            grayscale: false,
                        },
                        filters: vec![
                            Filter::Gaussian { sigma: 0.1 + 0.2 },
                            Filter::Mosaic { cell: 9 },
                            Filter::Splash {
                                radius: 7,
                                seed: u64::MAX,
                            },
                            Filter::Whirl {
                                cx: 0.5,
                                cy: 1.0 / 3.0,
                                radius: 120.5,
                                angle: -90.0,
                            },
                        ],
                        caption: Some(Caption {
                            text: "image \"one\"
line two"
                                .into(),
                            font: Some(ImageRef::new("fonts/x.ttf")),
                            ..Caption::default()
                        }),
                    },
                    edit2: ImageEdit::default(),
                    caption: Some(Caption {
                        text: "sequence".into(),
                        bold: true,
                        italic: true,
                        border_size: 2.5,
                        shadow: Shadow {
                            size: 3.0,
                            ..Shadow::default()
                        },
                        align: Align::Right,
                        ..Caption::default()
                    }),
                    filters: vec![Filter::Blur { radius: 2 }],
                },
                Sequence::default(),
            ],
            face: {
                let mut face = inbetween_doc::FaceModel::minimal();
                face.name = "my \"face\"".into();
                if let Some(d) = face.dots.get_mut(1) {
                    d.enabled = true;
                    d.x = 1.0 / 3.0;
                }
                face.guides.push([0, 5]);
                face
            },
            caption: Some(Caption {
                text: "movie".into(),
                font: Some(ImageRef::new("../f/y.otf")),
                ..Caption::default()
            }),
            effects: Effects {
                foreground: ImageLayer {
                    enabled: true,
                    image: ImageRef::new("movie/fg.png"),
                    opacity: 40.0,
                },
                ..Effects::default()
            },
            adjust: inbetween_doc::AdjustSettings {
                hue: 12.5,
                grayscale: true,
                ..inbetween_doc::AdjustSettings::default()
            },
            filters: vec![Filter::Gaussian { sigma: 1.5 }],
            sound: Some(inbetween_doc::Sound {
                path: ImageRef::new("audio/t.wav"),
                looped: true,
                fade_in: 0.5,
                fade_out: 1.25,
                ..inbetween_doc::Sound::default()
            }),
        }
    }

    #[test]
    fn round_trip_is_exact() {
        let p = sample();
        let text = project_to_string(&p).unwrap();
        assert_eq!(project_from_str(&text).unwrap(), p);
        // Stable: writing what we read gives the same text.
        assert_eq!(
            project_to_string(&project_from_str(&text).unwrap()).unwrap(),
            text
        );
    }

    #[test]
    fn identity_edits_and_no_captions_are_not_written() {
        let p = Project {
            sequences: vec![Sequence::default()],
            ..Project::default()
        };
        let text = project_to_string(&p).unwrap();
        assert!(!text.contains("edit1") && !text.contains("edit2") && !text.contains("caption"));
        assert_eq!(project_from_str(&text).unwrap(), p);
    }

    #[test]
    fn invalid_edits_and_captions_are_refused() {
        let with = |f: fn(&mut Project)| {
            let mut p = sample();
            f(&mut p);
            let text = serde_json::to_string(&serde_json::json!({
                "format": "inbetween-project", "version": 1, "project": p
            }))
            .unwrap();
            project_from_str(&text).is_err()
        };
        assert!(
            !with(|_| {}),
            "the unchanged sample must load through this path"
        );
        assert!(with(|p| p.sequences[0].edit1.quarter_turns = 9));
        assert!(with(
            |p| p.sequences[0].edit2.filters = vec![Filter::Blur { radius: 0 }]
        ));
        assert!(with(|p| p.sequences[0].edit1.angle = 1e9));
        assert!(with(|p| if let Some(c) = &mut p.caption {
            c.size = -3.0
        }));
        assert!(with(|p| if let Some(c) = &mut p.sequences[0].caption {
            c.text = "x".repeat(5000)
        }));
        assert!(with(|p| if let Some(c) = &mut p.caption {
            c.font = Some(ImageRef::new("a b"))
        }));
    }

    #[test]
    fn invalid_camera_track_and_effects_are_refused() {
        for field in [
            r#""camera":{"keys":[{"t":0.5,"scale_x":0.0}]}"#,
            r#""camera":{"keys":[{"t":2.0}]}"#,
            r#""camera":{"keys":[{"t":0.5},{"t":0.5}]}"#,
            r#""track":{"tension":99.0}"#,
            r#""track":{"points":[{"u":0.0,"v":0.0},{"u":0.5,"v":0.0}]}"#,
            r#""master_curve":{"points":[{"x":0.1,"y":1.0},{"x":1.0,"y":1.0}]}"#,
            r#""master_curve":{"points":[{"x":0.0,"y":3.0},{"x":1.0,"y":1.0}]}"#,
            r#""effects":{"transition":{"softness":500.0}}"#,
        ] {
            let text = format!(
                r#"{{"format":"inbetween-project","version":1,"project":{{"sequences":[{{{field}}}]}}}}"#
            );
            assert!(project_from_str(&text).is_err(), "{field}");
        }
    }

    #[test]
    fn older_projects_get_sequence_kind_and_neutral_camera() {
        let text = r#"{"format":"inbetween-project","version":1,"project":{"sequences":[{}]}}"#;
        let p = project_from_str(text).unwrap();
        assert_eq!(p.kind, ProjectType::Sequence);
        let s = &p.sequences[0];
        assert!(!s.camera.is_active() && !s.track.is_active() && s.master_curve.is_opaque());
        assert!(!s.needs_post_pass());
    }

    #[test]
    fn invalid_curves_are_refused_not_trusted() {
        for curve in [
            r#"{"points": [], "linear": false}"#,
            r#"{"points": [{"x": 0.0, "y": 0.0}], "linear": false}"#,
            r#"{"points": [{"x": 0.0, "y": 0.5}, {"x": 1.0, "y": 1.0}]}"#,
            r#"{"points": [{"x": 0.0, "y": 0.0}, {"x": 7.0, "y": 1.0}]}"#,
        ] {
            let text = format!(
                r#"{{"format":"inbetween-project","version":1,"project":{{"sequences":[{{"feature_curve":{curve}}}]}}}}"#
            );
            assert!(project_from_str(&text).is_err(), "{curve}");
        }
    }

    #[test]
    fn output_is_marked_and_versioned() {
        let text = project_to_string(&Project::default()).unwrap();
        assert!(text.contains("\"format\": \"inbetween-project\""));
        assert!(text.contains("\"version\": 1"));
    }

    #[test]
    fn minimal_file_gets_defaults() {
        let p = project_from_str(r#"{"format":"inbetween-project","version":1}"#).unwrap();
        assert_eq!(p, Project::default());
        let p = project_from_str(
            r#"{"format":"inbetween-project","version":1,"project":{"sequences":[{}]}}"#,
        )
        .unwrap();
        assert_eq!(p.sequences, vec![Sequence::default()]);
    }

    #[test]
    fn unknown_fields_are_ignored_at_every_level() {
        let text = r#"{
            "format": "inbetween-project", "version": 1, "futureTopLevel": {"a": [1, 2]},
            "project": {
                "future": true,
                "output": {"width": 100, "height": 50, "dpi": 300},
                "sequences": [{
                    "name": "x", "frames": 3, "newThing": null,
                    "dots": {"pairs": [{"first": {"x": 1, "y": 2, "z": 3}, "second": {"x": 4, "y": 5}, "tag": "a"}], "extra": 1}
                }]
            }
        }"#;
        let p = project_from_str(text).unwrap();
        assert_eq!(
            p.output,
            OutputSettings {
                width: 100,
                height: 50
            }
        );
        assert_eq!(p.sequences[0].frames, 3);
        assert_eq!(p.sequences[0].dots.pairs[0].second, Point::new(4.0, 5.0));
    }

    #[test]
    fn bom_is_accepted() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(project_to_string(&sample()).unwrap().as_bytes());
        assert_eq!(project_from_bytes(&bytes).unwrap(), sample());
    }

    #[test]
    fn wrong_or_missing_marker() {
        let e = project_from_str(r#"{"format":"something-else","version":1}"#).unwrap_err();
        assert!(
            matches!(e, FormatError::WrongFormat { found: Some(_), .. }),
            "{e}"
        );
        let e = project_from_str(r#"{"version":1}"#).unwrap_err();
        assert!(
            matches!(e, FormatError::WrongFormat { found: None, .. }),
            "{e}"
        );
        let e = project_from_str(r#"{"format":"inbetween-dots","version":1}"#).unwrap_err();
        assert!(matches!(e, FormatError::WrongFormat { .. }));
    }

    #[test]
    fn versions_are_checked() {
        let f = |v: &str| {
            project_from_str(&format!(
                r#"{{"format":"inbetween-project","version":{v}}}"#
            ))
        };
        assert!(f("1").is_ok());
        assert!(matches!(
            f("2"),
            Err(FormatError::NewerVersion { found: 2, max: 1 })
        ));
        assert!(matches!(
            f("4000000000"),
            Err(FormatError::NewerVersion { .. })
        ));
        assert!(matches!(f("0"), Err(FormatError::BadVersion(0))));
        assert!(matches!(f("-1"), Err(FormatError::Parse(_))));
        assert!(matches!(f("1.5"), Err(FormatError::Parse(_))));
        assert!(matches!(f("\"1\""), Err(FormatError::Parse(_))));
        let e = project_from_str(r#"{"format":"inbetween-project"}"#).unwrap_err();
        assert!(matches!(e, FormatError::Parse(_)));
        assert!(f("2").unwrap_err().to_string().contains("newer Inbetween"));
    }

    #[test]
    fn garbage_gives_errors() {
        for bad in [
            "",
            "   ",
            "null",
            "[]",
            "42",
            "\"text\"",
            "{",
            "{\"format\":",
            "not json at all",
            "{\"format\":\"inbetween-project\",\"version\":1,\"project\":{\"sequences\":{}}}",
            "{\"format\":\"inbetween-project\",\"version\":1,\"project\":{\"sequences\":[{\"frames\":-3}]}}",
            "{\"format\":\"inbetween-project\",\"version\":1,\"project\":{\"output\":{\"width\":1e999}}}",
            "{\"format\":\"inbetween-project\",\"version\":1,\"project\":{\"sequences\":[{\"morph\":\"bogus\"}]}}",
        ] {
            assert!(project_from_str(bad).is_err(), "accepted: {bad}");
        }
        assert!(project_from_bytes(&[0xFF, 0xFE, 0x00]).is_err());
        assert!(project_from_bytes(&[0x80; 64]).is_err());
    }

    #[test]
    fn content_limits_are_enforced_on_read_and_write() {
        let zero_frames =
            r#"{"format":"inbetween-project","version":1,"project":{"sequences":[{"frames":0}]}}"#;
        assert!(matches!(
            project_from_str(zero_frames),
            Err(FormatError::Invalid(_))
        ));
        let huge = r#"{"format":"inbetween-project","version":1,"project":{"output":{"width":99999999,"height":5}}}"#;
        assert!(matches!(
            project_from_str(huge),
            Err(FormatError::Invalid(_))
        ));

        let mut p = sample();
        p.sequences[0].frames = 0;
        assert!(matches!(
            project_to_string(&p),
            Err(FormatError::Invalid(_))
        ));
    }

    #[test]
    fn too_many_dots_and_sequences_are_rejected() {
        let pair = r#"{"first":{"x":1,"y":1},"second":{"x":2,"y":2}}"#;
        let n = inbetween_geom::MAX_DOTS + 1;
        let text = format!(
            r#"{{"format":"inbetween-project","version":1,"project":{{"sequences":[{{"dots":{{"pairs":[{}]}}}}]}}}}"#,
            vec![pair; n].join(",")
        );
        assert!(matches!(
            project_from_str(&text),
            Err(FormatError::Invalid(_))
        ));

        let text = format!(
            r#"{{"format":"inbetween-project","version":1,"project":{{"sequences":[{}]}}}}"#,
            vec!["{}"; MAX_SEQUENCES + 1].join(",")
        );
        assert!(matches!(
            project_from_str(&text),
            Err(FormatError::TooManySequences { .. })
        ));
    }

    #[test]
    fn absolute_image_paths_round_trip() {
        let mut p = sample();
        p.sequences[0].image1 = ImageRef::new("D:\\other drive\\a.png");
        p.sequences[0].image2 = ImageRef::new("/abs/b.png");
        p.sequences[0].effects.light.image = ImageRef::new("E:/l.png");
        let text = project_to_string(&p).unwrap();
        assert_eq!(project_from_str(&text).unwrap(), p);
    }

    #[test]
    fn nul_and_over_long_image_paths_are_rejected() {
        for path in ["a\\u0000b".to_string(), "x".repeat(5000)] {
            let text = format!(
                r#"{{"format":"inbetween-project","version":1,"project":{{"sequences":[{{"image1":"{path}"}}]}}}}"#
            );
            assert!(
                matches!(project_from_str(&text), Err(FormatError::BadPath { .. })),
                "{}",
                path.len()
            );
        }
        let mut p = sample();
        p.sequences[0].image2 = ImageRef::new("a\0b");
        assert!(matches!(
            project_to_string(&p),
            Err(FormatError::BadPath { .. })
        ));
    }

    fn tiny_png() -> Vec<u8> {
        let mut png = PNG_SIGNATURE.to_vec();
        png.extend_from_slice(b"rest of a pretend png");
        png
    }

    #[test]
    fn thumbnail_round_trips_and_is_optional() {
        let p = sample();
        let png = tiny_png();
        let t = Thumbnail::from_png(8, 6, &png).unwrap();
        let text = project_to_string_with_thumbnail(&p, Some(&t)).unwrap();
        assert!(text.contains("\"thumbnail\""));
        // The project itself reads back unchanged.
        assert_eq!(project_from_str(&text).unwrap(), p);
        assert_eq!(
            thumbnail_from_bytes(text.as_bytes()).unwrap(),
            Some((8, 6, png))
        );
        // Without one, nothing is written, and an older file still reads.
        let plain = project_to_string(&p).unwrap();
        assert!(!plain.contains("thumbnail"));
        assert_eq!(thumbnail_from_bytes(plain.as_bytes()).unwrap(), None);
        let older = r#"{"format":"inbetween-project","version":1,"project":{"sequences":[{}]}}"#;
        assert_eq!(thumbnail_from_bytes(older.as_bytes()).unwrap(), None);
    }

    #[test]
    fn bad_thumbnails_are_errors_not_panics() {
        let head = r#"{"format":"inbetween-project","version":1,"thumbnail":"#;
        let big_b64 = "A".repeat(MAX_THUMBNAIL_PNG_BYTES / 3 * 4 + 400);
        for thumb in [
            r#"{"width":8,"height":6,"png":"!!! not base64"}"#.to_string(),
            r#"{"width":8,"height":6,"png":"aGVsbG8="}"#.to_string(), // "hello": not a PNG
            r#"{"width":0,"height":6,"png":"iVBORw0KGgo="}"#.to_string(),
            r#"{"width":9999,"height":6,"png":"iVBORw0KGgo="}"#.to_string(),
            format!(r#"{{"width":8,"height":6,"png":"{big_b64}"}}"#),
            r#"{"width":8}"#.to_string(),
            r#"[1,2]"#.to_string(),
            r#""text""#.to_string(),
        ] {
            let text = format!("{head}{thumb}}}");
            assert!(
                thumbnail_from_bytes(text.as_bytes()).is_err(),
                "{thumb:.60}"
            );
        }
        assert!(Thumbnail::from_png(8, 6, b"not a png").is_err());
        assert!(Thumbnail::from_png(600, 6, &tiny_png()).is_err());
        let mut big = tiny_png();
        big.resize(MAX_THUMBNAIL_PNG_BYTES + 1, 0);
        assert!(Thumbnail::from_png(8, 6, &big).is_err());
        let bad = Thumbnail {
            width: 8,
            height: 6,
            png: "zzz".into(),
        };
        assert!(project_to_string_with_thumbnail(&sample(), Some(&bad)).is_err());
    }

    #[test]
    fn size_limit_is_checked_before_parsing() {
        let text = project_to_string(&sample()).unwrap();
        assert!(project_from_bytes_limited(text.as_bytes(), text.len()).is_ok());
        assert!(matches!(
            project_from_bytes_limited(text.as_bytes(), text.len() - 1),
            Err(FormatError::TooLarge { .. })
        ));
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_stack_overflow() {
        let depth = 200_000;
        let nested = format!(
            r#"{{"format":"inbetween-project","version":1,"x":{}{}}}"#,
            "[".repeat(depth),
            "]".repeat(depth)
        );
        // Skipping an unknown deep value is iterative in serde_json, so this may load; the
        // point is that it neither crashes nor overflows the stack.
        let _ = project_from_str(&nested);
        let deep_known = format!(
            r#"{{"format":"inbetween-project","version":1,"project":{}"#,
            "[".repeat(depth)
        );
        assert!(project_from_str(&deep_known).is_err());
    }

    #[test]
    fn error_messages_say_where() {
        let e = project_from_str("{\n  \"format\": \n").unwrap_err();
        assert!(e.to_string().contains("line"), "{e}");
    }
}
