use crate::collectd::CollectdError;
use mqtt_client::Error as MqttClientError;
use thin_edge_json::{
    group::{MeasurementGrouper, MeasurementGrouperError},
    serialize::ThinEdgeJsonSerializationError,
};
use tokio::sync::mpsc::error::SendError;

#[derive(thiserror::Error, Debug)]
pub enum DeviceMonitorError {
    #[error(transparent)]
    MqttClientError(#[from] MqttClientError),

    #[error(transparent)]
    InvalidCollectdMeasurementError(#[from] CollectdError),

    #[error(transparent)]
    InvalidThinEdgeJsonError(#[from] MeasurementGrouperError),

    #[error(transparent)]
    ThinEdgeJsonSerializationError(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    BatchingError(#[from] SendError<MeasurementGrouper>),

    #[error("Home directory is not found.")]
    HomeDirNotFound,
}
