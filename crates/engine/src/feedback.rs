//! Plain-language outcomes for commands. A command returns data for programs; a person at a
//! shell needs to know what actually happened, and above all when a command ran but changed
//! nothing. [`describe`] turns a command's result into one short sentence and a [`Tone`], so
//! every shell words its feedback the same way.

use serde_json::Value;

/// How a message should read: a done thing, a neutral note, or "it ran, but not as hoped".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Success,
    Info,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feedback {
    pub message: String,
    pub tone: Tone,
}

fn note(tone: Tone, message: String) -> Option<Feedback> {
    Some(Feedback { message, tone })
}

fn count(result: &Value, key: &str) -> Option<u64> {
    result.get(key).and_then(Value::as_u64)
}

fn plural(n: u64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn dots(n: u64) -> String {
    plural(n, "dot", "dots")
}

/// The file or folder name at the end of `path`, for messages that must stay short.
fn short(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(path)
}

fn text<'a>(result: &'a Value, key: &str) -> Option<&'a str> {
    result.get(key).and_then(Value::as_str)
}

/// Commands that fire many times a second (drags, nudges, selection) or that only open and
/// close undo groups. They never get a message.
pub fn is_quiet(id: &str) -> bool {
    matches!(
        id,
        "dots.move"
            | "dots.nudge"
            | "lines.drag"
            | "dots.select"
            | "dots.selectNone"
            | "edit.beginGroup"
            | "edit.endGroup"
    )
}

