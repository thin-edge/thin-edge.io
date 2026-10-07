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
    closed_by_bridge: bool,
}

impl BridgeConnectionLog {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            last_err: None,
            closed_by_bridge: false,
        }
    }

    /// Records that the bridge is closing this connection itself
    ///
    /// The error the event loop then reports describes a connection the bridge chose to
    /// drop, so it is not a failure to connect and the reason has already been logged
    pub(crate) fn closing_connection(&mut self) {
        self.closed_by_bridge = true;
    }

    pub fn update(&mut self, result: &NotificationRes) {
        let name = self.name;
        let err = match result {
            Ok(event) => {
                if let Event::Incoming(Incoming::ConnAck(_)) = event {
                    // A connection is live again, so any close the bridge asked for is done
                    self.closed_by_bridge = false;
                    log_event!(name, "MQTT bridge connected to {name} broker");
                }
                None
            }
            Err(err) if std::mem::take(&mut self.closed_by_bridge) => {
                log_event!(debug: name, "Closed the connection to {name} broker: {err}");
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

#[cfg(test)]
mod tests {
    use super::*;
    use rumqttc::ConnAck;
    use rumqttc::ConnectReturnCode;
    use tedge_test_utils::tracing::TracingCapture;
    use tracing::Level;

    #[test]
    fn a_connection_the_bridge_closes_itself_is_not_reported_as_a_failure() {
        let mut connection_log = BridgeConnectionLog::new("cloud");

        let (capture, _guard) = TracingCapture::default().start();
        connection_log.closing_connection();
        connection_log.update(&Err(ConnectionError::NetworkTimeout));

        assert!(
            capture.at(Level::ERROR).is_empty(),
            "a connection the bridge dropped on purpose must not be reported as a failure, \
             got: {:?}",
            capture.events()
        );
        assert!(
            capture
                .at(Level::DEBUG)
                .iter()
                .any(|message| message.contains("Closed the connection to cloud broker")),
            "the connection the bridge dropped should still be recorded, got: {:?}",
            capture.events()
        );
    }

    #[test]
    fn a_later_failure_is_still_reported_after_the_bridge_closes_a_connection() {
        let mut connection_log = BridgeConnectionLog::new("cloud");

        let (capture, _guard) = TracingCapture::default().start();
        connection_log.closing_connection();
        connection_log.update(&Err(ConnectionError::NetworkTimeout));
        connection_log.update(&Err(ConnectionError::NetworkTimeout));

        assert!(
            capture
                .at(Level::ERROR)
                .iter()
                .any(|message| message.contains("failed to connect")),
            "the connection the bridge did not ask to close should be reported, got: {:?}",
            capture.events()
        );
    }

    #[test]
    fn a_failure_is_reported_once_the_bridge_is_connected_again() {
        let mut connection_log = BridgeConnectionLog::new("cloud");

        let (capture, _guard) = TracingCapture::default().start();
        connection_log.closing_connection();
        connection_log.update(&Ok(Event::Incoming(Incoming::ConnAck(ConnAck {
            session_present: false,
            code: ConnectReturnCode::Success,
        }))));
        connection_log.update(&Err(ConnectionError::NetworkTimeout));

        assert!(
            capture
                .at(Level::ERROR)
                .iter()
                .any(|message| message.contains("failed to connect")),
            "a connection that replaced the one the bridge closed should be reported when it \
             fails, got: {:?}",
            capture.events()
        );
    }
}
