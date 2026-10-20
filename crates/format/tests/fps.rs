//! The movie frame rate in project files: old files load at 24, the default is not written, and
//! a hostile value is refused instead of reaching the engine.

use inbetween_doc::{Project, DEFAULT_FPS};
use inbetween_format::{project_from_str, project_to_string};

#[test]
fn old_files_load_at_24_and_the_default_is_not_written() {
    let text = project_to_string(&Project::default()).unwrap();
    assert!(!text.contains("fps"), "{text}");
    let old = project_from_str(&text).unwrap();
    assert_eq!(old.fps, DEFAULT_FPS);
}

#[test]
fn a_custom_rate_round_trips_exactly() {
    for fps in [1.0, 12.5, 29.97, 59.94, 60.0] {
        let p = Project {
            fps,
            ..Project::default()
        };
        let text = project_to_string(&p).unwrap();
        assert_eq!(project_from_str(&text).unwrap().fps, fps);
    }
}

#[test]
fn hostile_rates_are_refused() {
    let template = project_to_string(&Project::default()).unwrap();
    for bad in ["0", "0.5", "61", "-24", "1e308", "null", "\"fast\""] {
        let text = template.replacen(
            "\"project\": {",
            &format!("\"project\": {{\"fps\": {bad},"),
            1,
        );
        assert_ne!(text, template, "the test must change the file");
        assert!(project_from_str(&text).is_err(), "fps {bad} was accepted");
    }
}
