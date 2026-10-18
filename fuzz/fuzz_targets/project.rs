#![no_main]

use inbetween_format::{project_from_bytes, project_to_string};
use libfuzzer_sys::fuzz_target;

// Reading must never panic, and anything that reads must write and read back equal.
fuzz_target!(|data: &[u8]| {
    if let Ok(project) = project_from_bytes(data) {
        let text = project_to_string(&project).expect("a parsed project must serialise");
        let again = project_from_bytes(text.as_bytes()).expect("serialised text must parse");
        assert_eq!(again, project);
    }
});
