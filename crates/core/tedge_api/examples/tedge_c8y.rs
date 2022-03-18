use tedge_api::*;

#[tokio::main]
async fn main() {
    // The first step is to read the configuration
    // In a more realistic use-case the config would be loaded from a configuration file
    let c8y = C8YMapperConfig::default();
    let c8y_con = MqttConfig {
        session_name: "c8y-mapper".to_string(),
        subscriptions: vec!["c8y/#".to_string()],
    };
    let collectd = CollectdMapperConfig::default();
    let collectd_con = MqttConfig {
        session_name: "collectd-mapper".to_string(),
        subscriptions: vec!["c8y/#".to_string()],
    };
    let thin_edge_json = ThinEdgeJsonConfig::default();
    let thin_edge_json_con = MqttConfig {
        session_name: "tedge-mapper".to_string(),
        subscriptions: vec!["tedge/#".to_string()],
    };
    let sm_service = SoftwareManagementServiceConfig::default();
    let apt = ApamaPackagerConfig::default();
    let apama = ApamaPackagerConfig::default();

    // Not addressed here, but important:
    // If the config is incorrect the system must stop here
    // Ideally logging all the errors, not just the first one

    // The next step is to create the instances of the different plugins.
    // Here everything is static, but it would be nice to have some dynamic instantiation
    // to keep *only* the configured instances (say the device is not connected to c8y)
    // or as many as required (say several external sm plugins are declared).
    let mut runtime = Runtime::default();
    let c8y = runtime.instantiate(c8y)?;
    let c8y_con = runtime.instantiate(c8y_con)?;
    let collectd = runtime.instantiate(collectd)?;
    let collectd_con = runtime.instantiate(collectd_con)?;
    let thin_edge_json = runtime.instantiate(thin_edge_json)?;
    let thin_edge_json_con = runtime.instantiate(thin_edge_json_con)?;
    let sm_service = runtime.instantiate(sm_service)?;
    let apt = runtime.instantiate(apt)?;
    let apama = runtime.instantiate(apama)?;

    // The plugins have now to be connected to each others.
    // Here one use a specific methods that connect consumers and producers
    // One should be able to set dynamic connection via the configuration
    // i.e. naming consumers & producers and having runtime type checks to ensure type safety.
    c8y.set_mqtt_con(&c8y_con);
    collectd.set_mqtt_con(&collectd_con);
    thin_edge_json.set_mqtt_con(&thin_edge_json_con);

    c8y.add_measurement_producer(&collectd);
    c8y.add_measurement_producer(&thin_edge_json);

    c8y.set_sm_service(&sm_service);
    sm_service.add_package_manager(&apt);
    sm_service.add_package_manager(&apama);

    // Not addressed here, but critical:
    // If a dynamic connection is unsafe the system must stop here
    // Ideally logging all the errors, not just the first one

    // Up to now all the plugins were inactive.
    // Let them run!
    runtime.start();
}

/// Connect the device to Cumulocity
/// Translate telemetry data into c8y specific messages
/// and c8y operations into sm operations
#[derive(Default)]
struct C8YMapperConfig {}
struct C8YMapper {}

impl PluginConfig for C8YMapperConfig {
    type Plugin = C8YMapper;

    fn instantiate(self) -> Result<Self::Plugin, RuntimeError> {
        Ok(C8YMapper {})
    }
}
impl Plugin for C8YMapper {
    async fn start(&mut self) -> Result<(), RuntimeError> {
        todo!()
    }
}
impl Consumer<Measurement> for C8YMapper {}
impl Requester<SMRequest, SMResponse> for C8YMapper {}

/// Measurements received from Collectd via MQTT
#[derive(Default)]
struct CollectdMapperConfig {}
struct CollectdMapper {}

impl Producer<Measurement> for CollectdMapper {}

/// Measurements received from /tedge/measurements via MQTT
#[derive(Default)]
struct ThinEdgeJsonConfig {}
struct ThinEdgeJson {}

impl Producer<Measurement> for ThinEdgeJson {}

/// Handle sm operations
#[derive(Default)]
struct SoftwareManagementServiceConfig {}
struct SoftwareManagementService {}
impl Responder<SMRequest, SMResponse> for SoftwareManagementService {}

#[derive(Default)]
struct AptPackagerConfig {}
struct AptPackager {}
impl Responder<SMRequest, SMResponse> for AptPackager {}

#[derive(Default)]
struct ApamaPackagerConfig {}
struct ApamaPackager {}
impl Responder<SMRequest, SMResponse> for ApamaPackager {}

/// Plugins exchanging telemetry data
struct Measurement {
    source: String,
    name: String,
    timestamp: u64,
    value: f32,
}

/// Plugins exchanging SM operations
enum SMRequest {
    SoftwareList,
    SoftwareUpdate { update: Vec<SMOperation> },
}

enum SMResponse {
    SoftwareList { list: Vec<PackageVersion> },
    SoftwareUpdate { errors: Vec<SMError> },
}

enum SMOperation {
    Install { package: PackageVersion },
    Remove { package: PackageVersion },
}

struct SMError {
    operation: SMOperation,
    error: String,
}

struct PackageVersion {
    manager: String,
    package: String,
    version: String,
}

/// Plugins exchanging MQTT messages
struct MqttMessage {
    topic: String,
    payload: String,
    qos: QoS,
}

enum QoS {
    AtMostOnce,
    AtLeastOnce,
    ExactlyOnce,
}

#[derive(Clone, Debug, Default)]
struct MqttConfig {
    session_name: String,
    subscriptions: Vec<String>,
    // plus end-point and credentials.
}

struct MqttConnection {
    /// On a real case, must hold the connection not its config
    config: MqttConfig,
}

impl MqttConnection {
    fn new(config: &MqttConfig) -> Self {
        MqttConnection {
            config: config.clone(),
        }
    }
}

impl PluginConfig for MqttConfig {
    fn instantiate(&self, runtime: &Runtime) -> Result<(), RuntimeError> {
        runtime.register()
    }
}

impl Plugin for MqttConnection {}

impl Consumer<MqttMessage> for MqttConnection {}

impl Producer<MqttMessage> for MqttConnection {}
