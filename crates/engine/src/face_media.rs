//! Face commands that read and write image files: `face.extract`, `face.mix`, `face.library`
//! and `image.saveToLibrary`.

use crate::face_cmds::{
    check_distinct, explicit_box, face_points, morph_error, BOX_PARAMS, FIRST, SEQUENCE,
};
use crate::{CommandError, CommandSpec, ParamKind, ParamSpec, Params, Session};
use inbetween_codecs::Rgba8Image;
use inbetween_doc::{DotPair, DotSet, FitMode, ImageRef, MorphType, Sequence};
use inbetween_format::MixRecipe;
use inbetween_geom::Point;
use inbetween_morph::face::{self, FaceBox};
use inbetween_morph::{fit_transform, transparent_outside};
use inbetween_raster::Image;
use inbetween_render_cpu::Renderer;
use rayon::prelude::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Most candidates one `face.mix` call writes.
const MAX_CANDIDATES: i64 = 64;
const EDGE_EPS: f64 = 1e-6;

fn write_png(path: &Path, image: &Image) -> Result<(), CommandError> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| CommandError::Io(format!("{}: {e}", dir.display())))?;
    }
    let rgba = Rgba8Image::new(image.width(), image.height(), image.to_rgba8())?;
    inbetween_codecs::write_file(path, &rgba)?;
    Ok(())
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

fn path_text(path: &Path) -> String {
    absolute(path).display().to_string()
}

// -------------------------------------------------------------- face.extract

const EXTRACT_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "out_dir",
        ParamKind::Path,
        true,
        "Folder for the framed PNGs (created if missing)",
    )
    .folder(),
    BOX_PARAMS[0],
    BOX_PARAMS[1],
    BOX_PARAMS[2],
    BOX_PARAMS[3],
    FIRST,
    ParamSpec::new(
        "all_sequences",
        ParamKind::Bool,
        false,
        "Frame the faces of every sequence instead of just one",
    ),
    SEQUENCE,
];

/// Re-frame one image so the face fitted to its dots lands on `template` (a box in output
/// pixels). Returns the new image and the map from old to new output positions.
fn reframe(
    source: &Image,
    fit: FitMode,
    output: (u32, u32),
    src_box: &FaceBox,
    template: &FaceBox,
) -> Result<Image, CommandError> {
    let a = fit_transform(source.width(), source.height(), output.0, output.1, fit)
        .map_err(morph_error)?;
    let clip = transparent_outside(fit);
    let (sw, sh) = (f64::from(source.width()), f64::from(source.height()));
    let (kx, ky) = (src_box.w / template.w, src_box.h / template.h);
    let mut out = Image::new(output.0, output.1)?;
    let row_len = (output.0 as usize).saturating_mul(4).max(4);
    out.as_mut_slice()
        .par_chunks_mut(row_len)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, px) in row.chunks_exact_mut(4).enumerate() {
                let p = Point::new(x as f64 + 0.5, y as f64 + 0.5);
                let q = Point::new(
                    src_box.x + (p.x - template.x) * kx,
                    src_box.y + (p.y - template.y) * ky,
                );
                let s = a.apply(q);
                let value = if clip
                    && (s.x < -EDGE_EPS
                        || s.y < -EDGE_EPS
                        || s.x > sw + EDGE_EPS
                        || s.y > sh + EDGE_EPS)
                {
                    [0.0; 4]
                } else {
                    source.sample_bilinear(s.x as f32, s.y as f32)
                };
                px.copy_from_slice(&value);
            }
        });
    Ok(out)
}