/// What `id` did, given its `result`. `None` when the outcome is visible on its own (a dot
/// appearing under the pointer) and a message would only add noise.
pub fn describe(id: &str, result: &Value) -> Option<Feedback> {
    use Tone::{Info, Success, Warning};
    match id {
        "dots.autoMatch" => {
            let done = count(result, "completed")?;
            let skipped = count(result, "skipped").unwrap_or(0);
            match (done, skipped) {
                (0, 0) => note(Info, "Every dot already has a partner".into()),
                (0, s) => note(
                    Warning,
                    format!("Found no partners; {} left unmatched", dots(s)),
                ),
                (d, 0) => note(Success, format!("Matched {}", dots(d))),
                (d, s) => note(
                    Warning,
                    format!("Matched {}; {} left unmatched", dots(d), dots(s)),
                ),
            }
        }
        "dots.autoLandmarks" => {
            let added = count(result, "added")?;
            let rejected = count(result, "rejected").unwrap_or(0);
            match (added, rejected) {
                (0, _) => note(
                    Warning,
                    "Found no matching features; try a higher count or a lower quality".into(),
                ),
                (a, 0) => note(Success, format!("Added {}", dots(a))),
                (a, r) => note(
                    Success,
                    format!("Added {}; {} too close or unmatched", dots(a), dots(r)),
                ),
            }
        }
        "face.locate" => {
            let per = count(result, "dots_per_face")?;
            let seqs = result
                .get("sequences")
                .and_then(Value::as_array)
                .map_or(0, |a| a.len() as u64);
            note(
                Success,
                format!(
                    "Placed {} per face on {}",
                    per,
                    plural(seqs, "sequence", "sequences")
                ),
            )
        }
        "face.locate.smartDrag" => match count(result, "moved")? {
            0 => note(Info, "No dots moved".into()),
            n => note(Success, format!("Moved {}", dots(n))),
        },
        "edit.copy" => {
            let n = count(result, "copied")?;
            note(Info, format!("Copied {}", dots(n)))
        }
        "edit.cut" => note(Success, format!("Cut {}", dots(count(result, "cut")?))),
        "edit.delete" => match count(result, "deleted")? {
            0 => note(Info, "No dots selected".into()),
            n => note(Success, format!("Deleted {}", dots(n))),
        },
        "dots.clearAll" => match count(result, "deleted")? {
            0 => note(Info, "There were no dots to clear".into()),
            n => note(Success, format!("Cleared {}", dots(n))),
        },
        "edit.paste" => {
            let n = count(result, "pasted")?;
            note(Success, format!("Pasted {}", dots(n)))
        }
        "edit.undo" | "edit.redo" => {
            let word = if id == "edit.undo" { "undo" } else { "redo" };
            match result.get("done").and_then(Value::as_bool) {
                Some(true) => {
                    let past = if id == "edit.undo" { "Undid" } else { "Redid" };
                    note(Info, format!("{past} the last change"))
                }
                Some(false) => note(Info, format!("Nothing to {word}")),
                None => None,
            }
        }
        "dots.transform" | "dots.scale" | "dots.rotate" | "dots.skew" | "dots.flipH"
        | "dots.flipV" => {
            let op = text(result, "op").unwrap_or(id.trim_start_matches("dots."));
            let verb = match op {
                "scale" => "Scaled",
                "rotate" => "Rotated",
                "skew" => "Skewed",
                "flipH" => "Flipped horizontally",
                "flipV" => "Flipped vertically",
                _ => "Moved",
            };
            match count(result, "moved")? {
                0 => note(Warning, "No dots were moved; select some first".into()),
                n => note(Success, format!("{verb} {}", dots(n))),
            }
        }
        "dots.import" => note(
            Success,
            format!("Imported {}", dots(count(result, "dots")?)),
        ),
        "dots.export" => note(
            Success,
            format!("Exported {}", dots(count(result, "dots")?)),
        ),
        "project.save" | "project.saveAs" => {
            let name = short(text(result, "path")?);
            match count(result, "absolute_images") {
                Some(n) if n > 0 => note(
                    Warning,
                    format!(
                        "Saved {name}; {} kept absolute paths and will not move with it",
                        plural(n, "image", "images")
                    ),
                ),
                _ => note(Success, format!("Saved {name}")),
            }
        }
        "export.frame" => {
            let out = short(text(result, "out")?);
            note(Success, format!("Saved frame to {out}"))
        }
        "export.sequence" | "export.movie" => {
            let frames = count(result, "frames")?;
            let dir = short(text(result, "dir")?);
            note(
                Success,
                format!("Exported {} to {dir}", plural(frames, "frame", "frames")),
            )
        }
        "export.animation" | "export.video" | "export.pdf" => {
            let out = short(text(result, "out")?);
            note(Success, format!("Exported {out}"))
        }
        "sequence.add" | "sequence.addBlank" => {
            let n = count(result, "count")?;
            note(Success, format!("Added a sequence ({n} in all)"))
        }
        "sequence.duplicate" => note(Success, "Duplicated the sequence".into()),
        "sequence.delete" => {
            let n = count(result, "count")?;
            note(Success, format!("Deleted the sequence ({n} left)"))
        }
        "sequence.link" => note(Success, "Linked Image 1 to the previous sequence".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn msg(id: &str, result: Value) -> (String, Tone) {
        let f = describe(id, &result).unwrap_or_else(|| panic!("no feedback for {id}"));
        (f.message, f.tone)
    }

    #[test]
    fn a_match_that_found_nothing_is_a_warning() {
        let (m, t) = msg("dots.autoMatch", json!({"completed": 0, "skipped": 3}));
        assert_eq!(t, Tone::Warning);
        assert!(m.contains("3 dots"), "{m}");
    }

    #[test]
    fn a_match_with_nothing_to_do_says_so() {
        let (m, t) = msg("dots.autoMatch", json!({"completed": 0, "skipped": 0}));
        assert_eq!(
            (m.as_str(), t),
            ("Every dot already has a partner", Tone::Info)
        );
    }

    #[test]
    fn counts_agree_in_number() {
        assert_eq!(msg("edit.paste", json!({"pasted": 1})).0, "Pasted 1 dot");
        assert_eq!(msg("edit.paste", json!({"pasted": 2})).0, "Pasted 2 dots");
    }

    #[test]
    fn a_transform_that_moved_nothing_is_a_warning() {
        let (_, t) = msg("dots.rotate", json!({"op": "rotate", "moved": 0}));
        assert_eq!(t, Tone::Warning);
        let (m, _) = msg("dots.flipH", json!({"op": "flipH", "moved": 4}));
        assert_eq!(m, "Flipped horizontally 4 dots");
    }

    #[test]
    fn undo_reports_whether_anything_changed() {
        assert_eq!(
            msg("edit.undo", json!({"done": true})).0,
            "Undid the last change"
        );
        assert_eq!(
            msg("edit.redo", json!({"done": false})).0,
            "Nothing to redo"
        );
    }

    #[test]
    fn saving_names_the_file_not_the_whole_path() {
        let (m, t) = msg("project.save", json!({"path": "C:\\a\\b\\demo.inbetween"}));
        assert_eq!((m.as_str(), t), ("Saved demo.inbetween", Tone::Success));
        let (_, t) = msg(
            "project.save",
            json!({"path": "/a/demo.inbetween", "absolute_images": 2}),
        );
        assert_eq!(t, Tone::Warning);
    }

    #[test]
    fn hostile_results_never_panic() {
        for id in [
            "dots.autoMatch",
            "edit.paste",
            "export.frame",
            "project.save",
            "face.locate",
            "nonsense",
        ] {
            for result in [
                json!(null),
                json!([]),
                json!({"out": 5, "path": [], "completed": "x", "pasted": -1}),
                json!({"path": "", "dir": "///"}),
            ] {
                let _ = describe(id, &result);
            }
        }
    }

    #[test]
    fn every_described_id_is_a_real_command() {
        let reg = crate::Registry::standard();
        for id in [
            "dots.autoMatch",
            "dots.autoLandmarks",
            "face.locate",
            "face.locate.smartDrag",
            "edit.copy",
            "edit.cut",
            "edit.delete",
            "dots.clearAll",
            "edit.paste",
            "edit.undo",
            "edit.redo",
            "dots.transform",
            "dots.scale",
            "dots.rotate",
            "dots.skew",
            "dots.flipH",
            "dots.flipV",
            "dots.import",
            "dots.export",
            "project.save",
            "project.saveAs",
            "export.frame",
            "export.sequence",
            "export.movie",
            "export.animation",
            "export.video",
            "export.pdf",
            "sequence.add",
            "sequence.addBlank",
            "sequence.duplicate",
            "sequence.delete",
            "sequence.link",
        ] {
            assert!(reg.get(id).is_some(), "{id} is not a command");
        }
    }
}
