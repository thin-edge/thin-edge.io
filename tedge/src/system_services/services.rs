/// An enumeration of all supported system services.
#[derive(Debug, Copy, Clone)]
pub enum SystemService {
    /// Mosquitto broker
    Mosquitto,
    /// Azure TEdge mapper
    TEdgeMapperAz,
    /// Cumulocity TEdge mapper
    TEdgeMapperC8y,
}

impl std::fmt::Display for SystemService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Mosquitto => "mosquitto",
            Self::TEdgeMapperAz => "tedge-mapper-az",
            Self::TEdgeMapperC8y => "tedge-mapper-c8y",
        };
        write!(f, "{}", s)
    }
}
