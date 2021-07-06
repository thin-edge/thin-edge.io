use tokio::sync::mpsc::error::SendError;

#[derive(thiserror::Error, Debug)]
pub enum DeviceMonitorError {
    #[error(transparent)]
    MqttClientError(#[from] mqtt_client::MqttClientError),

    #[error(transparent)]
    InvalidCollectdMeasurementError(#[from] crate::collectd_mapper::collectd::CollectdError),

    #[error(transparent)]
    InvalidThinEdgeJsonError(#[from] thin_edge_json::group::MeasurementGrouperError),

    #[error(transparent)]
    ThinEdgeJsonSerializationError(
        #[from] thin_edge_json::serialize::ThinEdgeJsonSerializationError,
    ),

    #[error(transparent)]
    BatchingError(#[from] SendError<thin_edge_json::group::MeasurementGrouper>),
}
