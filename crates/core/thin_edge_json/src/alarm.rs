/// In-memory representation of ThinEdge JSON alarm.
#[derive(Debug)]
pub struct ThinEdgeAlarm {
    pub name: String,
    pub severity: AlarmSeverity,
    pub payload: Option<ThinEdgeAlarmJsonPayload>,
}

pub enum AlarmSeverity {
    critical,
    major,
    minor,
    warning,
}
/// In-memory representation of ThinEdge JSON alarm.
#[derive(Debug)]
pub struct ThinEdgeAlarmJsonPayload {
    pub message: String,
    pub status: AlarmStatus,
    pub timestamp: Option<DateTime<FixedOffset>>,
}