fn extract_sequence(
    session: &mut Session,
    index: usize,
    out_dir: &Path,
    template: Option<FaceBox>,
    first: usize,
) -> Result<usize, CommandError> {
    let model = session.project().face.clone();
    let count = model.active_count();
    let output = session.project().output;
    let template = match template {
        Some(t) => t,
        None => FaceBox::centered(output.width, output.height).map_err(morph_error)?,
    };
    // No face placed here yet: find one in each image and place the model on it.
    if crate::face_detect::AVAILABLE && session.sequence(index)?.dots.len() == first {
        let [one, two] =
            crate::face_detect::runs(session, index, None, crate::face_detect::Pick::One(0))?;
        if let (Some(a), Some(b)) = (one.first(), two.first()) {
            let target = session.sequence_mut(index)?;
            for (p, q) in a.iter().zip(b) {
                let mut pair = DotPair::fixed(*p);
                pair.set_end(2, *q);
                target.dots.pairs.push(pair);
            }
        }
    }
    let seq = session.sequence(index)?.clone();
    let mut done = 0;
    for which in [1u8, 2] {
        let reference = if which == 1 { &seq.image1 } else { &seq.image2 };
        if reference.0.is_empty() {
            continue;
        }
        let source = session.source(index, which)?;
        let points = face_points(&seq, first, count, i64::from(which))?;
        let src_box = face::fit_box(&model, &points).map_err(morph_error)?;
        let framed = reframe(
            &source,
            seq.fit,
            (output.width, output.height),
            &src_box,
            &template,
        )?;
        let path = out_dir.join(format!("sequence{}_image{which}.png", index + 1));
        write_png(&path, &framed)?;
        // Dots follow the picture: old position q now sits at the template-relative spot.
        let (kx, ky) = (template.w / src_box.w, template.h / src_box.h);
        let moved = |q: Point| {
            Point::new(
                template.x + (q.x - src_box.x) * kx,
                template.y + (q.y - src_box.y) * ky,
            )
        };
        let target = session.sequence_mut(index)?;
        for pair in &mut target.dots.pairs {
            if pair.has_end(which) {
                pair.set_end(which, moved(pair.end(which)));
            }
        }
        let image_ref = ImageRef::new(path_text(&path));
        if which == 1 {
            target.image1 = image_ref;
        } else {
            target.image2 = image_ref;
        }
        done += 1;
    }
    let target = session.sequence_mut(index)?;
    // The framed pictures are output-sized, so fitting is the identity from here on.
    target.fit = FitMode::Stretch;
    target.validate()?;
    check_distinct(target)?;
    Ok(done)
}

fn extract(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let targets: Vec<usize> = if params.boolean("all_sequences").unwrap_or(false) {
        (0..session.project().sequences.len()).collect()
    } else {
        vec![session.sequence_index(params)?]
    };
    let template = explicit_box(params)?;
    let first = params
        .integer("first")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let out_dir = PathBuf::from(params.string("out_dir").unwrap_or_default());
    let mut images = 0;
    for &index in &targets {
        images += extract_sequence(session, index, &out_dir, template, first)?;
    }
    Ok(json!({
        "sequences": targets,
        "images": images,
        "out_dir": path_text(&out_dir),
    }))
}

// ------------------------------------------------------------------ face.mix

const MIX_MODES: &[&str] = &["custom", "average", "random"];

const MIX_PARAMS: &[ParamSpec] = &[
    ParamSpec::new(
        "out",
        ParamKind::Path,
        true,
        "PNG file to write (with several candidates: NAME-001.png, NAME-002.png, ...)",
    )
    .save_file(&["png"]),
    ParamSpec::new(
        "sequences",
        ParamKind::String,
        false,
        "Which sequences' faces to mix, as numbers separated by commas, for example \"0,1,3\"",
    )
    .default_is("all of them"),
    ParamSpec::new(
        "image",
        ParamKind::Integer,
        false,
        "Mix each sequence's Image 1 or Image 2",
    )
    .range(1.0, 2.0)
    .default_is("1"),
    ParamSpec::new(
        "mode",
        ParamKind::String,
        false,
        "custom: use `features` and `shapes`; average: every face equally; random: seeded random percentages",
    )
    .one_of(MIX_MODES)
    .default_is("custom (average when no percentages are given)"),
    ParamSpec::new(
        "features",
        ParamKind::String,
        false,
        "Feature (colour and texture) percentage of each face, comma separated, for example \"50,30,20\"; they are scaled to total 100%",
    ),
    ParamSpec::new(
        "shapes",
        ParamKind::String,
        false,
        "Shape percentage of each face, comma separated; follows `features` when left out",
    ),
    ParamSpec::new(
        "lock",
        ParamKind::Bool,
        false,
        "Shape follows feature: the shape percentages are the feature percentages",
    ),
    ParamSpec::new("seed", ParamKind::Integer, false, "Seed for random mode")
        .range(0.0, 9.0e15)
        .default_is("0"),
    ParamSpec::new(
        "count",
        ParamKind::Integer,
        false,
        "How many candidates to write (random mode only; candidate k uses seed + k)",
    )
    .range(1.0, MAX_CANDIDATES as f64)
    .default_is("1"),
    FIRST,
    ParamSpec::new(
        "recipe_load",
        ParamKind::Path,
        false,
        "A .inbetweenmix recipe to use instead of mode, features, shapes, lock and seed",
    )
    .open_file(&["inbetweenmix"]),
    ParamSpec::new(
        "recipe_save",
        ParamKind::Path,
        false,
        "Write the recipe of this (single) mix to a .inbetweenmix file",
    )
    .save_file(&["inbetweenmix"]),
];

