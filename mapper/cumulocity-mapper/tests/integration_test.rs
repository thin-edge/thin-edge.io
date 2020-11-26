use tokio_test::{block_on,assert_ok};
use futures::select;
use futures::FutureExt;
mod common;

#[test]
fn it_maps_openjson_to_smartrest() {
    let scenario = async {
        common::subscribe("c8y", "c8y/s/us").await;
        common::publish_message("app", "tedge/measurements", b"{\"temperature\": 23}").await;

        let msg = common::expect_message("c8y", "c8y/s/us").await;
        msg == Some("211,23".into())
    };

    block_on(async {
        select! {
           bg_status = common::launch_mapper().fuse() => assert_ok!(bg_status),
           outcome = scenario.fuse() => assert!(outcome),
        }
    })
}
