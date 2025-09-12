use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::time::Duration;
use tedge_flows::flow::DateTime;
use tedge_flows::flow::Message;
use tedge_flows::flow::MessageSource;
use tedge_flows::MeaDB;
use tedge_flows::MessageProcessor;
use tempfile::TempDir;
use time::macros::datetime;
use tokio::time::sleep;

/// Helper function to create a temporary database path
fn temp_db_path() -> (TempDir, Utf8PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = Utf8PathBuf::from_path_buf(temp_dir.path().join("test_mea_db"))
        .expect("Failed to create UTF-8 path");
    (temp_dir, db_path)
}

/// Helper function to create a test message
fn create_test_message(topic: &str, payload: &str) -> Message {
    Message {
        topic: topic.to_string(),
        payload: payload.into(),
        timestamp: Some(DateTime::now()),
    }
}

/// Helper function to create a timestamp from a unix timestamp
fn create_timestamp(unix_timestamp: i64) -> DateTime {
    DateTime {
        seconds: unix_timestamp as u64,
        nanoseconds: 0,
    }
}

#[tokio::test]
async fn stores_message_and_retrieves_it_with_correct_timestamp() {
    let (_temp_dir, db_path) = temp_db_path();
    let mut db: MeaDB = MeaDB::open(&db_path)
        .await
        .expect("Failed to open database");

    let series = "test_series";
    let timestamp = create_timestamp(1640995200); // 2022-01-01 00:00:00 UTC
    let message = create_test_message("te/device/main///m/temperature", r#"{"temperature": 25.5}"#);

    // Store message
    db.store(series, timestamp, message.clone())
        .await
        .expect("Failed to store message");

    // Retrieve all messages
    let stored_messages = db
        .query_all(series)
        .await
        .expect("Failed to query messages");
    assert_eq!(
        stored_messages.len(),
        1,
        "stored_messages: {stored_messages:?}"
    );
    assert_eq!(stored_messages[0].0, timestamp);
    assert_eq!(stored_messages[0].1.topic, message.topic);
    assert_eq!(stored_messages[0].1.payload, message.payload);
}

#[tokio::test]
async fn drains_messages_older_than_cutoff_leaving_newer_ones() {
    let (_temp_dir, db_path) = temp_db_path();
    let mut db: MeaDB = MeaDB::open(&db_path)
        .await
        .expect("Failed to open database");

    let series = "drain_test_series";

    // Store messages at different times
    let old_timestamp = create_timestamp(1640995200); // 2022-01-01 00:00:00 UTC
    let new_timestamp = create_timestamp(1640995800); // 2022-01-01 00:10:00 UTC
    let future_timestamp = create_timestamp(1640996400); // 2022-01-01 00:20:00 UTC

    let old_msg = create_test_message("te/device/old///m/temp", r#"{"temp": 20.0}"#);
    let new_msg = create_test_message("te/device/new///m/temp", r#"{"temp": 25.0}"#);
    let future_msg = create_test_message("te/device/future///m/temp", r#"{"temp": 30.0}"#);

    db.store(series, old_timestamp, old_msg.clone())
        .await
        .unwrap();
    db.store(series, new_timestamp, new_msg.clone())
        .await
        .unwrap();
    db.store(series, future_timestamp, future_msg.clone())
        .await
        .unwrap();

    // Drain messages older than or equal to new_timestamp
    let drained = db.drain_older_than(new_timestamp, series).await.unwrap();

    // Should get old_msg and new_msg (2 messages)
    assert_eq!(drained.len(), 2, "drained: {drained:?}");

    // Verify the messages are in timestamp order
    assert_eq!(drained[0].0, old_timestamp);
    assert_eq!(drained[0].1.payload, br#"{"temp": 20.0}"#);
    assert_eq!(drained[1].0, new_timestamp);
    assert_eq!(drained[1].1.payload, br#"{"temp": 25.0}"#);

    // Verify future_msg is still in database
    let remaining = db.query_all(series).await.unwrap();
    assert_eq!(remaining.len(), 1, "remaining: {remaining:?}");
    assert_eq!(remaining[0].0, future_timestamp);
    assert_eq!(remaining[0].1.payload, br#"{"temp": 30.0}"#);
}

#[tokio::test]
async fn isolates_messages_between_different_series() {
    let (_temp_dir, db_path) = temp_db_path();
    let mut db: MeaDB = MeaDB::open(&db_path)
        .await
        .expect("Failed to open database");

    let series_a = "temperature_series";
    let series_b = "humidity_series";
    let timestamp = DateTime::now();

    let temp_msg =
        create_test_message("te/device/main///m/temperature", r#"{"temperature": 25.5}"#);
    let humidity_msg = create_test_message("te/device/main///m/humidity", r#"{"humidity": 60.0}"#);

    // Store in different series
    db.store(series_a, timestamp, temp_msg.clone())
        .await
        .unwrap();
    db.store(series_b, timestamp, humidity_msg.clone())
        .await
        .unwrap();

    // Query each series separately
    let temp_messages = db.query_all(series_a).await.unwrap();
    let humidity_messages = db.query_all(series_b).await.unwrap();

    assert_eq!(temp_messages.len(), 1, "temp_messages: {temp_messages:?}");
    assert_eq!(
        humidity_messages.len(),
        1,
        "humidity_messages: {humidity_messages:?}"
    );
    assert_eq!(temp_messages[0].1.payload, br#"{"temperature": 25.5}"#);
    assert_eq!(humidity_messages[0].1.payload, br#"{"humidity": 60.0}"#);
}

#[tokio::test]
async fn processes_mqtt_message_through_js_and_outputs_to_database() {
    // Create a temporary config directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    // Create a test JavaScript file
    let js_content = r#"
        export function onMessage(message) {
            return [message];
        }
    "#;
    std::fs::write(config_dir.join("test.js"), js_content).expect("Failed to write JS file");

    // Create a TOML config for MeaDB output flow
    let toml_content = r#"
        input.mqtt.topics = ["te/device/main///m/temperature"]

        steps = [
            { script = "test.js" }
        ]

        output.db.series = "temperature-measurements"
    "#;
    std::fs::write(config_dir.join("test.toml"), toml_content).expect("Failed to write TOML file");

    // Create message processor
    let mut processor = MessageProcessor::try_new(Utf8Path::from_path(config_dir).unwrap())
        .await
        .expect("Failed to create message processor");

    // Create test message
    let timestamp = DateTime::now();
    let message = create_test_message("te/device/main///m/temperature", r#"{"temperature": 22.5}"#);

    // Process message
    let results = processor
        .on_message(MessageSource::MQTT, timestamp, &message)
        .await;

    // Verify message was processed
    assert_eq!(results.len(), 1, "results: {results:?}");
    let (_flow_id, flow_result) = &results[0];
    assert!(flow_result.is_ok());

    let processed_messages = flow_result.as_ref().unwrap();
    assert_eq!(
        processed_messages.len(),
        1,
        "processed_messages: {processed_messages:?}"
    );
    assert_eq!(processed_messages[0].topic, message.topic);
    assert_eq!(processed_messages[0].payload, message.payload);
}

#[tokio::test]
async fn drains_database_and_processes_messages_through_js() {
    // Create temporary config directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    // Create JavaScript file
    let js_content = r#"
        export function onMessage(message) {
            return [message];
        }
    "#;
    std::fs::write(config_dir.join("process.js"), js_content).expect("Failed to write JS file");

    // Create TOML config for MeaDB input flow
    let toml_content = r#"
        input.db.series = "sensor-data"
        input.db.frequency = "5s"
        input.db.max_age = "1m"

        steps = [
            { script = "process.js" }
        ]

        output.mqtt.topics = ["te/device/main///e/processed"]
    "#;
    std::fs::write(config_dir.join("input_test.toml"), toml_content)
        .expect("Failed to write TOML config");

    // Create message processor
    let mut processor = MessageProcessor::try_new(Utf8Path::from_path(config_dir).unwrap())
        .await
        .expect("Failed to create message processor");

    // Pre-populate database with test data
    let old_timestamp = create_timestamp(1640995200); // Old timestamp
    let test_message = create_test_message("te/device/main///m/sensor", r#"{"humidity": 45.0}"#);

    processor
        .database
        .store("sensor-data", old_timestamp, test_message.clone())
        .await
        .expect("Failed to store test data");

    // Create a timestamp that should trigger draining (1 minute max age)
    let drain_timestamp = create_timestamp(1640995320); // 2 minutes later

    // Test database draining
    let drain_results = processor.drain_db(drain_timestamp).await;

    // Verify drain operation
    assert_eq!(drain_results.len(), 1, "drain_results: {drain_results:?}");
    let (_flow_id, drain_result) = &drain_results[0];
    assert!(drain_result.is_ok());

    let drained_messages = drain_result.as_ref().unwrap();
    assert_eq!(
        drained_messages.len(),
        1,
        "drained_messages: {drained_messages:?}"
    );
    assert_eq!(drained_messages[0].0, old_timestamp);
    assert_eq!(drained_messages[0].1.topic, test_message.topic);
}

#[tokio::test]
async fn only_drains_database_at_configured_frequency_intervals() {
    // Create temporary config directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    // Create minimal JavaScript
    std::fs::write(
        config_dir.join("identity.js"),
        "export function onMessage(message) { return [message]; }",
    )
    .expect("Failed to write JS file");

    // Create config with 10 second frequency
    let toml_content = r#"
        input.db.series = "frequent-data"
        input.db.frequency = "10s"
        input.db.max_age = "1m"

        steps = [
            { script = "identity.js" }
        ]
    "#;
    std::fs::write(config_dir.join("frequency_test.toml"), toml_content)
        .expect("Failed to write TOML config");

    let mut processor = MessageProcessor::try_new(Utf8Path::from_path(config_dir).unwrap())
        .await
        .expect("Failed to create message processor");

    // Test timestamps
    let base_time = 1640995200; // Base timestamp
    let exact_interval = create_timestamp(base_time + 10); // Exactly 10 seconds later
    let non_interval = create_timestamp(base_time + 7); // 7 seconds later

    // Test that draining happens at exact intervals
    let drain_results_interval = processor.drain_db(exact_interval).await;
    assert_eq!(
        drain_results_interval.len(),
        1,
        "drain_results_interval: {drain_results_interval:?}"
    );

    // Test that draining doesn't happen at non-intervals
    let drain_results_non_interval = processor.drain_db(non_interval).await;
    assert_eq!(
        drain_results_non_interval.len(),
        0,
        "drain_results_non_interval: {drain_results_non_interval:?}"
    );
}

#[tokio::test]
async fn drains_messages_older_than_max_age_retention_period() {
    // Create temporary config directory
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    std::fs::write(
        config_dir.join("filter.js"),
        "export function onMessage(message) { return [message]; }",
    )
    .expect("Failed to write JS file");

    // Config with 30 second max age
    let toml_content = r#"
        input.db.series = "age-filtered-data" 
        input.db.frequency = "10s"
        input.db.max_age = "30s"

        steps = [
            { script = "filter.js" }
        ]
    "#;
    std::fs::write(config_dir.join("age_test.toml"), toml_content)
        .expect("Failed to write TOML config");

    let mut processor = MessageProcessor::try_new(Utf8Path::from_path(config_dir).unwrap())
        .await
        .expect("Failed to create message processor");

    let current_time = 1640995200;
    let current_timestamp = create_timestamp(current_time);

    // Store messages at different ages
    let recent_msg = create_test_message("te/recent", r#"{"value": "recent"}"#);
    let old_msg = create_test_message("te/old", r#"{"value": "old"}"#);
    let very_old_msg = create_test_message("te/very_old", r#"{"value": "very_old"}"#);

    // Recent message (10 seconds ago - should stay in DB)
    let recent_timestamp = create_timestamp(current_time - 10);
    // Old message (25 seconds ago - should stay in DB)
    let old_timestamp = create_timestamp(current_time - 25);
    // Very old message (45 seconds ago - should be drained)
    let very_old_timestamp = create_timestamp(current_time - 45);

    processor
        .database
        .store("age-filtered-data", recent_timestamp, recent_msg)
        .await
        .unwrap();
    processor
        .database
        .store("age-filtered-data", old_timestamp, old_msg)
        .await
        .unwrap();
    processor
        .database
        .store("age-filtered-data", very_old_timestamp, very_old_msg)
        .await
        .unwrap();

    // Drain at the current time
    let drain_results = processor.drain_db(current_timestamp).await;

    assert_eq!(drain_results.len(), 1, "drain_results: {drain_results:?}");
    let (_, drain_result) = &drain_results[0];
    assert!(drain_result.is_ok());

    let drained_messages = drain_result.as_ref().unwrap();
    // Should only get messages older than max_age (30s)
    // very_old (45s ago) should be drained
    // recent (10s ago) and old (25s ago) should remain in database
    assert_eq!(
        drained_messages.len(),
        1,
        "drained_messages: {drained_messages:?}"
    );

    // Verify correct message was drained
    assert_eq!(
        drained_messages[0].1.payload, br#"{"value": "very_old"}"#,
        "Should drain the very_old message (45s ago)"
    );
}

#[tokio::test]
async fn chains_mqtt_storage_drain_and_output_flows_end_to_end() {
    // Test complete flow: MQTT input -> store to DB -> drain from DB -> process -> output
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    // Create processing script
    let js_content = r#"
        export function onMessage(message) {
            // Add processed flag to message
            let processed = JSON.parse(message.payload);
            processed.processed = true;
            return [{
                topic: "te/device/main///e/processed",
                payload: JSON.stringify(processed),
                timestamp: message.timestamp
            }];
        }
    "#;
    std::fs::write(config_dir.join("processor.js"), js_content).expect("Failed to write JS file");

    // Config 1: MQTT -> DB storage
    let storage_config = r#"
        input.mqtt.topics = ["te/device/main///m/sensor"]

        steps = [
            { script = "processor.js" }
        ]

        output.db.series = "processed-sensor-data"
    "#;
    std::fs::write(config_dir.join("storage_flow.toml"), storage_config)
        .expect("Failed to write storage config");

    // Config 2: DB -> processing -> MQTT
    let processing_config = r#"
        input.db.series = "processed-sensor-data"
        input.db.frequency = "5s"
        input.db.max_age = "1m"

        steps = [
            { script = "processor.js" }
        ]

        output.mqtt.topics = ["te/device/main///e/final"]
    "#;
    std::fs::write(config_dir.join("processing_flow.toml"), processing_config)
        .expect("Failed to write processing config");

    let mut processor = MessageProcessor::try_new(Utf8Path::from_path(config_dir).unwrap())
        .await
        .expect("Failed to create message processor");

    // Step 1: Process MQTT message through storage flow
    let input_timestamp = DateTime::try_from(datetime!(2022-01-01 00:00:00 UTC)).unwrap();
    let input_message = create_test_message(
        "te/device/main///m/sensor",
        r#"{"temperature": 23.5, "humidity": 55.0}"#,
    );

    let storage_results = processor
        .on_message(MessageSource::MQTT, input_timestamp, &input_message)
        .await;

    // Verify storage flow processed the message
    assert!(
        !storage_results.is_empty(),
        "storage_results: {storage_results:?}"
    );
    let storage_successful = storage_results.iter().any(|(_, result)| result.is_ok());
    assert!(storage_successful);

    // Step 2: Wait and then drain database
    sleep(Duration::from_millis(100)).await; // Small delay

    let drain_timestamp = DateTime::try_from(datetime!(2022-01-01 00:01:00 UTC)).unwrap();
    let drain_results = processor.drain_db(drain_timestamp).await;

    // Verify drain operation found data
    assert!(
        !drain_results.is_empty(),
        "drain_results: {drain_results:?}"
    );
    let drain_successful = drain_results.iter().any(|(_, result)| result.is_ok());
    assert!(drain_successful);

    // Step 3: Process drained messages
    for (_, drain_result) in drain_results {
        if let Ok(drained_messages) = drain_result {
            for (timestamp, message) in drained_messages {
                let process_results = processor
                    .on_message(MessageSource::MeaDB, timestamp, &message)
                    .await;

                // Verify processing results
                assert!(
                    !process_results.is_empty(),
                    "process_results: {process_results:?}"
                );
                let process_successful = process_results.iter().any(|(_, result)| result.is_ok());
                assert!(process_successful);
            }
        }
    }
}

#[tokio::test]
async fn processes_messages_only_from_matching_input_sources() {
    // Test that flows only accept messages from correct sources
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config_dir = temp_dir.path();

    std::fs::write(
        config_dir.join("passthrough.js"),
        "export function onMessage(message) { return [message]; }",
    )
    .expect("Failed to write JS file");

    // MQTT input flow - should only accept MQTT messages
    let mqtt_config = r#"
        input.mqtt.topics = ["te/device/main///m/test"]
        steps = [{ script = "passthrough.js" }]
    "#;
    std::fs::write(config_dir.join("mqtt_flow.toml"), mqtt_config)
        .expect("Failed to write MQTT config");

    // DB input flow - should only accept MeaDB messages
    let db_config = r#"
        input.db.series = "test-data"
        input.db.frequency = "1s"
        input.db.max_age = "1m"
        steps = [{ script = "passthrough.js" }]
    "#;
    std::fs::write(config_dir.join("db_flow.toml"), db_config).expect("Failed to write DB config");

    let mut processor = MessageProcessor::try_new(Utf8Path::from_path(config_dir).unwrap())
        .await
        .expect("Failed to create message processor");

    let timestamp = DateTime::now();
    let test_message = create_test_message("te/device/main///m/test", r#"{"value": 42}"#);

    // Test MQTT message source
    let mqtt_results = processor
        .on_message(MessageSource::MQTT, timestamp, &test_message)
        .await;

    // Test MeaDB message source
    let mea_db_results = processor
        .on_message(MessageSource::MeaDB, timestamp, &test_message)
        .await;

    // MQTT flow should process MQTT messages but not MeaDB messages
    // DB flow should process MeaDB messages but not MQTT messages

    // Find results for each flow
    let mqtt_flow_result = mqtt_results
        .iter()
        .find(|(flow_id, _)| flow_id.contains("mqtt_flow"));
    let db_flow_result = mea_db_results
        .iter()
        .find(|(flow_id, _)| flow_id.contains("db_flow"));

    // Verify source filtering worked correctly
    if let Some((_, result)) = mqtt_flow_result {
        assert!(result.is_ok());
        let messages = result.as_ref().unwrap();
        assert!(!messages.is_empty(), "mqtt_flow messages: {messages:?}");
    }

    if let Some((_, result)) = db_flow_result {
        assert!(result.is_ok());
        let messages = result.as_ref().unwrap();
        assert!(!messages.is_empty(), "db_flow messages: {messages:?}");
    }
}
