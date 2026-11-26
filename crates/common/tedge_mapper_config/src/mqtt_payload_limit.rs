use std::fmt;
use std::str::FromStr;

/// The largest payload MQTT allows, in bytes
const MQTT_MAX_PAYLOAD_SIZE: u32 = 268435455;

/// A limit on the size of MQTT payloads, between 1 byte and the MQTT maximum
#[derive(Debug, Clone, Copy, PartialEq, Eq, facet::Facet)]
#[facet(proxy = u32)]
pub struct MqttPayloadLimit(pub u32);

impl TryFrom<u32> for MqttPayloadLimit {
    type Error = String;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 0 || value > MQTT_MAX_PAYLOAD_SIZE {
            return Err(invalid(&value.to_string()));
        }
        Ok(Self(value))
    }
}

impl From<&MqttPayloadLimit> for u32 {
    fn from(value: &MqttPayloadLimit) -> Self {
        value.0
    }
}

impl FromStr for MqttPayloadLimit {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse::<u32>().map_err(|_| invalid(value))?.try_into()
    }
}

impl fmt::Display for MqttPayloadLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

tedge_config_engine::impl_single_value_append_remove!(MqttPayloadLimit);

fn invalid(value: &str) -> String {
    format!(
        "Invalid MQTT payload size limit: {value}. Provide a value between 1 and {MQTT_MAX_PAYLOAD_SIZE} bytes"
    )
}
