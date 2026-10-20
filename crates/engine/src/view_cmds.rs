//! Display and dot-tool preferences: `view.toggle.*`, `view.magnifier`, `prefs.dotStyle`,
//! `prefs.dotColor`, `dots.option.autoPartner`, `dots.option.autoJump`. They edit the session's
//! [`DisplayPrefs`]; they are not saved in the project and not undone.

use crate::{
    CommandError, CommandSpec, DisplayPrefs, DotColorMode, DotStyle, ParamKind, ParamSpec, Params,
    Session,
};
use serde_json::Value;

fn always(_: &Session) -> bool {
    true
}

fn report(prefs: &DisplayPrefs) -> Result<Value, CommandError> {
    serde_json::to_value(prefs).map_err(|e| CommandError::BadParams(e.to_string()))
}

/// Set one switch to `on`, or flip it when `on` is left out.
fn flip(
    session: &mut Session,
    params: &Params,
    field: fn(&mut DisplayPrefs) -> &mut bool,
) -> Result<Value, CommandError> {
    let mut prefs = *session.prefs();
    let f = field(&mut prefs);
    *f = params.boolean("on").unwrap_or(!*f);
    session.set_prefs(prefs);
    report(session.prefs())
}

/// Set one switch to `on`, or just report it when `on` is left out.
fn set_or_report(
    session: &mut Session,
    params: &Params,
    field: fn(&mut DisplayPrefs) -> &mut bool,
) -> Result<Value, CommandError> {
    if let Some(on) = params.boolean("on") {
        let mut prefs = *session.prefs();
        *field(&mut prefs) = on;
        session.set_prefs(prefs);
    }
    report(session.prefs())
}

const TOGGLE_PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "on",
    ParamKind::Bool,
    false,
    "true shows it, false hides it; omit to flip it",
)];

const OPTION_PARAMS: &[ParamSpec] = &[ParamSpec::new(
    "on",
    ParamKind::Bool,
    false,
    "true or false; omit to just report the current setting",
)];

fn dots(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    flip(s, p, |x| &mut x.show_dots)
}
fn lines(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    flip(s, p, |x| &mut x.show_lines)
}
fn triangles(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    flip(s, p, |x| &mut x.show_triangles)
}
fn track(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    flip(s, p, |x| &mut x.show_track)
}
fn grid(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    flip(s, p, |x| &mut x.show_grid)
}
fn auto_partner(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    set_or_report(s, p, |x| &mut x.auto_partner)
}
fn auto_jump(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    set_or_report(s, p, |x| &mut x.auto_jump)
}
fn smart_drag(s: &mut Session, p: &Params) -> Result<Value, CommandError> {
    set_or_report(s, p, |x| &mut x.smart_drag)
}

const MAGNIFIER_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "on",
        ParamKind::Bool,
        false,
        "true shows the loupe, false hides it; omit (with no power) to flip it",
    ),
    ParamSpec::new("power", ParamKind::Integer, false, "Magnification, 1 to 4").range(1.0, 4.0),
];

fn magnifier(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut prefs = *session.prefs();
    match (params.boolean("on"), params.integer("power")) {
        (None, None) => prefs.magnifier = !prefs.magnifier,
        (on, power) => {
            if let Some(on) = on {
                prefs.magnifier = on;
            }
            if let Some(power) = power {
                prefs.magnifier_power = u8::try_from(power).unwrap_or(2);
            }
        }
    }
    session.set_prefs(prefs);
    report(session.prefs())
}

const STYLE_PARAMS: &[ParamSpec] =
    &[
        ParamSpec::new("style", ParamKind::String, true, "How dots are drawn")
            .one_of(&["disc", "ring", "cross", "square", "diamond"]),
    ];

fn dot_style(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut prefs = *session.prefs();
    prefs.dot_style = match params.string("style").unwrap_or("disc") {
        "ring" => DotStyle::Ring,
        "cross" => DotStyle::Cross,
        "square" => DotStyle::Square,
        "diamond" => DotStyle::Diamond,
        _ => DotStyle::Disc,
    };
    session.set_prefs(prefs);
    report(session.prefs())
}

const COLOR_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "mode",
        ParamKind::String,
        false,
        "single uses one colour; cycle8 steps through eight colours by dot number",
    )
    .one_of(&["single", "cycle8"]),
    ParamSpec::new(
        "color",
        ParamKind::String,
        false,
        "The single colour as #RRGGBB",
    ),
    ParamSpec::new(
        "opacity",
        ParamKind::Integer,
        false,
        "Dot opacity, 64 to 255",
    )
    .range(64.0, 255.0),
];

fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('#')?;
    (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

fn dot_color(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let mut prefs = *session.prefs();
    if let Some(mode) = params.string("mode") {
        prefs.dot_color_mode = if mode == "single" {
            DotColorMode::Single
        } else {
            DotColorMode::Cycle8
        };
    }
    if let Some(text) = params.string("color") {
        prefs.dot_color = parse_hex(text).ok_or_else(|| {
            CommandError::BadParams(format!("`color` must look like #RRGGBB, not `{text}`"))
        })?;
    }
    if let Some(o) = params.integer("opacity") {
        prefs.dot_opacity = u8::try_from(o).unwrap_or(255);
    }
    session.set_prefs(prefs);
    report(session.prefs())
}

const fn spec(
    id: &'static str,
    label: &'static str,
    menu: &'static str,
    shortcut: Option<&'static str>,
    description: &'static str,
    params: &'static [ParamSpec],
    run: fn(&mut Session, &Params) -> Result<Value, CommandError>,
) -> CommandSpec {
    CommandSpec {
        id,
        label,
        menu,
        shortcut,
        description,
        params,
        enabled: always,
        disabled_reason: "",
        edits: false,
        run,
    }
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        spec(
            "view.toggle.dots",
            "Show Dots",
            "View/Dots",
            Some("Ctrl+D"),
            "Show or hide the key dots on both images.",
            TOGGLE_PARAMS,
            dots,
        ),
        spec(
            "view.toggle.lines",
            "Show Lines",
            "View/Lines",
            Some("Ctrl+L"),
            "Show or hide the lines between dots.",
            TOGGLE_PARAMS,
            lines,
        ),
        spec(
            "view.toggle.triangles",
            "Show Triangles",
            "View/Triangles",
            Some("Ctrl+T"),
            "Show or hide the triangle mesh the morph is built from.",
            TOGGLE_PARAMS,
            triangles,
        ),
        spec(
            "view.toggle.track",
            "Show Track",
            "View/Dot Tracks",
            Some("Ctrl+M"),
            "Show or hide the path each dot travels between the images.",
            TOGGLE_PARAMS,
            track,
        ),
        spec(
            "view.toggle.grid",
            "Show Grid",
            "View/Grid",
            Some("Ctrl+G"),
            "Show or hide a pixel grid over the images.",
            TOGGLE_PARAMS,
            grid,
        ),
        spec(
            "view.magnifier",
            "Magnifier",
            "View/Magnifier",
            None,
            "Show a loupe around the cursor, and set its power (1 to 4).",
            MAGNIFIER_PARAMS,
            magnifier,
        ),
        spec(
            "prefs.dotStyle",
            "Dot Style",
            "",
            None,
            "Choose how dots are drawn.",
            STYLE_PARAMS,
            dot_style,
        ),
        spec(
            "prefs.dotColor",
            "Dot Color",
            "",
            None,
            "Choose dot colouring (one colour, or eight in a cycle) and opacity.",
            COLOR_PARAMS,
            dot_color,
        ),
        spec(
            "dots.option.autoPartner",
            "Auto-Create Partner Dot",
            "Dots/Auto Partner",
            None,
            "When on, adding a dot also places its partner on the other image; when off, dots are added per image and stay unpaired until matched.",
            OPTION_PARAMS,
            auto_partner,
        ),
        spec(
            "dots.option.autoJump",
            "Jump To Partner",
            "Dots/Auto Jump",
            None,
            "When on, the cursor jumps to the partner position after a dot is added.",
            OPTION_PARAMS,
            auto_jump,
        ),
        spec(
            "face.option.smartDrag",
            "Smart Drag Face Dots",
            "Face/Smart Drag",
            None,
            "When on, dragging a dot on a placed face pulls the nearby face dots along (face.locate.smartDrag); hold Alt while dragging for a plain move. When off, Alt does the opposite.",
            OPTION_PARAMS,
            smart_drag,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use crate::{Registry, Session};
    use serde_json::json;

    #[test]
    fn toggles_flip_set_and_report() {
        let r = Registry::standard();
        let mut s = Session::new();
        assert!(!s.prefs().show_grid);
        r.run(&mut s, "view.toggle.grid", &json!({})).unwrap();
        assert!(s.prefs().show_grid);
        r.run(&mut s, "view.toggle.grid", &json!({"on": true}))
            .unwrap();
        assert!(s.prefs().show_grid);
        r.run(&mut s, "view.toggle.dots", &json!({"on": false}))
            .unwrap();
        assert!(!s.prefs().show_dots);
        let v = r
            .run(&mut s, "dots.option.autoPartner", &json!({}))
            .unwrap();
        assert_eq!(v["auto_partner"], true);
        r.run(&mut s, "dots.option.autoPartner", &json!({"on": false}))
            .unwrap();
        assert!(!s.prefs().auto_partner);
        r.run(&mut s, "dots.option.autoJump", &json!({"on": true}))
            .unwrap();
        assert!(s.prefs().auto_jump);
        // Preferences are not project edits: nothing to undo.
        assert!(!s.can_undo());
    }

    #[test]
    fn style_colour_and_loupe() {
        let r = Registry::standard();
        let mut s = Session::new();
        r.run(&mut s, "prefs.dotStyle", &json!({"style": "diamond"}))
            .unwrap();
        r.run(
            &mut s,
            "prefs.dotColor",
            &json!({"mode": "single", "color": "#102030", "opacity": 100}),
        )
        .unwrap();
        assert_eq!(
            (s.prefs().dot_color, s.prefs().dot_opacity),
            (0x102030, 100)
        );
        for bad in [
            json!({"color": "red"}),
            json!({"color": "#12345"}),
            json!({"opacity": 10}),
        ] {
            assert!(r.run(&mut s, "prefs.dotColor", &bad).is_err());
        }
        assert!(r
            .run(&mut s, "prefs.dotStyle", &json!({"style": "blob"}))
            .is_err());
        r.run(&mut s, "view.magnifier", &json!({"power": 3}))
            .unwrap();
        assert_eq!(s.prefs().magnifier_power, 3);
        assert!(!s.prefs().magnifier);
        r.run(&mut s, "view.magnifier", &json!({})).unwrap();
        assert!(s.prefs().magnifier);
        assert!(r
            .run(&mut s, "view.magnifier", &json!({"power": 9}))
            .is_err());
    }
}
