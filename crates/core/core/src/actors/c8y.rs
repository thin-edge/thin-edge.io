use actix::prelude::*;
use c8y_api::http_proxy::JwtAuthHttpProxy;
use c8y_smartrest::operations::Operations;
use tedge_config::{ConfigSettingAccessor, DeviceIdSetting, DeviceTypeSetting, TEdgeConfig};
use tedge_mapper::{c8y::converter::CumulocityConverter, core::size_threshold::SizeThreshold};
use tracing::debug;

use crate::messages::core::MqttPayload;

/// Platform specific actor which implements logic of handling the messages passed by a core.
#[derive(Debug)]
pub struct CumulocityActor {
    converter: CumulocityConverter<JwtAuthHttpProxy>,
}

impl CumulocityActor {
    pub fn try_new(
        config: TEdgeConfig,
        http_proxy: JwtAuthHttpProxy,
    ) -> Result<Self, anyhow::Error> {
        let size_threshold = SizeThreshold(16 * 1024);

        let operations = Operations::try_new("/etc/tedge/operations", "c8y")?;

        let device_name = config.query(DeviceIdSetting)?;
        let device_type = config.query(DeviceTypeSetting)?;

        Ok(Self {
            converter: CumulocityConverter::new(
                size_threshold,
                device_name,
                device_type,
                operations,
                http_proxy,
            ),
        })
    }
}

impl Actor for CumulocityActor {
    type Context = Context<Self>;
}

impl Handler<MqttPayload> for CumulocityActor {
    type Result = ();

    fn handle(&mut self, msg: MqttPayload, _ctx: &mut Self::Context) -> Self::Result {
        debug!("CumulocityActor: Handler: {msg:?}");

        let res = self
            .converter
            .try_convert_measurement(&msg.mqtt_msg)
            .unwrap();

        debug!("CumulocityActor: Converted: {res:?}");
    }
}
