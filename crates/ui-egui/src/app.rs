//! The application: owns the engine session and the view state, and wires the canvases,
//! menus, shortcuts and dialogs together. Anything that changes the project is an engine
//! command run by id; this file only decides which one.

use crate::crop::{CropTool, Handle};
use crate::curves::{self, CurveAction, CurveKind, CurveView};
use crate::dragdrop::{self, DropAction, DropZone, Dropped};
use crate::form::Form;
use crate::imaging::{content_mapping, to_color_image};
use crate::menus::{self, MenuNode};
use crate::open_visual::{OpenVisual, Outcome as VisualOutcome, Tab as VisualTab};
use crate::panels::{
    self, Action, CropView, DotDrag, Grabs, Hover, PanelDot, PanelLine, PreviewPanel, Shown,
    SourcePanel,
};
use crate::playback::{self, Cursor, Mode};
use crate::preview::{GpuHandle, Preview};
use crate::seqlist::{self, ListAction, Row};
use crate::shortcuts;
use crate::state::{CameraTarget, CaptionScope, EffectScope, Panel, ViewState};
use crate::view::View;
use egui::{Context, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use inbetween_engine::{
    CommandError, CommandSpec, ParamKind, PathUse, Prepared, Registry, Session,
};
use inbetween_morph::{fit_transform, Planner};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

mod actions_ui;
mod adjust_ui;
mod audio_ui;
mod camera_ui;
mod caption_ui;
mod crop_ui;
mod curve_ui;
mod drops_ui;
mod effects_ui;
mod face_library_ui;
pub use face_library_ui::LibraryDrag;
mod face_mix_ui;
mod face_model_ui;
mod form_ui;
mod frames_ui;
mod layout_ui;
mod prompt_ui;
mod select_ui;
mod track_ui;
mod transport_ui;
mod visual_ui;

/// Screen rectangles of the three canvases from the last frame (used by tests and screenshots).
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub source1: Rect,
    pub source2: Rect,
    pub preview: Rect,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            source1: Rect::ZERO,
            source2: Rect::ZERO,
            preview: Rect::ZERO,
        }
    }
}

/// Commands that throw the open project away. When it has unsaved changes the shell asks first.
/// Recent, Open Visually and dropped files all dispatch `project.open`, so they are covered.
pub const REPLACES_PROJECT: &[&str] = &["file.new", "project.new", "project.open", "file.wizard"];

/// The id that stands for closing the window in a pending prompt.
const QUIT: &str = "app.quit";

/// What waits for the answer to the unsaved-changes prompt.
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    id: String,
    params: Value,
}

/// The answers to the unsaved-changes prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptChoice {
    Save,
    DontSave,
    Cancel,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub text: String,
    pub is_error: bool,
}

/// Capture the window to a PNG after a few frames, then close. Used to check the real GPU path.
#[derive(Clone, Debug)]
pub struct ScreenshotPlan {
    pub path: PathBuf,
    pub after_frames: u32,
    /// 0 while pending, 1 when saved, 2 when it failed. The caller reads this after the window closes.
    pub outcome: Arc<AtomicU8>,
    seen: u32,
    requested: bool,
}

impl ScreenshotPlan {
    pub fn new(path: PathBuf, after_frames: u32, outcome: Arc<AtomicU8>) -> Self {
        Self {
            path,
            after_frames,
            outcome,
            seen: 0,
            requested: false,
        }
    }
}

struct PreparedCache {
    revision: u64,
    sequence: usize,
    result: Result<Prepared, String>,
}

/// What the sequence list shows, rebuilt when the project changes.
struct ListCache {
    revision: u64,
    rows: Vec<RowSnapshot>,
}

struct RowSnapshot {
    label: String,
    visible: bool,
    frames: u32,
    thumbs: [Option<TextureHandle>; 2],
}

struct SourceTexture {
    image: usize,
    handle: TextureHandle,
}

/// The mesh for the current dots, so the overlays do not retriangulate every frame.
struct MeshCache {
    revision: u64,
    planner: Option<Planner>,
    /// Triangles on Image 1 and Image 2 (the same triangulation, so they correspond).
    on_image1: Vec<[Pos2; 3]>,
    on_image2: Vec<[Pos2; 3]>,
    /// Each dot's path between the images, sampled through the planner.
    tracks: Vec<Vec<Pos2>>,
}

