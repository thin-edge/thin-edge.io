mod smartrest;

use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::{smartrest_deserializer::SmartRestLogRequest, topic::C8yTopic};
use tedge_config::{get_tedge_config, ConfigSettingAccessor, MqttPortSetting};

use futures::SinkExt;

use smartrest::{
    get_log_file_request_done_message, get_log_file_request_executing, read_tedge_logs,
};

const AGENT_LOG_DIR: &str = "/var/log/tedge/agent";
const MQTT_SESSION_NAME: &str = "log plugin mqtt session";
const HTTP_SESSION_NAME: &str = "log plugin http session";

/// creates an mqtt client with a given `session_name`
pub async fn create_mqtt_client(
    session_name: &str,
) -> Result<mqtt_channel::Connection, anyhow::Error> {
    let tedge_config = get_tedge_config()?;
    let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
    let mqtt_config = mqtt_channel::Config::default()
        .with_port(mqtt_port)
        .with_session_name(session_name)
        .with_subscriptions(mqtt_channel::TopicFilter::new_unchecked(
            C8yTopic::SmartRestResponse.as_str(),
        ));

    let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
    Ok(mqtt_client)
}

/// creates an http client with a given `session_name`
pub async fn create_http_client(session_name: &str) -> Result<JwtAuthHttpProxy, anyhow::Error> {
    let config = get_tedge_config()?;
    let mut http_proxy = JwtAuthHttpProxy::try_new(&config, session_name).await?;
    let () = http_proxy.init().await?;
    Ok(http_proxy)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // reading payload from command line arguments
    let payload = std::env::args().nth(1).expect("no payload given");

    // creating required clients
    let mut mqtt_client = create_mqtt_client(MQTT_SESSION_NAME).await?;
    let mut http_client = create_http_client(HTTP_SESSION_NAME).await?;

    // retrieve smartrest object from payload
    let smartrest_obj = SmartRestLogRequest::from_smartrest(&payload)?;

    // 1. set log file request to executing
    let msg = get_log_file_request_executing().await?;
    let () = mqtt_client.published.send(msg).await?;
    // 2. read logs
    let log_content = read_tedge_logs(&smartrest_obj, AGENT_LOG_DIR)?;

    // 3. upload log file
    let upload_event_url = http_client.upload_log_binary(&log_content).await?;

    // 4. set log file request to done
    let msg = get_log_file_request_done_message(&upload_event_url).await?;
    let () = mqtt_client.published.send(msg).await?;

    mqtt_client.close().await;

    Ok(())
}
