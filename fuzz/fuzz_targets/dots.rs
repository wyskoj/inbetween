#![no_main]

use inbetween_format::{dots_from_bytes, dots_to_string};
use libfuzzer_sys::fuzz_target;

// Reading must never panic, and anything that reads must write and read back equal.
fuzz_target!(|data: &[u8]| {
    if let Ok(dots) = dots_from_bytes(data) {
        let text = dots_to_string(&dots).expect("parsed dots must serialise");
        let again = dots_from_bytes(text.as_bytes()).expect("serialised text must parse");
        assert_eq!(again, dots);
    }
});
