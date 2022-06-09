use std::path::{Path, PathBuf};

use crate::{
    c8y::converter::CumulocityConverter,
    core::{component::TEdgeComponent, mapper::create_mapper, size_threshold::SizeThreshold},
};

use agent_interface::topic::ResponseTopic;
use async_trait::async_trait;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::operations::Operations;
use mqtt_channel::TopicFilter;
use tedge_config::{
    ConfigSettingAccessor, DeviceIdSetting, DeviceTypeSetting, MqttBindAddressSetting,
    MqttPortSetting, TEdgeConfig,
};
use tedge_utils::file::*;
use tracing::{info, info_span, Instrument};

use super::topic::C8yTopic;

const CUMULOCITY_MAPPER_NAME: &str = "tedge-mapper-c8y";
const MQTT_MESSAGE_SIZE_THRESHOLD: usize = 16184;

pub struct CumulocityMapper {}

impl CumulocityMapper {
    pub fn new() -> CumulocityMapper {
        CumulocityMapper {}
    }

    pub fn subscriptions(operations: &Operations) -> Result<TopicFilter, anyhow::Error> {
        let mut topic_filter = TopicFilter::new(ResponseTopic::SoftwareListResponse.as_str())?;
        topic_filter.add(ResponseTopic::SoftwareUpdateResponse.as_str())?;
        topic_filter.add(C8yTopic::SmartRestRequest.as_str())?;
        topic_filter.add(ResponseTopic::RestartResponse.as_str())?;

        for topic in operations.topics_for_operations() {
            topic_filter.add(&topic)?
        }

        Ok(topic_filter)
    }
}

#[async_trait]
impl TEdgeComponent for CumulocityMapper {
    fn session_name(&self) -> &str {
        CUMULOCITY_MAPPER_NAME
    }

    async fn init(&self, cfg_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper c8y");
        let config_dir = cfg_dir.display().to_string();
        create_directories(&config_dir)?;
        let operations = Operations::try_new(&format!("{config_dir}/operations"), "c8y")?;
        self.init_session(CumulocityMapper::subscriptions(&operations)?)
            .await?;
        Ok(())
    }

    async fn start(&self, tedge_config: TEdgeConfig, cfg_dir: &Path) -> Result<(), anyhow::Error> {
        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);
        let config_dir = cfg_dir.display().to_string();

        let operations = Operations::try_new(format!("{config_dir}/operations"), "c8y")?;
        let mut http_proxy = JwtAuthHttpProxy::try_new(&tedge_config).await?;
        http_proxy.init().await?;
        let device_name = tedge_config.query(DeviceIdSetting)?;
        let device_type = tedge_config.query(DeviceTypeSetting)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();

        let converter = Box::new(CumulocityConverter::new(
            size_threshold,
            device_name,
            device_type,
            operations,
            http_proxy,
        )?);

        let mut mapper = create_mapper(
            CUMULOCITY_MAPPER_NAME,
            mqtt_host.clone(),
            mqtt_port,
            converter,
        )
        .await?;

        let ops_dir = PathBuf::from(format!("{}/operations/c8y", &config_dir));

        mapper
            .run(Some(ops_dir))
            .instrument(info_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}

fn create_directories(config_dir: &str) -> Result<(), anyhow::Error> {
    create_directory_with_user_group(
        &format!("{config_dir}/operations/c8y"),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_file_with_user_group(
        &format!("{config_dir}/operations/c8y/c8y_SoftwareUpdate"),
        "tedge",
        "tedge",
        0o644,
    )?;
    create_file_with_user_group(
        &format!("{config_dir}/operations/c8y/c8y_Restart"),
        "tedge",
        "tedge",
        0o644,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use c8y_api::http_proxy::MockC8yJwtTokenRetriever;
    use c8y_smartrest::smartrest_deserializer::SmartRestJwtResponse;
    use mockito::mock;
    use mqtt_tests::{assert_received_all_expected, test_mqtt_broker};
    use serde_json::json;
    use std::time::Duration;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    const TEST_TIMEOUT_SECS: Duration = Duration::from_secs(5);
    const CUMULOCITY_MAPPER_NAME_TEST: &str = "tedge-mapper-c8y-test";
    const DEVICE_ID: &str = "test-device";
    const DEVICE_NAME: &str = "test-user";
    const DEVICE_TYPE: &str = "test-thin-edge.io";
    const MQTT_HOST: &str = "127.0.0.1";

    /// `test_tedge_mapper_with_mqtt_pub` will start tedge mapper and run the following tests:
    ///
    /// - receive pub message with wrong payload and expect an error message in tedge/errors topic
    /// - receive pub message with a correct payload and expect to observe this on the right topic
    #[test_case(
        "tedge/measurements",
        "tedge/errors",
        "{",
        "Invalid JSON: EOF while parsing an object at line 1 column 1: `{`"
    )] // fail case
    #[test_case(
        "tedge/measurements",
        "tedge/measurements",
        r#"{"temperature": 12, "time": "2021-06-15T17:01:15.806181503+02:00"}"#,
        r#"{"temperature": 12, "time": "2021-06-15T17:01:15.806181503+02:00"}"#
    )] // successs case
    #[tokio::test]
    async fn test_tedge_mapper_with_mqtt_pub(
        pub_topic: &str,
        sub_topic: &str,
        payload: &str,
        expected_msg: &str,
    ) -> Result<(), anyhow::Error> {
        // Mock endpoint to return C8Y internal id
        let _get_internal_id_mock = mock("GET", "/identity/externalIds/c8y_Serial/test-device")
            .with_status(200)
            .with_body(
                json!({ "externalId": DEVICE_ID, "managedObject": { "id": "123" } }).to_string(),
            )
            .create();

        let mut jwt_token_retriver = Box::new(MockC8yJwtTokenRetriever::new());
        jwt_token_retriver
            .expect_get_jwt_token()
            .returning(|| Ok(SmartRestJwtResponse::default()));

        let http_client = reqwest::ClientBuilder::new().build().unwrap();
        let proxy = JwtAuthHttpProxy::new(
            jwt_token_retriver,
            http_client,
            mockito::server_url().as_str(),
            DEVICE_ID,
        );

        // mapper config
        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);
        let operations = Operations::default();

        let tmp_dir = TempTedgeDir::new();
        let converter = Box::new(
            CumulocityConverter::from_logs_path(
                size_threshold,
                DEVICE_NAME.into(),
                DEVICE_TYPE.into(),
                operations,
                proxy,
                tmp_dir.path().to_path_buf(),
            )
            .unwrap(),
        );

        let broker = test_mqtt_broker();

        let mut mapper = create_mapper(
            CUMULOCITY_MAPPER_NAME_TEST,
            MQTT_HOST.into(),
            broker.port,
            converter,
        )
        .await?;

        // subscribe to `sub_topic`
        let mut messages = broker.messages_published_on(sub_topic).await;

        // run tedge_mapper in background
        tokio::spawn(async move {
            mapper
                .run(None)
                .instrument(info_span!(CUMULOCITY_MAPPER_NAME_TEST))
                .await
                .unwrap();
        });

        // publish `payload` to `pub_topic`
        let () = broker.publish(pub_topic, payload).await?;

        // check the `messages` returned contain `expected_msg`
        assert_received_all_expected(&mut messages, TEST_TIMEOUT_SECS, &[expected_msg]).await;
        Ok(())
    }
}
