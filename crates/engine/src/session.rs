use crate::movie_finish::{movie_progress, MovieFinish};
use crate::state::{DisplayPrefs, DotClip};
use crate::{CommandError, Params};
#[cfg(not(target_arch = "wasm32"))]
use inbetween_doc::Caption;
use inbetween_doc::{Project, Sequence};
use inbetween_raster::Image;
use inbetween_render_cpu::caption::Font;
use inbetween_render_cpu::{EffectImages, RenderError, Renderer};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// The undo history keeps at least this many steps however big the project is.
const MIN_UNDO: usize = 200;
/// Past `MIN_UNDO` steps, the oldest are dropped once the history is estimated to hold more
/// than this many bytes.
const UNDO_BUDGET: usize = 256 * 1024 * 1024;
/// A hard stop, so even tiny projects cannot grow the history without end.
const MAX_UNDO: usize = 100_000;

/// A cheap estimate of the memory one history step holds: the dots, which are what grows
/// large, plus a flat allowance per sequence and for the project itself.
fn snapshot_cost(project: &Project) -> usize {
    project.sequences.iter().fold(8 * 1024, |sum, s| {
        sum + 16 * 1024
            + std::mem::size_of_val(&s.dots.pairs[..])
            + std::mem::size_of_val(&s.dots.lines[..])
    })
}

/// Longest side of [`FramePreview`], in pixels.
pub const FRAME_PREVIEW_SIDE: u32 = 192;

/// A small straight-alpha RGBA copy of a frame, for showing progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramePreview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Sample `image` down by a whole factor so its longer side is at most [`FRAME_PREVIEW_SIDE`].
fn frame_preview(image: &Image) -> Option<FramePreview> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let step = w.max(h).div_ceil(FRAME_PREVIEW_SIDE as usize).max(1);
    let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
    let full = image.to_rgba8();
    let mut rgba = Vec::with_capacity(ow.saturating_mul(oh).saturating_mul(4));
    for y in 0..oh {
        for x in 0..ow {
            let i = (y * step).saturating_mul(w).saturating_add(x * step) * 4;
            rgba.extend_from_slice(full.get(i..i + 4)?);
        }
    }
    Some(FramePreview {
        width: ow as u32,
        height: oh as u32,
        rgba,
    })
}

/// An open project plus the caches that make repeated commands cheap.
///
/// Shells (the CLI, the GUI, MCP) hold one `Session` and change it only by running
/// commands from a [`crate::Registry`].
#[derive(Default)]
pub struct Session {
    project: Project,
    path: Option<PathBuf>,
    images: HashMap<PathBuf, CachedImage>,
    /// Edited sources by resolved path and a hash of the edit, so undo and redo reuse them.
    processed: HashMap<(PathBuf, u64), CachedProcessed>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    processed_clock: u64,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fonts: HashMap<PathBuf, CachedFont>,
    /// Drawn caption layers by caption, frame size and font.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    layers: HashMap<u64, Arc<Image>>,
    undo: Vec<Project>,
    redo: Vec<Project>,
    /// Depth of nested `edit.beginGroup` calls. While above zero, edits merge into one undo step.
    group_depth: u32,
    group_recorded: bool,
    /// Bumped by every change to the project, so a preview can tell when to redraw.
    revision: u64,
    saved_revision: u64,
    /// Selected dots of `selection_sequence`. Not part of undo.
    selection: BTreeSet<usize>,
    selection_sequence: usize,
    /// Set by a command that chose the selection after its own edit, so `record_edit` keeps it.
    selection_pinned: bool,
    clipboard: Option<DotClip>,
    prefs: DisplayPrefs,
    /// Set from another thread to stop the export in progress; cleared when one starts.
    cancel: Arc<std::sync::atomic::AtomicBool>,
    /// Frames the running export has produced, and how many it expects (0 when unknown).
    progress: Arc<std::sync::atomic::AtomicU64>,
    progress_total: Arc<std::sync::atomic::AtomicU64>,
    /// A small copy of the frame the running export finished last, for a progress window.
    latest_frame: Arc<std::sync::Mutex<Option<FramePreview>>>,
    /// The camera `camera.keyframes op=copy` took, for `op=paste`. Not part of undo.
    camera_clip: Option<inbetween_doc::Camera>,
    /// What `open_after` exports open their result with; `None` is the system's default.
    #[cfg(not(target_arch = "wasm32"))]
    opener: Option<crate::export_options::Opener>,
}

