//! Without the `bundled-models` feature the detector refuses to start instead of failing later.
#![cfg(not(feature = "bundled-models"))]

use inbetween_detect::{DetectError, Detector};

#[test]
fn new_returns_an_error_when_the_models_are_not_built_in() {
    match Detector::new() {
        Err(DetectError::Model(why)) => assert!(why.contains("models"), "{why}"),
        Err(other) => panic!("wrong error: {other}"),
        Ok(_) => panic!("a slim build must not have a detector"),
    }
}
