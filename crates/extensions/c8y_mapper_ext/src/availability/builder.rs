use crate::actor::PublishMessage;
use crate::availability::actor::AvailabilityActor;
use crate::availability::AvailabilityConfig;
use crate::availability::AvailabilityInput;
use crate::availability::AvailabilityOutput;
use crate::availability::TimerComplete;
use crate::availability::TimerStart;
use std::convert::Infallible;
use tedge_actors::Builder;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::RuntimeRequest;
use tedge_actors::RuntimeRequestSink;
use tedge_actors::Service;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::entity_store::EntityRegistrationMessage;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::HealthStatus;
use tedge_mqtt_ext::MqttMessage;

pub struct AvailabilityBuilder {
    config: AvailabilityConfig,
    box_builder: SimpleMessageBoxBuilder<AvailabilityInput, AvailabilityOutput>,
    timer_sender: DynSender<TimerStart>,
}

impl AvailabilityBuilder {
    pub fn new(
        config: AvailabilityConfig,
        mqtt: &mut (impl MessageSource<MqttMessage, Vec<ChannelFilter>> + MessageSink<PublishMessage>),
        timer: &mut impl Service<TimerStart, TimerComplete>,
    ) -> Self {
        let mut box_builder: SimpleMessageBoxBuilder<AvailabilityInput, AvailabilityOutput> =
            SimpleMessageBoxBuilder::new("AvailabilityMonitoring", 16);

        box_builder.connect_mapped_source(
            Self::channels(),
            mqtt,
            Self::mqtt_message_parser(config.clone()),
        );

        mqtt.connect_mapped_source(NoConfig, &mut box_builder, Self::mqtt_message_builder());

        let timer_sender = timer.connect_client(box_builder.get_sender().sender_clone());

        Self {
            config: config.clone(),
            box_builder,
            timer_sender,
        }
    }

    pub(crate) fn channels() -> Vec<ChannelFilter> {
        vec![ChannelFilter::EntityMetadata, ChannelFilter::Health]
    }

    fn mqtt_message_parser(
        config: AvailabilityConfig,
    ) -> impl Fn(MqttMessage) -> Option<AvailabilityInput> {
        move |message| {
            if let Ok((source, channel)) = config.mqtt_schema.entity_channel_of(&message.topic) {
                match channel {
                    Channel::EntityMetadata => {
                        if let Ok(registration_message) =
                            EntityRegistrationMessage::try_from(&message)
                        {
                            return Some(registration_message.into());
                        }
                    }
                    Channel::Health => {
                        let health_status: HealthStatus =
                            serde_json::from_slice(message.payload()).unwrap_or_default();
                        return Some((source, health_status).into());
                    }
                    _ => {}
                }
            }
            None
        }
    }

    fn mqtt_message_builder() -> impl Fn(AvailabilityOutput) -> Option<PublishMessage> {
        move |res| match res {
            AvailabilityOutput::C8ySmartRestSetInterval117(value) => {
                Some(PublishMessage(value.into()))
            }
            AvailabilityOutput::C8yJsonInventoryUpdate(value) => Some(PublishMessage(value.into())),
        }
    }
}

impl RuntimeRequestSink for AvailabilityBuilder {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

impl Builder<AvailabilityActor> for AvailabilityBuilder {
    type Error = Infallible;

    fn try_build(self) -> Result<AvailabilityActor, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> AvailabilityActor {
        let timer_sender =
            LoggingSender::new("AvailabilityActor => Timer".into(), self.timer_sender);
        let message_box = self.box_builder.build();

        AvailabilityActor::new(self.config, message_box, timer_sender)
    }
}

#[cfg(test)]
mod tests {
    //! The tests for the builders are more like integration tests in that we want to spawn the mapper with more or less
    //! production configuration but only tweak the configuration of this actor to ensure correctness, so it's put here
    //! to have access to private items.

    use super::*;

    use std::time::Duration;

    use serde_json::json;
    use tedge_actors::test_helpers::FakeServerBoxBuilder;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Actor;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_config::system_services::set_log_level;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use tracing::debug;
    use tracing::info;

    const TEST_TIMEOUT_MS: Duration = Duration::from_secs(2);

