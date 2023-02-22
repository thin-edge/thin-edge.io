use std::path::Path;
use std::path::PathBuf;

use crate::c8y::converter::CumulocityConverter;
use crate::c8y::converter::CumulocityDeviceInfo;
use crate::core::component::TEdgeComponent;
use crate::core::converter::make_valid_topic_or_panic;
use crate::core::converter::MapperConfig;
use crate::core::mapper::mqtt_config;
use crate::core::mapper::Mapper;
use crate::core::size_threshold::SizeThreshold;
use async_trait::async_trait;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::http_proxy::JwtAuthHttpProxy;
use c8y_api::smartrest::operations::Operations;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Connection;
use mqtt_channel::TopicFilter;
use tedge_api::health::health_check_topics;
use tedge_api::topic::ResponseTopic;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::DeviceTypeSetting;
use tedge_config::MqttBindAddressSetting;
use tedge_config::MqttPortSetting;
use tedge_config::TEdgeConfig;
use tedge_utils::file::*;
use tracing::info;
use tracing::info_span;
use tracing::Instrument;

use super::service_monitor::service_monitor_status_message;

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
        topic_filter.add(C8yTopic::SmartRestRequest.to_string().as_str())?;
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
        create_directories(cfg_dir)?;
        let operations = Operations::try_new(format!("{}/operations/c8y", cfg_dir.display()))?;
        self.init_session(CumulocityMapper::subscriptions(&operations)?)
            .await?;
        Ok(())
    }

    async fn start(&self, tedge_config: TEdgeConfig, cfg_dir: &Path) -> Result<(), anyhow::Error> {
        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);
        let config_dir = cfg_dir.display().to_string();

        let operations = Operations::try_new(format!("{config_dir}/operations/c8y"))?;
        let child_ops = Operations::get_child_ops(format!("{config_dir}/operations/c8y"))?;
        let mut http_proxy = JwtAuthHttpProxy::try_new(&tedge_config).await?;
        http_proxy.init().await?;
        let device_name = tedge_config.query(DeviceIdSetting)?;
        let device_type = tedge_config.query(DeviceTypeSetting)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?.to_string();

        let mapper_config = create_mapper_config(&operations);

        let mqtt_client = create_mqtt_client(
            CUMULOCITY_MAPPER_NAME,
            mqtt_host.clone(),
            mqtt_port,
            &mapper_config,
        )
        .await?;

        // Dedicated mqtt client just for sending a will message, when the mapper goes down
        let _mqtt_client_wm = create_mqtt_client_will_message(
            &device_name,
            CUMULOCITY_MAPPER_NAME,
            mqtt_host.clone(),
            mqtt_port,
        )
        .await?;

        let device_info = CumulocityDeviceInfo {
            device_name,
            device_type,
            operations,
        };

        let converter = Box::new(CumulocityConverter::new(
            size_threshold,
            device_info,
            http_proxy,
            cfg_dir,
            child_ops,
            mapper_config,
            mqtt_client.published.clone(),
        )?);

        let mut mapper = Mapper::new(
            CUMULOCITY_MAPPER_NAME.to_string(),
            mqtt_client.received,
            mqtt_client.published,
            mqtt_client.errors,
            converter,
        );

        let ops_dir = PathBuf::from(format!("{}/operations/c8y", &config_dir));

        mapper
            .run(Some(&ops_dir))
            .instrument(info_span!(CUMULOCITY_MAPPER_NAME))
            .await?;

        Ok(())
    }
}

pub fn create_mapper_config(operations: &Operations) -> MapperConfig {
    let mut topic_filter: TopicFilter = vec![
        "tedge/measurements",
        "tedge/measurements/+",
        "tedge/alarms/+/+",
        "tedge/alarms/+/+/+",
        "c8y-internal/alarms/+/+",
        "c8y-internal/alarms/+/+/+",
        "tedge/events/+",
        "tedge/events/+/+",
        "tedge/health/+",
        "tedge/health/+/+",
    ]
    .try_into()
    .expect("topics that mapper should subscribe to");

    topic_filter.add_all(CumulocityMapper::subscriptions(operations).unwrap());

    MapperConfig {
        in_topic_filter: topic_filter,
        out_topic: make_valid_topic_or_panic("c8y/measurement/measurements/create"),
        errors_topic: make_valid_topic_or_panic("tedge/errors"),
    }
}

