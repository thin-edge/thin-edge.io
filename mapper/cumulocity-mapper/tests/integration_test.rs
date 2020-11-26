use tokio_test::{block_on,assert_ok};
use futures::select;
use futures::FutureExt;
mod common;

#[test]
fn it_maps_openjson_to_smartrest() {
    let scenario = async {
        let mut c8y = common::MqttClient::new("c8y");
        let mut app = common::MqttClient::new("app");

        c8y.subscribe("c8y/s/us").await;
        app.publish("tedge/measurements", b"{\"temperature\": 23}").await;

        let msg = c8y.expect_message("c8y/s/us").await;
        msg == Some("211,23".into())
    };

    block_on(async {
        select! {
           bg_status = common::launch_mapper().fuse() => assert_ok!(bg_status),
           outcome = scenario.fuse() => assert!(outcome),
        }
    })
}
