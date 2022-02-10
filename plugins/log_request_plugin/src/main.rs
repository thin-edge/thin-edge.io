mod smartrest;

use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::{smartrest_deserializer::SmartRestLogRequest, topic::C8yTopic};
use tedge_config::{get_tedge_config, ConfigSettingAccessor, MqttPortSetting};

use futures::SinkExt;

use smartrest::{
    get_log_file_request_done_message, get_log_file_request_executing, read_tedge_logs,
};
use std::time::Duration;

use tokio::time::sleep;

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

    // force a pause to allow 4. to run
    let () = do_one_second_pause().await;

    Ok(())
}

/// does a one second pause.
///
/// NOTE: this is a quick-fix to enable step 4. in main to be executed.
/// if `do_one_second_pause()` is not called, step 4. in main
/// does not get triggered.
// see #846
async fn do_one_second_pause() {
    sleep(Duration::from_secs(1)).await;
}

#[cfg(test)]
mod tests {
    use crate::smartrest::read_tedge_logs;
    use c8y_smartrest::smartrest_deserializer::SmartRestLogRequest;
    use std::fs::File;
    use std::io::Write;

    fn parse_file_names_from_log_content(log_content: &str) -> [&str; 5] {
        let mut files: Vec<&str> = vec![];
        for line in log_content.lines() {
            if line.contains("filename: ") {
                let filename: &str = line.split("filename: ").last().unwrap();
                files.push(filename);
            }
        }
        match files.try_into() {
            Ok(arr) => arr,
            Err(_) => panic!("Could not convert to Array &str, size 5"),
        }
    }

    #[test]
    /// testing read_tedge_logs
    ///
    /// this test creates 5 fake log files in a temporary directory.
    /// files are dated 2021-01-0XT01:00Z, where X = a different day.
    ///
    /// this tests will assert that files are read alphanumerically from oldest to newest
    fn test_read_logs() {
        // order in which files are created
        const LOG_FILE_NAMES: [&str; 5] = [
            "software-list-2021-01-03T01:00:00Z.log",
            "software-list-2021-01-02T01:00:00Z.log",
            "software-list-2021-01-01T01:00:00Z.log",
            "software-update-2021-01-03T01:00:00Z.log",
            "software-update-2021-01-02T01:00:00Z.log",
        ];

        // expected (sorted) output
        const EXPECTED_OUTPUT: [&str; 5] = [
            "software-list-2021-01-01T01:00:00Z",
            "software-list-2021-01-02T01:00:00Z",
            "software-list-2021-01-03T01:00:00Z",
            "software-update-2021-01-02T01:00:00Z",
            "software-update-2021-01-03T01:00:00Z",
        ];

        let smartrest_obj = SmartRestLogRequest::from_smartrest(
            "522,DeviceSerial,syslog,2021-01-01T00:00:00+0200,2021-01-10T00:00:00+0200,,1000",
        )
        .unwrap();

        let temp_dir = tempfile::tempdir().unwrap();
        // creating the files
        for (idx, file) in LOG_FILE_NAMES.iter().enumerate() {
            let file_path = &temp_dir.path().join(file);
            let mut file = File::create(file_path).unwrap();
            writeln!(file, "file num {}", idx).unwrap();
        }

        // reading the logs and extracting the file names from the log output.
        let output = read_tedge_logs(&smartrest_obj, temp_dir.path().to_str().unwrap()).unwrap();
        let parsed_values = parse_file_names_from_log_content(&output);

        // asserting the order = `EXPECTED_OUTPUT`
        assert!(parsed_values.eq(&EXPECTED_OUTPUT));
    }
}