fn percentages(text: &str, what: &str) -> Result<Vec<f64>, CommandError> {
    text.split([',', ' ', ';'])
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| CommandError::BadParams(format!("`{what}` has a bad number: `{s}`")))
        })
        .collect()
}

fn sequence_list(session: &Session, text: Option<&str>) -> Result<Vec<usize>, CommandError> {
    let count = session.project().sequences.len();
    let list = match text {
        None => (0..count).collect(),
        Some(t) => t
            .split([',', ' ', ';'])
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|_| CommandError::BadParams(format!("`{s}` is not a sequence number")))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    if list.is_empty() {
        return Err(CommandError::BadParams("there is nothing to mix".into()));
    }
    if let Some(&bad) = list.iter().find(|i| **i >= count) {
        return Err(CommandError::NoSuchSequence { index: bad, count });
    }
    Ok(list)
}

/// The recipe that applies to candidate `k`: percentages (summing to 1) for features and shapes.
fn weights_for(
    recipe: &MixRecipe,
    inputs: usize,
    k: u64,
) -> Result<(Vec<f64>, Vec<f64>), CommandError> {
    let seed = recipe.seed.wrapping_add(k);
    let (features, shapes) = match recipe.mode.as_str() {
        "average" => (face::average_weights(inputs), face::average_weights(inputs)),
        "random" => {
            let features = face::random_weights(inputs, seed);
            let shapes = if recipe.lock {
                features.clone()
            } else {
                face::random_weights(inputs, seed ^ 0xD1B5_4A32_D192_ED03)
            };
            (features, shapes)
        }
        _ => {
            let features = if recipe.features.is_empty() {
                face::average_weights(inputs)
            } else {
                face::normalize(&recipe.features).map_err(morph_error)?
            };
            let shapes = if recipe.lock || recipe.shapes.is_empty() {
                features.clone()
            } else {
                face::normalize(&recipe.shapes).map_err(morph_error)?
            };
            (features, shapes)
        }
    };
    if features.len() != inputs || shapes.len() != inputs {
        return Err(CommandError::BadParams(format!(
            "{inputs} faces are mixed but {} feature and {} shape percentages were given",
            features.len(),
            shapes.len()
        )));
    }
    Ok((features, shapes))
}

fn candidate_path(out: &Path, k: i64, count: i64) -> PathBuf {
    if count <= 1 {
        return out.to_path_buf();
    }
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("mix");
    let ext = out.extension().and_then(|s| s.to_str()).unwrap_or("png");
    out.with_file_name(format!("{stem}-{:03}.{ext}", k + 1))
}

