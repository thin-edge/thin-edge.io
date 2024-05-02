use crate::overall_status;
use crate::BidirectionalChannelHalf;
use crate::Status;
use futures::channel::mpsc;
use futures::SinkExt;
use futures::StreamExt;
use rumqttc::AsyncClient;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::Incoming;
use rumqttc::QoS;
use std::collections::HashMap;
use tracing::error;
use tracing::log::info;

/// A tool for monitoring and publishing the health of the two bridge halves
///
/// When [Self::monitor] runs, this will watch the status of the bridge halves, and notify the
/// relevant MQTT topic about the overall health.
pub struct BridgeHealthMonitor<T> {
    target: AsyncClient,
    topic: String,
    rx_status: mpsc::Receiver<(&'static str, Status)>,
    companion_bridge_half: mpsc::Sender<Option<T>>,
}

impl<T> BridgeHealthMonitor<T> {
    pub(crate) fn new(
        target: AsyncClient,
        topic: String,
        bridge_half: &BidirectionalChannelHalf<Option<T>>,
    ) -> (mpsc::Sender<(&'static str, Status)>, Self) {
        let (tx, rx_status) = mpsc::channel(10);
        (
            tx,
            BridgeHealthMonitor {
                target,
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
            let (name, status) = self.rx_status.next().await.unwrap();
            *statuses.entry(name).or_insert(Some(status)) = Some(status);

            let status = statuses.values().fold(Some(Status::Up), overall_status);
            if last_status != status {
                last_status = status;
                self.target
                    .publish(&self.topic, QoS::AtLeastOnce, true, status.unwrap().json())
                    .await
                    .unwrap();
                // Send a note that a message has been published to maintain synchronisation
                // between the two bridge halves
                self.companion_bridge_half.send(None).await.unwrap();
            }
        }
    }
}

type NotificationRes = Result<Event, ConnectionError>;

/// A client for [BridgeHealthMonitor]
///
/// This is used by each bridge half to log and notify the monitor of health status updates
pub struct BridgeHealth {
    name: &'static str,
    tx_health: mpsc::Sender<(&'static str, Status)>,
    last_err: Option<String>,
}

impl BridgeHealth {
    pub(crate) fn new(name: &'static str, tx_health: mpsc::Sender<(&'static str, Status)>) -> Self {
        Self {
            name,
            tx_health,
            last_err: Some("dummy error".into()),
        }
    }

    pub async fn update(&mut self, result: &NotificationRes) {
        let name = self.name;
        let err = match result {
            Ok(event) => {
                if let Event::Incoming(Incoming::ConnAck(_)) = event {
                    info!("MQTT bridge connected to {name} broker")
                }
                None
            }
            Err(err) => Some(err.to_string()),
        };

        if self.last_err != err {
            if let Some(err) = &err {
                error!("MQTT bridge failed to connect to {name} broker: {err}")
            }
            self.last_err = err;
            let status = self.last_err.as_ref().map_or(Status::Up, |_| Status::Down);
            self.tx_health.send((name, status)).await.unwrap()
        }
    }
}
