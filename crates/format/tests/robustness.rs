//! Deterministic mutation fuzzing of both parsers on stable Rust. It runs in every
//! `cargo test`; the libFuzzer targets in `fuzz/` explore the same property for much longer.
//!
//! Property: no input panics, and anything that parses re-serialises and parses back equal.

use inbetween_doc::{DotPair, DotSet, FitMode, ImageRef, MeshPolicy, MorphType, Project, Sequence};
use inbetween_format::{dots_from_bytes, dots_to_string, project_from_bytes, project_to_string};
use inbetween_geom::Point;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*: tiny, deterministic, good enough to shuffle bytes.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

const TOKENS: &[&str] = &[
    "null",
    "true",
    "false",
    "-1",
    "0",
    "1e999",
    "-1e999",
    "1e-999",
    "4294967296",
    "-0",
    "99999999999999999999",
    "0.1",
    "\"\"",
    "\"/etc/passwd\"",
    "\"C:\\\\x\"",
    "[",
    "]",
    "{",
    "}",
    "[]",
    "{}",
    ",",
    ":",
    "\"version\"",
    "\"format\"",
    "\"frames\"",
    "\"pairs\"",
    "\\u0000",
    "\"\\ud800\"",
    "\u{feff}",
];

fn project_seed() -> String {
    let project = Project {
        sequences: vec![
            Sequence {
                name: "seed".into(),
                image1: ImageRef::new("images/a.png"),
                image2: ImageRef::new("b.png"),
                dots: DotSet::new(vec![
                    DotPair::new(Point::new(10.5, 20.25), Point::new(30.0, 40.0)),
                    DotPair::fixed(Point::new(-5.0, 7.0)),
                ]),
                frames: 12,
                hold_start: 2,
                hold_end: 4,
                morph: MorphType::Warp1,
                fit: FitMode::Contain,
                mesh: MeshPolicy::PerFrame,
            },
            Sequence::default(),
        ],
        ..Project::default()
    };
    project_to_string(&project).unwrap()
}

fn dots_seed() -> String {
    dots_to_string(&DotSet::new(vec![
        DotPair::new(Point::new(1.5, 2.5), Point::new(3.0, 4.0)),
        DotPair::fixed(Point::new(9.0, 9.5)),
    ]))
    .unwrap()
}

fn mutate(rng: &mut Rng, seed: &[u8]) -> Vec<u8> {
    let mut data = seed.to_vec();
    for _ in 0..=rng.below(4) {
        match rng.below(6) {
            0 if !data.is_empty() => {
                let i = rng.below(data.len());
                data[i] ^= 1 << rng.below(8);
            }
            1 if !data.is_empty() => {
                let i = rng.below(data.len());
                data[i] = rng.next() as u8;
            }
            2 if !data.is_empty() => {
                let i = rng.below(data.len());
                let n = rng.below(16).min(data.len() - i);
                data.drain(i..i + n);
            }
            3 => {
                let i = rng.below(data.len() + 1);
                let tok = TOKENS[rng.below(TOKENS.len())];
                data.splice(i..i, tok.bytes());
            }
            4 if !data.is_empty() => {
                let a = rng.below(data.len());
                let b = rng.below(data.len());
                let (a, b) = (a.min(b), a.max(b));
                let chunk = data[a..b].to_vec();
                let at = rng.below(data.len() + 1);
                data.splice(at..at, chunk);
            }
            _ => data.truncate(rng.below(data.len() + 1)),
        }
    }
    data
}

fn check_project(bytes: &[u8]) {
    if let Ok(project) = project_from_bytes(bytes) {
        let text = project_to_string(&project).expect("a parsed project must serialise");
        let again = project_from_bytes(text.as_bytes()).expect("serialised text must parse");
        assert_eq!(again, project);
    }
}

fn check_dots(bytes: &[u8]) {
    if let Ok(dots) = dots_from_bytes(bytes) {
        let text = dots_to_string(&dots).expect("parsed dots must serialise");
        let again = dots_from_bytes(text.as_bytes()).expect("serialised text must parse");
        assert_eq!(again, dots);
    }
}

