//! `export.pdf`: a contact sheet of the sources, a preview strip of each sequence, or every
//! frame of the movie, as a PDF with a caption under each picture.

use crate::CommandSpec;

pub(crate) fn specs() -> Vec<CommandSpec> {
    #[cfg(not(target_arch = "wasm32"))]
    return files::specs();
    #[cfg(target_arch = "wasm32")]
    return Vec::new();
}

#[cfg(not(target_arch = "wasm32"))]
mod files {
    use super::*;
    use crate::export_options::{self, OPEN_AFTER, SAVE_FIRST};
    use crate::movie::{self, MAX_MOVIE_FRAMES};
    use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
    use inbetween_export::pdf::{
        self, Cell, PageSize, PdfImage, SheetOptions, MAX_CELLS, MAX_COLUMNS,
    };
    use inbetween_raster::Image;
    use serde_json::{json, Value};
    use std::path::Path;

    const CONTENTS: &[&str] = &["sources", "preview", "frames"];
    const PAGES: &[&str] = &["a4", "letter"];

    const PARAMS: &[ParamSpec] = &[
        ParamSpec::new("out", ParamKind::Path, true, "The .pdf file to write").save_file(&["pdf"]),
        ParamSpec::new(
            "content",
            ParamKind::String,
            false,
            "sources (both images of each sequence), preview (a few frames of each sequence, see `per_sequence`) or frames (every movie frame in `first` to `last`)",
        )
        .one_of(CONTENTS)
        .default_is("preview"),
        ParamSpec::new(
            "per_sequence",
            ParamKind::Integer,
            false,
            "For `preview`: how many frames of each sequence, evenly spaced from its first to its last",
        )
        .range(1.0, 50.0)
        .default_is("5"),
        ParamSpec::new(
            "sequence",
            ParamKind::Integer,
            false,
            "Only this sequence (numbered from 0); `first` and `last` then count inside it",
        )
        .range(0.0, 1_000_000.0)
        .default_is("every visible sequence"),
        ParamSpec::new(
            "first",
            ParamKind::Integer,
            false,
            "For `frames`: the first frame, counting from 1",
        )
        .range(1.0, MAX_MOVIE_FRAMES as f64)
        .default_is("1"),
        ParamSpec::new(
            "last",
            ParamKind::Integer,
            false,
            "For `frames`: the last frame, counting from 1",
        )
        .range(1.0, MAX_MOVIE_FRAMES as f64)
        .default_is("the last frame"),
        ParamSpec::new(
            "columns",
            ParamKind::Integer,
            false,
            "Pictures across a page, 1 to 12",
        )
        .range(1.0, MAX_COLUMNS as f64)
        .default_is("3"),
        ParamSpec::new("page", ParamKind::String, false, "Paper size")
            .one_of(PAGES)
            .default_is("a4"),
        ParamSpec::new(
            "landscape",
            ParamKind::Bool,
            false,
            "Turn the page on its side",
        )
        .default_is("false"),
        ParamSpec::new(
            "captions",
            ParamKind::Bool,
            false,
            "Write a caption under each picture (what it is and which frame)",
        )
        .default_is("true"),
        ParamSpec::new(
            "title",
            ParamKind::String,
            false,
            "Heading on every page",
        )
        .default_is("the project's file name"),
        ParamSpec::new(
            "thumb",
            ParamKind::Integer,
            false,
            "Longest side of each embedded picture in pixels",
        )
        .range(32.0, 2048.0)
        .default_is("360"),
        ParamSpec::new(
            "quality",
            ParamKind::Integer,
            false,
            "JPEG quality of the embedded pictures, 1 to 100",
        )
        .range(1.0, 100.0)
        .default_is("85"),
        SAVE_FIRST,
        OPEN_AFTER,
    ];

    fn has_sequence(session: &Session) -> bool {
        session.has_sequences()
    }

    fn cell(image: &Image, caption: String, thumb: u32, quality: u8) -> Result<Cell, CommandError> {
        let small = image.downscale_to_fit(thumb)?;
        let jpeg = inbetween_codecs::encode_jpeg(
            small.width(),
            small.height(),
            &small.to_rgba8(),
            quality,
        )?;
        Ok(Cell {
            image: PdfImage {
                width: small.width(),
                height: small.height(),
                jpeg,
            },
            caption,
        })
    }

    fn too_many() -> CommandError {
        CommandError::BadParams(format!(
            "that is more than {MAX_CELLS} pictures; pick fewer frames or one sequence"
        ))
    }

