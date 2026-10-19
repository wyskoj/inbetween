//! The application: owns the engine session and the view state, and wires the canvases,
//! menus, shortcuts and dialogs together. Anything that changes the project is an engine
//! command run by id; this file only decides which one.

use crate::curves::{self, CurveAction, CurveKind, CurveView};
use crate::form::Form;
use crate::imaging::{content_mapping, to_color_image};
use crate::menus::{self, MenuNode};
use crate::panels::{self, Action, DotDrag, PreviewPanel, Shown, SourcePanel};
use crate::playback::{self, Cursor, Mode};
use crate::preview::{GpuHandle, Preview};
use crate::seqlist::{self, ListAction, Row};
use crate::shortcuts;
use crate::state::{Panel, ViewState};
use crate::view::View;
use egui::{Context, Pos2, Rect, TextureHandle, TextureOptions, Vec2};
use inbetween_engine::{
    CommandError, CommandSpec, ParamKind, PathUse, Prepared, Registry, Session,
};
use inbetween_morph::{fit_transform, Planner};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

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
}

fn pos(p: inbetween_geom::Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
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
    list: Option<ListCache>,
    /// Thumbnails by image reference; the image is kept so a changed file is noticed.
    thumbs: HashMap<String, (Arc<inbetween_raster::Image>, TextureHandle)>,
    curve_drag: Option<usize>,
    curve_group_open: bool,
    play_clock: f32,
    /// Direction of playback; only ping-pong ever turns it around.
    play_forward: bool,
    title: String,
    screenshot: Option<ScreenshotPlan>,
    pub layout: Layout,
}