fn mix(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let list = sequence_list(session, params.string("sequences"))?;
    let which = params.integer("image").unwrap_or(1);
    let first = params
        .integer("first")
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let count = params.integer("count").unwrap_or(1);
    let model = session.project().face.clone();
    let dots_per_face = model.active_count();
    let output = session.project().output;
    let output = (output.width, output.height);

    let mut recipe = match params.string("recipe_load") {
        Some(path) => inbetween_format::read_mix_recipe(Path::new(path))?,
        None => MixRecipe {
            mode: params.string("mode").unwrap_or("custom").to_string(),
            features: params
                .string("features")
                .map(|t| percentages(t, "features"))
                .transpose()?
                .unwrap_or_default(),
            shapes: params
                .string("shapes")
                .map(|t| percentages(t, "shapes"))
                .transpose()?
                .unwrap_or_default(),
            seed: params
                .integer("seed")
                .and_then(|s| u64::try_from(s).ok())
                .unwrap_or(0),
            lock: params.boolean("lock").unwrap_or(false),
        },
    };
    if count > 1 && recipe.mode != "random" {
        return Err(CommandError::BadParams(
            "several candidates only differ in random mode; set mode to random".into(),
        ));
    }
    if count > 1 && params.has("recipe_save") {
        return Err(CommandError::BadParams(
            "a recipe describes one mix; save it with count 1".into(),
        ));
    }

    // Load every face once.
    struct Face {
        image: std::sync::Arc<Image>,
        points: Vec<Point>,
        fit: FitMode,
    }
    let mut faces = Vec::with_capacity(list.len());
    for &index in &list {
        let seq = session.sequence(index)?.clone();
        let image = session.source(index, which as u8)?;
        faces.push(Face {
            image,
            points: face_points(&seq, first, dots_per_face, which)?,
            fit: seq.fit,
        });
    }
    let point_sets: Vec<Vec<Point>> = faces.iter().map(|f| f.points.clone()).collect();

    let out = PathBuf::from(params.string("out").unwrap_or_default());
    let mut files = Vec::new();
    let mut used = Vec::new();
    for k in 0..count {
        let (features, shapes) = weights_for(&recipe, faces.len(), k as u64)?;
        let target = face::blend_points(&point_sets, &shapes).map_err(morph_error)?;
        let mut result = Image::new(output.0, output.1)?;
        for (face, weight) in faces.iter().zip(&features) {
            if *weight <= 0.0 {
                continue;
            }
            // Warp this face's picture so its face dots land on the mixed shape.
            let warp = Sequence {
                dots: DotSet::new(
                    face.points
                        .iter()
                        .zip(&target)
                        .map(|(a, b)| DotPair::new(*a, *b))
                        .collect(),
                ),
                morph: MorphType::Warp1,
                fit: face.fit,
                frames: 2,
                ..Sequence::default()
            };
            let warped = Renderer::new(&warp, output, &face.image, None)?.render_t(1.0)?;
            let w = *weight as f32;
            for (r, s) in result.as_mut_slice().iter_mut().zip(warped.as_slice()) {
                *r += s * w;
            }
        }
        let path = candidate_path(&out, k, count);
        write_png(&path, &result)?;
        files.push(path_text(&path));
        used.push(json!({
            "features": features.iter().map(|w| w * 100.0).collect::<Vec<_>>(),
            "shapes": shapes.iter().map(|w| w * 100.0).collect::<Vec<_>>(),
            "seed": recipe.seed.wrapping_add(k as u64),
        }));
    }
    if let Some(path) = params.string("recipe_save") {
        // Save the percentages actually used, so a random mix can be reproduced exactly.
        let (features, shapes) = weights_for(&recipe, faces.len(), 0)?;
        recipe = MixRecipe {
            mode: "custom".into(),
            features: features.iter().map(|w| w * 100.0).collect(),
            shapes: shapes.iter().map(|w| w * 100.0).collect(),
            seed: recipe.seed,
            lock: false,
        };
        inbetween_format::write_mix_recipe(Path::new(path), &recipe)?;
    }
    Ok(json!({ "sequences": list, "files": files, "mixes": used }))
}

// -------------------------------------------------------------- face.library

const LIBRARY_OPS: &[&str] = &["list", "add", "remove", "use"];

const DIR: ParamSpec = ParamSpec::new(
    "dir",
    ParamKind::Path,
    true,
    "The library folder: each entry is NAME.png with NAME.inbetweendots beside it, optionally inside one level of sub-folders",
)
.folder();

const LIBRARY_PARAMS: &[ParamSpec] = &[
    DIR,
    ParamSpec::new(
        "op",
        ParamKind::String,
        false,
        "list (default); add (store a sequence's image and dots); remove; use (put a library image and its dots into a sequence)",
    )
    .one_of(LIBRARY_OPS),
    ParamSpec::new("name", ParamKind::String, false, "Entry name, for add, remove and use"),
    ParamSpec::new(
        "folder",
        ParamKind::String,
        false,
        "Sub-folder of the library (one level) the entry is in",
    ),
    ParamSpec::new(
        "image",
        ParamKind::Integer,
        false,
        "Which image of the sequence: 1 or 2",
    )
    .range(1.0, 2.0)
    .default_is("1"),
    ParamSpec::new(
        "overwrite",
        ParamKind::Bool,
        false,
        "Replace an entry that already exists (add)",
    ),
    SEQUENCE,
];

