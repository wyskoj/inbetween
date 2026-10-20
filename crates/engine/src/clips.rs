//! Video clips in exports: when a sequence's Image 1 is a video, its clip plays before the
//! morph (the morph starts from the clip's last frame); when Image 2 is a video, its clip plays
//! after the morph (which ends on the clip's first frame).
//!
//! Clip frames are fitted to the movie's frame the way the sequence fits its images, resampled
//! to the movie's frame rate and finished by the movie pass (effects, adjustment, filters,
//! caption). Clips are only played in exports of the whole movie, in order: leaving out a
//! range or reversing would have to number frames the project does not know about.

use crate::movie::{self, MovieRange};
use crate::movie_finish::movie_progress;
use crate::{CommandError, ParamKind, ParamSpec, Params, Session};
use inbetween_doc::{FitMode, ProjectType};
use inbetween_export::source::{self, ClipReader, Fit};
use inbetween_raster::Image;
use std::path::PathBuf;

pub(crate) const VIDEO_CLIPS: ParamSpec = ParamSpec::new(
    "video_clips",
    ParamKind::Bool,
    false,
    "When a sequence's image is a video, play its clip before (Image 1) or after (Image 2) the morph; false uses only the clip's last or first frame. Needs the whole movie: no `first`, `last`, `sequence` or `reverse`",
)
.default_is("true");

struct Clip {
    path: PathBuf,
}

struct Entry {
    sequence: usize,
    before: Option<Clip>,
    after: Option<Clip>,
}

/// The clips an export will play and how many frames they add.
pub(crate) struct ClipPlan {
    entries: Vec<Entry>,
    fps: f64,
    pub extra_frames: u64,
}

fn fit_of(mode: FitMode) -> Fit {
    match mode {
        FitMode::Stretch => Fit::Stretch,
        FitMode::Contain => Fit::Contain,
        FitMode::Cover => Fit::Cover,
    }
}

/// Work out which clips to play. `None` when there are none (or `video_clips` is off), so the
/// export is the plain one.
pub(crate) fn plan(
    session: &Session,
    params: &Params,
    range: &MovieRange,
    fps: f64,
) -> Result<Option<ClipPlan>, CommandError> {
    if params.boolean("video_clips") == Some(false) {
        return Ok(None);
    }
    let project = session.project();
    if project.kind == ProjectType::Layer {
        return Ok(None);
    }
    let video = |r: &inbetween_doc::ImageRef| source::is_video_source(&r.0);
    let any = project
        .sequences
        .iter()
        .any(|s| s.visible && (video(&s.image1) || (video(&s.image2) && s.morph.uses_image2())));
    if !any {
        return Ok(None);
    }
    let total = project.total_frames();
    let whole = range.first == 1 && range.last == total && !range.reverse;
    if !whole || params.has("sequence") {
        return Err(CommandError::BadParams(
            "this movie has video sources, whose clips play before and after their morphs; that needs the whole movie in order. Leave out `first`, `last`, `sequence` and `reverse`, or pass `video_clips` false to use only the clips' end frames".into(),
        ));
    }
    let tools = crate::source_media::tools();
    let mut entries = Vec::new();
    let mut extra = 0u64;
    for (index, seq) in project.sequences.iter().enumerate() {
        if !seq.visible || seq.total_frames() == 0 {
            continue;
        }
        let mut load =
            |r: &inbetween_doc::ImageRef, wanted: bool| -> Result<Option<Clip>, CommandError> {
                if !(wanted && video(r)) {
                    return Ok(None);
                }
                let path = session.resolve_image_path(&r.0);
                let info = source::probe(&tools, &path)?;
                extra = extra.saturating_add(info.frames_at(fps));
                Ok(Some(Clip { path }))
            };
        let before = load(&seq.image1, true)?;
        let after = load(&seq.image2, seq.morph.uses_image2())?;
        entries.push(Entry {
            sequence: index,
            before,
            after,
        });
    }
    Ok(Some(ClipPlan {
        entries,
        fps,
        extra_frames: extra,
    }))
}

/// Render the movie with its clips: each sequence's `before` clip, its morph frames, its
/// `after` clip. Returns how many frames were handed to `each`.
fn render(
    session: &mut Session,
    plan: &ClipPlan,
    each: &mut dyn FnMut(u64, &Image) -> Result<(), CommandError>,
) -> Result<u64, CommandError> {
    session.expect_frames(session.project().total_frames() + plan.extra_frames);
    let finish = session.movie_finish()?;
    let tools = crate::source_media::tools();
    let (width, height) = (
        session.project().output.width,
        session.project().output.height,
    );
    let morph_total = session.project().total_frames();
    let mut number = 1u64;
    let mut morph_done = 0u64;
    for entry in &plan.entries {
        let seq = session.sequence(entry.sequence)?.clone();
        let fit = fit_of(seq.fit);
        let play = |session: &Session,
                    clip: &Clip,
                    progress: f64,
                    number: &mut u64,
                    each: &mut dyn FnMut(u64, &Image) -> Result<(), CommandError>|
         -> Result<(), CommandError> {
            let mut reader = ClipReader::start(&tools, &clip.path, width, height, plan.fps, fit)?;
            while let Some(rgba) = reader.next_frame()? {
                session.check_cancel()?;
                let image = Image::from_rgba8(width, height, &rgba)?;
                each(*number, &finish.apply(image, progress)?)?;
                session.tick();
                *number += 1;
            }
            Ok(())
        };
        if let Some(clip) = &entry.before {
            let at = movie_progress(morph_done, morph_total);
            play(session, clip, at, &mut number, each)?;
        }
        let len = u64::from(seq.total_frames());
        let range = MovieRange {
            first: morph_done + 1,
            last: morph_done + len,
            reverse: false,
            drop_join_duplicates: false,
        };
        movie::render_movie(session, range, &mut |_, image| {
            each(number, image)?;
            number += 1;
            Ok(())
        })?;
        morph_done += len;
        if let Some(clip) = &entry.after {
            let at = movie_progress(morph_done.saturating_sub(1), morph_total);
            play(session, clip, at, &mut number, each)?;
        }
    }
    Ok(number - 1)
}

/// Render the movie for an export, with its video clips when it has any. `fps` is the export's
/// frame rate (clips are resampled to it).
pub(crate) fn render_export(
    session: &mut Session,
    plan: &Option<ClipPlan>,
    range: MovieRange,
    each: &mut dyn FnMut(u64, &Image) -> Result<(), CommandError>,
) -> Result<u64, CommandError> {
    match plan {
        None => movie::render_movie(session, range, each),
        Some(plan) => render(session, plan, each),
    }
}