// Only read by the file-backed image loader, which does not exist on wasm.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct CachedImage {
    /// File modification time and length when it was loaded; a change reloads it.
    stamp: (Option<SystemTime>, u64),
    image: Arc<Image>,
}

/// Modification time and length of a file, to notice when it changes.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
type FileStamp = (Option<SystemTime>, u64);

/// A parsed caption font file; a change to the file reloads it.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct CachedFont {
    stamp: (Option<SystemTime>, u64),
    font: Font,
}

/// An edited source, valid while the raw image it was made from is still the cached one.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct CachedProcessed {
    raw: Arc<Image>,
    image: Arc<Image>,
    last_used: u64,
}

/// Most edited sources kept.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
const MAX_PROCESSED: usize = 32;

/// Everything needed to render one sequence, resolved and loaded.
pub struct Prepared {
    pub sequence: Sequence,
    pub output: (u32, u32),
    pub image1: Arc<Image>,
    pub image2: Option<Arc<Image>>,
    /// Images of the effects that are switched on (`None` for the others).
    pub background: Option<Arc<Image>>,
    pub mask: Option<Arc<Image>>,
    pub foreground: Option<Arc<Image>>,
    pub transition: Option<Arc<Image>>,
    pub light: Option<Arc<Image>>,
    /// The sequence's caption, already drawn at the output size.
    pub caption: Option<Arc<Image>>,
    /// What the movie does to every finished frame (effects, adjustment, filters, caption).
    /// Exports apply it; the renderer does not, so a shell previewing the movie calls
    /// [`Prepared::finish`].
    pub movie: MovieFinish,
    /// The movie frame (counting from 0) this sequence starts at, and the movie's length.
    pub movie_start: u64,
    pub movie_total: u64,
}

impl Prepared {
    /// Apply the movie's own effects, adjustment, filters and caption to frame `frame` of this
    /// sequence (counting from 0). The movie trail needs earlier movie frames, so it is left
    /// out here; exports and `render_movie_frame` include it.
    pub fn finish(&self, image: Image, frame: u32) -> Result<Image, CommandError> {
        let n = self.movie_start.saturating_add(u64::from(frame));
        self.movie.apply(image, movie_progress(n, self.movie_total))
    }

