//! Rendering the whole movie frame by frame, for exports.

use crate::movie_finish::movie_progress;
use crate::{CommandError, Session};
use inbetween_doc::ProjectType;
use inbetween_raster::{over, Image};
use inbetween_render_cpu::trail;
use std::collections::VecDeque;

/// Most frames one movie export writes.
pub(crate) const MAX_MOVIE_FRAMES: u64 = 10_000_000;

/// Which movie frames to render and in what way. Frame numbers count from 1 across the visible
/// sequences, as `export.movie` does.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MovieRange {
    pub first: u64,
    pub last: u64,
    /// Play the movie backwards.
    pub reverse: bool,
    /// Skip a sequence's first or last frame when it is identical to the frame before it
    /// (the join of two continued sequences).
    pub drop_join_duplicates: bool,
}

/// Parse and check the shared `first`, `last`, `reverse` and `drop_duplicates` parameters.
pub(crate) fn range_from(
    session: &Session,
    params: &crate::Params,
) -> Result<MovieRange, CommandError> {
    let total = session.project().total_frames();
    if total == 0 {
        return Err(CommandError::BadParams(
            "no visible sequence has any frames; show a sequence first".into(),
        ));
    }
    let first = params.integer("first").map_or(1, |n| n as u64);
    let last = params
        .integer("last")
        .map_or(total, |n| n as u64)
        .min(total);
    if first > last {
        return Err(CommandError::BadParams(format!(
            "`first` ({first}) is after the last frame to write ({last}); the movie has {total} frames"
        )));
    }
    Ok(MovieRange {
        first,
        last,
        reverse: params.boolean("reverse") == Some(true),
        drop_join_duplicates: params.boolean("drop_duplicates") == Some(true),
    })
}

/// Render the frames in `range` and hand each to `each` with its movie frame number, finished
/// by the movie: trail, effects, adjustment, filters and caption. Returns how many frames were
/// handed over.
///
/// A movie trail looks at the frames played just before the current one. The range is
/// widened by that many frames (the extra ones are drawn but not handed over), so a single
/// frame comes out the same as it does inside a whole export.
pub(crate) fn render_movie(
    session: &mut Session,
    range: MovieRange,
    each: &mut dyn FnMut(u64, &Image) -> Result<(), CommandError>,
) -> Result<u64, CommandError> {
    let finish = session.movie_finish()?;
    let total = session.project().total_frames();
    let progress = |n: u64| movie_progress(n.saturating_sub(1), total);
    let Some(trail) = finish.trail() else {
        if finish.is_identity() {
            return render_frames(session, range, each);
        }
        return render_frames(session, range, &mut |n, image| {
            each(n, &finish.apply(image.clone(), progress(n))?)
        });
    };
    let past = trail::past_frames(trail.intensity) as usize;
    let mut wide = range;
    if range.reverse {
        wide.last = range.last.saturating_add(past as u64).min(total);
    } else {
        wide.first = range.first.saturating_sub(past as u64).max(1);
    }
    let mut ring: VecDeque<Image> = VecDeque::with_capacity(past + 1);
    let mut sent = 0u64;
    render_frames(session, wide, &mut |n, image| {
        if (range.first..=range.last).contains(&n) {
            let combined = trail::combine(&trail, image.clone(), ring.make_contiguous())?;
            each(n, &finish.apply(combined, progress(n))?)?;
            sent += 1;
        }
        ring.push_back(image.clone());
        if ring.len() > past {
            ring.pop_front();
        }
        Ok(())
    })?;
    Ok(sent)
}

