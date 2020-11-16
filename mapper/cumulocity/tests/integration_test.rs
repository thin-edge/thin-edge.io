use std::thread;
mod common;

#[test]
fn it_maps_openjson_to_smartrest() {
    thread::spawn(move || {
        let _ = common::launch_mapper();
    });

    common::subscribe("c8y", "c8y/s/us");
    common::publish_message("app", "tedge/measurements", b"{\"temperature\": 23}");

    let msg = common::expect_message("c8y", "c8y/s/us");
    assert_eq!(msg, Some("211,23".into()));
}

