use std::io::Write as _;
use std::sync::Arc;
use std::sync::LazyLock;

use tedge_config::tedge_toml::mapper_config::C8yMapperSpecificConfig;
use tedge_config::TEdgeConfig;
use tedge_config_macros::ProfileName;
use tedge_test_utils::fs::TempTedgeDir;

#[tokio::test]
async fn new_format_takes_precedence_over_legacy() {
    std::env::set_var("NO_COLOR", "true");
    let log_capture = TestLogCapture::new().await;

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(log_capture.clone())
        .finish();

    let _guard = tracing::subscriber::set_default(subscriber);

    let mapper_config = r#"
        url = "from-mapper-config.example.com"
    "#;
    let tedge_toml = r#"
        [c8y]
        url = "from-tedge-toml.example.com"
    "#;

    let ttd = TempTedgeDir::new();
    ttd.file("tedge.toml").with_raw_content(tedge_toml);
    ttd.dir("mappers")
        .file("c8y.toml")
        .with_raw_content(mapper_config);

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let c8y_config = tedge_config
        .mapper_config::<C8yMapperSpecificConfig>(&None::<ProfileName>)
        .await
        .unwrap();
    assert_eq!(
        c8y_config.mqtt().or_none().unwrap().host().to_string(),
        "from-mapper-config.example.com"
    );

    if log_capture.has_warnings() {
        let warning = log_capture
            .get_logs()
            .into_iter()
            .filter(|log| log.contains("WARN"))
            .find(|log| log.contains("Both") && log.contains("exist"))
            .expect("Should find conflict warning");

        let message = warning.rsplit_once(':').unwrap().1.trim();
        assert!(
            message.contains("mappers/c8y.toml"),
            "Warning should mention new config path"
        );
        assert!(
            message.contains("tedge.toml [c8y]"),
            "Warning should mention legacy config"
        );
        assert!(
            message.contains("Consider removing"),
            "Warning should suggest removing legacy config"
        );
    } else {
        panic!("Expected conflict warning to be logged, but found none")
    }
}

#[tokio::test]
async fn partial_migration_default_new_profile_legacy_errors() {
    let ttd = TempTedgeDir::new();

    ttd.dir("mappers")
        .file("c8y.toml")
        .with_toml_content(toml::toml! {
            url = "default.example.com"
        });

    ttd.file("tedge.toml").with_toml_content(toml::toml! {
        [c8y.profiles.prod]
        url = "prod-from-legacy.example.com"
    });

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();

    let default_result = tedge_config
        .mapper_config::<C8yMapperSpecificConfig>(&None::<ProfileName>)
        .await;
    assert!(default_result.is_ok());

    let prod_profile = ProfileName::try_from("prod".to_string()).unwrap();
    let prod_result = tedge_config
        .mapper_config::<C8yMapperSpecificConfig>(&Some(prod_profile))
        .await;

    let err = prod_result.unwrap_err();
    let expected_path = format!("{}/mappers/c8y.d/prod.toml", ttd.utf8_path());
    assert!(
        err.to_string().contains(&expected_path),
        "Error should mention the missing profile file path. Got: {err}",
    );
    assert!(
        err.to_string().contains("doesn't exist"),
        "Error should indicate file doesn't exist. Got: {err}",
    );
}

#[tokio::test]
async fn partial_migration_default_legacy_profile_new_errors() {
    let ttd = TempTedgeDir::new();

    ttd.dir("mappers")
        .dir("c8y.d")
        .file("prod.toml")
        .with_toml_content(toml::toml! {
            url = "prod.example.com"
        });

    ttd.file("tedge.toml").with_toml_content(toml::toml! {
        [c8y]
        url = "default-from-legacy.example.com"
    });

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();

    let prod_profile = ProfileName::try_from("prod".to_string()).unwrap();
    let prod_result = tedge_config
        .mapper_config::<C8yMapperSpecificConfig>(&Some(prod_profile))
        .await;
    assert!(prod_result.is_ok());

    let default_result = tedge_config
        .mapper_config::<C8yMapperSpecificConfig>(&None::<ProfileName>)
        .await;

    let err = default_result.unwrap_err();
    let expected_path = format!("{}/mappers/c8y.toml", ttd.utf8_path());
    assert!(
        err.to_string().contains(&expected_path),
        "Error should mention the missing default config file path. Got: {err}",
    );
    assert!(
        err.to_string().contains("doesn't exist"),
        "Error should indicate file doesn't exist. Got: {err}",
    );
}

#[tokio::test]
async fn empty_new_config_uses_tedge_toml_defaults() {
    let ttd = TempTedgeDir::new();

    ttd.dir("mappers").file("c8y.toml").with_raw_content("");

    ttd.file("tedge.toml").with_toml_content(toml::toml! {
        [device]
        id = "device-from-tedge-toml"
        [c8y]
        url = "should-be-ignored.example.com"
    });

    let tedge_config = TEdgeConfig::load(ttd.path()).await.unwrap();
    let c8y_config = tedge_config
        .mapper_config::<C8yMapperSpecificConfig>(&None::<ProfileName>)
        .await
        .unwrap();

    assert!(
        c8y_config.http().or_none().is_none(),
        "HTTP URL should not be set"
    );
    assert!(
        c8y_config.mqtt().or_none().is_none(),
        "MQTT URL should not be set"
    );

    assert_eq!(
        c8y_config.device.id().unwrap(),
        "device-from-tedge-toml",
        "Device ID should come from tedge.toml defaults"
    );
}

#[derive(Clone)]
struct TestLogCapture {
    logs: Arc<std::sync::Mutex<Vec<String>>>,
    _lock: Arc<tokio::sync::MutexGuard<'static, ()>>,
}

static LOG_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(<_>::default);

impl TestLogCapture {
    async fn new() -> Self {
        let lock = LOG_TEST_LOCK.lock().await;

        Self {
            logs: Arc::new(std::sync::Mutex::new(Vec::new())),
            _lock: Arc::new(lock),
        }
    }

    fn has_warnings(&self) -> bool {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .any(|log| log.contains("WARN"))
    }

    fn get_logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().clone()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for TestLogCapture {
    type Writer = TestWriter;

    fn make_writer(&'a self) -> Self::Writer {
        TestWriter {
            logs: self.logs.clone(),
            buf: Vec::new(),
        }
    }
}

struct TestWriter {
    logs: Arc<std::sync::Mutex<Vec<String>>>,
    buf: Vec<u8>,
}

impl std::io::Write for TestWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            if let Ok(s) = String::from_utf8(self.buf.clone()) {
                self.logs.lock().unwrap().push(s);
            }
            self.buf.clear()
        }
        Ok(())
    }
}

impl Drop for TestWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
