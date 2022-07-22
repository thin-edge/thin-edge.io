/// An enumeration of all supported system services.
#[derive(Debug, Copy, Clone)]
pub enum SystemService {
    /// Mosquitto broker
    Mosquitto,
    /// AWS Tedge mapper
    TEdgeMapperAws,
    /// Azure TEdge mapper
    TEdgeMapperAz,
    /// Cumulocity TEdge mapper
    TEdgeMapperC8y,
    /// TEdge SM agent
    TEdgeSMAgent,
}

impl std::fmt::Display for SystemService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Mosquitto => "mosquitto",
            Self::TEdgeMapperAws => "tedge-mapper-aws",
            Self::TEdgeMapperAz => "tedge-mapper-az",
            Self::TEdgeMapperC8y => "tedge-mapper-c8y",
            Self::TEdgeSMAgent => "tedge-agent",
        };
        write!(f, "{}", s)
    }
}

impl SystemService {
    pub(crate) fn as_service_name(service: SystemService) -> &'static str {
        match service {
            SystemService::Mosquitto => "mosquitto",
            SystemService::TEdgeMapperAws => "tedge-mapper-aws",
            SystemService::TEdgeMapperAz => "tedge-mapper-az",
            SystemService::TEdgeMapperC8y => "tedge-mapper-c8y",
            SystemService::TEdgeSMAgent => "tedge-agent",
        }
    }
}
