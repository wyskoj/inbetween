//! Calls every registered command with adversarial parameters, in sessions from empty to
//! deliberately broken, and requires that nothing panics and nothing is `Internal`.
//!
//! This file holds a single test on purpose: it changes the working directory so that
//! hostile relative output paths land in a scratch folder.

use inbetween_doc::{
    DotPair, DotSet, ImageRef, MorphType, OutputSettings, Project, Sequence, MAX_FRAMES,
};
use inbetween_engine::{CommandError, CommandSpec, ParamKind, Registry, Session};
use inbetween_geom::Point;
use inbetween_testkit::demo;
use serde_json::{json, Map, Value};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

struct Fixture {
    root: PathBuf,
    project: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("inbetween-hunt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        // Work two levels down so `..` and `../..` stay inside the scratch tree.
        let work = root.join("a").join("b");
        std::fs::create_dir_all(&work).unwrap();
        std::env::set_current_dir(&work).unwrap();

        let (sun, moon) = demo::pair(24, 18).unwrap();
        for (name, img) in [("sun.png", sun), ("moon.png", moon)] {
            let png = inbetween_codecs::encode_png(24, 18, &img.to_rgba8()).unwrap();
            std::fs::write(root.join(name), png).unwrap();
        }
        std::fs::write(
            root.join("garbage.bin"),
            b"\x00\x01 not an image or a project",
        )
        .unwrap();
        let project = root.join("proj.inbetween");
        inbetween_format::write_project(&project, &valid_project()).unwrap();
        let dots = DotSet::new(valid_project().sequences[0].dots.pairs.clone());
        inbetween_format::write_dots(&root.join("set.inbetweendots"), &dots).unwrap();
        Self { root, project }
    }

