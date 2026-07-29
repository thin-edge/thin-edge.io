use crate::overall_status;
use crate::BridgeAsyncClient;
use crate::BridgeMessageSender;
use crate::MqttClient;
use crate::Status;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::Publish;
use rumqttc::QoS;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Publishes the overall health of the two bridge halves
///
/// The bridge is up only when both halves are up, i.e. both can relay the messages they are
/// given. Consumers rely on that: `tedge connect` and the c8y mapper publish cloud-bound
/// messages as soon as they see the bridge up, and on a fresh session such a message is
/// dropped if the bridge has not subscribed yet.
pub struct BridgeHealthMonitor {
    topic: String,
    rx_status: mpsc::Receiver<(&'static str, Status)>,
    companion_bridge_half: BridgeMessageSender,
}

impl BridgeHealthMonitor {
    pub(crate) fn new<Client: MqttClient + 'static>(
        topic: String,
        bridge_half: &BridgeAsyncClient<Client>,
    ) -> (mpsc::Sender<(&'static str, Status)>, Self) {
        let (tx, rx_status) = mpsc::channel(10);
        (
            tx,
            BridgeHealthMonitor {
                topic,
                rx_status,
                companion_bridge_half: bridge_half.clone_sender(),
            },
        )
    }

    pub async fn monitor(mut self) -> ! {
        let mut statuses = HashMap::from([("local", None), ("cloud", None)]);
        let mut last_status = None;
        loop {
            let (name, status) = self.rx_status.recv().await.unwrap();
            *statuses.entry(name).or_insert(Some(status)) = Some(status);

            let status = statuses.values().fold(Some(Status::Up), overall_status);
            if last_status != status {
                last_status = status;

                let mut health_msg =
                    Publish::new(&self.topic, QoS::AtLeastOnce, status.unwrap().json());
                health_msg.retain = true;

                // Publish the health message over MQTT, but with no duplicate for the companion
                // as this message doesn't have to be acknowledged
                self.companion_bridge_half.internal_publish(health_msg);
            }
        }
    }
}

type NotificationRes = Result<Event, ConnectionError>;

/// Logs the connection events of one bridge half
///
/// A failure is logged only when it differs from the previous one, so a broker that keeps
/// refusing connections is reported once rather than on every reconnection attempt
pub struct BridgeConnectionLog {
    name: &'static str,
    last_err: Option<String>,
}

impl BridgeConnectionLog {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            last_err: None,
        }
    }

    pub fn update(&mut self, result: &NotificationRes) {
        let name = self.name;
        let err = match result {
            Ok(event) => {
                if let Event::Incoming(Incoming::ConnAck(_)) = event {
                    log_event!(name, "MQTT bridge connected to {name} broker");
                }
                None
            }
            Err(err) => Some(err.to_string()),
        };

        if self.last_err != err {
            if let Some(err) = &err {
                log_event!(error: name, "MQTT bridge failed to connect to {name} broker: {err}");
            }
            self.last_err = err;
        }
    }
}
