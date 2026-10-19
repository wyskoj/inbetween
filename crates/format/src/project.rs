use crate::{check_header, check_relative, decode_text, parse_error, FormatError};
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

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    version: u32,
    project: &'a Project,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    #[serde(default)]
    project: Project,
}

/// Serialise to pretty JSON. Fails if the project breaks a model limit or holds an
/// absolute image path, so a saved file always loads back.
pub fn project_to_string(project: &Project) -> Result<String, FormatError> {
    validate(project)?;
    let env = EnvelopeOut {
        format: PROJECT_FORMAT,
        version: PROJECT_VERSION,
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
    for seq in &project.sequences {
        check_relative(&seq.image1.0)?;
        check_relative(&seq.image2.0)?;
        for image in seq.effects.all_images() {
            check_relative(&image.0)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{
        BackColor, Camera, CameraKey, Curve, CurvePoint, DotPair, DotSet, Effects, FitMode,
        ImageLayer, ImageRef, Light, Mask, MeshPolicy, MorphType, OutputSettings, ProjectType,
        Radial, Sequence, Track, TrackPoint, Trail, Transition,
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
            r#""effects":{"light":{"enabled":true,"image":"/abs/l.png"}}"#,
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
    fn absolute_image_paths_are_rejected() {
        for path in ["/etc/passwd", "C:\\\\x.png", "\\\\\\\\server\\\\share"] {
            let text = format!(
                r#"{{"format":"inbetween-project","version":1,"project":{{"sequences":[{{"image1":"{path}"}}]}}}}"#
            );
            assert!(
                matches!(project_from_str(&text), Err(FormatError::BadPath { .. })),
                "{path}"
            );
        }
        let mut p = sample();
        p.sequences[0].image2 = ImageRef::new("/abs/b.png");
        assert!(matches!(
            project_to_string(&p),
            Err(FormatError::BadPath { .. })
        ));
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
