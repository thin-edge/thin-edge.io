use c8y_plugin::C8Y;
use mqtt_plugin::{MqttConfig, MqttConnection};
use tedge_actors::{instance, Runtime};
use thin_edge_json_plugin::ThinEdgeJson;

#[tokio::main]
async fn main() {
    let local_mqtt_port = 1883;
    let measurement_input = "tedge/measurements".to_string();
    let measurement_output = "c8y/measurement/measurements/create".to_string();
    let c8y_mqtt_config = MqttConfig {
        session_name: "tedge-c8y".to_string(),
        port: local_mqtt_port,
        subscriptions: vec![],
    };
    let thin_edge_json_mqtt_config = MqttConfig {
        session_name: "tedge-measurement".to_string(),
        port: local_mqtt_port,
        subscriptions: vec![measurement_input],
    };

    // Create actor instances
    let mut c8y = instance::<C8Y>(measurement_output);
    let c8y_mqtt_con = instance::<MqttConnection>(c8y_mqtt_config);
    let mut thin_edge_json = instance::<ThinEdgeJson>(());
    let mut thin_edge_json_mqtt_con = instance::<MqttConnection>(thin_edge_json_mqtt_config);

    // Connect the actors
    thin_edge_json_mqtt_con.set_recipient(thin_edge_json.address().into());
    thin_edge_json.set_recipient(c8y.address().into());
    c8y.set_recipient(c8y_mqtt_con.address().into());

    // Run the actors
    let mut runtime = Runtime::try_new().expect("Fail to create the runtime");

    runtime.run(c8y).await.expect("a running c8y actor");
    runtime
        .run(c8y_mqtt_con)
        .await
        .expect("a running mqtt actor connected to c8y");
    runtime
        .run(thin_edge_json)
        .await
        .expect("a running actor translating thin-edge json");
    runtime
        .run(thin_edge_json_mqtt_con)
        .await
        .expect("a running mqtt actor connected to the local MQTT bus");

    runtime.run_to_completion().await
}
