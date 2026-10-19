//! Rendering the whole movie frame by frame, for exports.

use crate::{CommandError, Session};
use inbetween_doc::ProjectType;
use inbetween_raster::{over, Image};

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

/// Render the frames in `range` and hand each to `each` with its movie frame number. Returns
/// how many frames were handed over.
pub(crate) fn render_movie(
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