    fn s(&self, name: &str) -> String {
        self.root.join(name).display().to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(std::env::temp_dir());
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

type Xy = (f64, f64);
type MakeSession = Box<dyn Fn() -> Session>;

fn pairs(list: &[(Xy, Xy)]) -> DotSet {
    DotSet::new(
        list.iter()
            .map(|&((a, b), (c, d))| DotPair::new(Point::new(a, b), Point::new(c, d)))
            .collect(),
    )
}

fn sequence(name: &str, dots: DotSet) -> Sequence {
    Sequence {
        name: name.into(),
        image1: ImageRef::new("sun.png"),
        image2: ImageRef::new("moon.png"),
        dots,
        frames: 4,
        ..Sequence::default()
    }
}

fn valid_project() -> Project {
    let dots = demo::dot_pairs(24, 18);
    Project {
        output: OutputSettings {
            width: 24,
            height: 18,
        },
        sequences: vec![sequence("valid", pairs(&dots))],
        ..Project::default()
    }
}

/// Sequences that break assumptions in every way the model allows (and some it does not,
/// since `Session::set_project` takes any `Project`).
fn broken_project() -> Project {
    let mut seqs = vec![
        sequence(
            "duplicate dots",
            pairs(&[((5.0, 5.0), (6.0, 6.0)), ((5.0, 5.0), (9.0, 9.0))]),
        ),
        sequence(
            "collinear",
            pairs(&[
                ((2.0, 9.0), (12.0, 2.0)),
                ((8.0, 9.0), (12.0, 9.0)),
                ((14.0, 9.0), (12.0, 16.0)),
            ]),
        ),
        sequence(
            "outside",
            pairs(&[((-50.0, -50.0), (90.0, 90.0)), ((1e6, 1e6), (-1e6, 3.0))]),
        ),
        sequence("nan dot", pairs(&[((f64::NAN, 1.0), (2.0, 2.0))])),
        sequence("infinite dot", pairs(&[((f64::INFINITY, 1.0), (2.0, 2.0))])),
        sequence("huge dot", pairs(&[((1e300, 1e300), (2.0, 2.0))])),
        sequence("no dots", DotSet::default()),
        Sequence {
            frames: 0,
            ..sequence("zero frames", DotSet::default())
        },
        Sequence {
            frames: u32::MAX,
            ..sequence("max frames", DotSet::default())
        },
        Sequence {
            frames: MAX_FRAMES,
            ..sequence(
                "limit frames, bad dot",
                pairs(&[((f64::NAN, 0.0), (0.0, 0.0))]),
            )
        },
        Sequence {
            morph: MorphType::Warp1,
            image2: ImageRef::new("missing.png"),
            ..sequence("warp, no image 2", DotSet::default())
        },
        Sequence {
            image1: ImageRef::new(""),
            ..sequence("empty image 1", DotSet::default())
        },
        Sequence {
            image2: ImageRef::new(""),
            ..sequence("empty image 2", DotSet::default())
        },
        Sequence {
            image1: ImageRef::new("garbage.bin"),
            ..sequence("garbage image", DotSet::default())
        },
        Sequence {
            image1: ImageRef::new("a"),
            ..sequence("directory image", DotSet::default())
        },
        Sequence {
            image1: ImageRef::new("../../../../../nowhere.png"),
            ..sequence("escaping image", DotSet::default())
        },
        Sequence {
            image1: ImageRef::new("/etc/passwd"),
            ..sequence("absolute image", DotSet::default())
        },
        Sequence {
            image1: ImageRef::new("a\0b.png"),
            ..sequence("nul image", DotSet::default())
        },
    ];
    seqs.push(sequence("last", pairs(&[((3.0, 3.0), (4.0, 4.0))])));
    Project {
        output: OutputSettings {
            width: 24,
            height: 18,
        },
        sequences: seqs,
        ..Project::default()
    }
}

fn session_variants(fx: &Fixture) -> Vec<(&'static str, MakeSession)> {
    let root = fx.root.clone();
    let proj_path = fx.project.clone();
    let broken = broken_project();
    let broken_path = fx.root.join("broken.inbetween");
    let with_output = |w: u32, h: u32| {
        let mut p = valid_project();
        p.output = OutputSettings {
            width: w,
            height: h,
        };
        p
    };
    let (b1, b2, b3, b4) = (
        broken.clone(),
        with_output(0, 0),
        with_output(u32::MAX, 3),
        with_output(1, 1),
    );
    let pp = proj_path.clone();
    let bp = broken_path.clone();
    let (bp2, bp3, bp4) = (broken_path.clone(), broken_path.clone(), broken_path);
    let _ = root;
    vec![
        ("empty", Box::new(Session::new)),
        (
            "valid",
            Box::new(move || {
                let mut s = Session::new();
                s.set_project(valid_project(), Some(pp.clone()));
                s
            }),
        ),
        (
            "unsaved valid",
            Box::new(|| {
                let mut s = Session::new();
                s.set_project(valid_project(), None);
                s
            }),
        ),
        (
            "broken sequences",
            Box::new(move || {
                let mut s = Session::new();
                s.set_project(b1.clone(), Some(bp.clone()));
                s
            }),
        ),
        (
            "zero output",
            Box::new(move || {
                let mut s = Session::new();
                s.set_project(b2.clone(), Some(bp2.clone()));
                s
            }),
        ),
        (
            "absurd output",
            Box::new(move || {
                let mut s = Session::new();
                s.set_project(b3.clone(), Some(bp3.clone()));
                s
            }),
        ),
        (
            "one pixel output",
            Box::new(move || {
                let mut s = Session::new();
                s.set_project(b4.clone(), Some(bp4.clone()));
                s
            }),
        ),
    ]
}

fn nested(depth: usize) -> Value {
    (0..depth).fold(json!(1), |inner, _| json!([inner]))
}

/// Hostile values of every JSON type, including paths into the scratch folder.
fn hostile_values(fx: &Fixture) -> Vec<Value> {
    vec![
        Value::Null,
        json!(true),
        json!(false),
        json!(0),
        json!(1),
        json!(-1),
        json!(2),
        json!(0.5),
        json!(1.0),
        json!(1e308),
        json!(-1e308),
        json!(1e-320),
        json!(u64::MAX),
        json!(i64::MIN),
        json!(MAX_FRAMES),
        json!(MAX_FRAMES + 1),
        json!(""),
        json!(" "),
        json!("\0"),
        json!("a".repeat(10_000)),
        json!("é🙂"),
        json!("NaN"),
        json!("-1"),
        json!("."),
        json!(".."),
        json!("../.."),
        json!("/"),
        json!("C:\\"),
        json!("CON"),
        json!(fx.root.display().to_string()),
        json!(fx.s("missing/dir/x.png")),
        json!(fx.s("garbage.bin")),
        json!(fx.s("sun.png")),
        json!(fx.s("proj.inbetween")),
        json!(fx.s("set.inbetweendots")),
        json!([]),
        json!([1, 2, 3]),
        json!({}),
        json!({"a": 1}),
        nested(200),
    ]
}

/// Whether a value could make a command write somewhere it should not in this test.
fn is_dangerous_output(v: &Value) -> bool {
    match v.as_str() {
        Some(s) => {
            matches!(s, "/" | "C:\\" | "CON")
                || s.ends_with("sun.png")
                || s.ends_with("proj.inbetween")
                || s.ends_with("set.inbetweendots")
                || s.ends_with("garbage.bin")
        }
        None => false,
    }
}

fn is_output(spec: &CommandSpec, name: &str) -> bool {
    name == "out"
        || name == "out_dir"
        || (name == "path" && matches!(spec.id, "project.save" | "dots.export"))
}

fn baseline(fx: &Fixture, spec: &CommandSpec, name: &str, seq: usize) -> Value {
    match name {
        "path" => match spec.id {
            "project.open" => json!(fx.project.display().to_string()),
            "dots.import" => json!(fx.s("set.inbetweendots-copy")),
            _ => json!(fx.s("written.out")),
        },
        "out" => json!(fx.s("one.png")),
        "out_dir" => json!(fx.s("frames")),
        "sequence" => json!(seq),
        "frames" => json!(3),
        "t" => json!(0.5),
        "frame" => json!(1),
        "prefix" => json!("p"),
        "first" => json!(1),
        "last" => json!(2),
        _ => json!(1),
    }
}

fn describe(spec: &CommandSpec, variant: &str, params: &Value) -> String {
    let text = params.to_string();
    let short: String = text.chars().take(160).collect();
    format!("{} in `{variant}` session with {short}", spec.id)
}

#[test]
fn every_command_survives_adversarial_parameters() {
    let fx = Fixture::new();
    // dots.import needs a readable copy; keep the original safe from `dots.export`.
    std::fs::copy(
        fx.root.join("set.inbetweendots"),
        fx.root.join("set.inbetweendots-copy"),
    )
    .unwrap();
    let registry = Registry::standard();
    let values = hostile_values(&fx);
    let variants = session_variants(&fx);
    let mut failures: Vec<String> = Vec::new();
    let mut runs = 0usize;
    let mut oks = 0usize;

    let mut attempt =
        |spec: &CommandSpec, variant: &str, make: &dyn Fn() -> Session, params: Value| {
            runs += 1;
            let mut session = make();
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                registry.run(&mut session, spec.id, &params)
            }));
            match outcome {
                Err(_) => failures.push(format!("PANIC: {}", describe(spec, variant, &params))),
                Ok(Err(CommandError::Internal(m))) => {
                    failures.push(format!(
                        "INTERNAL ({m}): {}",
                        describe(spec, variant, &params)
                    ));
                }
                Ok(Ok(_)) => oks += 1,
                Ok(Err(e)) => {
                    // Errors must be readable: never empty.
                    if e.to_string().trim().is_empty() {
                        failures.push(format!("EMPTY ERROR: {}", describe(spec, variant, &params)));
                    }
                }
            }
        };

    for spec in registry.specs() {
        println!("hunting {}", spec.id);
        for (variant, make) in &variants {
            // How many sequence indexes are worth trying in this session.
            let seq_count = make().project().sequences.len().max(1);

            // Whole-parameter values that are not objects, and the empty cases.
            for v in &values {
                if v.is_object() {
                    continue;
                }
                attempt(spec, variant, make.as_ref(), v.clone());
            }
            attempt(spec, variant, make.as_ref(), json!({}));

            for seq in 0..seq_count.min(20) {
                let mut base = Map::new();
                for p in spec.params {
                    if p.required || p.name == "sequence" {
                        base.insert(p.name.to_string(), baseline(&fx, spec, p.name, seq));
                    }
                }
                // The valid baseline itself, plus an unknown parameter.
                attempt(spec, variant, make.as_ref(), Value::Object(base.clone()));
                let mut unknown = base.clone();
                unknown.insert("bogus".into(), json!(1));
                attempt(spec, variant, make.as_ref(), Value::Object(unknown));

                // The full hostile matrix runs for the first sequence; the others only
                // need their valid baseline, which already exercises their broken content.
                if seq > 0 {
                    continue;
                }

                // Each parameter, one at a time, with every hostile value.
                for p in spec.params {
                    for v in &values {
                        if is_output(spec, p.name)
                            && (is_dangerous_output(v)
                                || v.is_string()
                                    && v.as_str()
                                        .is_some_and(|s| s == "." || s == ".." || s == "../.."))
                        {
                            continue;
                        }
                        let mut params = base.clone();
                        params.insert(p.name.to_string(), v.clone());
                        attempt(spec, variant, make.as_ref(), Value::Object(params));
                    }
                }
                // Every parameter set to the same hostile value at once.
                for v in &values {
                    let writes = spec.params.iter().any(|p| is_output(spec, p.name));
                    if writes
                        && (is_dangerous_output(v)
                            || v.as_str().is_some_and(|s| s.starts_with('.')))
                    {
                        continue;
                    }
                    let params: Map<String, Value> = spec
                        .params
                        .iter()
                        .map(|p| (p.name.to_string(), v.clone()))
                        .collect();
                    attempt(spec, variant, make.as_ref(), Value::Object(params));
                }
            }
        }
    }

    println!(
        "panic hunt: {runs} calls, {oks} succeeded, {} failures",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} problems:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The hunt is only meaningful if it also reached the success paths.
    assert!(
        oks > 50,
        "only {oks} calls succeeded; the baselines are probably wrong"
    );
    assert!(runs > 3_000, "only {runs} calls ran");
    let _ = (Path::new(""), ParamKind::Bool);
}
