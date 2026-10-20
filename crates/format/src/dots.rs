use crate::{check_header, decode_text, parse_error, FormatError, MAX_FILE_BYTES};
use inbetween_doc::{DocError, DotPair, DotSet, Line};
use inbetween_geom::MAX_DOTS;
use serde::{Deserialize, Serialize};

/// Marker written into every dot-set file.
pub const DOTS_FORMAT: &str = "inbetween-dots";
/// The newest dot-set file version this build reads and the one it writes.
pub const DOTS_VERSION: u32 = 1;

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    version: u32,
    pairs: &'a [DotPair],
    #[serde(skip_serializing_if = "<[Line]>::is_empty")]
    lines: &'a [Line],
}

#[derive(Deserialize)]
struct EnvelopeIn {
    #[serde(default)]
    pairs: Vec<DotPair>,
    #[serde(default)]
    lines: Vec<Line>,
}

fn validate(set: &DotSet) -> Result<(), FormatError> {
    if set.len() > MAX_DOTS {
        return Err(DocError::TooManyDots {
            count: set.len(),
            max: MAX_DOTS,
        }
        .into());
    }
    if let Some(i) = set
        .pairs
        .iter()
        .position(|p| !(p.first.is_finite() && p.second.is_finite()))
    {
        return Err(DocError::NonFiniteDot(i).into());
    }
    set.validate_lines()?;
    Ok(())
}

/// Serialise a dot set to pretty JSON.
pub fn dots_to_string(set: &DotSet) -> Result<String, FormatError> {
    validate(set)?;
    let env = EnvelopeOut {
        format: DOTS_FORMAT,
        version: DOTS_VERSION,
        pairs: &set.pairs,
        lines: &set.lines,
    };
    let mut text = serde_json::to_string_pretty(&env)
        .map_err(|e| FormatError::Parse(format!("could not serialise: {e}")))?;
    text.push('\n');
    Ok(text)
}

/// Parse dot-set text.
pub fn dots_from_str(text: &str) -> Result<DotSet, FormatError> {
    dots_from_bytes(text.as_bytes())
}

/// Parse dot-set bytes (UTF-8, with or without a byte-order mark).
pub fn dots_from_bytes(bytes: &[u8]) -> Result<DotSet, FormatError> {
    let text = decode_text(bytes, MAX_FILE_BYTES)?;
    check_header(text, DOTS_FORMAT, DOTS_VERSION)?;
    let env: EnvelopeIn = serde_json::from_str(text).map_err(parse_error)?;
    let mut set = DotSet::new(env.pairs);
    set.lines = env.lines;
    validate(&set)?;
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_geom::Point;

    fn sample() -> DotSet {
        DotSet::new(vec![
            DotPair::new(Point::new(1.5, 2.5), Point::new(-3.0, 4.0)),
            DotPair::fixed(Point::new(1.0 / 3.0, 1e-9)),
        ])
    }

    #[test]
    fn round_trip_is_exact() {
        let text = dots_to_string(&sample()).unwrap();
        assert!(text.contains("inbetween-dots"));
        assert_eq!(dots_from_str(&text).unwrap(), sample());
        assert_eq!(
            dots_from_str(&dots_to_string(&DotSet::default()).unwrap()).unwrap(),
            DotSet::default()
        );
    }

    #[test]
    fn sides_and_lines_round_trip_and_old_files_are_unchanged() {
        let plain = dots_to_string(&sample()).unwrap();
        assert!(!plain.contains("side") && !plain.contains("lines"));
        let mut s = sample();
        s.pairs[0] = DotPair::unpaired(Point::new(3.0, 4.0), inbetween_doc::Side::Only2);
        assert!(s.connect(0, 1));
        let text = dots_to_string(&s).unwrap();
        assert_eq!(dots_from_str(&text).unwrap(), s);
        let bad = r#"{"format":"inbetween-dots","version":1,"pairs":[{}],"lines":[{"a":0,"b":7}]}"#;
        assert!(dots_from_str(bad).is_err());
    }

    #[test]
    fn tolerant_reading() {
        let d = dots_from_str(r#"{"format":"inbetween-dots","version":1}"#).unwrap();
        assert!(d.is_empty());
        let d = dots_from_str(
            r#"{"format":"inbetween-dots","version":1,"note":"hi","pairs":[{"first":{"x":1,"y":2},"extra":9}]}"#,
        )
        .unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d.pairs[0].first, Point::new(1.0, 2.0));
        assert_eq!(d.pairs[0].second, Point::default());
    }

    #[test]
    fn rejects_other_files_and_bad_input() {
        for bad in [
            "",
            "{}",
            "[]",
            r#"{"format":"inbetween-project","version":1}"#,
            r#"{"format":"inbetween-dots","version":2}"#,
            r#"{"format":"inbetween-dots","version":0}"#,
            r#"{"format":"inbetween-dots","version":1,"pairs":{}}"#,
            r#"{"format":"inbetween-dots","version":1,"pairs":[{"first":{"x":"a","y":1}}]}"#,
            r#"{"format":"inbetween-dots","version":1,"pairs":[{"first":{"x":1e999,"y":1}}]}"#,
        ] {
            assert!(dots_from_str(bad).is_err(), "accepted: {bad}");
        }
        assert!(dots_from_bytes(&[0xC0, 0xAF]).is_err());
    }

    #[test]
    fn dot_count_limit() {
        let pair = r#"{"first":{"x":1,"y":1},"second":{"x":2,"y":2}}"#;
        let text = format!(
            r#"{{"format":"inbetween-dots","version":1,"pairs":[{}]}}"#,
            vec![pair; MAX_DOTS + 1].join(",")
        );
        assert!(matches!(dots_from_str(&text), Err(FormatError::Invalid(_))));
    }

    #[test]
    fn non_finite_dots_are_not_written() {
        let bad = DotSet::new(vec![DotPair::fixed(Point::new(f64::NAN, 0.0))]);
        assert!(matches!(
            dots_to_string(&bad),
            Err(FormatError::Invalid(DocError::NonFiniteDot(0)))
        ));
    }
}
