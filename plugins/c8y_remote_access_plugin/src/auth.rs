use std::time::Duration;

use futures::SinkExt;
use futures::Stream;
use futures::StreamExt;
use miette::miette;
use miette::Context;
use miette::IntoDiagnostic;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;

use crate::config::TedgeMqttConfig;
use crate::csv::deserialize_csv_record;

pub struct Jwt(String);

impl Jwt {
    pub fn authorization_header(&self) -> String {
        format!("Bearer {}", self.0)
    }

    pub async fn retrieve(config: &TedgeMqttConfig) -> miette::Result<Jwt> {
        let mqtt_config = mqtt_channel::Config::new(
            config.bind_address.as_deref().unwrap_or("localhost"),
            config.port.unwrap_or(1883),
        )
        .with_subscriptions(TopicFilter::new_unchecked("c8y/s/dat"));

        let mut connection = tokio::time::timeout(
            Duration::from_secs(5),
            mqtt_channel::Connection::new(&mqtt_config),
        )
        .await
        .into_diagnostic()
        .context("Connecting to mosquito")?
        .into_diagnostic()?;

        connection
            .published
            .send(Message::new(&Topic::new_unchecked("c8y/s/uat"), ""))
            .await
            .into_diagnostic()
            .context("Connecting to mosquitto")?;

        let mut possible_jwt = None;
        if let Some(message) = wait_for_response(&mut connection.received).await? {
            let (_smartrest_id, jwt): (i32, String) =
                deserialize_csv_record(std::str::from_utf8(&message.payload).unwrap()).unwrap();
            possible_jwt = Some(Jwt(jwt))
        }

        possible_jwt.ok_or_else(|| miette!("JWT not retrieved"))
    }
}

async fn wait_for_response(
    messages: &mut (impl Stream<Item = Message> + Unpin),
) -> miette::Result<Option<Message>> {
    let duration = Duration::from_secs(10);
    tokio::time::timeout(duration, messages.next())
        .await
        .into_diagnostic()
        .with_context(|| format!("Waiting {duration:?} for Cumulocity to respond with JWT"))
}