    /// A CPU renderer for this sequence, effects included.
    pub fn renderer(&self) -> Result<inbetween_render_cpu::Renderer<'_>, RenderError> {
        Ok(Renderer::new(
            &self.sequence,
            self.output,
            &self.image1,
            self.image2.as_deref(),
        )?
        .with_caption(self.caption.as_deref())
        .with_effect_images(EffectImages {
            background: self.background.as_deref(),
            mask: self.mask.as_deref(),
            foreground: self.foreground.as_deref(),
            transition: self.transition.as_deref(),
            light: self.light.as_deref(),
        }))
    }
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// A flag another thread can set to stop the running export at the next frame. The export
    /// then fails with [`CommandError::Cancelled`]; the flag clears when the next one starts.
    pub fn cancel_handle(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.cancel)
    }

    /// Replace what opens an export's result (the default hands it to the system).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_opener(&mut self, opener: crate::export_options::Opener) {
        self.opener = Some(opener);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn opener(&self) -> crate::export_options::Opener {
        self.opener
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(crate::export_options::system_opener))
    }

    pub(crate) fn camera_clip(&self) -> Option<&inbetween_doc::Camera> {
        self.camera_clip.as_ref()
    }

    pub(crate) fn set_camera_clip(&mut self, camera: inbetween_doc::Camera) {
        self.camera_clip = Some(camera);
    }

    /// Counters a shell can watch while an export runs: frames done, and frames expected
    /// (0 when the command does not know).
    pub fn progress_handles(
        &self,
    ) -> (
        Arc<std::sync::atomic::AtomicU64>,
        Arc<std::sync::atomic::AtomicU64>,
    ) {
        (Arc::clone(&self.progress), Arc::clone(&self.progress_total))
    }

    /// The frame the running export finished last, shrunk, for a progress window. Taking it
    /// leaves nothing until the next frame is done.
    pub fn take_frame_preview(&self) -> Option<FramePreview> {
        self.latest_frame.lock().ok()?.take()
    }

    /// A handle to the same slot, for a thread that cannot hold the session.
    pub fn frame_preview_handle(&self) -> Arc<std::sync::Mutex<Option<FramePreview>>> {
        Arc::clone(&self.latest_frame)
    }

    /// [`Session::tick`] for a frame that was just produced: also keeps a small copy of it.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn tick_frame(&self, image: &Image) {
        self.tick();
        if let (Some(preview), Ok(mut slot)) = (frame_preview(image), self.latest_frame.lock()) {
            *slot = Some(preview);
        }
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn tick(&self) {
        self.progress
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn expect_frames(&self, total: u64) {
        self.progress_total
            .store(total, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn begin_cancellable(&self) {
        self.progress.store(0, std::sync::atomic::Ordering::Relaxed);
        self.progress_total
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn check_cancel(&self) -> Result<(), CommandError> {
        if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            Err(CommandError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn project(&self) -> &Project {
        &self.project
    }

    /// Changes after every edit, undo, redo and open. Compare with a remembered value to know
    /// whether anything derived from the project (a preview, a mesh) is stale.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Whether the project has changes that were not saved.
    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The file the project was opened from or last saved to.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn has_sequences(&self) -> bool {
        !self.project.sequences.is_empty()
    }

    /// Replace the whole project (used by `project.open` and `project.new`). History is
    /// cleared and the project counts as saved.
    pub fn set_project(&mut self, project: Project, path: Option<PathBuf>) {
        self.selection.clear();
        self.project = project;
        self.path = path;
        self.images.clear();
        self.undo.clear();
        self.redo.clear();
        self.group_depth = 0;
        self.group_recorded = false;
        self.revision += 1;
        self.saved_revision = self.revision;
    }

    pub fn set_path(&mut self, path: PathBuf) {
        if self.path.as_ref() != Some(&path) {
            self.images.clear();
        }
        self.path = Some(path);
    }

    /// Record that the project was just saved.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// Mutable access for commands. The registry records undo and bumps the revision when an
    /// edit command succeeds, so commands must not do either themselves.
    pub(crate) fn project_mut(&mut self) -> &mut Project {
        &mut self.project
    }

    /// Put back a snapshot after a failed edit, without touching history or the revision.
    pub(crate) fn restore(&mut self, project: Project) {
        self.project = project;
    }

    /// The selected dot indices, ascending, in the sequence `selection_sequence`.
    pub fn selection(&self) -> &BTreeSet<usize> {
        &self.selection
    }

    /// The sequence the selection belongs to.
    pub fn selection_sequence(&self) -> usize {
        self.selection_sequence
    }

    /// Select `dots` of sequence `sequence`, dropping indices that do not exist.
    pub fn set_selection(&mut self, sequence: usize, dots: BTreeSet<usize>) {
        self.selection_sequence = sequence;
        self.selection = dots;
        self.clamp_selection();
    }

    /// Like `set_selection`, for an editing command that selects what it just made: the
    /// registry then keeps the selection instead of clearing it for the changed dot count.
    pub fn keep_selection(&mut self, sequence: usize, dots: BTreeSet<usize>) {
        self.set_selection(sequence, dots);
        self.selection_pinned = true;
    }

    pub fn clipboard(&self) -> Option<&DotClip> {
        self.clipboard.as_ref()
    }

    pub fn set_clipboard(&mut self, clip: Option<DotClip>) {
        self.clipboard = clip;
    }

    pub fn prefs(&self) -> &DisplayPrefs {
        &self.prefs
    }

    /// Replace the display preferences (out-of-range values are pulled into range).
    pub fn set_prefs(&mut self, prefs: DisplayPrefs) {
        self.prefs = prefs.sanitized();
    }

    fn dot_count(&self) -> Option<usize> {
        self.project
            .sequences
            .get(self.selection_sequence)
            .map(|s| s.dots.len())
    }

    /// Drop selected dots that no longer exist.
    fn clamp_selection(&mut self) {
        match self.dot_count() {
            Some(n) => self.selection.retain(|&i| i < n),
            None => self.selection.clear(),
        }
    }

    /// Clear the selection if the dot count changed (dots were renumbered), else clamp it.
    fn fix_selection(&mut self, count_before: Option<usize>) {
        if count_before != self.dot_count() {
            self.selection.clear();
        }
        self.clamp_selection();
    }

    /// Note that an edit command succeeded; `before` is the project as it was.
    pub(crate) fn record_edit(&mut self, before: Project) {
        if std::mem::take(&mut self.selection_pinned) {
            self.clamp_selection();
        } else {
            self.fix_selection(
                before
                    .sequences
                    .get(self.selection_sequence)
                    .map(|s| s.dots.len()),
            );
        }
        self.revision += 1;
        self.redo.clear();
        if self.group_depth > 0 {
            if self.group_recorded {
                return;
            }
            self.group_recorded = true;
        }
        self.undo.push(before);
        self.trim_undo();
    }

    /// Drop the oldest steps while the history is over its byte budget, keeping `MIN_UNDO`.
    fn trim_undo(&mut self) {
        let mut total: usize = self.undo.iter().map(snapshot_cost).sum();
        let mut drop = 0;
        while self.undo.len() - drop > MAX_UNDO
            || (self.undo.len() - drop > MIN_UNDO && total > UNDO_BUDGET)
        {
            total = total.saturating_sub(self.undo.get(drop).map_or(0, snapshot_cost));
            drop += 1;
        }
        if drop > 0 {
            self.undo.drain(..drop);
        }
    }

    /// How many steps `undo` can go back.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Start merging edits into one undo step (nestable).
    pub(crate) fn begin_group(&mut self) {
        if self.group_depth == 0 {
            self.group_recorded = false;
        }
        self.group_depth += 1;
    }

    pub(crate) fn end_group(&mut self) {
        self.group_depth = self.group_depth.saturating_sub(1);
        if self.group_depth == 0 {
            self.group_recorded = false;
        }
    }

    /// Step back one edit. Returns false if there is nothing to undo.
    pub(crate) fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let count = self.dot_count();
        self.redo
            .push(std::mem::replace(&mut self.project, previous));
        self.fix_selection(count);
        self.group_depth = 0;
        self.group_recorded = false;
        self.revision += 1;
        true
    }

    /// Step forward one undone edit. Returns false if there is nothing to redo.
    pub(crate) fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let count = self.dot_count();
        self.undo.push(std::mem::replace(&mut self.project, next));
        self.fix_selection(count);
        self.group_depth = 0;
        self.group_recorded = false;
        self.revision += 1;
        true
    }

    /// Read the optional `sequence` parameter (default 0) and check it exists.
    pub fn sequence_index(&self, params: &Params) -> Result<usize, CommandError> {
        let index = params.integer("sequence").map_or(0, |n| n.max(0) as usize);
        if index >= self.project.sequences.len() {
            return Err(CommandError::NoSuchSequence {
                index,
                count: self.project.sequences.len(),
            });
        }
        Ok(index)
    }

    pub fn sequence(&self, index: usize) -> Result<&Sequence, CommandError> {
        self.project
            .sequences
            .get(index)
            .ok_or(CommandError::NoSuchSequence {
                index,
                count: self.project.sequences.len(),
            })
    }

    pub fn sequence_mut(&mut self, index: usize) -> Result<&mut Sequence, CommandError> {
        let count = self.project.sequences.len();
        self.project
            .sequences
            .get_mut(index)
            .ok_or(CommandError::NoSuchSequence { index, count })
    }

    /// Where an image reference points: absolute paths as they are, relative ones against the
    /// project file's folder (or the working directory for a project with no file yet).
    pub fn resolve_image_path(&self, reference: &str) -> PathBuf {
        resolve_against(self.path.as_deref(), reference)
    }

    /// Load (or reuse) the images for a sequence and bundle them for the renderer.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn prepare(&mut self, index: usize) -> Result<Prepared, CommandError> {
        let sequence = self.sequence(index)?.clone();
        let output = (self.project.output.width, self.project.output.height);
        let image1 = self.source(index, 1)?;
        let image2 = if sequence.morph.uses_image2() {
            Some(self.source(index, 2)?)
        } else {
            None
        };
        let fx = &sequence.effects;
        let background = self.load_effect(
            fx.background.enabled,
            &fx.background.image.0,
            "background",
            index,
        )?;
        let mask = self.load_effect(fx.mask.enabled, &fx.mask.image.0, "mask", index)?;
        let foreground = self.load_effect(
            fx.foreground.enabled,
            &fx.foreground.image.0,
            "foreground",
            index,
        )?;
        let transition = self.load_effect(
            fx.transition.enabled,
            &fx.transition.image.0,
            "transition",
            index,
        )?;
        let light = self.load_effect(fx.light.enabled, &fx.light.image.0, "light", index)?;
        let caption = match &sequence.caption {
            Some(c) => self.caption_layer(c, output)?,
            None => None,
        };
        let movie = self.movie_finish()?;
        let project = &self.project;
        let movie_start = if project.kind == inbetween_doc::ProjectType::Sequence {
            project
                .sequences
                .iter()
                .take(index)
                .filter(|s| s.visible)
                .map(|s| u64::from(s.total_frames()))
                .sum()
        } else {
            0
        };
        let movie_total = project.total_frames();
        Ok(Prepared {
            sequence,
            output,
            image1,
            image2,
            background,
            mask,
            foreground,
            transition,
            light,
            caption,
            movie,
            movie_start,
            movie_total,
        })
    }

    /// Load an effect's image if the effect is switched on.
    #[cfg(not(target_arch = "wasm32"))]
    fn load_effect(
        &mut self,
        enabled: bool,
        reference: &str,
        name: &str,
        seq: usize,
    ) -> Result<Option<Arc<Image>>, CommandError> {
        if !enabled {
            return Ok(None);
        }
        if reference.is_empty() {
            return Err(CommandError::Image(format!(
                "the {name} effect of sequence {seq} is on but has no image; set one with effect.{name}"
            )));
        }
        self.load_path(reference, &format!("The {name} effect image"), 0)
            .map(Some)
    }

    /// The source image of sequence `seq` (`which` 1 or 2) with its edit applied. Without an
    /// edit this is the cached raw image itself (the same `Arc`). Edited results are cached by
    /// the file and the edit, so undoing and redoing an edit costs nothing.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn source(&mut self, seq: usize, which: u8) -> Result<Arc<Image>, CommandError> {
        let edit = self.sequence(seq)?.edit(which).clone();
        self.source_with(seq, which, &edit)
    }

    /// Like [`Session::source`], but with `edit` standing in for the image's own edit (a
    /// shell previewing a change, such as the crop tool showing the picture uncropped).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn source_with(
        &mut self,
        seq: usize,
        which: u8,
        edit: &inbetween_doc::ImageEdit,
    ) -> Result<Arc<Image>, CommandError> {
        let sequence = self.sequence(seq)?;
        let reference = if which == 1 {
            sequence.image1.0.clone()
        } else {
            sequence.image2.0.clone()
        };
        let edit = edit.clone();
        let raw = self.load_image(&reference, which, seq)?;
        if edit.is_identity() {
            return Ok(raw);
        }
        let json =
            serde_json::to_string(&edit).map_err(|e| CommandError::Internal(e.to_string()))?;
        let font = match edit.caption.as_ref().filter(|c| c.is_visible()) {
            Some(c) => self.caption_font(c)?,
            None => None,
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&json, &mut hasher);
        std::hash::Hash::hash(&font.as_ref().map(|f| f.1), &mut hasher);
        let key = (
            self.resolve_image_path(&reference),
            std::hash::Hasher::finish(&hasher),
        );
        self.processed_clock += 1;
        let clock = self.processed_clock;
        if let Some(hit) = self.processed.get_mut(&key) {
            if Arc::ptr_eq(&hit.raw, &raw) {
                hit.last_used = clock;
                return Ok(Arc::clone(&hit.image));
            }
        }
        let image = match inbetween_render_cpu::source::apply_edit(
            &raw,
            &edit,
            font.as_ref().map(|f| &f.0),
        ) {
            Ok(Some(image)) => Arc::new(image),
            Ok(None) => return Ok(raw),
            Err(e) => {
                return Err(CommandError::Image(format!(
                    "Image {which} of sequence {seq}: could not apply its edits: {e}"
                )))
            }
        };
        if self.processed.len() >= MAX_PROCESSED {
            let oldest = self
                .processed
                .iter()
                .min_by_key(|(_, v)| v.last_used)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                self.processed.remove(&k);
            }
        }
        self.processed.insert(
            key,
            CachedProcessed {
                raw,
                image: Arc::clone(&image),
                last_used: clock,
            },
        );
        Ok(image)
    }

    /// The caption's own font file, loaded through the cache, with its file stamp. `None` when
    /// the caption uses the bundled font.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn caption_font(
        &mut self,
        caption: &Caption,
    ) -> Result<Option<(Font, FileStamp)>, CommandError> {
        let Some(reference) = caption.font.as_ref().filter(|r| !r.0.is_empty()) else {
            return Ok(None);
        };
        inbetween_format::check_image_path(&reference.0)?;
        let full = self.resolve_image_path(&reference.0);
        let meta = std::fs::metadata(&full)
            .map_err(|e| CommandError::Image(format!("caption font {}: {e}", full.display())))?;
        let stamp = (meta.modified().ok(), meta.len());
        if let Some(hit) = self.fonts.get(&full).filter(|c| c.stamp == stamp) {
            return Ok(Some((hit.font.clone(), stamp)));
        }
        if meta.len() > inbetween_render_cpu::caption::MAX_FONT_BYTES as u64 {
            return Err(CommandError::Image(format!(
                "caption font {}: the file is larger than {} MiB",
                full.display(),
                inbetween_render_cpu::caption::MAX_FONT_BYTES >> 20
            )));
        }
        let bytes = std::fs::read(&full)
            .map_err(|e| CommandError::Image(format!("caption font {}: {e}", full.display())))?;
        let font = Font::from_bytes(bytes)
            .map_err(|e| CommandError::Image(format!("caption font {}: {e}", full.display())))?;
        self.fonts.insert(
            full,
            CachedFont {
                stamp,
                font: font.clone(),
            },
        );
        Ok(Some((font, stamp)))
    }

    /// `caption` drawn as a layer of `frame` size, cached. `None` when it draws nothing.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn caption_layer(
        &mut self,
        caption: &Caption,
        frame: (u32, u32),
    ) -> Result<Option<Arc<Image>>, CommandError> {
        if !caption.is_visible() {
            return Ok(None);
        }
        let font = self.caption_font(caption)?;
        let json =
            serde_json::to_string(caption).map_err(|e| CommandError::Internal(e.to_string()))?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&(&json, frame, font.as_ref().map(|f| f.1)), &mut hasher);
        let key = std::hash::Hasher::finish(&hasher);
        if let Some(hit) = self.layers.get(&key) {
            return Ok(Some(Arc::clone(hit)));
        }
        let font = match font {
            Some((f, _)) => f,
            None => Font::bundled()?,
        };
        let layer = inbetween_render_cpu::caption::render_layer(caption, &font, frame)?;
        let Some(layer) = layer else {
            return Ok(None);
        };
        if self.layers.len() >= 16 {
            self.layers.clear();
        }
        let layer = Arc::new(layer);
        self.layers.insert(key, Arc::clone(&layer));
        Ok(Some(layer))
    }

    /// The movie-wide caption drawn at the output size, if there is one.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn movie_caption(&mut self) -> Result<Option<Arc<Image>>, CommandError> {
        let Some(caption) = self.project.caption.clone() else {
            return Ok(None);
        };
        let output = (self.project.output.width, self.project.output.height);
        self.caption_layer(&caption, output)
    }

    /// The movie's effect images, adjustment, filters and caption, loaded for rendering.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn movie_finish(&mut self) -> Result<MovieFinish, CommandError> {
        let fx = self.project.effects.clone();
        let load = |session: &mut Self, on: bool, image: &inbetween_doc::ImageRef, name: &str| {
            if !on {
                return Ok(None);
            }
            if image.0.is_empty() {
                return Err(CommandError::Image(format!(
                    "the movie {name} effect is on but has no image; set one with effect.{name} scope=movie"
                )));
            }
            session
                .load_path(&image.0, &format!("The movie {name} effect image"), 0)
                .map(Some)
        };
        let background = load(
            self,
            fx.background.enabled,
            &fx.background.image,
            "background",
        )?;
        let mask = load(self, fx.mask.enabled, &fx.mask.image, "mask")?;
        let foreground = load(
            self,
            fx.foreground.enabled,
            &fx.foreground.image,
            "foreground",
        )?;
        let light = load(self, fx.light.enabled, &fx.light.image, "light")?;
        Ok(MovieFinish {
            effects: fx,
            adjust: self.project.adjust,
            filters: self.project.filters.clone(),
            background,
            mask,
            foreground,
            light,
            caption: self.movie_caption()?,
        })
    }

    /// How many edited sources are cached (for tests and diagnostics).
    pub fn cached_sources(&self) -> usize {
        self.processed.len()
    }

    /// Load one image reference through the cache.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_image(
        &mut self,
        reference: &str,
        which: u8,
        seq: usize,
    ) -> Result<Arc<Image>, CommandError> {
        if reference.is_empty() {
            return Err(CommandError::Image(format!(
                "Image {which} of sequence {seq} has no file set; choose one with sequence.image"
            )));
        }
        self.load_path(reference, &format!("Image {which}"), which)
    }

    /// Load a movie effect image (or any file by its path) through the cache.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_effect_path(&mut self, reference: &str) -> Result<Arc<Image>, CommandError> {
        self.load_path(reference, "The movie effect image", 0)
    }

    #[cfg(not(target_arch = "wasm32"))]
    /// `which` is the image number (1 or 2; 0 for effect images). A video file stands for its
    /// last frame as Image 1 and its first frame otherwise, so each end is cached on its own.
    fn load_path(
        &mut self,
        reference: &str,
        label: &str,
        which: u8,
    ) -> Result<Arc<Image>, CommandError> {
        inbetween_format::check_image_path(reference)?;
        let full = self.resolve_image_path(reference);
        let meta = std::fs::metadata(&full)
            .map_err(|e| CommandError::Image(format!("{label}: {}: {e}", full.display())))?;
        let stamp = (meta.modified().ok(), meta.len());
        let video = inbetween_export::source::is_video_source(reference);
        let key = if video {
            PathBuf::from(format!("{}#{}", full.display(), u8::from(which == 1)))
        } else {
            full.clone()
        };
        if let Some(hit) = self.images.get(&key).filter(|c| c.stamp == stamp) {
            return Ok(Arc::clone(&hit.image));
        }
        let rgba = if video {
            crate::source_media::video_frame(&full, which)
                .map_err(|e| CommandError::Image(format!("{label}: {e}")))?
        } else {
            inbetween_codecs::read_file(&full)?
        };
        let image = Arc::new(Image::from_rgba8(rgba.width, rgba.height, &rgba.data)?);
        self.images.insert(
            key,
            CachedImage {
                stamp,
                image: Arc::clone(&image),
            },
        );
        Ok(image)
    }

    /// How many images are cached (for tests and diagnostics).
    pub fn cached_images(&self) -> usize {
        self.images.len()
    }
}

/// Where an image reference points when read from the project file `project_file`: absolute
/// paths as they are, relative ones against that file's folder (the working directory when
/// there is no file).
pub(crate) fn resolve_against(project_file: Option<&Path>, reference: &str) -> PathBuf {
    let path = Path::new(reference);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let base = project_file
        .and_then(Path::parent)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    base.join(path)
}
