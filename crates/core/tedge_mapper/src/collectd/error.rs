use tokio::sync::mpsc::error::SendError;

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum DeviceMonitorError {
    #[error(transparent)]
    FromMqttClient(#[from] mqtt_channel::MqttError),

    #[error(transparent)]
    FromInvalidCollectdMeasurement(#[from] crate::collectd::collectd::CollectdError),

    #[error(transparent)]
    FromInvalidThinEdgeJson(#[from] tedge_api::group::MeasurementGrouperError),

    #[error(transparent)]
    FromThinEdgeJsonSerializationError(
        #[from] tedge_api::serialize::ThinEdgeJsonSerializationError,
    ),

    #[error(transparent)]
    FromBatchingError(#[from] SendError<tedge_api::group::MeasurementGrouper>),
}
