use std::collections::VecDeque;

use bytes::Bytes;
use rumqttc::AsyncClient;
use rumqttc::ClientError;
use rumqttc::ConnectionError;
use rumqttc::Event;
use rumqttc::EventLoop;
use rumqttc::MqttOptions;
use rumqttc::Outgoing;
use rumqttc::Packet;
use rumqttc::Publish;
use rumqttc::QoS;
use rumqttc::Request;
use rumqttc::SubscribeFilter;
use rumqttc::{self};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::MqttEvents;

/// A wrapper around [AsyncClient] for logging packets with [LoggingEventLoop]
#[derive(Clone, Debug)]
pub struct LoggingAsyncClient {
    inner: AsyncClient,
    log_tx: UnboundedSender<Vec<SubscribeFilter>>,
}

impl LoggingAsyncClient {
    /// Creates a new `LoggingAsyncClient` and `LoggingEventLoop`, setting up the
    /// internal channel for communication and storing the log prefix.
    pub fn new(options: MqttOptions, cap: usize, log_prefix: String) -> (Self, LoggingEventLoop) {
        let (client, eventloop) = AsyncClient::new(options, cap);
        let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel();

        (
            Self {
                inner: client,
                log_tx,
            },
            LoggingEventLoop {
                inner: eventloop,
                log_rx,
                log_prefix,
                has_logged_connect: false,
            },
        )
    }

    pub async fn subscribe<S: Into<String>>(&self, topic: S, qos: QoS) -> Result<(), ClientError> {
        let topic_str = topic.into();
        let subscriptions = vec![SubscribeFilter::new(topic_str.clone(), qos)];

        let _ = self.log_tx.send(subscriptions);
        self.inner.subscribe(topic_str, qos).await
    }

    pub async fn publish(
        &self,
        topic: String,
        qos: QoS,
        retain: bool,
        payload: Bytes,
    ) -> Result<(), ClientError> {
        self.inner.publish(topic, qos, retain, payload).await
    }

    pub async fn ack(&self, publish: &Publish) -> Result<(), ClientError> {
        self.inner.ack(publish).await
    }
}

/// A wrapper around [rumqttc::EventLoop] that logs key MQTT events with a
/// configurable prefix.
pub struct LoggingEventLoop {
    inner: EventLoop,
    log_rx: UnboundedReceiver<Vec<SubscribeFilter>>,
    log_prefix: String,
    has_logged_connect: bool,
}

impl LoggingEventLoop {
    pub async fn poll(&mut self) -> Result<Event, rumqttc::ConnectionError> {
        let prefix = &self.log_prefix;

        // We can't log for sure when we attempt to connect as rumqttc hides
        // this from us
        if !self.has_logged_connect {
            log_event!(prefix, "Attempting to connect to broker");
            self.has_logged_connect = true;
        }

        let event = self.inner.poll().await;

        match &event {
            Ok(Event::Outgoing(Outgoing::Subscribe(pkid))) => {
                // The subscription details should be waiting in the channel.
                match self.log_rx.try_recv() {
                    Ok(topics) => {
                        log_event!(
                            prefix,
                            "Sending SUBSCRIBE packet with PKID: {} for topics: {:?}",
                            pkid,
                            topics
                        );
                    }
                    Err(TryRecvError::Empty) => {
                        log_event!(warn: prefix,
                            "Outgoing::Subscribe event with no pending subscription info. PKID: {}",
                            pkid
                        );
                    }
                    Err(TryRecvError::Disconnected) => {
                        log_event!(warn: prefix,
                            "Logging channel disconnected unexpectedly",
                        );
                    }
                }
            }
            Ok(Event::Incoming(Packet::ConnAck(connack))) => {
                log_event!(
                    prefix,
                    "Received CONNACK (Connection Acknowledged): {:?}",
                    connack
                );
            }
            Ok(Event::Incoming(Packet::SubAck(suback))) => {
                log_event!(
                    prefix,
                    "Received SUBACK (Subscription Acknowledged): {:?}",
                    suback
                );
            }
            Ok(Event::Incoming(Packet::PingReq)) => {
                log_event!(debug: prefix, "Received PINGREQ (Ping Request)");
            }
            Ok(Event::Incoming(Packet::PingResp)) => {
                log_event!(debug: prefix, "Received PINGRESP (Ping Response)");
            }
            Ok(Event::Outgoing(Outgoing::PingReq)) => {
                log_event!(debug: prefix, "Sent PINGREQ (Ping Request)");
            }
            Ok(Event::Outgoing(Outgoing::PingResp)) => {
                log_event!(debug: prefix, "Sent PINGRESP (Ping Response)");
            }
            Err(e) => {
                log_event!(warn: prefix, "Connection error: {:?}", e);
                // We have disconnected, so we should log that we're attempting
                // to reconnect next time we poll
                self.has_logged_connect = false;
            }
            Ok(_) => {}
        }

        event
    }
}

#[async_trait::async_trait]
impl MqttEvents for LoggingEventLoop {
    async fn poll(&mut self) -> Result<Event, ConnectionError> {
        LoggingEventLoop::poll(self).await
    }

    fn take_pending(&mut self) -> VecDeque<Request> {
        std::mem::take(&mut self.inner.pending)
    }

    fn set_pending(&mut self, requests: Vec<Request>) {
        self.inner.pending = requests.into_iter().collect();
    }
}