    fn file_name(reference: &str) -> String {
        Path::new(reference)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The visible sequences to draw: one if `sequence` names it, otherwise all.
    fn sequences(session: &Session, params: &Params) -> Result<Vec<usize>, CommandError> {
        let project = session.project();
        match params.integer("sequence") {
            Some(n) => {
                let index = usize::try_from(n).unwrap_or(usize::MAX);
                if index >= project.sequences.len() {
                    return Err(CommandError::NoSuchSequence {
                        index,
                        count: project.sequences.len(),
                    });
                }
                Ok(vec![index])
            }
            None => Ok(project
                .sequences
                .iter()
                .enumerate()
                .filter(|(_, s)| s.visible)
                .map(|(i, _)| i)
                .collect()),
        }
    }

    fn source_cells(
        session: &mut Session,
        list: &[usize],
        thumb: u32,
        quality: u8,
    ) -> Result<Vec<Cell>, CommandError> {
        let mut cells = Vec::new();
        for &index in list {
            let seq = session.sequence(index)?.clone();
            for (which, reference) in [(1u8, &seq.image1), (2u8, &seq.image2)] {
                if reference.0.is_empty() {
                    continue;
                }
                let image = session.source(index, which)?;
                let caption = format!("{}: Image {which}, {}", seq.name, file_name(&reference.0));
                cells.push(cell(&image, caption, thumb, quality)?);
                if cells.len() > MAX_CELLS {
                    return Err(too_many());
                }
            }
        }
        Ok(cells)
    }

    fn preview_cells(
        session: &mut Session,
        list: &[usize],
        per: usize,
        thumb: u32,
        quality: u8,
    ) -> Result<Vec<Cell>, CommandError> {
        let mut cells = Vec::new();
        for &index in list {
            session.check_cancel()?;
            let prepared = session.prepare(index)?;
            let renderer = prepared.renderer()?;
            let total = renderer.frame_count().max(1);
            let picks = per.min(total as usize).max(1);
            let mut seen = Vec::new();
            for k in 0..picks {
                // Evenly spaced from the first frame to the last.
                let local = if picks == 1 {
                    0
                } else {
                    ((k as f64) * f64::from(total - 1) / ((picks - 1) as f64)).round() as u32
                };
                if seen.contains(&local) {
                    continue;
                }
                seen.push(local);
                let frame = prepared.finish(renderer.render_frame(local)?, local)?;
                let name = &prepared.sequence.name;
                let caption = format!("{name}: frame {} of {total}", local + 1);
                cells.push(cell(&frame, caption, thumb, quality)?);
                if cells.len() > MAX_CELLS {
                    return Err(too_many());
                }
            }
        }
        Ok(cells)
    }

    fn frame_cells(
        session: &mut Session,
        params: &Params,
        thumb: u32,
        quality: u8,
    ) -> Result<Vec<Cell>, CommandError> {
        let range = movie::range_from(session, params)?;
        if range.last - range.first + 1 > MAX_CELLS as u64 {
            return Err(too_many());
        }
        let mut cells = Vec::new();
        movie::render_movie(session, range, &mut |n, image| {
            cells.push(cell(image, format!("frame {n}"), thumb, quality)?);
            Ok(())
        })?;
        Ok(cells)
    }

    fn run(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
        let saved = export_options::before(session, params)?;
        let out = params.string("out").unwrap_or_default();
        let thumb = params
            .integer("thumb")
            .map_or(360, |n| n.clamp(32, 2048) as u32);
        let quality = params
            .integer("quality")
            .map_or(85, |n| n.clamp(1, 100) as u8);
        session.begin_cancellable();
        let content = params.string("content").unwrap_or("preview");
        let cells = match content {
            "sources" => {
                let list = sequences(session, params)?;
                source_cells(session, &list, thumb, quality)?
            }
            "frames" => frame_cells(session, params, thumb, quality)?,
            _ => {
                let list = sequences(session, params)?;
                let per = params
                    .integer("per_sequence")
                    .map_or(5, |n| n.clamp(1, 50) as usize);
                preview_cells(session, &list, per, thumb, quality)?
            }
        };
        if cells.is_empty() {
            return Err(CommandError::BadParams(
                "there is nothing to put on the sheet (no visible sequence has a picture)".into(),
            ));
        }
        let base = if params.string("page") == Some("letter") {
            PageSize::LETTER
        } else {
            PageSize::A4
        };
        let title = params
            .string("title")
            .map(str::to_string)
            .unwrap_or_else(|| {
                session
                    .path()
                    .and_then(|p| p.file_stem())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Inbetween".to_string())
            });
        let opts = SheetOptions {
            page: if params.boolean("landscape") == Some(true) {
                base.landscape()
            } else {
                base
            },
            columns: params
                .integer("columns")
                .map_or(3, |n| n.clamp(1, 12) as u32),
            title,
            captions: params.boolean("captions") != Some(false),
        };
        let pdf_bytes = pdf::contact_sheet(&cells, &opts)?;
        let pages = pdf::pages_needed(cells.len(), &opts)?;
        let path = Path::new(out);
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display())))?;
        }
        std::fs::write(path, &pdf_bytes)
            .map_err(|e| CommandError::Io(format!("{}: {e}", path.display())))?;
        let report = json!({
            "out": out,
            "content": content,
            "pictures": cells.len(),
            "pages": pages,
            "bytes": pdf_bytes.len(),
        });
        Ok(export_options::after(session, params, saved, path, report))
    }

    pub(super) fn specs() -> Vec<CommandSpec> {
        vec![CommandSpec {
            id: "export.pdf",
            label: "Export Contact Sheet (PDF)",
            menu: "File/Export/Contact Sheet...",
            shortcut: None,
            description: "Write a PDF contact sheet: the source images of every sequence, a few frames of each sequence, or every frame of the movie, in a grid with a caption under each picture.",
            params: PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequences to draw",
            edits: false,
            run,
        }]
    }
}
