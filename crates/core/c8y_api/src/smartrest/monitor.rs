use mqtt_channel::{Message, Topic};

pub fn service_monitor_status_down_message(device_name: &str, daemon_name: &str, status: &str, service_type: &str, child_id: Option<String> ) -> Message {
    Message {
        topic: Topic::new_unchecked("c8y/s/us"),
        payload: format!("102,{device_name}_{daemon_name},thin-edge.io,{daemon_name},down")
            .into_bytes(),
        qos: mqtt_channel::QoS::AtLeastOnce,
        retain: true,
    }
}