impl InbetweenApp {
    /// `gpu` is the device eframe opened, if it has one; with `None` the preview runs on the CPU.
    pub fn new(ctx: &Context, gpu: Option<GpuHandle>, mut state: ViewState) -> Self {
        let _ = ctx; // fonts and visuals are installed on the first frame, in `show`
        state.sanitize();
        state.playing = false;
        let registry = Registry::standard();
        let menus = menus::build(registry.specs());
        Self {
            registry,
            menus,
            session: Session::new(),
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
            list: None,
            thumbs: HashMap::new(),
            curve_drag: None,
            curve_group_open: false,
            play_clock: 0.0,
            play_forward: true,
            title: String::new(),
            screenshot: None,
            layout: Layout::default(),
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
                    self.state.selected = None;
                    self.state.frame = 0;
                    self.preview.clear(self.gpu.as_ref());
                    self.prepared = None;
                    self.mesh = None;
                    self.list = None;
                }
                self.follow_list_change(id, done);
                // Drags and playback would flood the status bar.
                let curve_drag = params["op"] == "move";
                if !curve_drag && !matches!(id, "dots.move" | "edit.beginGroup" | "edit.endGroup") {
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
                if id != "dots.move" {
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
        self.state.selected = None;
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
        let curve = match id {
            "curve.feature" => Some(CurveKind::Feature),
            "curve.shape" => Some(CurveKind::Shape),
            _ => None,
        };
        if curve.is_some() {
            self.state.curve_window = curve;
            return;
        }
        // `sequence` alone does not need a form: the sequence being edited is used.
        if spec
            .params
            .iter()
            .any(|p| p.required && p.name != "sequence")
        {
            self.form = Some(Form::for_spec(&spec));
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

    /// The current value of an on/off sequence option (`None` for any other command).
    pub fn toggle_state(&self, id: &str) -> Option<bool> {
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
        match self.run_command(form.id, &params) {
            Ok(_) => self.form = None,
            Err(e) => {
                if let Some(f) = self.form.as_mut() {
                    f.error = Some(e.to_string());
                }
            }
        }
    }

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
                    MeshCache {
                        revision,
                        planner: Some(planner),
                        on_image1: a,
                        on_image2: b,
                    }
                }
                Err(_) => MeshCache {
                    revision,
                    planner: None,
                    on_image1: vec![],
                    on_image2: vec![],
                },
            },
            None => MeshCache {
                revision,
                planner: None,
                on_image1: vec![],
                on_image2: vec![],
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
        }
        let (dots, frames) = match self.prepared() {
            Some(p) => (p.sequence.dots.len(), p.sequence.total_frames()),
            None => (0, 1),
        };
        if self.state.selected.is_some_and(|i| i >= dots) {
            self.state.selected = None;
        }
        self.state.frame = self.state.frame.min(frames.saturating_sub(1));
    }

    // -------------------------------------------------------------------------------- actions

    fn apply(&mut self, action: Action) {
        match action {
            Action::Select(s) => self.state.selected = s,
            Action::Add { x, y } => {
                if self.prepared().is_none() {
                    self.status = Some(Status {
                        text: "Load both images before placing dots.".to_string(),
                        is_error: true,
                    });
                    return;
                }
                if let Ok(r) = self.run_command("dots.add", &json!({ "x": x, "y": y })) {
                    self.state.selected = r["index"].as_u64().map(|i| i as usize);
                }
            }
            Action::BeginDrag { .. } => {
                if self.run_command("edit.beginGroup", &json!({})).is_ok() {
                    self.group_open = true;
                }
            }
            Action::Drag { index, image, x, y } => {
                let _ = self.run_command(
                    "dots.move",
                    &json!({ "index": index, "image": image, "x": x, "y": y }),
                );
            }
            Action::EndDrag => self.end_group(),
            Action::Delete(i) => {
                if self
                    .run_command("dots.delete", &json!({ "index": i }))
                    .is_ok()
                {
                    self.state.selected = None;
                }
            }
        }
    }

    fn end_group(&mut self) {
        if self.group_open {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.group_open = false;
        }
    }

    fn handle_keys(&mut self, ctx: &Context) {
        if self.form.is_some() || ctx.wants_keyboard_input() {
            return;
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
            // Delete needs an index, so the shell handles it below.
            if id == "dots.delete" {
                continue;
            }
            if let Some(sc) = shortcut {
                if ctx.input_mut(|i| i.consume_shortcut(&sc)) {
                    self.invoke(id);
                    return;
                }
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.state.show_sequence_list = !self.state.show_sequence_list;
        }
        let (delete, escape, space, fit, left, right) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
            )
        });
        if delete {
            if let Some(i) = self.state.selected {
                self.apply(Action::Delete(i));
            }
        }
        if escape {
            self.state.selected = None;
        }
        if space {
            self.state.playing = !self.state.playing;
        }
        if fit {
            self.state.refit_all();
        }
        let frames = self.total_frames();
        if left && self.state.frame > 0 {
            self.state.frame -= 1;
        }
        if right && self.state.frame + 1 < frames {
            self.state.frame += 1;
        }
    }

    fn advance_playback(&mut self, ctx: &Context) {
        let frames = self.total_frames();
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
                frame: self.state.frame,
                forward: self.play_forward,
            };
            let mode = Mode {
                looping: self.state.looping,
                ping_pong: self.state.ping_pong,
            };
            let (next, go) = playback::step(at, frames, mode);
            self.state.frame = next.frame;
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
            ctx.data_mut(|d| d.insert_temp(ready, true));
            ctx.request_repaint();
            return;
        }
        self.handle_keys(ctx);
        self.refresh_prepared();
        self.clamp_selection_and_frame();
        self.refresh_mesh();
        self.refresh_list(ctx);
        self.advance_playback(ctx);
        self.refresh_preview(ctx);
        self.update_title(ctx);

        self.menu_bar(ctx);
        self.status_bar(ctx);
        self.transport_bar(ctx);
        self.sequence_panel(ctx);
        self.canvases(ctx);
        self.curve_window(ctx);
        self.form_window(ctx);
        self.help_window(ctx);
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
        let image = self.session.load_image(reference, which, sequence).ok()?;
        if let Some((old, handle)) = self.thumbs.get(reference) {
            if Arc::ptr_eq(old, &image) {
                return Some(handle.clone());
            }
        }
        let handle = ctx.load_texture(
            format!("thumbnail {reference}"),
            to_color_image(&image, 64),
            TextureOptions::LINEAR,
        );
        self.thumbs
            .insert(reference.to_string(), (image, handle.clone()));
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
        self.thumbs.retain(|k, _| used.contains(k));
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
        let mut actions = Vec::new();
        egui::SidePanel::left("sequences")
            .default_width(220.0)
            .width_range(160.0..=420.0)
            .show(ctx, |ui| {
                actions = seqlist::sequence_list(ui, &rows, movie);
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

    fn curve_window(&mut self, ctx: &Context) {
        let Some(mut kind) = self.state.curve_window else {
            return;
        };
        let Ok(seq) = self.session.sequence(self.state.sequence) else {
            return;
        };
        let curve = match kind {
            CurveKind::Feature => seq.feature_curve.clone(),
            CurveKind::Shape => seq.shape_curve.clone(),
        };
        let last = f64::from(seq.frames.saturating_sub(1).max(1));
        let ticks: Vec<f64> = (0..seq.frames.min(100_000))
            .map(|i| f64::from(i) / last)
            .collect();
        let now = seq.frame_time(self.state.frame);
        let mut open = true;
        let mut drag = self.curve_drag;
        let mut actions: Vec<CurveAction> = Vec::new();
        let mut linear = curve.linear;
        egui::Window::new("Curves")
            .open(&mut open)
            .resizable(false)
            .default_pos([260.0, 80.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for k in [CurveKind::Feature, CurveKind::Shape] {
                        ui.selectable_value(&mut kind, k, k.label());
                    }
                });
                ui.weak(kind.help());
                ui.add_space(4.0);
                let view = CurveView {
                    curve: &curve,
                    ticks: &ticks,
                    now,
                };
                actions.extend(curves::curve_editor(ui, &view, &mut drag));
                ui.weak("Click to add a point, drag to move, right-click to remove.");
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut linear, "Linear").changed() {
                        actions.push(CurveAction::SetLinear(linear));
                    }
                    if ui.button("Reset").clicked() {
                        actions.push(CurveAction::Reset);
                    }
                    if ui
                        .button("Apply to all sequences")
                        .on_hover_text("Copy this curve to every sequence")
                        .clicked()
                    {
                        actions.push(CurveAction::ApplyAll);
                    }
                });
            });
        self.curve_drag = drag;
        let command = kind.command();
        for action in actions {
            self.apply_curve(command, action);
        }
        if self.curve_group_open && (self.curve_drag.is_none() || !open) {
            let _ = self.run_command("edit.endGroup", &json!({}));
            self.curve_group_open = false;
            self.curve_drag = None;
        }
        self.state.curve_window = open.then_some(kind);
    }

    /// Run one curve edit as the matching `curve.*` command.
    pub fn apply_curve(&mut self, command: &str, action: CurveAction) {
        let _ = match action {
            CurveAction::Add { x, y } => {
                self.run_command(command, &json!({ "op": "add", "x": x, "y": y }))
            }
            CurveAction::BeginDrag => {
                let r = self.run_command("edit.beginGroup", &json!({}));
                self.curve_group_open = r.is_ok();
                r
            }
            CurveAction::Move { index, x, y } => self.run_command(
                command,
                &json!({ "op": "move", "index": index, "x": x, "y": y }),
            ),
            CurveAction::EndDrag => Ok(Value::Null),
            CurveAction::Remove(i) => {
                self.run_command(command, &json!({ "op": "remove", "index": i }))
            }
            CurveAction::SetLinear(b) => {
                self.run_command(command, &json!({ "op": "linear", "linear": b }))
            }
            CurveAction::Reset => self.run_command(command, &json!({ "op": "reset" })),
            CurveAction::ApplyAll => self.run_command(command, &json!({ "op": "apply_all" })),
        };
    }

    fn refresh_preview(&mut self, ctx: &Context) {
        let revision = self.session.revision();
        let frame = self.state.frame;
        let gpu = self.gpu.clone();
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
        let mut view = (
            self.state.show_mesh,
            self.state.show_numbers,
            self.state.link_sources,
        );
        let mut view_action: Option<&'static str> = None;
        let toggles: Vec<(&'static str, bool)> = TOGGLES
            .iter()
            .filter_map(|id| Some((*id, self.toggle_state(id)?)))
            .collect();
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                for node in &self.menus {
                    if let MenuNode::Menu { name, children } = node {
                        ui.menu_button(name, |ui| {
                            menu_items(
                                ui,
                                children,
                                &self.registry,
                                &self.session,
                                &toggles,
                                &mut clicked,
                            );
                        });
                    }
                }
                ui.menu_button("View", |ui| {
                    if ui.button("Fit all panels").clicked() {
                        view_action = Some("fit");
                        ui.close_menu();
                    }
                    if ui.button("Actual size (100%)").clicked() {
                        view_action = Some("actual");
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.checkbox(&mut view.0, "Show mesh");
                    ui.checkbox(&mut view.1, "Show dot numbers");
                    ui.checkbox(&mut view.2, "Link the two image views");
                    ui.checkbox(&mut self.state.show_sequence_list, "Sequence list (F2)");
                    ui.separator();
                    if ui.button("Help").clicked() {
                        view_action = Some("help");
                        ui.close_menu();
                    }
                });
            });
        });
        (
            self.state.show_mesh,
            self.state.show_numbers,
            self.state.link_sources,
        ) = view;
        match view_action {
            Some("fit") => self.state.refit_all(),
            Some("actual") => self.actual_size(),
            Some("help") => self.state.show_help = !self.state.show_help,
            _ => {}
        }
        if let Some(id) = clicked {
            self.invoke(id);
        }
    }

    fn actual_size(&mut self) {
        let Some(p) = self.prepared() else { return };
        let frame = Vec2::new(p.output.0 as f32, p.output.1 as f32);
        let layout = self.layout;
        for (panel, rect) in [
            (Panel::Source1, layout.source1),
            (Panel::Source2, layout.source2),
            (Panel::Preview, layout.preview),
        ] {
            *self.state.view_mut(panel) = View::actual_size(frame, rect);
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

    fn transport_bar(&mut self, ctx: &Context) {
        let frames = self.total_frames();
        let enabled = frames > 1;
        // Just the timing, so the closure below does not borrow the session.
        let timing = self.prepared().map(|p| inbetween_doc::Sequence {
            frames: p.sequence.frames,
            hold_start: p.sequence.hold_start,
            hold_end: p.sequence.hold_end,
            ..Default::default()
        });
        egui::TopBottomPanel::bottom("transport").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    if ui.button("|<").on_hover_text("First frame").clicked() {
                        self.state.frame = 0;
                        self.play_forward = true;
                    }
                    if ui.button("<").on_hover_text("Previous frame").clicked() {
                        self.state.frame = self.state.frame.saturating_sub(1);
                    }
                    let label = if self.state.playing { "Pause" } else { "Play" };
                    if ui.button(label).clicked() {
                        self.state.playing = !self.state.playing;
                        self.play_forward = true;
                    }
                    if ui.button(">").on_hover_text("Next frame").clicked() {
                        self.state.frame = (self.state.frame + 1).min(frames.saturating_sub(1));
                    }
                    if ui.button(">|").on_hover_text("Last frame").clicked() {
                        self.state.frame = frames.saturating_sub(1);
                    }
                    let mut frame = self.state.frame;
                    ui.spacing_mut().slider_width = (ui.available_width() - 460.0).max(80.0);
                    ui.add(
                        egui::Slider::new(&mut frame, 0..=frames.saturating_sub(1)).text("frame"),
                    );
                    self.state.frame = frame;
                    let t = timing.map_or(0.0, |s| s.frame_time(self.state.frame));
                    ui.label(format!("{} / {frames}   t = {t:.2}", self.state.frame + 1));
                    ui.checkbox(&mut self.state.looping, "Loop");
                    ui.checkbox(&mut self.state.ping_pong, "Ping-pong");
                    ui.add(
                        egui::DragValue::new(&mut self.state.fps)
                            .range(1.0..=60.0)
                            .suffix(" fps"),
                    );
                });
            });
        });
    }

    fn canvases(&mut self, ctx: &Context) {
        // Everything the panels need is copied out first, so drawing never borrows the session.
        let snapshot = self.snapshot(ctx);
        let mut views = self.state.views;
        let mut drag = self.drag;
        let mut actions: Vec<Action> = Vec::new();
        let (show_mesh, show_numbers, selected) = (
            self.state.show_mesh,
            self.state.show_numbers,
            self.state.selected,
        );
        let top_fraction = self.state.top_fraction;
        let mut layout = self.layout;

        egui::CentralPanel::default().show(ctx, |ui| {
            let avail = ui.available_rect_before_wrap();
            let gap = 6.0;
            let top_h = (avail.height() * top_fraction - gap / 2.0).max(60.0);
            let half = ((avail.width() - gap) / 2.0).max(60.0);
            let left = Rect::from_min_size(avail.min, Vec2::new(half, top_h));
            let right = Rect::from_min_size(
                Pos2::new(avail.min.x + half + gap, avail.min.y),
                Vec2::new(half, top_h),
            );
            let bottom =
                Rect::from_min_max(Pos2::new(avail.min.x, avail.min.y + top_h + gap), avail.max);
            let frame = snapshot.frame;

            for (index, rect) in [(0usize, left), (1, right)] {
                let panel = SourcePanel {
                    image: index as u8 + 1,
                    title: snapshot.titles[index].clone(),
                    frame,
                    shown: snapshot.shown[index],
                    dots: &snapshot.dots[index],
                    mesh: &snapshot.mesh[index],
                    selected,
                    show_mesh,
                    show_numbers,
                    message: Some(snapshot.message.clone()),
                };
                let out = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        panels::source_panel(ui, &panel, &mut views[index], &mut drag)
                    })
                    .inner;
                actions.extend(out.actions);
                if index == 0 {
                    layout.source1 = out.rect;
                } else {
                    layout.source2 = out.rect;
                }
            }
            let preview = PreviewPanel {
                title: snapshot.titles[2].clone(),
                frame,
                shown: snapshot.shown[2],
                mesh: &snapshot.mesh[2],
                show_mesh,
                message: snapshot.preview_message.clone(),
            };
            layout.preview = ui
                .scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| {
                    panels::preview_panel(ui, &preview, &mut views[2])
                })
                .inner;
        });

        // Linked source views move together: whichever one changed drives the other.
        if self.state.link_sources {
            let old = self.state.views;
            if views[0] != old[0] {
                views[1] = views[0];
            } else if views[1] != old[1] {
                views[0] = views[1];
            }
        }
        self.state.views = views;
        self.drag = drag;
        self.layout = layout;
        for action in actions {
            self.apply(action);
        }
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
            snap.dots[i] = seq
                .dots
                .pairs
                .iter()
                .map(|d| pos(if i == 0 { d.first } else { d.second }))
                .collect();
        }
        if let Some(m) = &self.mesh {
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
        snap
    }

    fn form_window(&mut self, ctx: &Context) {
        let Some(mut form) = self.form.clone() else {
            return;
        };
        let mut submit = false;
        let mut cancel = false;
        egui::Window::new(&form.title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.label(&form.description);
                ui.add_space(6.0);
                egui::Grid::new("form")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        for field in &mut form.fields {
                            let required = if field.spec.required { " *" } else { "" };
                            ui.label(format!("{}{required}", field.spec.name))
                                .on_hover_text(field.spec.description);
                            ui.horizontal(|ui| {
                                if field.spec.kind == ParamKind::PathList {
                                    path_list_editor(ui, field);
                                } else if field.spec.kind == ParamKind::Bool {
                                    ui.checkbox(&mut field.flag, "");
                                } else {
                                    let hint = field.spec.default.unwrap_or(field.spec.description);
                                    ui.add(
                                        egui::TextEdit::singleline(&mut field.text)
                                            .desired_width(300.0)
                                            .hint_text(hint),
                                    );
                                }
                                if field.spec.kind == ParamKind::Path
                                    && ui.button("Browse...").clicked()
                                {
                                    if let Some(p) = browse(&field.spec) {
                                        field.text = p;
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                if let Some(e) = &form.error {
                    ui.add_space(4.0);
                    ui.colored_label(egui::Color32::from_rgb(255, 120, 110), e);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Run").clicked() {
                        submit = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            submit = true;
        }
        self.form = Some(form);
        if cancel {
            self.form = None;
        } else if submit {
            self.submit_form();
        }
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
        eframe::set_value(storage, STATE_KEY, &self.state);
    }
}

const STATE_KEY: &str = "inbetween.view_state";

/// What the canvases draw this frame, copied out of the session.
struct Snapshot {
    frame: Vec2,
    titles: [String; 3],
    shown: [Option<Shown>; 3],
    dots: [Vec<Pos2>; 2],
    mesh: [Vec<[Pos2; 3]>; 3],
    message: String,
    preview_message: Option<String>,
}

impl Snapshot {
    fn empty() -> Self {
        Self {
            frame: Vec2::new(640.0, 480.0),
            titles: ["Image 1".into(), "Image 2".into(), "Preview".into()],
            shown: [None; 3],
            dots: [Vec::new(), Vec::new()],
            mesh: [Vec::new(), Vec::new(), Vec::new()],
            message: "Open a project, or start one from two images (File menu).".into(),
            preview_message: None,
        }
    }
}

/// Commands that switch a sequence option on or off. The menu shows a check box and a click
/// flips the option; the engine command takes `on` and reports the state when it is left out.
const TOGGLES: &[&str] = &["morph.option.retriangulate", "morph.option.dropCornerTris"];

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

/// A native file dialog for a path parameter.
/// The editor for a list of paths: one row per path with move and remove buttons, and an
/// Add button that opens a multi-select file dialog.
fn path_list_editor(ui: &mut egui::Ui, field: &mut crate::form::Field) {
    ui.vertical(|ui| {
        let mut remove = None;
        let mut swap = None;
        let count = field.list.len();
        for (i, path) in field.list.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}.", i + 1));
                let name = std::path::Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone());
                ui.label(name).on_hover_text(path);
                if ui.add_enabled(i > 0, egui::Button::new("Up")).clicked() {
                    swap = Some((i - 1, i));
                }
                if ui
                    .add_enabled(i + 1 < count, egui::Button::new("Down"))
                    .clicked()
                {
                    swap = Some((i, i + 1));
                }
                if ui.button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some((a, b)) = swap {
            field.list.swap(a, b);
        }
        if let Some(i) = remove {
            field.list.remove(i);
        }
        if ui.button("Add...").clicked() {
            field.list.extend(browse_many(&field.spec));
        }
    });
}

fn browse_many(spec: &inbetween_engine::ParamSpec) -> Vec<String> {
    let mut dialog = rfd::FileDialog::new();
    if !spec.extensions.is_empty() {
        dialog = dialog.add_filter("Files", spec.extensions);
    }
    dialog
        .pick_files()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect()
}

fn browse(spec: &inbetween_engine::ParamSpec) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if !spec.extensions.is_empty() {
        dialog = dialog.add_filter("Files", spec.extensions);
    }
    let picked = match spec.path_use {
        Some(PathUse::Folder) => dialog.pick_folder(),
        Some(PathUse::SaveFile) => dialog.save_file(),
        Some(PathUse::OpenFile) | None => dialog.pick_file(),
    };
    picked.map(|p| p.display().to_string())
}