fn render_frames(
    session: &mut Session,
    range: MovieRange,
    each: &mut dyn FnMut(u64, &Image) -> Result<(), CommandError>,
) -> Result<u64, CommandError> {
    if session.project().kind == ProjectType::Layer {
        return render_layers(session, range, each);
    }
    // (sequence index, movie frame where it starts counting from 0, length)
    let mut segments = Vec::new();
    let mut start = 0u64;
    for (index, s) in session.project().sequences.iter().enumerate() {
        if s.visible {
            let len = u64::from(s.total_frames());
            segments.push((index, start, len));
            start += len;
        }
    }
    if range.reverse {
        segments.reverse();
    }
    let mut sent = 0u64;
    let mut previous: Option<Image> = None;
    for (index, lo, len) in segments {
        let hi = lo + len; // this sequence owns movie frames lo+1 ..= hi
        let (from, to) = (range.first.max(lo + 1), range.last.min(hi));
        if from > to {
            continue;
        }
        let prepared = session.prepare(index)?;
        let renderer = prepared.renderer()?;
        let numbers: Box<dyn Iterator<Item = u64>> = if range.reverse {
            Box::new((from..=to).rev())
        } else {
            Box::new(from..=to)
        };
        for n in numbers {
            let local = n - 1 - lo;
            let image = renderer.render_frame(u32::try_from(local).unwrap_or(u32::MAX))?;
            if range.drop_join_duplicates {
                let at_join = local == 0 || local + 1 == len;
                if at_join && previous.as_ref() == Some(&image) {
                    continue;
                }
                previous = Some(image.clone());
            }
            each(n, &image)?;
            sent += 1;
        }
    }
    Ok(sent)
}

/// Layer projects: every movie frame is the visible sequences' frames stacked, the first
/// sequence on top. (Joins between sequences do not exist here, so `drop_duplicates` is
/// ignored.)
fn render_layers(
    session: &mut Session,
    range: MovieRange,
    each: &mut dyn FnMut(u64, &Image) -> Result<(), CommandError>,
) -> Result<u64, CommandError> {
    let indexes: Vec<usize> = session
        .project()
        .sequences
        .iter()
        .enumerate()
        .filter(|(_, s)| s.visible && s.total_frames() > 0)
        .map(|(i, _)| i)
        .collect();
    let mut prepared = Vec::with_capacity(indexes.len());
    for &i in &indexes {
        prepared.push(session.prepare(i)?);
    }
    let mut renderers = Vec::with_capacity(prepared.len());
    for (&i, p) in indexes.iter().zip(&prepared) {
        renderers.push((i, p.renderer()?));
    }
    let numbers: Box<dyn Iterator<Item = u64>> = if range.reverse {
        Box::new((range.first..=range.last).rev())
    } else {
        Box::new(range.first..=range.last)
    };
    let mut sent = 0u64;
    for n in numbers {
        let frames = session.project().layers_at(n.saturating_sub(1));
        let mut stack: Option<Image> = None;
        for (sequence, local) in frames {
            let Some((_, renderer)) = renderers.iter().find(|(i, _)| *i == sequence) else {
                continue;
            };
            let layer = renderer.render_frame(local)?;
            stack = Some(match stack {
                None => layer,
                Some(mut below) => {
                    for (b, t) in below
                        .as_mut_slice()
                        .chunks_exact_mut(4)
                        .zip(layer.as_slice().chunks_exact(4))
                    {
                        let px = over([t[0], t[1], t[2], t[3]], [b[0], b[1], b[2], b[3]]);
                        b.copy_from_slice(&px);
                    }
                    below
                }
            });
        }
        let Some(image) = stack else { continue };
        each(n, &image)?;
        sent += 1;
    }
    Ok(sent)
}

impl Session {
    /// Movie frame `n` (counting from 1 across the visible sequences, or the stacked layers of
    /// a layer project) with the movie caption drawn over it.
    pub fn render_movie_frame(&mut self, n: u64) -> Result<Image, CommandError> {
        let total = self.project().total_frames();
        if n == 0 || n > total {
            return Err(CommandError::BadParams(format!(
                "movie frame {n} is outside the movie (1 to {total})"
            )));
        }
        let range = MovieRange {
            first: n,
            last: n,
            reverse: false,
            drop_join_duplicates: false,
        };
        let mut found = None;
        render_movie(self, range, &mut |_, image| {
            found = Some(image.clone());
            Ok(())
        })?;
        found.ok_or_else(|| CommandError::BadParams(format!("movie frame {n} has nothing to draw")))
    }

    /// Movie frame `n` shrunk to fit a `max_side` square, for frame strips.
    pub fn movie_thumbnail(&mut self, n: u64, max_side: u32) -> Result<Image, CommandError> {
        let frame = self.render_movie_frame(n)?;
        frame
            .downscale_to_fit(max_side)
            .map_err(|e| CommandError::Image(e.to_string()))
    }
}
