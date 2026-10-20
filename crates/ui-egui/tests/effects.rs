//! Effects in the shell: menu entries come from the registry, and the preview draws them.

use egui::Vec2;
use egui_kittest::Harness;
use inbetween_ui_egui::{json, InbetweenApp, ViewState};

fn demo_dir() -> String {
    format!("{}/../../demo", env!("CARGO_MANIFEST_DIR"))
}

fn harness() -> Harness<'static, InbetweenApp> {
    let mut app = InbetweenApp::new(&egui::Context::default(), None, ViewState::default());
    app.run_command(
        "project.open",
        &json!({ "path": format!("{}/demo.inbetween", demo_dir()) }),
    )
    .unwrap();
    let mut h = Harness::builder()
        .with_size(Vec2::new(1400.0, 900.0))
        .build_state(|ctx, app: &mut InbetweenApp| app.show(ctx), app);
    h.run();
    h
}

#[test]
fn effect_and_export_commands_are_in_the_menus() {
    let r = inbetween_engine::Registry::standard();
    for (id, menu) in [
        ("effect.mask", "Effects/"),
        ("effect.backColor", "Effects/"),
        ("effect.background", "Effects/"),
        ("effect.foreground", "Effects/"),
        ("export.animation", "File/Export/"),
        ("export.video", "File/Export/"),
    ] {
        let spec = r.specs().iter().find(|s| s.id == id).unwrap();
        assert!(spec.menu.starts_with(menu), "{id}: {}", spec.menu);
    }
}

#[test]
fn effects_edit_through_the_shell_and_undo() {
    let mut h = harness();
    let sun = format!("{}/sun.png", demo_dir());
    let on = |h: &Harness<'static, InbetweenApp>| {
        h.state().session().project().sequences[0]
            .effects
            .any_active()
    };
    h.state_mut()
        .run_command("effect.backColor", &json!({ "color": "#00ff00" }))
        .unwrap();
    h.state_mut()
        .run_command("effect.foreground", &json!({ "image": sun, "opacity": 50 }))
        .unwrap();
    h.run(); // the preview renders with effects without a GPU or a crash
    assert!(on(&h));
    // A bad file is refused and leaves the effects as they were.
    assert!(h
        .state_mut()
        .run_command("effect.mask", &json!({ "image": "no-such-file.png" }))
        .is_err());
    h.state_mut().run_command("edit.undo", &json!({})).unwrap();
    h.state_mut().run_command("edit.undo", &json!({})).unwrap();
    h.run();
    assert!(!on(&h));
}

mod window {
    use super::*;
    use egui_kittest::kittest::Queryable;
    use inbetween_ui_egui::dragdrop::{route_drop, DropAction, DropZone, Dropped};
    use inbetween_ui_egui::state::EffectScope;

    fn shot(h: &mut Harness<'static, InbetweenApp>, name: &str) {
        let Some(dir) = std::env::var_os("INBETWEEN_UI_SHOTS") else {
            return;
        };
        if let Ok(image) = h.render() {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            image.save(dir.join(format!("{name}.png"))).unwrap();
        }
    }

    fn dropped(path: &str) -> Vec<Dropped> {
        vec![Dropped {
            path: Some(path.to_string()),
            name: path.to_string(),
        }]
    }

    #[test]
    fn effects_menu_items_open_the_window_and_its_controls_edit_and_undo_as_one_step() {
        let mut h = harness();
        h.state_mut().invoke("effect.mask");
        h.run();
        assert_eq!(h.state().state.effects_window, Some(EffectScope::Sequence));
        let before = h.state().session().project().sequences[0].effects.clone();
        h.get_by_label("Back colour").click();
        h.run();
        h.get_by_label("Radial rays").click();
        h.run();
        let fx = &h.state().session().project().sequences[0].effects;
        assert!(fx.back_color.enabled && fx.radial.enabled);
        // Closing keeps the session as one undo step.
        h.get_by_label("Close").click();
        h.run();
        assert_eq!(h.state().state.effects_window, None);
        h.state_mut().run_command("edit.undo", &json!({})).unwrap();
        assert_eq!(h.state().session().project().sequences[0].effects, before);
    }

    #[test]
    fn the_movie_scope_edits_movie_effects_and_clear_resets_them() {
        let mut h = harness();
        h.state_mut().invoke("effect.sound");
        h.run();
        assert_eq!(h.state().state.effects_window, Some(EffectScope::Movie));
        h.get_by_label("Back colour").click();
        h.run();
        assert!(h.state().session().project().effects.back_color.enabled);
        assert!(
            !h.state().session().project().sequences[0]
                .effects
                .back_color
                .enabled
        );
        h.get_all_by_label("Clear").next().unwrap().click();
        h.run();
        assert!(!h.state().session().project().effects.back_color.enabled);
        shot(&mut h, "effects_window_movie");
        h.state_mut().invoke("effect.mask");
        h.run();
        shot(&mut h, "effects_window_sequence");
    }

    #[test]
    fn dropping_files_on_slots_sets_images_and_the_sound() {
        let mut h = harness();
        let sun = format!("{}/sun.png", demo_dir());
        let zone = DropZone::EffectSlot {
            effect: "effect.foreground",
            scope: EffectScope::Movie,
        };
        let actions = route_drop(&dropped(&sun), zone, false);
        assert!(matches!(actions[0], DropAction::SetEffectImage { .. }));
        h.state_mut().apply_drops(actions);
        let fg = &h.state().session().project().effects.foreground;
        assert!(fg.enabled && fg.image.0.ends_with("sun.png"));
        // A sound dropped anywhere becomes the movie's sound; a bad one is only a status.
        let wav = std::env::temp_dir().join(format!("ib-ui-{}.wav", std::process::id()));
        std::fs::write(&wav, b"RIFF\0\0\0\0WAVE").unwrap();
        let wav = wav.display().to_string();
        h.state_mut()
            .apply_drops(route_drop(&dropped(&wav), DropZone::Elsewhere, false));
        assert!(h.state().session().project().sound.is_none());
        assert!(h.state().status().is_some_and(|s| s.is_error));
        let _ = std::fs::remove_file(&wav);
    }
}

mod adjust_scope {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn the_adjust_window_can_target_the_movie_and_sequence_filters() {
        let mut h = harness();
        h.state_mut().invoke("image.adjust");
        h.run();
        h.get_all_by_label("Movie").last().unwrap().click();
        h.run();
        h.get_by_label("Negative").click();
        h.run();
        assert!(h.state().session().project().adjust.negative);
        assert!(
            !h.state().session().project().sequences[0]
                .edit1
                .adjust
                .negative
        );
        h.get_by_label("Sequence filters").click();
        h.run();
        // No colour controls at sequence scope; a filter goes to the sequence.
        h.state_mut()
            .run_command(
                "movie.filter.blur",
                &json!({ "scope": "sequence", "radius": 2 }),
            )
            .unwrap();
        h.run();
        assert_eq!(h.state().session().project().sequences[0].filters.len(), 1);
        h.get_by_label("OK").click();
        h.run();
        // The whole session is one undo step per window: undo removes the lot it grouped.
        h.state_mut().run_command("edit.undo", &json!({})).unwrap();
        assert!(!h.state().session().project().adjust.negative);
    }
}
