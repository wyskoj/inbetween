//! Face-tool files, each JSON in the usual envelope:
//!
//! - `inbetween-face-model` (`.inbetweenface`): a face dot model.
//! - `inbetween-landmarks` (`.inbetweenlandmarks`): landmark positions in a model's order, the
//!   hand-over point for any detector that can write `{"points": [[x, y], ...]}`.
//! - `inbetween-mix` (`.inbetweenmix`): the recipe of a face mix, so it can be reproduced.

use crate::{check_header, decode_text, parse_error, FormatError, MAX_FILE_BYTES};
use inbetween_doc::{DocError, FaceModel};
use inbetween_geom::Point;
use serde::{Deserialize, Serialize};

pub const FACE_MODEL_FORMAT: &str = "inbetween-face-model";
pub const LANDMARKS_FORMAT: &str = "inbetween-landmarks";
pub const MIX_FORMAT: &str = "inbetween-mix";
/// The newest version of each of these files this build reads and writes.
pub const FACE_FILE_VERSION: u32 = 1;
/// Most landmark positions a landmarks file may hold.
pub const MAX_LANDMARKS: usize = 4096;
/// Most inputs a mix recipe may name.
pub const MAX_MIX_INPUTS: usize = 64;

#[derive(Serialize)]
struct Out<'a, T: Serialize> {
    format: &'static str,
    version: u32,
    #[serde(flatten)]
    body: &'a T,
}

fn to_text<T: Serialize>(format: &'static str, body: &T) -> Result<String, FormatError> {
    let out = Out {
        format,
        version: FACE_FILE_VERSION,
        body,
    };
    let mut text = serde_json::to_string_pretty(&out)
        .map_err(|e| FormatError::Parse(format!("could not serialise: {e}")))?;
    text.push('\n');
    Ok(text)
}