const SAVE_PARAMS: &[ParamSpec] = &[
    DIR,
    ParamSpec::new("name", ParamKind::String, true, "Entry name"),
    LIBRARY_PARAMS[3],
    LIBRARY_PARAMS[4],
    LIBRARY_PARAMS[5],
    SEQUENCE,
];

/// One path component that cannot climb out of the library.
fn component(text: &str, what: &str) -> Result<String, CommandError> {
    let ok = !text.is_empty()
        && text.len() <= 64
        && !text.starts_with('.')
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'));
    if ok {
        Ok(text.to_string())
    } else {
        Err(CommandError::BadParams(format!(
            "the {what} `{text}` must be 1 to 64 letters, digits, spaces, `-`, `_` or `.`, and not start with a dot"
        )))
    }
}

struct Entry {
    png: PathBuf,
    dots: PathBuf,
}

fn entry_paths(params: &Params) -> Result<Entry, CommandError> {
    let name = component(
        params
            .string("name")
            .ok_or_else(|| CommandError::BadParams("this operation needs `name`".into()))?,
        "name",
    )?;
    let mut dir = PathBuf::from(params.string("dir").unwrap_or_default());
    if let Some(folder) = params.string("folder") {
        dir.push(component(folder, "folder")?);
    }
    Ok(Entry {
        png: dir.join(format!("{name}.png")),
        dots: dir.join(format!("{name}.inbetweendots")),
    })
}

fn add_to_library(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let which = params.integer("image").unwrap_or(1);
    let entry = entry_paths(params)?;
    if !params.boolean("overwrite").unwrap_or(false) && (entry.png.exists() || entry.dots.exists())
    {
        return Err(CommandError::BadParams(format!(
            "{} already exists; pass overwrite to replace it",
            entry.png.display()
        )));
    }
    let seq = session.sequence(index)?.clone();
    let image = session.source(index, which as u8)?;
    let output = session.project().output;
    let fit = fit_transform(
        image.width(),
        image.height(),
        output.width,
        output.height,
        seq.fit,
    )
    .map_err(morph_error)?;
    // Dots are stored in the picture's own pixels so they fit it in any project.
    let dots = DotSet::new(
        seq.dots
            .pairs
            .iter()
            .filter(|p| p.has_end(if which == 2 { 2 } else { 1 }))
            .map(|p| DotPair::fixed(fit.apply(p.end(if which == 2 { 2 } else { 1 }))))
            .collect(),
    );
    write_png(&entry.png, &image)?;
    inbetween_format::write_dots(&entry.dots, &dots)?;
    Ok(json!({
        "saved": path_text(&entry.png),
        "dots": dots.len(),
    }))
}

fn use_from_library(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    let index = session.sequence_index(params)?;
    let which = params.integer("image").unwrap_or(1);
    let entry = entry_paths(params)?;
    let dots = inbetween_format::read_dots(&entry.dots)?;
    let reference = path_text(&entry.png);
    let image = session.load_image(&reference, which as u8, index)?;
    let output = session.project().output;
    let seq = session.sequence_mut(index)?;
    let fit = fit_transform(
        image.width(),
        image.height(),
        output.width,
        output.height,
        seq.fit,
    )
    .map_err(morph_error)?;
    let back = fit
        .inverse()
        .map_err(|_| CommandError::BadParams("the image cannot be fitted to the frame".into()))?;
    let placed: Vec<Point> = dots.pairs.iter().map(|p| back.apply(p.first)).collect();
    let image_ref = ImageRef::new(reference);
    if which == 2 {
        seq.image2 = image_ref;
    } else {
        seq.image1 = image_ref;
    }
    if seq.dots.len() == placed.len() {
        for (pair, p) in seq.dots.pairs.iter_mut().zip(&placed) {
            pair.set_end(if which == 2 { 2 } else { 1 }, *p);
        }
    } else {
        seq.dots.pairs = placed.iter().map(|p| DotPair::fixed(*p)).collect();
        seq.dots.lines.clear();
    }
    seq.validate()?;
    check_distinct(seq)?;
    Ok(json!({ "sequence": index, "image": which, "dots": seq.dots.len() }))
}