pub async fn create_mqtt_client(
    app_name: &str,
    mqtt_host: String,
    mqtt_port: u16,
    mapper_config: &MapperConfig,
) -> Result<Connection, anyhow::Error> {
    let health_check_topics: TopicFilter = health_check_topics(app_name);
    let mut topic_filter = mapper_config.in_topic_filter.clone();
    topic_filter.add_all(health_check_topics.clone());

    let mqtt_client =
        Connection::new(&mqtt_config(app_name, &mqtt_host, mqtt_port, topic_filter)?).await?;

    Ok(mqtt_client)
}

pub async fn create_mqtt_client_will_message(
    device_name: &str,
    app_name: &str,
    mqtt_host: String,
    mqtt_port: u16,
) -> Result<Connection, anyhow::Error> {
    let mqtt_config = mqtt_channel::Config::default()
        .with_host(mqtt_host)
        .with_port(mqtt_port)
        .with_last_will_message(service_monitor_status_message(
            device_name,
            app_name,
            "down",
            "thin-edge.io",
            None,
        ));
    let mqtt_client = Connection::new(&mqtt_config).await?;

    Ok(mqtt_client)
}

fn create_directories(config_dir: &Path) -> Result<(), anyhow::Error> {
    create_directory_with_user_group(
        format!("{}/operations/c8y", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_file_with_user_group(
        format!("{}/operations/c8y/c8y_SoftwareUpdate", config_dir.display()),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    create_file_with_user_group(
        format!("{}/operations/c8y/c8y_Restart", config_dir.display()),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    // Create directory for device custom fragments
    create_directory_with_user_group(
        format!("{}/device", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::c8y::tests::create_test_mqtt_client;
    use crate::core::mapper::create_mapper;
    use c8y_api::http_proxy::MockC8yJwtTokenRetriever;
    use c8y_api::smartrest::smartrest_deserializer::SmartRestJwtResponse;
    use mockito::mock;
    use mqtt_tests::assert_received_all_expected;
    use mqtt_tests::test_mqtt_broker;
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
        "Invalid JSON: EOF while parsing an object at line 1 column 1: `{\n`"
    )] // fail case
    #[test_case(
        "tedge/measurements",
        "tedge/measurements",
        r#"{"temperature": 12, "time": "2021-06-15T17:01:15.806181503+02:00"}"#,
        r#"{"temperature": 12, "time": "2021-06-15T17:01:15.806181503+02:00"}"#
    )] // success case
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

        // required to create a converter
        let size_threshold = SizeThreshold(MQTT_MESSAGE_SIZE_THRESHOLD);
        let operations = Operations::default();
        let tmp_dir = TempTedgeDir::new();
        let mapper_config = create_mapper_config(&operations);
        let mqtt_client = create_test_mqtt_client(&mapper_config).await;

        let converter = Box::new(
            CumulocityConverter::from_logs_path(
                size_threshold,
                DEVICE_NAME.into(),
                DEVICE_TYPE.into(),
                operations,
                proxy,
                tmp_dir.path().to_path_buf(),
                tmp_dir.path().to_path_buf(),
                mapper_config,
                mqtt_client.published.clone(),
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

        // run tedge-mapper in background
        tokio::spawn(async move {
            mapper
                .run(None)
                .instrument(info_span!(CUMULOCITY_MAPPER_NAME_TEST))
                .await
                .unwrap();
        });

        // publish `payload` to `pub_topic`
        broker.publish(pub_topic, payload).await?;

        // check the `messages` returned contain `expected_msg`
        assert_received_all_expected(&mut messages, TEST_TIMEOUT_SECS, &[expected_msg]).await;
        Ok(())
    }
}