    /// Ensure that the `AvailabilityActor` doesn't enter into a deadlock with C8yMapperActor (#3279).
    ///
    /// If many messages are sent, we should never reach a state where both AvailabilityActor can't complete sending its
    /// output and the actor sending registration messages to AvailabilityActor can't complete sending it its input
    /// (because the channel is full).
    #[tokio::test(flavor = "multi_thread")]
    async fn no_deadlock_with_c8y_actor() {
        // number of messages to send to trigger a deadlock, might depend on the hardware; for small values the test
        // should pass everytime, for bigger values it can trigger a deadlock and fail if actors aren't connected
        // correctly
        const NUM_MESSAGES: u32 = 30;

        std::env::set_var("RUST_LOG", "debug,tedge_api=warn");
        set_log_level(tracing::Level::DEBUG);

        // spawn the C8yMapperActor, because we're testing the interaction with it
        let ttd = TempTedgeDir::new();
        let c8y_actor_config = crate::tests::test_mapper_config(&ttd);
        let mut test_builder = crate::tests::c8y_mapper_test_builder(c8y_actor_config);

        let config = AvailabilityConfig {
            main_device_id: "test-device".into(),
            mqtt_schema: MqttSchema::default(),
            c8y_prefix: "c8y".try_into().unwrap(),
            enable: true,
            interval: Duration::from_secs(10 * 60),
        };

        // spawn the AvailabilityActor with very small channel buffers so few messages can be queued and most has to be processed immediately
        let mut box_builder: SimpleMessageBoxBuilder<AvailabilityInput, AvailabilityOutput> =
            SimpleMessageBoxBuilder::new("AvailabilityMonitoring", 16);

        // marcel: here we're essentially copying the builder's new method, which currently connects to c8y actor
        // builder. this sucks because when we decide to change connections between actors, this call site will have to
        // be updated as well for the test to keep working; an optimal way to handle this would be to basically spawn
        // the actors how we would in a non-test environment, already connected, and then be able to change things in a
        // given actor, or replace certain actors we're interested in with stubs, with the ability to preserve the
        // connection to their peers
        box_builder.connect_mapped_source(
            AvailabilityBuilder::channels(),
            &mut test_builder.c8y,
            AvailabilityBuilder::mqtt_message_parser(config.clone()),
        );

        test_builder.c8y.connect_mapped_source(
            NoConfig,
            &mut box_builder,
            AvailabilityBuilder::mqtt_message_builder(),
        );

        // don't have to use the same timer as c8y actor
        let mut timer_builder: FakeServerBoxBuilder<TimerStart, TimerComplete> =
            FakeServerBoxBuilder::default();

        let timer_sender = timer_builder.connect_client(box_builder.get_sender().sender_clone());

        let availability_builder = AvailabilityBuilder {
            config: config.clone(),
            box_builder,
            timer_sender,
        };
        let mut timer_server = timer_builder.build();

        let availability_actor = availability_builder.build();
        tokio::spawn(async move { availability_actor.run().await });

        let test_handle =
            crate::tests::spawn_c8y_mapper_actor_with_builder(test_builder, &ttd, true).await;

        // assert that when registering a lot of child devices, all of them are processed and the deadlock doesn't happen
        let mut mqtt = test_handle.mqtt.with_timeout(TEST_TIMEOUT_MS);

        // skip all the mapper init messages: twin, 114, 117 for the main device, etc.
        mqtt.skip(6).await;

        // skip timer for main device
        timer_server.recv().await.unwrap();

        info!("Sending single messages");

        // send some messages and wait for response immediately to make sure the base functionality works and it's
        // indeed a deadlock that causes the test to fail
        for i in 0..NUM_MESSAGES {
            let id = format!("te/device/child_correct{i}//");
            let expected_xid = format!("test-device:device:child_correct{i}");
            // registration message
            let registration_message = MqttMessage::new(
                &Topic::new_unchecked(&id),
                json!({"@id": expected_xid, "@type": "child-device"}).to_string(),
            );
            // this will fail if channels become full and stop being drained due to a deadlock
            mqtt.send(registration_message).await.unwrap();

            let expected_topic = format!("c8y/s/us/test-device:device:child_correct{i}");

            // skip 101 message for the child device
            mqtt.skip(1).await;

            // SmartREST 117 for the child device
            assert_received_contains_str(&mut mqtt, [(expected_topic.as_str(), "117,10")]).await;

            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(2), timer_server.try_recv())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();
        }

        info!("Sending multiple messages");

        // now send multiple messages in bulk to try to trigger a deadlock
        for i in 0..NUM_MESSAGES {
            let id = format!("te/device/child{i}//");
            let expected_xid = format!("test-device:device:child{i}");
            // registration message
            let registration_message = MqttMessage::new(
                &Topic::new_unchecked(&id),
                json!({"@id": expected_xid, "@type": "child-device"}).to_string(),
            );
            // this will fail if channels become full and stop being drained due to a deadlock
            mqtt.send(registration_message)
                .await
                .expect("this channel should be drained by the connected MQTT message sink");

            // drain timer actor channel
            debug!("draining for: {i}");
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(2), timer_server.try_recv())
                    .await
                    .unwrap()
                    .unwrap()
                    .unwrap();

            // it's terrible
            if (i + 1) % 5 == 0 {
                for j in 0..5 {
                    let id = i - 4 + j;
                    let expected_topic = format!("c8y/s/us/test-device:device:child{id}");

                    // skip 101 message for the child device
                    mqtt.skip(1).await;

                    // SmartREST 117 for the child device
                    assert_received_contains_str(&mut mqtt, [(expected_topic.as_str(), "117,10")])
                        .await;
                }
            }
        }
    }
}
