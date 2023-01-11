use tedge_mqtt_ext::MqttMessage;
use tokio::sync::mpsc::{Receiver, Sender};

pub struct LogManagerActor {
    mqtt_sender_address: Sender<MqttMessage>,
    mqtt_receiver_address: Receiver<MqttMessage>,
}

impl LogManagerActor {
    pub fn new(
        mqtt_sender_address: Sender<MqttMessage>,
        mqtt_receiver_address: Receiver<MqttMessage>,
    ) -> Self {
        LogManagerActor {
            mqtt_sender_address,
            mqtt_receiver_address,
        }
    }
}
