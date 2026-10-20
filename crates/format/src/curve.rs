use crate::{check_header, decode_text, parse_error, FormatError, MAX_FILE_BYTES};
use inbetween_doc::Curve;
use serde::{Deserialize, Serialize};

/// Marker written into every curve library file.
pub const CURVE_FORMAT: &str = "inbetween-curve";
/// The newest curve file version this build reads and the one it writes.
pub const CURVE_VERSION: u32 = 1;

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    version: u32,
    curve: &'a Curve,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    #[serde(default)]
    curve: Curve,
}

/// Serialise a curve to pretty JSON. Any curve a sequence can hold is accepted (a master curve's
/// ends may sit anywhere in 0 to 1).
pub fn curve_to_string(curve: &Curve) -> Result<String, FormatError> {
    curve.validate_free()?;
    let env = EnvelopeOut {
        format: CURVE_FORMAT,
        version: CURVE_VERSION,
        curve,
    };
    let mut text = serde_json::to_string_pretty(&env)
        .map_err(|e| FormatError::Parse(format!("could not serialise: {e}")))?;
    text.push('\n');
    Ok(text)
}

/// Parse curve bytes (UTF-8, with or without a byte-order mark). The curve is checked as far as
/// any curve can be; the caller checks it against the curve it replaces.
pub fn curve_from_bytes(bytes: &[u8]) -> Result<Curve, FormatError> {
    let text = decode_text(bytes, MAX_FILE_BYTES)?;
    check_header(text, CURVE_FORMAT, CURVE_VERSION)?;
    let env: EnvelopeIn = serde_json::from_str(text).map_err(parse_error)?;
    env.curve.validate_free()?;
    Ok(env.curve)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::CurvePoint;

    #[test]
    fn round_trip_is_exact() {
        let c = Curve {
            points: vec![
                CurvePoint::new(0.0, 0.25),
                CurvePoint::new(0.4, 1.0 / 3.0),
                CurvePoint::new(1.0, 1.0),
            ],
            linear: true,
        };
        let text = curve_to_string(&c).unwrap();
        assert!(text.contains("inbetween-curve"));
        assert_eq!(curve_from_bytes(text.as_bytes()).unwrap(), c);
    }

    #[test]
    fn bad_files_are_errors() {
        for bad in [
            "",
            "{}",
            r#"{"format":"inbetween-camera","version":1}"#,
            r#"{"format":"inbetween-curve","version":2}"#,
            r#"{"format":"inbetween-curve","version":1,"curve":{"points":[{"x":0,"y":0}]}}"#,
            r#"{"format":"inbetween-curve","version":1,"curve":{"points":[{"x":0.2,"y":0},{"x":1,"y":1}]}}"#,
            r#"{"format":"inbetween-curve","version":1,"curve":{"points":[{"x":0,"y":0},{"x":1,"y":7}]}}"#,
        ] {
            assert!(curve_from_bytes(bad.as_bytes()).is_err(), "accepted: {bad}");
        }
        assert_eq!(
            curve_from_bytes(br#"{"format":"inbetween-curve","version":1}"#).unwrap(),
            Curve::default()
        );
    }
}
