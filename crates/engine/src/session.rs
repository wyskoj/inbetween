use crate::{CommandError, Params};
use inbetween_doc::{Project, Sequence};
use inbetween_raster::Image;
use inbetween_render_cpu::{EffectImages, RenderError, Renderer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// How many undo steps are kept.
const MAX_UNDO: usize = 200;

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
    processed_clock: u64,
    undo: Vec<Project>,
    redo: Vec<Project>,
    /// Depth of nested `edit.beginGroup` calls. While above zero, edits merge into one undo step.
    group_depth: u32,
    group_recorded: bool,
    /// Bumped by every change to the project, so a preview can tell when to redraw.
    revision: u64,
    saved_revision: u64,
}

// Only read by the file-backed image loader, which does not exist on wasm.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct CachedImage {
    /// File modification time and length when it was loaded; a change reloads it.
    stamp: (Option<SystemTime>, u64),
    image: Arc<Image>,
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
}

impl Prepared {
    /// A CPU renderer for this sequence, effects included.
    pub fn renderer(&self) -> Result<inbetween_render_cpu::Renderer<'_>, RenderError> {
        Ok(Renderer::new(
            &self.sequence,
            self.output,
            &self.image1,
            self.image2.as_deref(),
        )?
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

    /// Note that an edit command succeeded; `before` is the project as it was.
    pub(crate) fn record_edit(&mut self, before: Project) {
        self.revision += 1;
        self.redo.clear();
        if self.group_depth > 0 {
            if self.group_recorded {
                return;
            }
            self.group_recorded = true;
        }
        self.undo.push(before);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
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
        self.redo
            .push(std::mem::replace(&mut self.project, previous));
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
        self.undo.push(std::mem::replace(&mut self.project, next));
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
        self.load_path(reference, &format!("The {name} effect image"))
            .map(Some)
    }

    /// The source image of sequence `seq` (`which` 1 or 2) with its edit applied. Without an
    /// edit this is the cached raw image itself (the same `Arc`). Edited results are cached by
    /// the file and the edit, so undoing and redoing an edit costs nothing.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn source(&mut self, seq: usize, which: u8) -> Result<Arc<Image>, CommandError> {
        let sequence = self.sequence(seq)?;
        let reference = if which == 1 {
            sequence.image1.0.clone()
        } else {
            sequence.image2.0.clone()
        };
        let edit = sequence.edit(which).clone();
        let raw = self.load_image(&reference, which, seq)?;
        if edit.is_identity() {
            return Ok(raw);
        }
        let json =
            serde_json::to_string(&edit).map_err(|e| CommandError::Internal(e.to_string()))?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&json, &mut hasher);
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
        let image = match inbetween_render_cpu::source::apply_edit(&raw, &edit) {
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
        self.load_path(reference, &format!("Image {which}"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_path(&mut self, reference: &str, label: &str) -> Result<Arc<Image>, CommandError> {
        inbetween_format::check_image_path(reference)?;
        let full = self.resolve_image_path(reference);
        let meta = std::fs::metadata(&full)
            .map_err(|e| CommandError::Image(format!("{label}: {}: {e}", full.display())))?;
        let stamp = (meta.modified().ok(), meta.len());
        if let Some(hit) = self.images.get(&full).filter(|c| c.stamp == stamp) {
            return Ok(Arc::clone(&hit.image));
        }
        let rgba = inbetween_codecs::read_file(&full)?;
        let image = Arc::new(Image::from_rgba8(rgba.width, rgba.height, &rgba.data)?);
        self.images.insert(
            full,
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
