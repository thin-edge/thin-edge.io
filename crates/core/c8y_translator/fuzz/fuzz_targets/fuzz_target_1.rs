#![no_main]
use libfuzzer_sys::fuzz_target;

use c8y_translator::CumulocityJson;

fuzz_target!(|data: &[u8]| {
    let _ = CumulocityJson::from_thin_edge_json(data);
});
