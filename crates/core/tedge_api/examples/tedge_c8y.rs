use tedge_api::*;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
    let mut c8y = c8y.instantiate()?;
    let mut c8y_con = c8y_con.instantiate()?;
    let mut collectd = collectd.instantiate()?;
    let mut collectd_con = collectd_con.instantiate()?;
    let mut thin_edge_json = thin_edge_json.instantiate()?;
    let mut thin_edge_json_con = thin_edge_json_con.instantiate()?;
    let mut sm_service = sm_service.instantiate()?;
    let mut apt = apt.instantiate()?;
    let mut apama = apama.instantiate()?;

    // The plugins have now to be connected to each others.
    // Here one use a specific methods that connect consumers and producers
    // One should be able to set dynamic connection via the configuration
    // i.e. naming consumers & producers and having runtime type checks to ensure type safety.
    c8y.set_mqtt_con(&mut c8y_con);
    collectd.set_mqtt_con(&mut collectd_con);
    thin_edge_json.set_mqtt_con(&mut thin_edge_json_con);

    c8y.add_measurement_producer(&mut collectd);
    c8y.add_measurement_producer(&mut thin_edge_json);

    c8y.set_sm_service(&mut sm_service);
    sm_service.add_package_manager(&mut apt);
    sm_service.add_package_manager(&mut apama);

    // Not addressed here, but critical:
    // If a dynamic connection is unsafe the system must stop here
    // Ideally logging all the errors, not just the first one

    // Up to now all the plugins were inactive.
    // Let them run!
    // Do we need some runtime here?
    c8y.start().await?;
    collectd.start().await?;
    thin_edge_json.start().await?;
    sm_service.start().await?;
    apt.start().await?;
    apama.start().await?;
    c8y_con.start().await?;
    collectd_con.start().await?;
    thin_edge_json_con.start().await?;

    Ok(())
}

use async_trait::async_trait;

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

#[async_trait]
impl Plugin for C8YMapper {
    async fn start(self) -> Result<(), RuntimeError> {
        todo!()
    }
}
impl Consumer<Measurement> for C8YMapper {}
impl Requester<SMRequest, SMResponse> for C8YMapper {}

impl C8YMapper {
    pub fn set_mqtt_con(&mut self, con: &mut (impl Producer<MqttMessage> + Consumer<MqttMessage>)) {
        todo!()
    }
    pub fn add_measurement_producer(&mut self, producer: &mut impl Producer<Measurement>) {
        todo!()
    }
    pub fn set_sm_service(&mut self, sm: &mut impl Responder<SMRequest, SMResponse>) {
        todo!()
    }
}

/// Measurements received from Collectd via MQTT
#[derive(Default)]
struct CollectdMapperConfig {}
struct CollectdMapper {}

impl Producer<Measurement> for CollectdMapper {}

impl PluginConfig for CollectdMapperConfig {
    type Plugin = CollectdMapper;

    fn instantiate(self) -> Result<Self::Plugin, RuntimeError> {
        todo!()
    }
}

#[async_trait]
impl Plugin for CollectdMapper {
    async fn start(self) -> Result<(), RuntimeError> {
        todo!()
    }
}

impl CollectdMapper {
    pub fn set_mqtt_con(&mut self, con: &mut (impl Producer<MqttMessage> + Consumer<MqttMessage>)) {
        todo!()
    }
}

/// Measurements received from /tedge/measurements via MQTT
#[derive(Default)]
struct ThinEdgeJsonConfig {}
struct ThinEdgeJson {}

impl Producer<Measurement> for ThinEdgeJson {}

impl PluginConfig for ThinEdgeJsonConfig {
    type Plugin = ThinEdgeJson;

    fn instantiate(self) -> Result<Self::Plugin, RuntimeError> {
        todo!()
    }
}

#[async_trait]
impl Plugin for ThinEdgeJson {
    async fn start(self) -> Result<(), RuntimeError> {
        todo!()
    }
}

impl ThinEdgeJson {
    pub fn set_mqtt_con(&mut self, con: &mut (impl Producer<MqttMessage> + Consumer<MqttMessage>)) {
        todo!()
    }
}

/// Handle sm operations
#[derive(Default)]
struct SoftwareManagementServiceConfig {}
struct SoftwareManagementService {}
impl Responder<SMRequest, SMResponse> for SoftwareManagementService {}

impl PluginConfig for SoftwareManagementServiceConfig {
    type Plugin = SoftwareManagementService;

    fn instantiate(self) -> Result<Self::Plugin, RuntimeError> {
        todo!()
    }
}

#[async_trait]
impl Plugin for SoftwareManagementService {
    async fn start(self) -> Result<(), RuntimeError> {
        todo!()
    }
}

impl SoftwareManagementService {
    pub fn add_package_manager(&mut self, package_manager: &mut impl Responder<SMRequest, SMResponse>) {
        todo!()
    }
}

#[derive(Default)]
struct AptPackagerConfig {}
struct AptPackager {}
impl Responder<SMRequest, SMResponse> for AptPackager {}

impl PluginConfig for AptPackagerConfig {
    type Plugin = AptPackager;

    fn instantiate(self) -> Result<Self::Plugin, RuntimeError> {
        todo!()
    }
}

#[async_trait]
impl Plugin for AptPackager {
    async fn start(self) -> Result<(), RuntimeError> {
        todo!()
    }
}

#[derive(Default)]
struct ApamaPackagerConfig {}
struct ApamaPackager {}
impl Responder<SMRequest, SMResponse> for ApamaPackager {}

impl PluginConfig for ApamaPackagerConfig {
    type Plugin = ApamaPackager;

    fn instantiate(self) -> Result<Self::Plugin, RuntimeError> {
        todo!()
    }
}

#[async_trait]
impl Plugin for ApamaPackager {
    async fn start(self) -> Result<(), RuntimeError> {
        todo!()
    }
}

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
    type Plugin = MqttConnection;

    fn instantiate(self) -> Result<Self::Plugin, RuntimeError> {
        Ok(MqttConnection{config: self})
    }
}

#[async_trait]
impl Plugin for MqttConnection {
    async fn start(self) -> Result<(), RuntimeError> {
        todo!()
    }
}

impl Consumer<MqttMessage> for MqttConnection {}

impl Producer<MqttMessage> for MqttConnection {}