#[test]
fn project_parser_survives_mutations() {
    let seed = project_seed();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..30_000 {
        check_project(&mutate(&mut rng, seed.as_bytes()));
    }
}

#[test]
fn dots_parser_survives_mutations() {
    let seed = dots_seed();
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for _ in 0..30_000 {
        check_dots(&mutate(&mut rng, seed.as_bytes()));
    }
}

#[test]
fn every_truncation_and_every_single_byte_change_is_handled() {
    for (seed, check) in [
        (project_seed(), check_project as fn(&[u8])),
        (dots_seed(), check_dots as fn(&[u8])),
    ] {
        let bytes = seed.as_bytes();
        for cut in 0..=bytes.len() {
            check(&bytes[..cut]);
        }
        for i in 0..bytes.len() {
            for b in [0u8, b'"', b'{', b'}', b'[', b']', b'-', b'9', 0x80, 0xFF] {
                let mut copy = bytes.to_vec();
                copy[i] = b;
                check(&copy);
            }
        }
    }
}

#[test]
fn random_bytes_and_hostile_shapes_are_handled() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..5_000 {
        let len = rng.below(256);
        let junk: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        check_project(&junk);
        check_dots(&junk);
    }
    for hostile in [
        "[".repeat(100_000),
        "{\"a\":".repeat(100_000),
        format!(
            "{{\"format\":\"inbetween-project\",\"version\":1,\"project\":{}",
            "{\"output\":".repeat(50_000)
        ),
        format!("\"{}\"", "a".repeat(1_000_000)),
        format!(
            "{{\"format\":\"inbetween-dots\",\"version\":1,\"pairs\":[{}]}}",
            "0,".repeat(100_000)
        ),
    ] {
        check_project(hostile.as_bytes());
        check_dots(hostile.as_bytes());
    }
}

#[test]
fn committed_fuzz_seeds_are_valid_files() {
    let project = include_str!("../../../fuzz/seeds/project/demo.inbetween");
    let dots = include_str!("../../../fuzz/seeds/dots/demo.inbetweendots");
    let p = project_from_bytes(project.as_bytes()).expect("project seed must load");
    assert_eq!(p.sequences.len(), 1);
    assert_eq!(p.sequences[0].dots.len(), 1);
    assert_eq!(
        dots_from_bytes(dots.as_bytes())
            .expect("dots seed must load")
            .len(),
        2
    );
}

/// Regression: without serde_json's `float_roundtrip` feature, parsing can be one bit off,
/// so dot positions drifted when a project was saved and loaded.
#[test]
fn floats_survive_a_save_and_load_bit_for_bit() {
    let mut rng = Rng(0xFEED_FACE_CAFE_BEEF);
    let mut pairs = Vec::new();
    while pairs.len() < 20_000 {
        let mut coord = || {
            let v = f64::from_bits(rng.next());
            // Any finite value, plus plenty of "ordinary" magnitudes.
            if v.is_finite() && rng.below(2) == 0 {
                v
            } else {
                (rng.next() >> 11) as f64 / (1u64 << 53) as f64 * 4096.0 - 2048.0
            }
        };
        let (a, b, c, d) = (coord(), coord(), coord(), coord());
        pairs.push(DotPair::new(Point::new(a, b), Point::new(c, d)));
    }
    let set = DotSet::new(pairs);
    let back = dots_from_bytes(dots_to_string(&set).unwrap().as_bytes()).unwrap();
    for (i, (want, got)) in set.pairs.iter().zip(&back.pairs).enumerate() {
        for (w, g) in [
            (want.first.x, got.first.x),
            (want.first.y, got.first.y),
            (want.second.x, got.second.x),
            (want.second.y, got.second.y),
        ] {
            assert_eq!(
                w.to_bits(),
                g.to_bits(),
                "pair {i}: {w:e} came back as {g:e}"
            );
        }
    }
}
