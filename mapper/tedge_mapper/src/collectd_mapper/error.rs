use tokio::sync::mpsc::error::SendError;

#[derive(thiserror::Error, Debug)]
pub enum DeviceMonitorError {
    #[error(transparent)]
    MqttClient(#[from] mqtt_client::MqttClientError),

    #[error(transparent)]
    InvalidCollectdMeasurement(#[from] crate::collectd_mapper::collectd::CollectdError),

    #[error(transparent)]
    InvalidThinEdgeJson(#[from] thin_edge_json::group::MeasurementGrouperError),

    #[error(transparent)]
    ThinEdgeJsonSerialization(#[from] thin_edge_json::serialize::ThinEdgeJsonSerializationError),

    #[error(transparent)]
    Batching(#[from] SendError<thin_edge_json::group::MeasurementGrouper>),
}
