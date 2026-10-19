use crate::{CommandError, ParamSpec, Params, Session};
use serde::Serialize;
use serde_json::Value;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// One command: everything a menu, a CLI and an agent need to offer and run it.
#[derive(Clone, Copy)]
pub struct CommandSpec {
    /// Dotted id from `docs/feature-spec.md`, e.g. `export.frame`.
    pub id: &'static str,
    pub label: &'static str,
    /// Menu path, e.g. `File/Export/Frame`. Empty when the command has no menu entry.
    pub menu: &'static str,
    pub shortcut: Option<&'static str>,
    pub description: &'static str,
    pub params: &'static [ParamSpec],
    /// Whether the command can run in this state. Shells grey out the menu item.
    pub enabled: fn(&Session) -> bool,
    /// Why it is unavailable when `enabled` is false.
    pub disabled_reason: &'static str,
    /// Whether the command changes the project. The registry then snapshots the project first,
    /// records an undo step when the command succeeds, and restores the snapshot if it fails.
    pub edits: bool,
    pub run: fn(&mut Session, &Params) -> Result<Value, CommandError>,
}

/// The serialisable description of a command, for `ib commands` and MCP.
#[derive(Clone, Debug, Serialize)]
pub struct CommandInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub menu: &'static str,
    pub shortcut: Option<&'static str>,
    pub description: &'static str,
    pub params: Vec<ParamSpec>,
}

pub struct Registry {
    specs: Vec<CommandSpec>,
}

impl Registry {
    /// Every command Inbetween has.
    pub fn standard() -> Self {
        let mut specs = Vec::new();
        specs.extend(crate::project_cmds::specs());
        specs.extend(crate::edit_cmds::specs());
        specs.extend(crate::dots_cmds::specs());
        specs.extend(crate::sequence_cmds::specs());
        specs.extend(crate::list_cmds::specs());
        specs.extend(crate::curve_cmds::specs());
        specs.extend(crate::effect_cmds::specs());
        specs.extend(crate::fx_cmds::specs());
        specs.extend(crate::image_cmds::specs());
        specs.extend(crate::filter_cmds::specs());
        specs.extend(crate::caption_cmds::specs());
        specs.extend(crate::camera_cmds::specs());
        specs.extend(crate::track_cmds::specs());
        specs.extend(crate::layer_cmds::specs());
        specs.extend(crate::export_cmds::specs());
        specs.extend(crate::anim_cmds::specs());
        specs.extend(crate::face_cmds::specs());
        #[cfg(not(target_arch = "wasm32"))]
        specs.extend(crate::file_cmds::specs());
        #[cfg(not(target_arch = "wasm32"))]
        specs.extend(crate::face_media::specs());
        Self { specs }
    }

    pub fn specs(&self) -> &[CommandSpec] {
        &self.specs
    }

    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.specs.iter().find(|s| s.id == id)
    }

    /// Whether `id` exists and can run right now.
    pub fn is_enabled(&self, id: &str, session: &Session) -> bool {
        self.get(id).is_some_and(|s| (s.enabled)(session))
    }

    pub fn describe(&self) -> Vec<CommandInfo> {
        self.specs
            .iter()
            .map(|s| CommandInfo {
                id: s.id,
                label: s.label,
                menu: s.menu,
                shortcut: s.shortcut,
                description: s.description,
                params: s.params.to_vec(),
            })
            .collect()
    }

    /// Validate `params` against the command's schema, check it is enabled, and run it.
    pub fn run(
        &self,
        session: &mut Session,
        id: &str,
        params: &Value,
    ) -> Result<Value, CommandError> {
        let spec = self
            .get(id)
            .ok_or_else(|| CommandError::UnknownCommand(id.to_string()))?;
        let params = Params::parse(spec.params, params)?;
        if !(spec.enabled)(session) {
            return Err(CommandError::Disabled {
                id: spec.id,
                reason: spec.disabled_reason,
            });
        }
        let before = spec.edits.then(|| session.project().clone());
        let result = (spec.run)(session, &params);
        if let Some(before) = before {
            match &result {
                Ok(_) => session.record_edit(before),
                Err(_) => session.restore(before),
            }
        }
        result
    }

    /// Like [`Registry::run`], but turns a panic into [`CommandError::Internal`] so a shell
    /// keeps going. Tests use `run` so panics stay visible.
    pub fn run_caught(
        &self,
        session: &mut Session,
        id: &str,
        params: &Value,
    ) -> Result<Value, CommandError> {
        catch_unwind(AssertUnwindSafe(|| self.run(session, id, params))).unwrap_or_else(|p| {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            Err(CommandError::Internal(msg))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ids_are_unique_and_well_formed() {
        let r = Registry::standard();
        for (i, a) in r.specs().iter().enumerate() {
            assert!(a.id.contains('.') && a.id == a.id.trim(), "{}", a.id);
            assert!(!a.label.is_empty() && !a.description.is_empty(), "{}", a.id);
            assert!(
                r.specs()[i + 1..].iter().all(|b| b.id != a.id),
                "duplicate {}",
                a.id
            );
            let names: Vec<_> = a.params.iter().map(|p| p.name).collect();
            let mut sorted = names.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(names.len(), sorted.len(), "{} repeats a parameter", a.id);
        }
    }

    #[test]
    fn roadmap_commands_exist() {
        let r = Registry::standard();
        for id in [
            "project.open",
            "project.save",
            "project.saveAs",
            "file.new",
            "file.appendProject",
            "dots.import",
            "dots.export",
            "image.rotate",
            "image.flip",
            "image.crop",
            "image.adjust",
            "image.reset",
            "image.swap",
            "image.caption",
            "caption.sequence",
            "caption.movie",
            "image.filter.smooth",
            "image.filter.blur",
            "image.filter.gaussian",
            "image.filter.zoomBlur",
            "image.filter.radialBlur",
            "image.filter.sharpen",
            "image.filter.emboss",
            "image.filter.mosaic",
            "image.filter.oilPaint",
            "image.filter.splash",
            "image.filter.cylinder",
            "image.filter.whirl",
            "image.filter.pinch",
            "sequence.length",
            "export.frame",
            "export.sequence",
        ] {
            assert!(r.get(id).is_some(), "missing {id}");
        }
    }

    #[test]
    fn unknown_ids_and_bad_params_are_errors() {
        let r = Registry::standard();
        let mut s = Session::new();
        assert!(matches!(
            r.run(&mut s, "nope.nothing", &json!({})),
            Err(CommandError::UnknownCommand(_))
        ));
        assert!(matches!(
            r.run(&mut s, "project.open", &json!({"bogus": 1})),
            Err(CommandError::BadParams(_))
        ));
    }

    #[test]
    fn disabled_commands_say_why() {
        let r = Registry::standard();
        let mut s = Session::new();
        let e = r.run(&mut s, "sequence.length", &json!({})).unwrap_err();
        assert!(matches!(e, CommandError::Disabled { .. }), "{e}");
        assert!(!r.is_enabled("sequence.length", &s));
        assert!(r.is_enabled("project.info", &s));
        assert!(!r.is_enabled("no.such", &s));
    }

    #[test]
    fn description_is_serialisable() {
        let r = Registry::standard();
        let v = serde_json::to_value(r.describe()).unwrap();
        assert_eq!(v.as_array().map(Vec::len), Some(r.specs().len()));
        assert!(v[0]["id"].is_string() && v[0]["params"].is_array());
    }
}
