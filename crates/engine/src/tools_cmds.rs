//! Tools: `tools.sequencePacker` (pack an image list or a folder of images into a movie) and
//! `tools.benchmark` (measure preview and export throughput).

use crate::CommandSpec;

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[cfg(not(target_arch = "wasm32"))]
    return files::specs();
    #[cfg(target_arch = "wasm32")]
    return Vec::new();
}

/// Compare two names so that digit runs compare as numbers (`img2` before `img10`) and the
/// rest compares case-insensitively.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(&n) = it.peek() {
                        if !n.is_ascii_digit() {
                            break;
                        }
                        digits.push(n);
                        it.next();
                    }
                    digits
                };
                let (p, q) = (take(&mut x), take(&mut y));
                let (p, q) = (p.trim_start_matches('0'), q.trim_start_matches('0'));
                // Longer runs of digits are bigger numbers; equal lengths compare as text.
                let order = p.len().cmp(&q.len()).then_with(|| p.cmp(q));
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                let order = c.to_lowercase().cmp(d.to_lowercase());
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use crate::file_cmds::{absolute, stem};
    use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
    use inbetween_doc::{ImageRef, MorphType, OutputSettings, Sequence, MAX_SIDE};
    use inbetween_format::MAX_SEQUENCES;
    use serde_json::{json, Value};
    use std::path::Path;
    use std::time::Instant;

    /// Most files a folder scan looks at.
    const MAX_SCANNED: usize = 100_000;

    const ORDERS: &[&str] = &["given", "name", "modified"];

    const PACKER_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "images",
            ParamKind::PathList,
            false,
            "Images to pack, in the order given",
        )
        .open_files(crate::source_media::SOURCE_EXTENSIONS)
        .range(1.0, 1000.0),
        ParamSpec::new(
            "folder",
            ParamKind::Path,
            false,
            "A folder whose images are packed after `images` (files that are not readable images are skipped and reported)",
        )
        .folder(),
        ParamSpec::new(
            "order",
            ParamKind::String,
            false,
            "given (as listed, folder files by name), name (everything by name, numbers in order) or modified (oldest first)",
        )
        .one_of(ORDERS)
        .default_is("given"),
        ParamSpec::new(
            "loop",
            ParamKind::Bool,
            false,
            "Also morph the last image back into the first",
        )
        .default_is("false"),
        ParamSpec::new("morph", ParamKind::String, false, "Morph type of every sequence")
            .one_of(&[
                "morph",
                "morph_hold",
                "warp1",
                "warp2",
                "fade_out1",
                "fade_in2",
                "direct",
            ])
            .default_is("morph"),
        ParamSpec::new(
            "frames",
            ParamKind::Integer,
            false,
            "Morph frames in each sequence",
        )
        .range(2.0, 999.0)
        .default_is("30"),
        ParamSpec::new(
            "width",
            ParamKind::Integer,
            false,
            "Output width when the project has no sequences yet",
        )
        .range(1.0, MAX_SIDE as f64)
        .default_is("the first image's width"),
        ParamSpec::new(
            "height",
            ParamKind::Integer,
            false,
            "Output height when the project has no sequences yet",
        )
        .range(1.0, MAX_SIDE as f64)
        .default_is("the first image's height"),
    ];

    struct Found {
        path: String,
        modified: std::time::SystemTime,
    }

    fn modified(path: &Path) -> std::time::SystemTime {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH)
    }

    /// The image files of `folder`: regular files with an image extension.
    fn scan(folder: &Path) -> Result<Vec<Found>, CommandError> {
        let read = std::fs::read_dir(folder)
            .map_err(|e| CommandError::Io(format!("{}: {e}", folder.display())))?;
        let mut found = Vec::new();
        for entry in read.filter_map(Result::ok).take(MAX_SCANNED) {
            let path = entry.path();
            let is_image = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                crate::source_media::SOURCE_EXTENSIONS
                    .iter()
                    .any(|x| x.eq_ignore_ascii_case(e))
            });
            if is_image && path.is_file() {
                found.push(Found {
                    path: path.display().to_string(),
                    modified: modified(&path),
                });
            }
        }
        Ok(found)
    }

    fn file_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn packer(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let order = params.string("order").unwrap_or("given");
        let mut listed: Vec<Found> = params
            .strings("images")
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                let path = absolute(p)?;
                let modified = modified(Path::new(&path));
                Ok(Found { path, modified })
            })
            .collect::<Result<_, CommandError>>()?;
        let mut from_folder = match params.string("folder") {
            Some(f) => scan(Path::new(f))?,
            None => Vec::new(),
        };
        from_folder.sort_by(|a, b| natural_cmp(&file_name(&a.path), &file_name(&b.path)));
        listed.extend(from_folder);
        match order {
            "name" => listed.sort_by(|a, b| natural_cmp(&file_name(&a.path), &file_name(&b.path))),
            "modified" => listed.sort_by_key(|f| f.modified),
            _ => {}
        }
        // Files that cannot be read as images are left out, and said so.
        let (mut images, mut skipped) = (Vec::new(), Vec::new());
        let mut first_size = None;
        for f in listed {
            match crate::source_media::read_any(Path::new(&f.path)) {
                Ok(img) => {
                    first_size.get_or_insert((img.width, img.height));
                    images.push(f.path);
                }
                Err(e) => skipped.push(json!({ "path": f.path, "error": e.to_string() })),
            }
        }
        if images.len() < 2 {
            return Err(CommandError::BadParams(format!(
                "need at least two readable images to pack, found {}{}",
                images.len(),
                match skipped.first() {
                    None => String::new(),
                    Some(first) => format!(
                        " ({} skipped as unreadable, the first: {})",
                        skipped.len(),
                        first["error"].as_str().unwrap_or("unknown error")
                    ),
                }
            )));
        }
        let morph = params
            .string("morph")
            .and_then(MorphType::from_id)
            .unwrap_or_default();
        let frames = params.integer("frames").map_or(30, |n| n as u32);
        let mut pairs: Vec<(&String, &String)> = images.iter().zip(images.iter().skip(1)).collect();
        if params.boolean("loop") == Some(true) {
            if let (Some(last), Some(first)) = (images.last(), images.first()) {
                pairs.push((last, first));
            }
        }
        let existing = session.project().sequences.len();
        if existing.saturating_add(pairs.len()) > MAX_SEQUENCES {
            return Err(CommandError::BadParams(format!(
                "packing {} sequences into a project that has {existing} would pass the limit of {MAX_SEQUENCES}",
                pairs.len()
            )));
        }
        let sequences: Vec<Sequence> = pairs
            .into_iter()
            .map(|(a, b)| Sequence {
                name: format!("{} to {}", stem(a), stem(b)),
                image1: ImageRef::new(a.as_str()),
                image2: ImageRef::new(b.as_str()),
                frames,
                morph,
                ..Sequence::default()
            })
            .collect();
        let added = sequences.len();
        let project = session.project_mut();
        if existing == 0 {
            let dim = |name: &str, default: u32| {
                params
                    .integer(name)
                    .map_or(default, |n| n.clamp(1, i64::from(MAX_SIDE)) as u32)
            };
            let (w, h) = first_size.unwrap_or((1, 1));
            project.output = OutputSettings {
                width: dim("width", w),
                height: dim("height", h),
            };
        }
        project.sequences.extend(sequences);
        project.validate()?;
        Ok(json!({
            "first": existing,
            "added": added,
            "sequences": project.sequences.len(),
            "images": images.len(),
            "skipped": skipped,
            "width": project.output.width,
            "height": project.output.height,
        }))
    }

    const BENCH_PARAMS: &[ParamSpec] = &[
        ParamSpec::new(
            "sequence",
            ParamKind::Integer,
            false,
            "Sequence number, starting at 0",
        )
        .range(0.0, 1_000_000.0)
        .default_is("0"),
        ParamSpec::new(
            "frames",
            ParamKind::Integer,
            false,
            "How many frames to time (the sequence's frames repeat if it has fewer)",
        )
        .range(1.0, 2000.0)
        .default_is("12"),
    ];

    fn ms(since: Instant) -> f64 {
        since.elapsed().as_secs_f64() * 1000.0
    }

    fn per_second(frames: u32, total_ms: f64) -> f64 {
        if total_ms > 0.0 {
            f64::from(frames) * 1000.0 / total_ms
        } else {
            0.0
        }
    }

    /// Time what the preview does (prepare, then render and finish each frame) and what an
    /// export does on top (encode each frame as PNG). Nothing is written and the project is
    /// not changed.
    fn benchmark(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let index = session.sequence_index(params)?;
        let wanted = params
            .integer("frames")
            .map_or(12, |n| n.clamp(1, 2000) as u32);
        session.begin_cancellable();
        let started = Instant::now();
        let prepared = session.prepare(index)?;
        let prepare_ms = ms(started);
        let renderer = prepared.renderer()?;
        let total = renderer.frame_count().max(1);
        let (width, height) = prepared.output;

        let started = Instant::now();
        let mut frames = Vec::new();
        for n in 0..wanted {
            session.check_cancel()?;
            let image = renderer.render_frame(n % total)?;
            frames.push(prepared.finish(image, n % total)?);
        }
        let render_ms = ms(started);

        let started = Instant::now();
        let mut bytes = 0usize;
        for image in &frames {
            session.check_cancel()?;
            bytes += inbetween_codecs::encode_png(width, height, &image.to_rgba8())?.len();
        }
        let encode_ms = ms(started);

        Ok(json!({
            "sequence": index,
            "frames": wanted,
            "width": width,
            "height": height,
            "prepare_ms": prepare_ms,
            "preview": {
                "ms_per_frame": render_ms / f64::from(wanted),
                "fps": per_second(wanted, render_ms),
            },
            "export": {
                "ms_per_frame": (render_ms + encode_ms) / f64::from(wanted),
                "fps": per_second(wanted, render_ms + encode_ms),
                "png_bytes_per_frame": bytes / wanted as usize,
            },
        }))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![
            CommandSpec {
                id: "tools.sequencePacker",
                label: "Sequence Packer",
                menu: "Movie/Sequence Packer...",
                shortcut: None,
                description: "Pack a list of images and/or a folder of images into the movie: each image morphs into the next (and the last into the first with `loop`). Sequences are added after the current ones; an empty project takes the first image's size. Add dots afterwards.",
                params: PACKER_PARAMS,
                enabled: |_| true,
                disabled_reason: "",
                edits: true,
                run: packer,
            },
            CommandSpec {
                id: "tools.benchmark",
                label: "Benchmark",
                menu: "Movie/Benchmark",
                shortcut: None,
                description: "Time rendering a sequence's frames the way the preview does and the way an export does (render plus PNG encoding), and report milliseconds and frames per second. Writes nothing and changes nothing.",
                params: BENCH_PARAMS,
                enabled: Session::has_sequences,
                disabled_reason: "the project has no sequences to time",
                edits: false,
                run: benchmark,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering::{Equal, Greater, Less};

    #[test]
    fn numbers_inside_names_compare_as_numbers() {
        assert_eq!(natural_cmp("img2.png", "img10.png"), Less);
        assert_eq!(natural_cmp("img10.png", "img2.png"), Greater);
        assert_eq!(natural_cmp("a007", "a7"), Less);
        assert_eq!(natural_cmp("B", "a"), Greater);
        assert_eq!(natural_cmp("x", "x"), Equal);
        assert_eq!(natural_cmp("x1", "x"), Greater);
        let mut v = vec!["f10", "f9", "f100", "f1", "F2"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["f1", "F2", "f9", "f10", "f100"]);
        // Huge digit runs do not overflow.
        assert_eq!(natural_cmp(&"9".repeat(400), &"1".repeat(401)), Less);
    }
}
