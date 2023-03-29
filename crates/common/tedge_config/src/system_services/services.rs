/// An enumeration of all supported system services.
#[derive(Debug, Copy, Clone, strum_macros::Display, strum_macros::IntoStaticStr)]
pub enum SystemService {
    #[strum(serialize = "mosquitto")]
    /// Mosquitto broker
    Mosquitto,
    #[strum(serialize = "tedge-mapper-az")]
    /// Azure TEdge mapper
    TEdgeMapperAz,
    #[strum(serialize = "tedge-mapper-aws")]
    /// AWS TEdge mapper
    TEdgeMapperAws,
    #[strum(serialize = "tedge-mapper-c8y")]
    /// Cumulocity TEdge mapper
    TEdgeMapperC8y,
    #[strum(serialize = "tedge-agent")]
    /// TEdge SM agent
    TEdgeSMAgent,
}

impl SystemService {
    pub(crate) fn as_service_name(service: SystemService) -> &'static str {
        service.into()
    }
}
