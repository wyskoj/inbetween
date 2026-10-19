//! The Open Visually window.

use super::*;

impl InbetweenApp {
    pub fn open_visual(&self) -> Option<&OpenVisual> {
        self.open_visual.as_ref()
    }

    pub(super) fn open_visual_window(&mut self) {
        self.open_visual = Some(OpenVisual::default());
        self.refresh_visual();
    }

    /// Browse `folder` in the Open Visually window (opening it if needed).
    pub fn browse_visual_folder(&mut self, folder: &str) {
        let v = self.open_visual.get_or_insert_with(OpenVisual::default);
        v.tab = VisualTab::Folder;
        v.folder = Some(folder.to_string());
        self.refresh_visual();
    }

    /// Ask the engine for the list the window shows (recent projects, or the chosen folder).
    pub(super) fn refresh_visual(&mut self) {
        let Some(v) = self.open_visual.as_ref() else {
            return;
        };
        let params = match (v.tab, &v.folder) {
            (VisualTab::Recent, _) => Some(json!({ "paths": self.state.recent })),
            (VisualTab::Folder, Some(f)) => Some(json!({ "folder": f })),
            (VisualTab::Folder, None) => None,
        };
        let result = params.map(|p| {
            self.registry
                .run_caught(&mut self.session, "file.openVisual", &p)
        });
        if let Some(v) = self.open_visual.as_mut() {
            match result {
                Some(Ok(done)) => v.set_entries(&done),
                Some(Err(e)) => {
                    v.set_entries(&Value::Null);
                    v.note = Some(e.to_string());
                }
                None => v.set_entries(&Value::Null),
            }
        }
    }

    pub(super) fn visual_window(&mut self, ctx: &Context) {
        let Some(mut v) = self.open_visual.take() else {
            return;
        };
        let outcome = v.show(ctx);
        self.open_visual = Some(v);
        match outcome {
            VisualOutcome::None => {}
            VisualOutcome::Close => self.open_visual = None,
            VisualOutcome::Refresh => self.refresh_visual(),
            VisualOutcome::PickFolder => {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.browse_visual_folder(&p.display().to_string());
                }
            }
            VisualOutcome::Open(path) => {
                self.open_visual = None;
                self.open_project(&path);
            }
        }
    }
}
