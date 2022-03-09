#[tokio::main]
async fn main() {
    let telemetry_peers = TelemetryPeers::default();
    let sm_peers = SMPeers::default();

    let c8y = C8YMapperBuilder::new(&telemetry_peers, &sm_peers);
    let collectd = CollectdMapperBuilder::new(&telemetry_peers);
    let thin_edge_json = ThinEdgeJsonBuilder::new(&telemetry_peers);
    let sm = SoftwareManagementServiceBuilder::new(&sm_peers);

    let config = toml::from_str("").expect("configured services");

    let sm = sm.instance(&config);
    let collectd = collectd.instance(&config);
    let thin_edge_json = thin_edge_json.instance(&config);
    let c8y = c8y.instance(&config).with_sm(&sm).with_measurement(&collectd).with_measurement(&thin_edge_json);

    c8y.run();
}

/// Connect the device to Cumulocity
/// Translate telemetry data into c8y specific messages
/// and c8y operations into sm operations
struct C8YMapperBuilder {}
struct C8YMapper {}

impl Consumer<Measurement> for C8YMapper {}
impl Requester<SMRequest,SMResponse> for C8YMapper {}

/// Measurements received from Collectd via MQTT
struct CollectdMapperBuilder {}
struct CollectdMapper {}

impl Producer<Measurement> for CollectdMapper {}

/// Measurements received from /tedge/measurements via MQTT
struct ThinEdgeJsonBuilder {}
struct ThinEdgeJson {}

impl Producer<Measurement> for ThinEdgeJson {}

/// Handle sm operations
struct SoftwareManagementServiceBuilder {}
struct SoftwareManagementService {}

impl Responder<SMRequest,SMResponse> for SoftwareManagementService {}

/// Plugins exchanging telemetry data
type TelemetryPeers = PubSubPeers<Measurement>;
struct Measurement {
    source: String,
    name: String,
    timestamp: u64,
    value: f32,
}

/// Plugins exchanging SM operations
type SMPeers = ReqResPeers<SMRequest,SMResponse>;
enum SMRequest {
    SoftwareList,
    SoftwareUpdate { update: Vec<PackageVersion> },
}
enum SMResponse {
    SoftwareList { list: Vec<PackageVersion> },
    SoftwareUpdate { result: Result<(),String> },
}
struct PackageVersion {
    manager: String,
    package: String,
    version: String,
}