fn pos(p: inbetween_geom::Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

/// Each dot's path through the morph, sampled at 17 times.
fn dot_tracks(planner: &Planner) -> Vec<Vec<Pos2>> {
    let mut paths: Vec<Vec<Pos2>> = Vec::new();
    for k in 0..=16 {
        let Some(dots) = planner.dots_at(f64::from(k) / 16.0) else {
            return Vec::new();
        };
        if paths.is_empty() {
            paths = vec![Vec::new(); dots.len()];
        }
        for (path, p) in paths.iter_mut().zip(dots) {
            path.push(pos(p));
        }
    }
    paths
}

fn tri_points(t: &inbetween_geom::Triangle) -> [Pos2; 3] {
    [pos(t.a), pos(t.b), pos(t.c)]
}

pub struct InbetweenApp {
    registry: Registry,
    menus: Vec<MenuNode>,
    session: Session,
    pub state: ViewState,
    gpu: Option<GpuHandle>,
    form: Option<Form>,
    status: Option<Status>,
    prepared: Option<PreparedCache>,
    sources: [Option<SourceTexture>; 2],
    preview: Preview,
    mesh: Option<MeshCache>,
    drag: Option<DotDrag>,
    group_open: bool,
    /// What the pointer was over in each source panel last frame.
    hover: [Hover; 2],
    /// The last dot of the Line tool's chain.
    chain: Option<usize>,
    /// Where to move the pointer after a dot was added (auto-jump).
    jump: Option<Pos2>,
    list: Option<ListCache>,
    /// Thumbnails by image reference; the image is kept so a changed file is noticed.
    thumbs: HashMap<String, (Arc<inbetween_raster::Image>, TextureHandle)>,
    curve_drag: Option<usize>,
    curve_group_open: bool,
    /// The Open Visually window, when open.
    open_visual: Option<OpenVisual>,
    /// A command (or quitting) waiting for the answer to "save your changes?".
    pending: Option<Pending>,
    /// Waiting for Save As to finish before this goes ahead.
    after_save: Option<Pending>,
    /// The person said to quit (or discard and quit): let the window close.
    allow_close: bool,
    close_now: bool,
    /// What the pointer holds besides a dot (a crop handle, the caption).
    grabs: Grabs,
    crop_drag: Option<crop_ui::CropDrag>,
    crop_texture: Option<crop_ui::CropTexture>,
    /// The Adjust window keeps one undo group open while it is up.
    adjust_group_open: bool,
    /// The movie's sound player, and the sound file it was last given.
    audio: crate::audio::Player,
    audio_key: Option<String>,
    audio_pcm: Option<Arc<inbetween_audio::Pcm>>,
    /// So does the Effects window.
    effects_group_open: bool,
    /// Its thumbnail slots as drawn last frame (the drop targets), and the ones being drawn.
    fx_slots: Vec<effects_ui::FxSlot>,
    fx_slots_next: Vec<effects_ui::FxSlot>,
    adjust_changed: bool,
    adjust_scope: adjust_ui::AdjustScope,
    /// The Caption window keeps one undo group open while it is up.
    caption_group_open: bool,
    caption_changed: bool,
    /// Where the caption's anchor was when a drag on it began.
    caption_origin: Option<(f64, f64)>,
    /// The Camera window keeps one undo group open while it is up.
    camera_group_open: bool,
    /// The camera handle being dragged, with the pose it started from.
    camera_drag: Option<camera_ui::CameraDrag>,
    /// What the face windows remember between frames.
    face: face_model_ui::FaceUi,
    track_ui: track_ui::TrackUi,
    play_clock: f32,
    /// Direction of playback; only ping-pong ever turns it around.
    play_forward: bool,
    title: String,
    screenshot: Option<ScreenshotPlan>,
    pub layout: Layout,
    fullscreen: bool,
    fullscreen_request: Option<bool>,
    strip: frames_ui::StripCache,
}

impl InbetweenApp {
    /// `gpu` is the device eframe opened, if it has one; with `None` the preview runs on the CPU.
    pub fn new(ctx: &Context, gpu: Option<GpuHandle>, mut state: ViewState) -> Self {
        let _ = ctx; // fonts and visuals are installed on the first frame, in `show`
        state.sanitize();
        state.playing = false;
        let registry = Registry::standard();
        let menus = menus::build(registry.specs());
        let mut session = Session::new();
        session.set_prefs(state.prefs);
        Self {
            registry,
            menus,
            session,
            state,
            gpu,
            form: None,
            status: None,
            prepared: None,
            sources: [None, None],
            preview: Preview::default(),
            mesh: None,
            drag: None,
            group_open: false,
            hover: [Hover::default(); 2],
            chain: None,
            jump: None,
            list: None,
            thumbs: HashMap::new(),
            curve_drag: None,
            curve_group_open: false,
            open_visual: None,
            pending: None,
            after_save: None,
            allow_close: false,
            close_now: false,
            grabs: Grabs::default(),
            crop_drag: None,
            crop_texture: None,
            adjust_group_open: false,
            audio: crate::audio::Player::default(),
            audio_key: None,
            audio_pcm: None,
            effects_group_open: false,
            fx_slots: Vec::new(),
            fx_slots_next: Vec::new(),
            adjust_changed: false,
            adjust_scope: Default::default(),
            caption_group_open: false,
            caption_changed: false,
            caption_origin: None,
            camera_group_open: false,
            camera_drag: None,
            face: Default::default(),
            track_ui: Default::default(),
            play_clock: 0.0,
            play_forward: true,
            title: String::new(),
            screenshot: None,
            layout: Layout::default(),
            fullscreen: false,
            fullscreen_request: None,
            strip: frames_ui::StripCache::default(),
        }
    }

    pub fn from_creation_context(cc: &eframe::CreationContext<'_>) -> Self {
        let state = cc
            .storage
            .and_then(|s| eframe::get_value::<ViewState>(s, STATE_KEY))
            .unwrap_or_default();
        let gpu = cc
            .wgpu_render_state
            .as_ref()
            .map(GpuHandle::from_render_state);
        Self::new(&cc.egui_ctx, gpu, state)
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    pub fn form_mut(&mut self) -> Option<&mut Form> {
        self.form.as_mut()
    }

    pub fn form(&self) -> Option<&Form> {
        self.form.as_ref()
    }

    pub fn backend_name(&self) -> &'static str {
        self.preview.backend_name()
    }

    pub fn request_screenshot(&mut self, plan: ScreenshotPlan) {
        self.screenshot = Some(plan);
    }

    /// Run a command by id, the same way the CLI does. Returns the command's result.
    pub fn run_command(&mut self, id: &str, params: &Value) -> Result<Value, CommandError> {
        let params = &self.with_current_sequence(id, params);
        let result = self.registry.run_caught(&mut self.session, id, params);
        match &result {
            Ok(done) => {
                if matches!(
                    id,
                    "project.open" | "project.new" | "file.new" | "file.wizard"
                ) {
                    self.state.refit_all();
                    self.state.sequence = 0;
                    self.chain = None;
                    self.state.frame = 0;
                    self.preview.clear(self.gpu.as_ref());
                    self.prepared = None;
                    self.mesh = None;
                    self.list = None;
                }
                self.follow_list_change(id, done);
                if matches!(id, "edit.undo" | "edit.redo") {
                    // Undo ends any open group; a window that is still up keeps its edits
                    // together by opening a fresh one.
                    for open in [
                        self.adjust_group_open,
                        self.caption_group_open,
                        self.effects_group_open,
                    ] {
                        if open {
                            let _ = self.registry.run_caught(
                                &mut self.session,
                                "edit.beginGroup",
                                &json!({}),
                            );
                        }
                    }
                }
                if self.state.adjust_window.is_some()
                    && (id.starts_with("image.") || id.starts_with("movie."))
                {
                    self.adjust_changed = true;
                }
                if matches!(id, "image.saveToLibrary" | "face.library") {
                    if let Some(dir) = params["dir"].as_str().filter(|d| !d.is_empty()) {
                        self.state.library_dir = dir.to_string();
                    }
                }
                if matches!(id, "project.open" | "project.save" | "project.saveAs") {
                    if let Some(path) = self.session.path().map(|p| p.display().to_string()) {
                        self.state.push_recent(&path);
                    }
                }
                // Drags and playback would flood the status bar.
                let curve_drag = params["op"] == "move";
                let live_window = self.state.adjust_window.is_some()
                    || self.state.caption_window.is_some()
                    || self.state.camera_window.is_some()
                    || self.state.face_model_window
                    || self.state.face_mix_window
                    || self.state.face_library_window
                    || self.state.track_window
                    || self.state.effects_window.is_some();
                if !curve_drag
                    && !live_window
                    && !matches!(
                        id,
                        "dots.move"
                            | "dots.nudge"
                            | "lines.drag"
                            | "dots.select"
                            | "dots.selectNone"
                            | "edit.beginGroup"
                            | "edit.endGroup"
                    )
                {
                    if let Some(spec) = self.registry.get(id) {
                        self.status = Some(Status {
                            text: format!("{}: done", spec.label),
                            is_error: false,
                        });
                    }
                }
            }
            Err(e) => {
                // A blocked drag onto another dot is not worth shouting about.
                if !matches!(id, "dots.move" | "dots.nudge" | "lines.drag") {
                    self.status = Some(Status {
                        text: e.to_string(),
                        is_error: true,
                    });
                }
            }
        }
        result
    }

    /// Commands that take a `sequence` act on the one being edited unless told otherwise.
    fn with_current_sequence(&self, id: &str, params: &Value) -> Value {
        let takes_sequence = self
            .registry
            .get(id)
            .is_some_and(|s| s.params.iter().any(|p| p.name == "sequence"));
        match params {
            Value::Object(o) if takes_sequence && !o.contains_key("sequence") => {
                let mut o = o.clone();
                o.insert("sequence".to_string(), json!(self.state.sequence));
                Value::Object(o)
            }
            Value::Null if takes_sequence => json!({ "sequence": self.state.sequence }),
            other => other.clone(),
        }
    }

    /// Make `index` the sequence the panels edit and the preview plays.
    pub fn select_sequence(&mut self, index: usize) {
        if index == self.state.sequence {
            return;
        }
        self.state.sequence = index;
        self.chain = None;
        self.session.set_selection(index, BTreeSet::new());
        self.state.frame = 0;
        self.state.playing = false;
        self.prepared = None;
        self.mesh = None;
        self.preview.clear(self.gpu.as_ref());
    }

    /// Keep the current sequence pointing at the right row after the list changed.
    fn follow_list_change(&mut self, id: &str, done: &Value) {
        let index = done["sequence"].as_u64().map(|n| n as usize);
        match (id, index) {
            (
                "sequence.add" | "sequence.addBlank" | "sequence.duplicate" | "sequence.reorder",
                Some(n),
            ) => {
                self.select_sequence(n);
            }
            ("file.appendProject", _) => {
                if let Some(first) = done["first"].as_u64() {
                    self.select_sequence(first as usize);
                }
            }
            ("sequence.delete", _) => {
                let deleted = done["deleted"].as_u64().map_or(usize::MAX, |n| n as usize);
                if deleted < self.state.sequence {
                    self.state.sequence -= 1;
                    self.prepared = None;
                    self.mesh = None;
                }
            }
            _ => {}
        }
    }

    /// Run a menu command: straight away when it needs no input, through a form otherwise.
    pub fn invoke(&mut self, id: &str) {
        let Some(spec) = self.registry.get(id).copied() else {
            return;
        };
        if !self.registry.is_enabled(id, &self.session) {
            return;
        }
        if let Some(now) = self.toggle_state(id) {
            // An on/off option: flip it.
            let _ = self.run_command(id, &json!({ "on": !now }));
            return;
        }
        if id == "file.openVisual" {
            self.open_visual_window();
            return;
        }
        let curve = match id {
            "curve.feature" => Some(CurveKind::Feature),
            "curve.shape" => Some(CurveKind::Shape),
            "curve.master" => Some(CurveKind::Master),
            "camera.curve" => Some(CurveKind::Camera),
            _ => None,
        };
        if curve.is_some() {
            self.state.curve_window = curve;
            return;
        }
        let focus = self.state.focus_image;
        match id {
            "image.crop" => {
                self.start_crop(focus);
                return;
            }
            "image.adjust" => {
                self.open_adjust(focus);
                return;
            }
            "image.caption" => {
                self.open_caption(if focus == 2 {
                    CaptionScope::Image2
                } else {
                    CaptionScope::Image1
                });
                return;
            }
            "caption.sequence" => {
                self.open_caption(CaptionScope::Sequence);
                return;
            }
            "curve.track" => {
                self.open_track();
                return;
            }
            "face.model" => {
                self.open_face_model();
                return;
            }
            "face.mix" => {
                self.open_face_mix();
                return;
            }
            "face.library" => {
                self.open_face_library();
                return;
            }
            "camera.keyframes" => {
                self.open_camera(CameraTarget::default());
                return;
            }
            "effect.camera" => {
                self.open_camera(CameraTarget {
                    effect: Some("mask".to_string()),
                    movie: false,
                });
                return;
            }
            "caption.movie" => {
                self.open_caption(CaptionScope::Movie);
                return;
            }
            "effect.sound" => {
                self.open_effects(EffectScope::Movie);
                return;
            }
            "effect.backColor" | "effect.background" | "effect.mask" | "effect.foreground"
            | "effect.transition" | "effect.light" | "effect.trail" | "effect.radial" => {
                self.open_effects(EffectScope::Sequence);
                return;
            }
            _ => {}
        }
        // `sequence` alone does not need a form: the sequence being edited is used. Image tools
        // whose settings all have defaults still ask first, so a click never adds a surprise.
        let form_first = FORM_FIRST.contains(&id)
            || (id.starts_with("image.filter.")
                && !matches!(
                    id,
                    "image.filter.remove" | "image.filter.move" | "image.filter.clear"
                ));
        if form_first
            || spec
                .params
                .iter()
                .any(|p| p.required && p.name != "sequence")
        {
            let mut form = Form::for_spec(&spec);
            self.prefill(&mut form);
            self.form = Some(form);
            return;
        }
        if self.guard(id, &json!({})) {
            return;
        }
        if let Err(e) = self.run_command(id, &json!({})) {
            // A command whose optional path is needed after all (Save before any file exists).
            if matches!(e, CommandError::BadParams(_)) && !spec.params.is_empty() {
                let mut form = Form::for_spec(&spec);
                form.error = Some(e.to_string());
                self.form = Some(form);
            }
        }
    }

    /// Fill in what the shell knows: the image under focus, the last library folder.
    fn prefill(&self, form: &mut Form) {
        if form.fields.iter().any(|f| f.spec.name == "image") {
            form.set("image", self.state.focus_image.to_string());
        }
        if matches!(form.id, "image.saveToLibrary" | "face.library")
            && !self.state.library_dir.is_empty()
        {
            form.set("dir", self.state.library_dir.clone());
        }
    }

    /// The current value of an on/off sequence option (`None` for any other command).
    pub fn toggle_state(&self, id: &str) -> Option<bool> {
        let prefs = self.session.prefs();
        match id {
            "view.toggle.dots" => return Some(prefs.show_dots),
            "view.toggle.lines" => return Some(prefs.show_lines),
            "view.toggle.triangles" => return Some(prefs.show_triangles),
            "view.toggle.track" => return Some(prefs.show_track),
            "view.toggle.grid" => return Some(prefs.show_grid),
            "dots.option.autoPartner" => return Some(prefs.auto_partner),
            "dots.option.autoJump" => return Some(prefs.auto_jump),
            "face.option.smartDrag" => return Some(prefs.smart_drag),
            _ => {}
        }
        let seq = self.session.sequence(self.state.sequence).ok()?;
        match id {
            "morph.option.retriangulate" => Some(seq.mesh == inbetween_doc::MeshPolicy::PerFrame),
            "morph.option.dropCornerTris" => Some(seq.drop_corner_tris),
            _ => None,
        }
    }

    /// Run the open form with the values typed so far (the Run button does the same).
    pub fn submit_form(&mut self) {
        let Some(form) = self.form.clone() else {
            return;
        };
        let params = match form.to_params() {
            Ok(p) => p,
            Err(msg) => {
                if let Some(f) = self.form.as_mut() {
                    f.error = Some(msg);
                }
                return;
            }
        };
        if self.guard(form.id, &params) {
            return; // the form stays open until the prompt is answered
        }
        match self.run_command(form.id, &params) {
            Ok(_) => {
                self.form = None;
                if form.id == "project.saveAs" {
                    if let Some(p) = self.after_save.take() {
                        self.run_pending(p);
                    }
                }
            }
            Err(e) => {
                if let Some(f) = self.form.as_mut() {
                    f.error = Some(e.to_string());
                }
            }
        }
    }

    // ------------------------------------------------------------------ unsaved changes

    /// If running `id` would throw away unsaved work, hold it and ask first. Returns whether
    /// the caller must wait (the command has not run).
    fn guard(&mut self, id: &str, params: &Value) -> bool {
        if REPLACES_PROJECT.contains(&id) && self.session.is_dirty() {
            self.pending = Some(Pending {
                id: id.to_string(),
                params: params.clone(),
            });
            return true;
        }
        false
    }

    /// Open a project file (from Recent, Open Visually or a drop), asking first if there are
    /// unsaved changes.
    pub fn open_project(&mut self, path: &str) {
        let params = json!({ "path": path });
        if !self.guard("project.open", &params) {
            let _ = self.run_command("project.open", &params);
        }
    }

    /// The window was asked to close. Returns whether it may: with unsaved changes the prompt
    /// opens instead and the caller must keep the window.
    pub fn request_quit(&mut self) -> bool {
        if self.allow_close || !self.session.is_dirty() {
            return true;
        }
        self.pending = Some(Pending {
            id: QUIT.to_string(),
            params: Value::Null,
        });
        false
    }

    /// Whether the save-your-changes prompt is showing.
    pub fn prompt_open(&self) -> bool {
        self.pending.is_some()
    }

    /// Answer the prompt (the buttons do the same).
    pub fn answer_prompt(&mut self, choice: PromptChoice) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        match choice {
            PromptChoice::Cancel => {}
            PromptChoice::DontSave => self.run_pending(pending),
            PromptChoice::Save if self.session.path().is_some() => {
                if self.run_command("project.save", &json!({})).is_ok() {
                    self.run_pending(pending);
                }
            }
            PromptChoice::Save => {
                // No file yet: ask for one, then carry on.
                if let Some(spec) = self.registry.get("project.saveAs").copied() {
                    self.after_save = Some(pending);
                    self.form = Some(Form::for_spec(&spec));
                }
            }
        }
    }

    fn run_pending(&mut self, pending: Pending) {
        if pending.id == QUIT {
            self.allow_close = true;
            self.close_now = true;
            return;
        }
        if self.run_command(&pending.id, &pending.params).is_ok()
            && self.form.as_ref().is_some_and(|f| f.id == pending.id)
        {
            self.form = None;
        }
    }

    // ------------------------------------------------------------------------ drag and drop

    // ------------------------------------------------------------------------ open visually

    // ---------------------------------------------------------------------------------- data

    fn refresh_prepared(&mut self) {
        let revision = self.session.revision();
        let sequence = self.state.sequence;
        if self
            .prepared
            .as_ref()
            .is_some_and(|p| p.revision == revision && p.sequence == sequence)
        {
            return;
        }
        let result = if self.session.has_sequences() {
            self.session.prepare(sequence).map_err(|e| e.to_string())
        } else {
            Err("Open a project, or start one from two images (File menu).".to_string())
        };
        self.prepared = Some(PreparedCache {
            revision,
            sequence,
            result,
        });
    }

    fn prepared(&self) -> Option<&Prepared> {
        self.prepared.as_ref().and_then(|p| p.result.as_ref().ok())
    }

    fn refresh_mesh(&mut self) {
        let revision = self.session.revision();
        if self.mesh.as_ref().is_some_and(|m| m.revision == revision) {
            return;
        }
        let mesh = match self.prepared() {
            Some(p) => match Planner::new(&p.sequence, p.output.0, p.output.1) {
                Ok(planner) => {
                    let (a, b) = planner
                        .plan(0.5)
                        .map(|plan| {
                            (
                                plan.tris.iter().map(|t| tri_points(&t.src1)).collect(),
                                plan.tris.iter().map(|t| tri_points(&t.src2)).collect(),
                            )
                        })
                        .unwrap_or_default();
                    let tracks = dot_tracks(&planner);
                    MeshCache {
                        revision,
                        planner: Some(planner),
                        on_image1: a,
                        on_image2: b,
                        tracks,
                    }
                }
                Err(_) => MeshCache {
                    revision,
                    planner: None,
                    on_image1: vec![],
                    on_image2: vec![],
                    tracks: vec![],
                },
            },
            None => MeshCache {
                revision,
                planner: None,
                on_image1: vec![],
                on_image2: vec![],
                tracks: vec![],
            },
        };
        self.mesh = Some(mesh);
    }

    fn source_texture(&mut self, ctx: &Context, which: usize) -> Option<egui::TextureId> {
        let image = {
            let p = self.prepared()?;
            if which == 0 {
                Arc::clone(&p.image1)
            } else {
                Arc::clone(p.image2.as_ref()?)
            }
        };
        let key = Arc::as_ptr(&image) as usize;
        if self.sources[which].as_ref().is_none_or(|s| s.image != key) {
            let color = to_color_image(&image, ctx.input(|i| i.max_texture_side));
            let handle = ctx.load_texture(
                format!("image {}", which + 1),
                color,
                TextureOptions::LINEAR,
            );
            self.sources[which] = Some(SourceTexture { image: key, handle });
        }
        self.sources[which].as_ref().map(|s| s.handle.id())
    }

    /// Frames the preview can show: the morph plus its hold frames (1 with nothing loaded).
    fn total_frames(&self) -> u32 {
        self.prepared().map_or(1, |p| p.sequence.total_frames())
    }

    fn clamp_selection_and_frame(&mut self) {
        let count = self.session.project().sequences.len();
        if count > 0 && self.state.sequence >= count {
            self.select_sequence(count - 1);
            self.refresh_prepared();
        } else if count == 0 {
            self.state.sequence = 0;
            self.state.crop = None;
            if self.state.adjust_window.is_some() {
                self.close_adjust(true);
            }
            if self
                .state
                .caption_window
                .is_some_and(|s| s != CaptionScope::Movie)
            {
                self.close_caption();
            }
        }
        let frames = if self.layer_movie() {
            self.transport_frames()
        } else {
            self.prepared().map_or(1, |p| p.sequence.total_frames())
        };
        self.state.frame = self.state.frame.min(frames.saturating_sub(1));
    }

    // -------------------------------------------------------------------------------- actions

    fn apply(&mut self, action: Action) {
        match action {
            Action::CropBegin { handle, at } => self.crop_begin(handle, at),
            Action::CropMove { to } => self.crop_move(to),
            Action::CropEnd => self.crop_drag = None,
            Action::CaptionBegin => self.caption_begin(),
            Action::CaptionMove { dx, dy } => self.caption_move(dx, dy),
            Action::CaptionEnd => self.caption_origin = None,
            Action::CameraBegin { grab, at } => self.camera_begin(grab, at),
            Action::CameraMove { to } => self.camera_move(to),
            Action::CameraEnd => self.camera_drag = None,
            other => self.apply_dots(other),
        }
    }

    fn end_group(&mut self) {
        if self.group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.group_open = false;
        }
    }

    fn handle_keys(&mut self, ctx: &Context) {
        if self.form.is_some() || self.pending.is_some() || ctx.wants_keyboard_input() {
            return;
        }
        if self.state.crop.is_some() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                self.apply_crop();
                return;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.cancel_crop();
                return;
            }
        }
        // Redo has two spellings; check the more specific one first.
        let redo = egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::Z,
        );
        if ctx.input_mut(|i| i.consume_shortcut(&redo)) {
            self.invoke("edit.redo");
            return;
        }
        let specs: Vec<(&'static str, Option<egui::KeyboardShortcut>)> = self
            .registry
            .specs()
            .iter()
            .map(|s: &CommandSpec| (s.id, s.shortcut.and_then(shortcuts::parse)))
            .collect();
        for (id, shortcut) in specs {
            if let Some(sc) = shortcut {
                if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                    self.invoke(id);
                    return;
                }
            }
        }
        if self.action_keys(ctx) {
            return;
        }
        if self.dot_keys(ctx) {
            return;
        }
        let (delete, space, fit, left, right) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Backspace),
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::F) && !i.modifiers.command && !i.modifiers.alt,
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        if delete && !self.session.selection().is_empty() {
            self.invoke("edit.delete");
        }
        if space {
            self.state.playing = !self.state.playing;
        }
        if fit {
            self.state.refit_all();
        }
        if left {
            self.step_transport(-1);
        }
        if right {
            self.step_transport(1);
        }
    }

    fn advance_playback(&mut self, ctx: &Context) {
        let frames = self.transport_frames();
        if !self.state.playing || frames < 2 {
            self.state.playing = false;
            self.play_clock = 0.0;
            return;
        }
        self.play_clock += ctx.input(|i| i.stable_dt).min(0.25);
        let step = 1.0 / self.state.fps.max(1.0);
        while self.play_clock >= step {
            self.play_clock -= step;
            let at = Cursor {
                frame: self.transport_pos(),
                forward: self.play_forward,
            };
            let mode = Mode {
                looping: self.state.looping,
                ping_pong: self.state.ping_pong,
            };
            let (next, go) = playback::step(at, frames, mode);
            self.set_transport_pos(next.frame);
            self.play_forward = next.forward;
            if !go {
                self.state.playing = false;
                break;
            }
        }
        ctx.request_repaint();
    }

    // ------------------------------------------------------------------------------------ UI

    /// Draw the whole interface for one frame.
    pub fn show(&mut self, ctx: &Context) {
        // egui applies new fonts at the start of the next frame, so the first frame only sets
        // up and the real interface starts on the second.
        let ready = egui::Id::new("inbetween_ready");
        if !ctx.data(|d| d.get_temp::<bool>(ready).unwrap_or(false)) {
            crate::fonts::install(ctx);
            let mut visuals = egui::Visuals::dark();
            visuals.window_shadow = egui::epaint::Shadow::NONE;
            ctx.set_visuals(visuals);
            // Ctrl+= and Ctrl+- zoom the image panels, not the whole interface.
            ctx.options_mut(|o| o.zoom_with_keyboard = false);
            ctx.data_mut(|d| d.insert_temp(ready, true));
            ctx.request_repaint();
            return;
        }
        // Playback runs at the movie's frame rate.
        self.state.fps = self.session.project().fps as f32;
        self.apply_view_requests(ctx);
        self.handle_drops(ctx);
        self.handle_keys(ctx);
        self.refresh_prepared();
        self.clamp_selection_and_frame();
        self.refresh_mesh();
        self.refresh_list(ctx);
        self.advance_playback(ctx);
        self.sync_audio();
        self.refresh_preview(ctx);
        self.update_title(ctx);

        self.menu_bar(ctx);
        self.tool_strip(ctx);
        if let Some(p) = self.jump.take() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CursorPosition(p));
        }
        self.status_bar(ctx);
        self.transport_bar(ctx);
        self.sequence_panel(ctx);
        self.canvases(ctx);
        self.curve_window(ctx);
        self.crop_window(ctx);
        self.adjust_window(ctx);
        self.effects_window(ctx);
        self.caption_window(ctx);
        self.camera_window(ctx);
        self.face_model_window(ctx);
        self.face_mix_window(ctx);
        self.face_library_window(ctx);
        self.track_window(ctx);
        self.form_window(ctx);
        self.visual_window(ctx);
        self.drop_overlay(ctx);
        self.prompt_window(ctx);
        self.help_window(ctx);
        self.handle_close(ctx);
        self.screenshot_step(ctx);
        if self.group_open && self.drag.is_none() {
            self.end_group();
        }
    }

    // ------------------------------------------------------------------------ sequence list

    /// One thumbnail texture for an image reference, reusing the last one while the file is
    /// unchanged.
    fn thumbnail(
        &mut self,
        ctx: &Context,
        reference: &str,
        which: u8,
        sequence: usize,
    ) -> Option<TextureHandle> {
        if reference.is_empty() {
            return None;
        }
        let image = self.session.source(sequence, which).ok()?;
        let key = format!("{sequence}/{which}/{reference}");
        if let Some((old, handle)) = self.thumbs.get(&key) {
            if Arc::ptr_eq(old, &image) {
                return Some(handle.clone());
            }
        }
        let handle = ctx.load_texture(
            format!("thumbnail {reference}"),
            to_color_image(&image, 64),
            TextureOptions::LINEAR,
        );
        self.thumbs.insert(key, (image, handle.clone()));
        Some(handle)
    }

    fn refresh_list(&mut self, ctx: &Context) {
        let revision = self.session.revision();
        if self.list.as_ref().is_some_and(|l| l.revision == revision) {
            return;
        }
        let sequences = self.session.project().sequences.clone();
        let mut used = HashSet::new();
        let mut rows = Vec::with_capacity(sequences.len());
        for (i, s) in sequences.iter().enumerate() {
            used.insert(s.image1.0.clone());
            used.insert(s.image2.0.clone());
            rows.push(RowSnapshot {
                label: if s.name.is_empty() {
                    format!("Sequence {}", i + 1)
                } else {
                    s.name.clone()
                },
                visible: s.visible,
                frames: s.total_frames(),
                thumbs: [
                    self.thumbnail(ctx, &s.image1.0, 1, i),
                    self.thumbnail(ctx, &s.image2.0, 2, i),
                ],
            });
        }
        self.thumbs
            .retain(|k, _| k.starts_with("fx:") || used.contains(k));
        self.list = Some(ListCache { revision, rows });
    }

    fn sequence_panel(&mut self, ctx: &Context) {
        if !self.state.show_sequence_list {
            return;
        }
        let current = self.state.sequence;
        let rows: Vec<Row> = self.list.as_ref().map_or_else(Vec::new, |l| {
            l.rows
                .iter()
                .enumerate()
                .map(|(i, r)| Row {
                    label: r.label.clone(),
                    visible: r.visible,
                    frames: r.frames,
                    thumbs: [
                        r.thumbs[0].as_ref().map(TextureHandle::id),
                        r.thumbs[1].as_ref().map(TextureHandle::id),
                    ],
                    selected: i == current,
                })
                .collect()
        });
        let movie = self.session.project().total_frames();
        let layers = self.session.project().kind == inbetween_doc::ProjectType::Layer;
        let mut actions = Vec::new();
        egui::SidePanel::left("sequences")
            .default_width(220.0)
            .width_range(160.0..=420.0)
            .show(ctx, |ui| {
                actions = seqlist::sequence_list(ui, &rows, movie, layers);
            });
        for action in actions {
            self.apply_list(action);
        }
    }

    /// Run what a click in the sequence list asked for. Errors land in the status bar.
    pub fn apply_list(&mut self, action: ListAction) {
        let _ = match action {
            ListAction::Select(i) => {
                self.select_sequence(i);
                return;
            }
            ListAction::Visibility { index, solo } => {
                let mut p = json!({ "sequence": index });
                if solo {
                    p["solo"] = json!(true);
                }
                self.run_command("sequence.visibility", &p)
            }
            ListAction::Move(i, how) => {
                self.run_command("sequence.reorder", &json!({ "sequence": i, "move": how }))
            }
            ListAction::MoveTo { from, to } => {
                self.run_command("sequence.reorder", &json!({ "sequence": from, "to": to }))
            }
            ListAction::CopyFrame {
                from,
                from_image,
                to,
                to_image,
            } => self.run_command(
                "sequence.copyFrame",
                &json!({
                    "from_sequence": from,
                    "from_image": from_image,
                    "to_sequence": to,
                    "to_image": to_image,
                }),
            ),
            ListAction::ProjectType(kind) => {
                self.run_command("project.type", &json!({ "kind": kind }))
            }
            ListAction::Duplicate(i) => {
                self.run_command("sequence.duplicate", &json!({ "sequence": i }))
            }
            ListAction::Delete(i) => self.run_command("sequence.delete", &json!({ "sequence": i })),
            ListAction::Link(i) => self.run_command("sequence.link", &json!({ "sequence": i })),
            ListAction::Rename(i) => {
                self.select_sequence(i);
                self.invoke("sequence.rename");
                return;
            }
            ListAction::AddBlank => self.run_command("sequence.addBlank", &json!({})),
            ListAction::AddContinued => {
                self.invoke("sequence.add");
                return;
            }
        };
    }

    // ------------------------------------------------------------------------------ curves

    fn refresh_preview(&mut self, ctx: &Context) {
        let revision = self.session.revision();
        let frame = self.state.frame;
        let gpu = self.gpu.clone();
        if self.layer_movie() && self.session.has_sequences() {
            let session = &mut self.session;
            self.preview
                .refresh_composite(ctx, gpu.as_ref(), revision, frame, || {
                    session
                        .render_movie_frame(u64::from(frame) + 1)
                        .map_err(|e| e.to_string())
                });
            return;
        }
        if let Some(PreparedCache {
            result: Ok(prepared),
            ..
        }) = &self.prepared
        {
            self.preview
                .refresh(ctx, gpu.as_ref(), prepared, revision, frame);
        } else {
            self.preview.clear(gpu.as_ref());
        }
    }

    fn update_title(&mut self, ctx: &Context) {
        let name = self.session.path().and_then(|p| p.file_name()).map_or_else(
            || "untitled".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let dirty = if self.session.is_dirty() { "*" } else { "" };
        let title = format!("{name}{dirty} - Inbetween");
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn menu_bar(&mut self, ctx: &Context) {
        let mut clicked: Option<&'static str> = None;
        let mut view_action: Option<&'static str> = None;
        let entries = self.action_entries();
        let toggles: Vec<(&'static str, bool)> = TOGGLES
            .iter()
            .filter_map(|id| Some((*id, self.toggle_state(id)?)))
            .collect();
        let recent: Vec<(String, bool)> = self
            .state
            .recent
            .iter()
            .map(|p| (p.clone(), std::path::Path::new(p).exists()))
            .collect();
        let mut recent_click: Option<(String, bool)> = None;
        let mut quick: Option<usize> = None;
        let has_sequences = self.session.has_sequences();
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                for node in &self.menus {
                    if let MenuNode::Menu { name, children } = node {
                        if name == "View" {
                            continue;
                        }
                        ui.menu_button(name, |ui| {
                            menu_items(
                                ui,
                                children,
                                &self.registry,
                                &self.session,
                                &toggles,
                                &mut clicked,
                            );
                            if name == "File" {
                                ui.separator();
                                recent_menu(ui, &recent, &mut recent_click);
                            }
                            if name == "Image" {
                                ui.separator();
                                for (i, (label, ..)) in QUICK_IMAGE.iter().enumerate() {
                                    if ui
                                        .add_enabled(has_sequences, egui::Button::new(*label))
                                        .clicked()
                                    {
                                        quick = Some(i);
                                        ui.close_menu();
                                    }
                                }
                            }
                        });
                    }
                }
                ui.menu_button("View", |ui| {
                    for (id, label, shortcut, gap) in &entries {
                        if *gap {
                            ui.separator();
                        }
                        if *id == "view.sequenceList" {
                            // The engine's display commands sit between the layouts and the rest.
                            for node in &self.menus {
                                if let MenuNode::Menu { name, children } = node {
                                    if name == "View" {
                                        menu_items(
                                            ui,
                                            children,
                                            &self.registry,
                                            &self.session,
                                            &toggles,
                                            &mut clicked,
                                        );
                                    }
                                }
                            }
                            ui.separator();
                        }
                        let mut button = egui::Button::new(label.as_str());
                        if let Some(sc) = shortcut {
                            button = button.shortcut_text(shortcuts::label(sc));
                        }
                        if ui.add(button).clicked() {
                            view_action = Some(id);
                            ui.close_menu();
                        }
                    }
                });
            });
        });
        if let Some(id) = view_action {
            self.run_action(id);
        }
        if let Some((path, exists)) = recent_click {
            if exists {
                self.open_project(&path);
            } else {
                self.state.remove_recent(&path);
            }
        }
        if let Some(i) = quick {
            self.run_quick_image(i);
        }
        if let Some(id) = clicked {
            self.invoke(id);
        }
    }

    /// Run item `index` of the Image menu's quick list on the source panel last used.
    pub fn run_quick_image(&mut self, index: usize) {
        if let Some((_, id, key, value)) = QUICK_IMAGE.get(index) {
            let focus = self.state.focus_image;
            let _ = self.run_command(id, &json!({ *key: *value, "image": focus }));
        }
    }

    /// The source panel the pointer last pressed in is the one the Image menu's quick items
    /// act on.
    fn track_focus(&mut self, ctx: &Context) {
        let pressed = ctx.input(|i| i.pointer.any_pressed());
        let at = ctx.input(|i| i.pointer.interact_pos());
        if let (true, Some(p)) = (pressed, at) {
            if self.layout.source1.contains(p) {
                self.state.focus_image = 1;
            } else if self.layout.source2.contains(p) {
                self.state.focus_image = 2;
            }
        }
    }

    fn status_bar(&mut self, ctx: &Context) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match &self.status {
                    Some(s) if s.is_error => {
                        ui.colored_label(egui::Color32::from_rgb(255, 120, 110), &s.text);
                    }
                    Some(s) => {
                        ui.label(&s.text);
                    }
                    None => {
                        ui.label("Ready");
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let dots = self.prepared().map_or(0, |p| p.sequence.dots.len());
                    let zoom = self.state.view(Panel::Source1).zoom() * 100.0;
                    let backend = match &self.gpu {
                        Some(g) if self.preview.backend_name() == "GPU" => {
                            format!("GPU preview ({})", g.ctx.adapter_name())
                        }
                        _ => "CPU preview".to_string(),
                    };
                    ui.label(format!("{dots} dots  |  zoom {zoom:.0}%  |  {backend}"));
                });
            });
        });
    }

    fn snapshot(&mut self, ctx: &Context) -> Snapshot {
        let mut snap = Snapshot::empty();
        let frames_now = self.state.frame;
        match self.prepared.as_ref().map(|p| &p.result) {
            Some(Err(message)) => {
                snap.message.clone_from(message);
                snap.preview_message = Some(message.clone());
                return snap;
            }
            None => return snap,
            Some(Ok(_)) => {}
        }
        let tex = [self.source_texture(ctx, 0), self.source_texture(ctx, 1)];
        let Some(p) = self.prepared() else {
            return snap;
        };
        let frame = Vec2::new(p.output.0 as f32, p.output.1 as f32);
        snap.frame = frame;
        let seq = &p.sequence;
        let sizes = [
            (p.image1.width(), p.image1.height()),
            p.image2
                .as_ref()
                .map_or((0, 0), |i| (i.width(), i.height())),
        ];
        let name = |r: &inbetween_doc::ImageRef| {
            std::path::Path::new(&r.0)
                .file_name()
                .map_or_else(|| r.0.clone(), |n| n.to_string_lossy().into_owned())
        };
        let refs = [&seq.image1, &seq.image2];
        for i in 0..2 {
            if let (Some(id), true) = (tex[i], sizes[i].0 > 0) {
                if let Ok(fit) =
                    fit_transform(sizes[i].0, sizes[i].1, p.output.0, p.output.1, seq.fit)
                {
                    let (frame_rect, uv) = content_mapping(
                        &fit,
                        Vec2::new(sizes[i].0 as f32, sizes[i].1 as f32),
                        frame,
                    );
                    snap.shown[i] = Some(Shown { id, frame_rect, uv });
                }
            }
            snap.titles[i] = if sizes[i].0 > 0 {
                format!(
                    "Image {}: {}  ({} x {})",
                    i + 1,
                    name(refs[i]),
                    sizes[i].0,
                    sizes[i].1
                )
            } else {
                format!("Image {}: not used by this morph type", i + 1)
            };
            let img = i as u8 + 1;
            snap.dots[i] = seq
                .dots
                .pairs
                .iter()
                .enumerate()
                .filter(|(_, d)| d.has_end(img))
                .map(|(k, d)| PanelDot {
                    pos: pos(d.end(img)),
                    index: k,
                    paired: d.is_paired(),
                })
                .collect();
            snap.lines[i] = seq
                .dots
                .lines
                .iter()
                .enumerate()
                .filter_map(|(n, l)| {
                    let a = seq
                        .dots
                        .pairs
                        .get(l.a as usize)
                        .filter(|d| d.has_end(img))?;
                    let b = seq
                        .dots
                        .pairs
                        .get(l.b as usize)
                        .filter(|d| d.has_end(img))?;
                    Some(PanelLine {
                        a: pos(a.end(img)),
                        b: pos(b.end(img)),
                        index: n,
                        a_dot: l.a as usize,
                        b_dot: l.b as usize,
                    })
                })
                .collect();
            let other = 1 - i;
            if let Some(h) = self.hover[other].dot {
                match seq.dots.pairs.get(h) {
                    Some(d) if d.is_paired() => snap.partner_hover[i] = Some(h),
                    Some(d) if !d.has_end(img) => {
                        let from = other as u8 + 1;
                        let to = inbetween_morph::automatch::transfer_point(
                            seq,
                            d.end(from),
                            from,
                            p.output.0,
                            p.output.1,
                        );
                        snap.ghost[i] = Some(pos(to));
                    }
                    _ => {}
                }
            }
            snap.chain_from[i] = self
                .chain
                .and_then(|c| seq.dots.pairs.get(c))
                .filter(|d| d.has_end(img))
                .map(|d| pos(d.end(img)));
        }
        if let Some(l) = self.hover[0].line.or(self.hover[1].line) {
            if let Some(line) = seq.dots.lines.get(l) {
                snap.hot_group = seq.dots.connected(line.a as usize);
            }
        }
        if let Some(m) = &self.mesh {
            snap.tracks.clone_from(&m.tracks);
            snap.mesh[0].clone_from(&m.on_image1);
            snap.mesh[1].clone_from(&m.on_image2);
            if let Some(planner) = &m.planner {
                if let Ok(plan) = planner.plan_frame(frames_now) {
                    snap.mesh[2] = plan.tris.iter().map(|t| tri_points(&t.dst)).collect();
                }
            }
        }
        if let Some(id) = self.preview.texture_id() {
            snap.shown[2] = Some(Shown {
                id,
                frame_rect: Rect::from_min_size(Pos2::ZERO, frame),
                uv: Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            });
        }
        let t = seq.frame_time(frames_now);
        let hold = if frames_now < seq.hold_start || frames_now >= seq.hold_start + seq.frames {
            ", hold"
        } else {
            ""
        };
        snap.titles[2] = format!(
            "Preview: frame {} of {}  (t = {t:.2}{hold})  [{}]",
            frames_now + 1,
            seq.total_frames(),
            self.preview.backend_name()
        );
        snap.preview_message = self.preview.error.clone();
        for i in 0..2 {
            self.crop_snapshot(ctx, i, &mut snap);
        }
        self.caption_snapshot(&mut snap);
        self.camera_snapshot(&mut snap);
        snap
    }

    fn help_window(&mut self, ctx: &Context) {
        if !self.state.show_help {
            return;
        }
        let mut open = true;
        egui::Window::new("Help").open(&mut open).collapsible(false).show(ctx, |ui| {
            for line in [
                "Click an image to add a dot pair; drag a dot to move it on that image.",
                "Right-click a dot (or press Delete) to remove the pair.",
                "Mouse wheel zooms at the cursor. Drag empty space, or use the middle or right button, to pan.",
                "F fits every panel. Space plays and pauses. Left and Right step through frames.",
                "Ctrl+Z undoes, Ctrl+Y or Ctrl+Shift+Z redoes. A whole drag is one undo step.",
                "Everything in the menus is also available from the ib command line.",
            ] {
                ui.label(line);
            }
        });
        self.state.show_help = open;
    }

    fn screenshot_step(&mut self, ctx: &Context) {
        let Some(plan) = self.screenshot.as_mut() else {
            return;
        };
        plan.seen += 1;
        ctx.request_repaint();
        if plan.seen >= plan.after_frames && !plan.requested {
            plan.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let shot = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(Arc::clone(image)),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let saved = inbetween_codecs::encode_png(w as u32, h as u32, &bytes)
                .map_err(|e| e.to_string())
                .and_then(|png| std::fs::write(&plan.path, png).map_err(|e| e.to_string()));
            if let Err(e) = &saved {
                eprintln!("could not save the screenshot: {e}");
            }
            plan.outcome
                .store(if saved.is_ok() { 1 } else { 2 }, Ordering::SeqCst);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl eframe::App for InbetweenApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.show(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.state.playing = false;
        self.state.prefs = *self.session.prefs();
        eframe::set_value(storage, STATE_KEY, &self.state);
    }
}

const STATE_KEY: &str = "inbetween.view_state";

/// What the canvases draw this frame, copied out of the session.
struct Snapshot {
    frame: Vec2,
    titles: [String; 3],
    shown: [Option<Shown>; 3],
    dots: [Vec<PanelDot>; 2],
    lines: [Vec<PanelLine>; 2],
    partner_hover: [Option<usize>; 2],
    ghost: [Option<Pos2>; 2],
    chain_from: [Option<Pos2>; 2],
    hot_group: Vec<usize>,
    tracks: Vec<Vec<Pos2>>,
    mesh: [Vec<[Pos2; 3]>; 3],
    message: String,
    preview_message: Option<String>,
    /// The crop tool, on the panel that has it.
    crop: [Option<CropView>; 2],
    /// Where an editable caption sits on each canvas, in frame coordinates.
    caption: [Option<Rect>; 3],
    /// The camera being edited, drawn with handles on the preview.
    camera: Option<panels::CameraOverlay>,
}

impl Snapshot {
    fn empty() -> Self {
        Self {
            frame: Vec2::new(640.0, 480.0),
            titles: ["Image 1".into(), "Image 2".into(), "Preview".into()],
            shown: [None; 3],
            dots: [Vec::new(), Vec::new()],
            lines: [Vec::new(), Vec::new()],
            partner_hover: [None; 2],
            ghost: [None; 2],
            chain_from: [None; 2],
            hot_group: Vec::new(),
            tracks: Vec::new(),
            mesh: [Vec::new(), Vec::new(), Vec::new()],
            message: "Open a project, or start one from two images (File menu).".into(),
            preview_message: None,
            crop: [None, None],
            caption: [None; 3],
            camera: None,
        }
    }
}

/// Image commands that always ask (they would otherwise run at once with their defaults).
const FORM_FIRST: &[&str] = &["image.rotate", "image.reset"];

/// The Image menu's quick items: label, command, parameter name and value. Each acts on the
/// source panel last used.
pub const QUICK_IMAGE: &[(&str, &str, &str, &str)] = &[
    ("Rotate 90\u{b0} CW", "image.rotate", "turn", "cw"),
    ("Rotate 90\u{b0} CCW", "image.rotate", "turn", "ccw"),
    ("Rotate 180\u{b0}", "image.rotate", "turn", "180"),
    ("Flip Horizontal", "image.flip", "axis", "horizontal"),
    ("Flip Vertical", "image.flip", "axis", "vertical"),
];

/// Commands that switch a sequence option on or off. The menu shows a check box and a click
/// flips the option; the engine command takes `on` and reports the state when it is left out.
const TOGGLES: &[&str] = &[
    "morph.option.retriangulate",
    "morph.option.dropCornerTris",
    "view.toggle.dots",
    "view.toggle.lines",
    "view.toggle.triangles",
    "view.toggle.track",
    "view.toggle.grid",
    "dots.option.autoPartner",
    "dots.option.autoJump",
    "face.option.smartDrag",
];

fn menu_items(
    ui: &mut egui::Ui,
    nodes: &[MenuNode],
    registry: &Registry,
    session: &Session,
    toggles: &[(&'static str, bool)],
    clicked: &mut Option<&'static str>,
) {
    for node in nodes {
        match node {
            MenuNode::Menu { name, children } => {
                ui.menu_button(name, |ui| {
                    menu_items(ui, children, registry, session, toggles, clicked)
                });
            }
            MenuNode::Command { id, label } => {
                let Some(spec) = registry.get(id) else {
                    continue;
                };
                let enabled = (spec.enabled)(session);
                let mut text = label.clone();
                if let Some((_, on)) = toggles.iter().find(|(t, _)| t == id) {
                    // An on/off option shows its state instead of opening a form.
                    let mark = if *on { "[x]" } else { "[  ]" };
                    text = format!("{mark} {}", text.trim_end_matches("..."));
                }
                let mut button = egui::Button::new(text);
                if let Some(sc) = spec.shortcut {
                    button = button.shortcut_text(shortcuts::label(sc));
                }
                let response = ui
                    .add_enabled(enabled, button)
                    .on_hover_text(spec.description);
                if !enabled {
                    response.on_disabled_hover_text(spec.disabled_reason);
                } else if response.clicked() {
                    *clicked = Some(spec.id);
                    ui.close_menu();
                }
            }
        }
    }
}

/// The File > Open Recent submenu. A project that is gone is greyed; clicking it takes it off
/// the list.
fn recent_menu(ui: &mut egui::Ui, recent: &[(String, bool)], click: &mut Option<(String, bool)>) {
    ui.menu_button("Open Recent", |ui| {
        if recent.is_empty() {
            ui.add_enabled(false, egui::Button::new("(none)"));
        }
        for (path, exists) in recent {
            let name = std::path::Path::new(path)
                .file_name()
                .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
            let response = if *exists {
                ui.button(name).on_hover_text(path)
            } else {
                ui.button(egui::RichText::new(format!("{name} (missing, click to remove)")).weak())
                    .on_hover_text(path)
            };
            if response.clicked() {
                *click = Some((path.clone(), *exists));
                ui.close_menu();
            }
        }
    });
}