fn remove_from_library(params: &Params) -> Result<Value, CommandError> {
    let entry = entry_paths(params)?;
    if !entry.png.is_file() {
        return Err(CommandError::BadParams(format!(
            "there is no library image {}",
            entry.png.display()
        )));
    }
    std::fs::remove_file(&entry.png)
        .map_err(|e| CommandError::Io(format!("{}: {e}", entry.png.display())))?;
    // The dots file may be missing in a hand-edited library; that is fine.
    let _ = std::fs::remove_file(&entry.dots);
    Ok(json!({ "removed": path_text(&entry.png) }))
}

fn list_library(params: &Params) -> Result<Value, CommandError> {
    let root = PathBuf::from(params.string("dir").unwrap_or_default());
    let mut entries = Vec::new();
    let mut scan = |dir: &Path, folder: &str| {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for item in read.flatten() {
            let path = item.path();
            if path.extension().and_then(|e| e.to_str()) != Some("png") || !path.is_file() {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let dots = inbetween_format::read_dots(&path.with_extension("inbetweendots"))
                .map(|d| d.len())
                .ok();
            entries.push((folder.to_string(), name.to_string(), dots));
        }
    };
    scan(&root, "");
    if let Ok(read) = std::fs::read_dir(&root) {
        for item in read.flatten() {
            if item.path().is_dir() {
                if let Some(folder) = item.file_name().to_str() {
                    scan(&item.path(), folder);
                }
            }
        }
    }
    entries.sort();
    Ok(json!({
        "dir": path_text(&root),
        "entries": entries.iter().map(|(folder, name, dots)| json!({
            "folder": folder, "name": name, "dots": dots,
        })).collect::<Vec<_>>(),
    }))
}

fn library(session: &mut Session, params: &Params) -> Result<Value, CommandError> {
    match params.string("op").unwrap_or("list") {
        "list" => list_library(params),
        "add" => add_to_library(session, params),
        "remove" => remove_from_library(params),
        "use" => use_from_library(session, params),
        other => Err(CommandError::BadParams(format!("unknown op `{other}`"))),
    }
}

fn has_sequence(session: &Session) -> bool {
    session.has_sequences()
}

pub(crate) fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "face.extract",
            label: "Extract Faces",
            menu: "Face/Extract Faces...",
            shortcut: None,
            description: "Frame the faces consistently: fit the face model to each image's face dots, then crop and scale each image so its face fills the same template box (given, or a centred default). Writes output-sized PNGs, points the sequence at them and moves its dots along. Run face.locate first.",
            params: EXTRACT_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no sequence to frame",
            edits: true,
            run: extract,
        },
        CommandSpec {
            id: "face.mix",
            label: "Mix Faces",
            menu: "Face/Mix Faces...",
            shortcut: None,
            description: "Blend several faces into one still: every face is warped to a weighted average shape (the shape percentages) and the warped pictures are blended (the feature percentages). Custom, average or seeded random percentages, a lock that makes shape follow feature, a candidate gallery, and a saved recipe to reproduce a mix.",
            params: MIX_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no faces to mix",
            edits: false,
            run: mix,
        },
        CommandSpec {
            id: "face.library",
            label: "Face Library",
            menu: "Face/Face Library...",
            shortcut: None,
            description: "A folder of images with their dots: list, add, remove, or use an entry in a sequence (its image and dots). Dots are stored in the picture's own pixels so they fit it in any project.",
            params: LIBRARY_PARAMS,
            enabled: |_| true,
            disabled_reason: "",
            edits: true,
            run: library,
        },
        CommandSpec {
            id: "image.saveToLibrary",
            label: "Save Image to Library",
            menu: "Face/Save to Library...",
            shortcut: None,
            description: "Store a sequence's image and its dots in the face library (the add operation of face.library).",
            params: SAVE_PARAMS,
            enabled: has_sequence,
            disabled_reason: "the project has no image to save",
            edits: false,
            run: add_to_library,
        },
    ]
}
