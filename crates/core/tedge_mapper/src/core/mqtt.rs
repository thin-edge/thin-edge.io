use std::sync::Arc;
use tedge_config::all_or_nothing;
use tedge_config::models::proxy_scheme::ProxyScheme;
use tedge_config::TEdgeConfig;
use tedge_mqtt_bridge::rumqttc::Proxy;
use tedge_mqtt_bridge::rumqttc::ProxyAuth;
use tedge_mqtt_bridge::rumqttc::ProxyType;
use tedge_mqtt_bridge::rumqttc::TlsConfiguration;
use tedge_mqtt_bridge::MqttOptions;

pub fn configure_proxy(
    tedge_config: &TEdgeConfig,
    cloud_config: &mut MqttOptions,
) -> anyhow::Result<()> {
    let rustls_config = tedge_config.cloud_client_tls_config();
    let proxy_config = &tedge_config.proxy;
    if let Some(address) = proxy_config.address.or_none() {
        let credentials =
            all_or_nothing((proxy_config.username.clone(), proxy_config.password.clone()))
                .map_err(|e| anyhow::anyhow!(e))?;
        cloud_config.set_proxy(Proxy {
            addr: address.host().to_string(),
            port: address.port().0,
            auth: match credentials {
                Some((username, password)) => ProxyAuth::Basic { username, password },
                None => ProxyAuth::None,
            },
            ty: match address.scheme() {
                ProxyScheme::Http => ProxyType::Http,
                ProxyScheme::Https => {
                    ProxyType::Https(TlsConfiguration::Rustls(Arc::new(rustls_config)))
                }
            },
        });
    }
    Ok(())
}
