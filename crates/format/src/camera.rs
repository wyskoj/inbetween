use crate::{check_header, decode_text, parse_error, FormatError, MAX_FILE_BYTES};
use inbetween_doc::Camera;
use serde::{Deserialize, Serialize};

/// Marker written into every camera file.
pub const CAMERA_FORMAT: &str = "inbetween-camera";
/// The newest camera file version this build reads and the one it writes.
pub const CAMERA_VERSION: u32 = 1;

#[derive(Serialize)]
struct EnvelopeOut<'a> {
    format: &'static str,
    version: u32,
    camera: &'a Camera,
}

#[derive(Deserialize)]
struct EnvelopeIn {
    #[serde(default)]
    camera: Camera,
}

/// Serialise a camera to pretty JSON.
pub fn camera_to_string(camera: &Camera) -> Result<String, FormatError> {
    camera.validate()?;
    let env = EnvelopeOut {
        format: CAMERA_FORMAT,
        version: CAMERA_VERSION,
        camera,
    };
    let mut text = serde_json::to_string_pretty(&env)
        .map_err(|e| FormatError::Parse(format!("could not serialise: {e}")))?;
    text.push('\n');
    Ok(text)
}

/// Parse camera bytes (UTF-8, with or without a byte-order mark).
pub fn camera_from_bytes(bytes: &[u8]) -> Result<Camera, FormatError> {
    let text = decode_text(bytes, MAX_FILE_BYTES)?;
    check_header(text, CAMERA_FORMAT, CAMERA_VERSION)?;
    let env: EnvelopeIn = serde_json::from_str(text).map_err(parse_error)?;
    env.camera.validate()?;
    Ok(env.camera)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inbetween_doc::{CameraAspect, CameraKey};

    fn sample() -> Camera {
        Camera {
            enabled: true,
            keys: vec![
                CameraKey::identity(0.0),
                CameraKey {
                    t: 1.0,
                    x: 0.25,
                    scale_x: -2.0,
                    rotation: 45.0,
                    ..CameraKey::identity(1.0)
                },
            ],
            aspect: CameraAspect::Image1,
            ..Camera::default()
        }
    }

    #[test]
    fn round_trip_is_exact() {
        let text = camera_to_string(&sample()).unwrap();
        assert!(text.contains("inbetween-camera"));
        assert_eq!(camera_from_bytes(text.as_bytes()).unwrap(), sample());
    }

    #[test]
    fn bad_files_are_errors() {
        for bad in [
            "",
            "{}",
            r#"{"format":"inbetween-dots","version":1}"#,
            r#"{"format":"inbetween-camera","version":2}"#,
            r#"{"format":"inbetween-camera","version":1,"camera":{"keys":[{"t":2.0}]}}"#,
            r#"{"format":"inbetween-camera","version":1,"camera":{"keys":[{"t":0.5,"scale_x":0.0}]}}"#,
        ] {
            assert!(
                camera_from_bytes(bad.as_bytes()).is_err(),
                "accepted: {bad}"
            );
        }
        assert!(camera_from_bytes(br#"{"format":"inbetween-camera","version":1}"#).is_ok());
    }
}