fn from_bytes<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
    format: &'static str,
) -> Result<T, FormatError> {
    let text = decode_text(bytes, MAX_FILE_BYTES)?;
    check_header(text, format, FACE_FILE_VERSION)?;
    serde_json::from_str(text).map_err(parse_error)
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ModelBody {
    model: FaceModel,
}

/// Serialise a face model.
pub fn face_model_to_string(model: &FaceModel) -> Result<String, FormatError> {
    model.validate()?;
    to_text(
        FACE_MODEL_FORMAT,
        &ModelBody {
            model: model.clone(),
        },
    )
}

/// Parse a face model file.
pub fn face_model_from_bytes(bytes: &[u8]) -> Result<FaceModel, FormatError> {
    let body: ModelBody = from_bytes(bytes, FACE_MODEL_FORMAT)?;
    body.model.validate()?;
    Ok(body.model)
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct LandmarkBody {
    points: Vec<[f64; 2]>,
}

fn check_landmarks(points: &[Point]) -> Result<(), FormatError> {
    if points.len() > MAX_LANDMARKS {
        return Err(DocError::BadFace("too many landmark positions").into());
    }
    if points.iter().any(|p| !p.is_finite()) {
        return Err(DocError::BadFace("a landmark position is not a finite number").into());
    }
    Ok(())
}

/// Serialise landmark positions.
pub fn landmarks_to_string(points: &[Point]) -> Result<String, FormatError> {
    check_landmarks(points)?;
    to_text(
        LANDMARKS_FORMAT,
        &LandmarkBody {
            points: points.iter().map(|p| [p.x, p.y]).collect(),
        },
    )
}

/// Parse a landmarks file.
pub fn landmarks_from_bytes(bytes: &[u8]) -> Result<Vec<Point>, FormatError> {
    let body: LandmarkBody = from_bytes(bytes, LANDMARKS_FORMAT)?;
    let points: Vec<Point> = body.points.iter().map(|p| Point::new(p[0], p[1])).collect();
    check_landmarks(&points)?;
    Ok(points)
}

/// The recipe of a face mix.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MixRecipe {
    /// `custom`, `average` or `random`.
    pub mode: String,
    /// Feature (colour) percentage per input.
    pub features: Vec<f64>,
    /// Shape percentage per input.
    pub shapes: Vec<f64>,
    pub seed: u64,
    /// Shape follows the feature percentages.
    pub lock: bool,
}

impl Default for MixRecipe {
    fn default() -> Self {
        Self {
            mode: "custom".into(),
            features: Vec::new(),
            shapes: Vec::new(),
            seed: 0,
            lock: false,
        }
    }
}

impl MixRecipe {
    pub fn validate(&self) -> Result<(), FormatError> {
        let bad = |why| Err(DocError::BadFace(why).into());
        if !["custom", "average", "random"].contains(&self.mode.as_str()) {
            return bad("a mix recipe's mode must be custom, average or random");
        }
        if self.features.len() > MAX_MIX_INPUTS || self.shapes.len() > MAX_MIX_INPUTS {
            return bad("a mix recipe names too many inputs");
        }
        if self
            .features
            .iter()
            .chain(&self.shapes)
            .any(|w| !w.is_finite() || *w < 0.0 || *w > 1.0e6)
        {
            return bad("mix percentages must be finite, not negative and not absurd");
        }
        Ok(())
    }
}

/// Serialise a mix recipe.
pub fn mix_recipe_to_string(recipe: &MixRecipe) -> Result<String, FormatError> {
    recipe.validate()?;
    to_text(MIX_FORMAT, recipe)
}

/// Parse a mix recipe file.
pub fn mix_recipe_from_bytes(bytes: &[u8]) -> Result<MixRecipe, FormatError> {
    let recipe: MixRecipe = from_bytes(bytes, MIX_FORMAT)?;
    recipe.validate()?;
    Ok(recipe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_model_round_trip_keeps_edits() {
        let mut m = FaceModel::minimal();
        m.name = "mine \u{1F600}".into();
        m.dots[3].x = 1.0 / 3.0;
        m.guides.push([0, 4]);
        let text = face_model_to_string(&m).unwrap();
        assert!(text.contains("inbetween-face-model"));
        assert_eq!(face_model_from_bytes(text.as_bytes()).unwrap(), m);
    }

    #[test]
    fn landmarks_round_trip_exactly() {
        let pts = vec![Point::new(0.1 + 0.2, 1e21), Point::new(-3.0, 1.0 / 3.0)];
        let text = landmarks_to_string(&pts).unwrap();
        assert_eq!(landmarks_from_bytes(text.as_bytes()).unwrap(), pts);
        assert!(landmarks_to_string(&[Point::new(f64::NAN, 0.0)]).is_err());
        assert!(landmarks_to_string(&vec![Point::new(0.0, 0.0); MAX_LANDMARKS + 1]).is_err());
    }

    #[test]
    fn recipe_round_trip_and_validation() {
        let r = MixRecipe {
            mode: "random".into(),
            features: vec![30.0, 70.0],
            shapes: vec![50.0, 50.0],
            seed: u64::MAX,
            lock: true,
        };
        let text = mix_recipe_to_string(&r).unwrap();
        assert_eq!(mix_recipe_from_bytes(text.as_bytes()).unwrap(), r);
        for bad in [
            MixRecipe {
                mode: "weird".into(),
                ..MixRecipe::default()
            },
            MixRecipe {
                features: vec![-1.0],
                ..MixRecipe::default()
            },
            MixRecipe {
                shapes: vec![f64::INFINITY],
                ..MixRecipe::default()
            },
            MixRecipe {
                features: vec![1.0; MAX_MIX_INPUTS + 1],
                ..MixRecipe::default()
            },
        ] {
            assert!(mix_recipe_to_string(&bad).is_err());
        }
    }

    #[test]
    fn wrong_malformed_and_newer_files_are_errors() {
        let model = face_model_to_string(&FaceModel::standard()).unwrap();
        assert!(matches!(
            landmarks_from_bytes(model.as_bytes()),
            Err(FormatError::WrongFormat { .. })
        ));
        assert!(face_model_from_bytes(b"{").is_err());
        assert!(face_model_from_bytes(b"[]").is_err());
        assert!(face_model_from_bytes(&[0xFF, 0xFE]).is_err());
        let newer = model.replace("\"version\": 1", "\"version\": 99");
        assert!(matches!(
            face_model_from_bytes(newer.as_bytes()),
            Err(FormatError::NewerVersion { .. })
        ));
        // A hostile model inside a well-formed envelope is refused.
        let hostile = model.replace("\"x\": 0.04", "\"x\": 1e999");
        assert!(face_model_from_bytes(hostile.as_bytes()).is_err());
        let bad_guide = model.replacen("\"guides\": [", "\"guides\": [[0, 9999],", 1);
        assert!(face_model_from_bytes(bad_guide.as_bytes()).is_err());
        // Missing sections take defaults.
        let minimal = br#"{"format":"inbetween-landmarks","version":1}"#;
        assert!(landmarks_from_bytes(minimal).unwrap().is_empty());
    }
}
