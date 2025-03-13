use std::fmt;
use tedge_config::tedge_toml::ProfileName;

/// An enumeration of all supported system services.
#[derive(Debug, Copy, Clone, strum_macros::IntoStaticStr)]
pub enum SystemService<'a> {
    #[strum(serialize = "mosquitto")]
    /// Mosquitto broker
    Mosquitto,
    #[strum(serialize = "tedge-mapper-az")]
    /// Azure TEdge mapper
    TEdgeMapperAz(Option<&'a ProfileName>),
    #[strum(serialize = "tedge-mapper-aws")]
    /// AWS TEdge mapper
    TEdgeMapperAws(Option<&'a ProfileName>),
    #[strum(serialize = "tedge-mapper-c8y")]
    /// Cumulocity TEdge mapper
    TEdgeMapperC8y(Option<&'a ProfileName>),
    #[strum(serialize = "tedge-agent")]
    /// TEdge SM agent
    TEdgeSMAgent,
}

impl fmt::Display for SystemService<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mosquitto => write!(f, "mosquitto"),
            Self::TEdgeMapperAz(None) => write!(f, "tedge-mapper-az"),
            Self::TEdgeMapperAz(Some(profile)) => write!(f, "tedge-mapper-az@{profile}"),
            Self::TEdgeMapperAws(None) => write!(f, "tedge-mapper-aws"),
            Self::TEdgeMapperAws(Some(profile)) => write!(f, "tedge-mapper-aws@{profile}"),
            Self::TEdgeMapperC8y(None) => write!(f, "tedge-mapper-c8y"),
            Self::TEdgeMapperC8y(Some(profile)) => write!(f, "tedge-mapper-c8y@{profile}"),
            Self::TEdgeSMAgent => write!(f, "tedge-agent"),
        }
    }
}
