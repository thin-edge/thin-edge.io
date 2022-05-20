use c8y_plugin::C8Y;
use mqtt_plugin::{MqttConfig, MqttConnection};
use tedge_actors::{instance, ActorRuntime};
use thin_edge_json_plugin::ThinEdgeJson;

#[tokio::main]
async fn main() {
    let local_mqtt_port = 1883;
    let measurement_input = "tedge/measurements".to_string();
    let measurement_output = "c8y/measurement/measurements/create".to_string();
    let c8y_mqtt_config = MqttConfig {
        port: local_mqtt_port,
        subscriptions: vec![],
    };
    let thin_edge_json_mqtt_config = MqttConfig {
        port: local_mqtt_port,
        subscriptions: vec![measurement_input],
    };

    // Create actor instances
    let c8y = instance::<C8Y>(&measurement_output).expect("a c8y actor instance");
    let c8y_mqtt_con =
        instance::<MqttConnection>(&c8y_mqtt_config).expect("an mqtt actor to connect to c8y");
    let thin_edge_json =
        instance::<ThinEdgeJson>(&()).expect("an actor translating thin-edge json");
    let thin_edge_json_mqtt_con = instance::<MqttConnection>(&thin_edge_json_mqtt_config)
        .expect("an mqtt actor to connect to the local MQTT bus");

    // Connect the actors
    let thin_edge_json_mqtt_con = thin_edge_json_mqtt_con.with_recipient(thin_edge_json.address());
    let thin_edge_json = thin_edge_json.with_recipient(c8y.address());
    let c8y = c8y.with_recipient(c8y_mqtt_con.address());

    // Run the actors
    let runtime = ActorRuntime::try_new().expect("Fail to create the runtime");

    runtime.run(c8y).await;
    runtime.run(c8y_mqtt_con).await;
    runtime.run(thin_edge_json).await;
    runtime.run(thin_edge_json_mqtt_con).await;

    // FIXME ;-)
    std::thread::sleep(std::time::Duration::from_secs(100));
}
