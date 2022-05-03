#![no_main]
use libfuzzer_sys::fuzz_target;

use c8y_translator::json;

fuzz_target!(|data: &[u8]| {
    let _ = json::from_thin_edge_json(std::str::from_utf8(data).unwrap());
});
