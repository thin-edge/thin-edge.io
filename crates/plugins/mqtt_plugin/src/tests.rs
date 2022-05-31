use crate::*;
use std::time::Duration;
use tedge_actors::*;

#[tokio::test]
async fn it_works() -> Result<(), anyhow::Error> {
    // Given an MQTT broker
    let broker = mqtt_tests::test_mqtt_broker();

    let input_topic = "actor/input";
    let output_topic = "actor/output";
    let mut output = broker.messages_published_on(output_topic).await;

    // Create actor instances
    let mut main_actor = instance::<UppercaseConverter>(output_topic.to_string());
    let mut mqtt_actor = instance::<MqttConnection>(MqttConfig {
        session_name: "test-mqtt-plugin".to_string(),
        port: broker.port,
        subscriptions: vec![input_topic.to_string()],
    });

    // Connect the actors: `main_actor <=> mqtt_actor`
    main_actor.set_recipient(mqtt_actor.address().into());
    mqtt_actor.set_recipient(main_actor.address().into());

    // One can then run the actors
    let mut runtime = Runtime::try_new().expect("Fail to create the runtime");
    runtime.run(main_actor).await?;
    runtime.run(mqtt_actor).await?;

    // Give some time for the actor to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Any messages published on the input topic ...
    broker.publish(input_topic, "msg 1").await?;
    broker.publish(input_topic, "msg 2").await?;
    broker.publish(input_topic, "msg 3").await?;

    // ... should then be published uppercase on the output topic
    mqtt_tests::assert_received(
        &mut output,
        Duration::from_millis(1000),
        vec!["MSG 1", "MSG 2", "MSG 3"],
    )
    .await;

    Ok(())
}

/// An actor that converts string MQTT messages to uppercase
struct UppercaseConverter {
    output_topic: String,
}

#[async_trait]
impl Actor for UppercaseConverter {
    type Config = String;
    type Input = MqttMessage;
    type Output = MqttMessage;

    fn try_new(output_topic: Self::Config) -> Result<Self, RuntimeError> {
        Ok(UppercaseConverter { output_topic })
    }

    async fn start(
        &mut self,
        _runtime: RuntimeHandler,
        _output: Recipient<Self::Output>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }

    async fn react(
        &mut self,
        message: MqttMessage,
        _runtime: &mut RuntimeHandler,
        output: &mut Recipient<MqttMessage>,
    ) -> Result<(), RuntimeError> {
        let response = MqttMessage {
            topic: self.output_topic.clone(),
            payload: message.payload.to_uppercase(),
        };
        output.send_message(response).await
    }
}
