use super::*;
//use mqtt_client::{Client, Message, Topic, TopicFilter};
pub struct Azure {

}

impl Azure {
   pub fn get_azure_topics(config: &BridgeConfig)->Vec<String> {
        let pub_msg_topic =  format!("messages/events/ out 1 az/ devices/{}/",config.remote_clientid);
        let sub_msg_topic =  format!("messages/devicebound/# out 1 az/ devices/{}/",config.remote_clientid);
        let topics = vec![
            pub_msg_topic,
            sub_msg_topic,
            r##"$iothub/twin/res/# in 1"##.into(),
            r#"$iothub/twin/GET/?$rid=1 out 1"#.into(),
        ];
        topics
    }

// Here We check the az device twin properties over mqtt to check if connection has been open.
    // First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
    // device twin property output
    // Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID
    // The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}
    // Here if the status is 200 then its success
    
   /*
    #[tokio::main]
    async fn check_connection() -> Result<(), ConnectError> {

        const AZURE_TOPIC_DEVICETWIN_DOWNSTREAM: &str = r##"$iothub/twin/res/#"##;
        const AZURE_TOPIC_DEVICETWIN_UPSTREAM: &str = r#"$iothub/twin/GET/?$rid=1"#;
        const CLIENT_ID: &str = "check_connection";

        let template_pub_topic = Topic::new("$iothub/twin/GET/?$rid=1")?;
        let template_sub_filter = TopicFilter::new("$iothub/twin/res/200/?$rid=1")?;

        let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default()).await?;
        let mut device_twin_response = mqtt.subscribe(template_sub_filter).await?;

        //let (sender, receiver) = tokio::sync::oneshot::channel();

/*
        let _task_handle = tokio::spawn(async move {
            while let Some(message) = device_twin_response.next().await {
                println!("msg====>{:#?}",message);
                let _ = sender.send(true);
                break;

                if std::str::from_utf8(message.topic)
                    .unwrap_or("")
                    .contains("200")
                {
                    let _ = sender.send(true);
                    break;
                }
            }
        });

*/
        mqtt.publish(Message::new(&template_pub_topic, "")).await?;

        println!("-------waiting for response---------------");
                
        if let Some(message) = device_twin_response.next().await{
                println!("msg====>{:#?}",message);
        }
        /*
        match fut.await {
            Ok(Ok(true)) => {
                println!("Received message.");
            }
            _err => {
                return Err(ConnectError::BridgeConnectionFailed {cloud: String::from("azure")});
            }
        }
        */

        Ok(())
    }
*/

}



/*
use std::path::Path;
use std::time::Duration;

use structopt::StructOpt;
use tempfile::NamedTempFile;
use tokio::time::timeout;
use url::Url;

use crate::command::Command;
use crate::config::{
    TEdgeConfig, _AZURE_CONNECT, _AZURE_ROOT_CERT_PATH, _AZURE_URL, DEVICE_CERT_PATH,
    DEVICE_ID, DEVICE_KEY_PATH, TEDGE_HOME_DIR,
};
use crate::utils::{paths, services};
use mqtt_client::{Client, Message, Topic, TopicFilter};
use super::*;

const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 5;
const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "bridges";
const WAIT_FOR_CHECK_SECONDS: u64 = 10;


#[derive(StructOpt, Debug)]
pub struct Connect {}

impl Command for Connect {
    fn to_string(&self) -> String {
        "execute `tedge connect`.".into()
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        Ok(self.new_bridge()?)
    }
}

impl Connect {
    fn new_bridge(&self) -> Result<(), ConnectError> {
        println!("Checking if systemd and mosquitto are available.\n");
        let _ = services::all_services_available()?;

        println!("Checking if configuration for requested bridge already exists.\n");
        let _ = self.config_exists()?;

        println!("Creating configuration for requested bridge.\n");
        let config = self.load_config()?;

        println!("Saving configuration for requested bridge.\n");
        if let Err(err) = self.write_bridge_config_to_file(&config) {
            // We want to preserve previous errors and therefore discard result of this function.
            let _ = self.clean_up();
            return Err(err);
        }

        
        println!("Restarting mosquitto, [requires elevated permission], authorise when asked.\n");
        if let Err(err) = services::mosquitto_restart_daemon() {
            self.clean_up()?;
            return Err(err.into());
        }
        println!(
            "Awaiting mosquitto to start. This may take up to {} seconds.\n",
            MOSQUITTO_RESTART_TIMEOUT_SECONDS
        );
        std::thread::sleep(std::time::Duration::from_secs(
            MOSQUITTO_RESTART_TIMEOUT_SECONDS,
        ));

        println!(
            "Sending packets to check connection. This may take up to {} seconds.\n",
            WAIT_FOR_CHECK_SECONDS
        );
        self.check_connection()?;

        println!("Persisting mosquitto on reboot.\n");
        if let Err(err) = services::mosquitto_enable_daemon() {
            self.clean_up()?;
            return Err(err.into());
        }

        println!("Saving configuration.");
        self.save_az_config()?;

        println!("Successfully created bridge connection!");
        
       
        Ok(())
    }

    // To preserve error chain and not discard other errors we need to ignore error here
    // (don't use '?' with the call to this function to preserve original error).
    fn clean_up(&self) -> Result<(), ConnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            AZURE_CONFIG_FILENAME,
        ])?;
        let _ = std::fs::remove_file(&path).or_else(services::ok_if_not_found)?;

        Ok(())
    }

    // Here We check the az device twin properties over mqtt to check if connection has been open.
    // First the mqtt client will subscribe to a topic az/$iothub/twin/res/#, listen to the
    // device twin property output
    // Empty payload will be published to az/$iothub/twin/GET/?$rid=1, here 1 is request ID
    // The result will be published by the iothub on the az/$iothub/twin/res/{status}/?$rid={request id}
    // Here if the status is 200 then its success
    
    #[tokio::main]
    async fn check_connection(&self) -> Result<(), ConnectError> {

        const AZURE_TOPIC_DEVICETWIN_DOWNSTREAM: &str = r##"$iothub/twin/res/#"##;
        const AZURE_TOPIC_DEVICETWIN_UPSTREAM: &str = r#"$iothub/twin/GET/?$rid=1"#;
        const CLIENT_ID: &str = "check_connection";

        let template_pub_topic = Topic::new("$iothub/twin/GET/?$rid=1")?;
        let template_sub_filter = TopicFilter::new("$iothub/twin/res/200/?$rid=1")?;

        let mqtt = Client::connect(CLIENT_ID, &mqtt_client::Config::default()).await?;
        let mut device_twin_response = mqtt.subscribe(template_sub_filter).await?;

        //let (sender, receiver) = tokio::sync::oneshot::channel();

/*
        let _task_handle = tokio::spawn(async move {
            while let Some(message) = device_twin_response.next().await {
                println!("msg====>{:#?}",message);
                let _ = sender.send(true);
                break;

                if std::str::from_utf8(message.topic)
                    .unwrap_or("")
                    .contains("200")
                {
                    let _ = sender.send(true);
                    break;
                }
            }
        });

*/
        mqtt.publish(Message::new(&template_pub_topic, "")).await?;

        println!("-------waiting for response---------------");
                
        if let Some(message) = device_twin_response.next().await{
                println!("msg====>{:#?}",message);
        }
        /*
        match fut.await {
            Ok(Ok(true)) => {
                println!("Received message.");
            }
            _err => {
                return Err(ConnectError::BridgeConnectionFailed {cloud: String::from("azure")});
            }
        }
        */

        Ok(())
    }
    

    fn config_exists(&self) -> Result<(), ConnectError> {
        let path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            AZURE_CONFIG_FILENAME,
        ])?;

        if Path::new(&path).exists() {
            return Err(ConnectError::ConfigurationExists {cloud: String::from("az")});
        }

        Ok(())
    }

    fn load_config(&self) -> Result<Config, ConnectError> {
        Config::try_new_az()?.validate()
    }

    fn save_az_config(&self) -> Result<(), ConnectError> {
        let mut config = TEdgeConfig::from_default_config()?;
        TEdgeConfig::set_config_value(&mut config, _AZURE_CONNECT, "true".into())?;
        Ok(TEdgeConfig::write_to_default_config(&config)?)
    }

    fn write_bridge_config_to_file(&self, config: &Config) -> Result<(), ConnectError> {
        let mut temp_file = NamedTempFile::new()?;
        match config {
            Config::AZURE(az) => az.serialize(&mut temp_file)?,
        }

        let dir_path = paths::build_path_from_home(&[TEDGE_HOME_DIR, TEDGE_BRIDGE_CONF_DIR_PATH])?;

        // This will forcefully create directory structure if it doesn't exist, we should find better way to do it, maybe config should deal with it?
        let _ = paths::create_directories(&dir_path)?;

        let config_path = paths::build_path_from_home(&[
            TEDGE_HOME_DIR,
            TEDGE_BRIDGE_CONF_DIR_PATH,
            AZURE_CONFIG_FILENAME,
        ])?;

        let _ = paths::persist_tempfile(temp_file, &config_path)?;

        Ok(())
    }
}
#[derive(Debug, PartialEq)]
enum Config {
    AZURE(AzConfig),
}

impl Config {
    fn try_new_az() -> Result<Config, ConnectError> {
        Ok(Config::AZURE(AzConfig::try_new()?))
    }

    fn validate(self) -> Result<Config, ConnectError> {
        match self {
            Config::AZURE(config) => {
                config.validate()?;
                Ok(Config::AZURE(config))
            }

        }
    }
}


#[derive(Debug, PartialEq)]
struct AzConfig {
    connection: String,
    address: String,
    remote_username: String,
    bridge_cafile: String,
    remote_clientid: String,
    bridge_certfile: String,
    bridge_keyfile: String,
    try_private: bool,
    start_type: String,
    cleansession: bool,
    bridge_insecure: bool,
    notifications: bool,
    bridge_attempt_unsubscribe: bool,
    topics: Vec<String>,
}


/// Mosquitto config parameters required for AZURE bridge to be established:
/// # AZURE Bridge
/// connection edge_to_az
/// address mqtt.$AZURE_URL:8883
/// bridge_cafile $AZURE_CERT
/// remote_clientid $DEVICE_ID
/// remote_username $AZURE_USERNAME
/// bridge_certfile $CERT_PATH
/// bridge_keyfile $KEY_PATH
/// try_private false
/// start_type automatic
impl Default for AzConfig {
    fn default() -> AzConfig {
        AzConfig {
            connection: "edge_to_az".into(),
            address: "".into(),
            remote_username: "".into(),
            bridge_cafile: "".into(),
            remote_clientid: "alpha".into(),
            bridge_certfile: "".into(),
            bridge_keyfile: "".into(),
            try_private: false,
            start_type: "automatic".into(),
            cleansession: true,
            bridge_insecure: false,
            notifications: false,
            bridge_attempt_unsubscribe: false,
            topics: vec![
                // az JSON
                r##"$iothub/twin/res/# in 1"##.into(),
                r#"$iothub/twin/GET/?$rid=1 out 1"#.into(),

            ],
        }
    }
}

impl AzConfig {
    fn try_new() -> Result<AzConfig, ConnectError> {
        let config = TEdgeConfig::from_default_config()?;
        let address = get_config_value(&config, _AZURE_URL)?;

        let remote_clientid = get_config_value(&config, DEVICE_ID)?;
        let iothub_name: Vec<&str> = address.split(":").collect();  
        let remote_username = format!("{}",iothub_name.into_iter().nth(0).unwrap())+"/"+&remote_clientid.to_string()+"/?api-version=2018-06-30";

        let bridge_cafile = get_config_value(&config, _AZURE_ROOT_CERT_PATH)?;
        let bridge_certfile = get_config_value(&config, DEVICE_CERT_PATH)?;
        let bridge_keyfile = get_config_value(&config, DEVICE_KEY_PATH)?;
//        println!("the user name s {}", remote_username);

        Ok(AzConfig {
            connection: "edge_to_az".into(),
            address,
            bridge_cafile,
            remote_clientid,
            remote_username,
            bridge_certfile,
            bridge_keyfile,
            ..AzConfig::default()
        })
    }

    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        writeln!(writer, "### Bridge",)?;
        writeln!(writer, "connection {}", self.connection)?;
        writeln!(writer, "address {}", self.address)?;
        writeln!(writer, "bridge_cafile {}", self.bridge_cafile)?;
        writeln!(writer, "remote_clientid {}", self.remote_clientid)?;
        writeln!(writer, "remote_username {}", self.remote_username)?;
        writeln!(writer, "bridge_certfile {}", self.bridge_certfile)?;
        writeln!(writer, "bridge_keyfile {}", self.bridge_keyfile)?;
        writeln!(writer, "try_private {}", self.try_private)?;
        writeln!(writer, "start_type {}", self.start_type)?;
        writeln!(writer, "cleansession {}", self.cleansession)?;
        writeln!(writer, "bridge_insecure {}", self.bridge_insecure)?;
        writeln!(writer, "notifications {}", self.notifications)?;
        writeln!(writer, "bridge_attempt_unsubscribe {}", self.bridge_attempt_unsubscribe)?;
        let pub_msg_topic =  format!("messages/events/ out 1 az/ devices/{}/",self.remote_clientid);
        let sub_msg_topic =  format!("messages/devicebound/# out 1 az/ devices/{}/",self.remote_clientid);
        writeln!(writer, "\n### Topics",)?;
        for topic in &self.topics {
            writeln!(writer, "topic {}", topic)?;
        }
        writeln!(writer,"topic {}", pub_msg_topic)?;
        writeln!(writer,"topic {}", sub_msg_topic)?;
        Ok(())
    }

    fn validate(&self) -> Result<(), ConnectError> {
        Url::parse(&self.address)?;

        if !Path::new(&self.bridge_cafile).exists() {
            return Err(ConnectError::Certificate);
        }

        if !Path::new(&self.bridge_certfile).exists() {
            return Err(ConnectError::Certificate);
        }

        if !Path::new(&self.bridge_keyfile).exists() {
            return Err(ConnectError::Certificate);
        }

        Ok(())
    }
}

fn get_config_value(config: &TEdgeConfig, key: &str) -> Result<String, ConnectError> {
    Ok(config
        .get_config_value(key)?
        .ok_or_else(|| ConnectError::MissingRequiredConfigurationItem { item: key.into() })?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CORRECT_URL: &str = "http://test.com";
    const INCORRECT_URL: &str = "noturl";
    const INCORRECT_PATH: &str = "/path";

    #[test]
    fn config_az_validate_ok() {
        let ca_file = NamedTempFile::new().unwrap();
        let bridge_cafile = ca_file.path().to_str().unwrap().to_owned();

        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let key_file = NamedTempFile::new().unwrap();
        let bridge_keyfile = key_file.path().to_str().unwrap().to_owned();

        let config = Config::AZURE(AzConfig {
            address: CORRECT_URL.into(),
            bridge_cafile,
            bridge_certfile,
            bridge_keyfile,
            ..AzConfig::default()
        });

        assert!(config.validate().is_ok());
    }

    #[test]
    fn config_az_validate_wrong_url() {
        let config = Config::AZURE(AzConfig {
            address: INCORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..AzConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_az_validate_wrong_cert_path() {
        let config = Config::AZURE(AzConfig {
            address: CORRECT_URL.into(),
            bridge_certfile: INCORRECT_PATH.into(),
            bridge_keyfile: INCORRECT_PATH.into(),
            ..AzConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_az_validate_wrong_key_path() {
        let cert_file = NamedTempFile::new().unwrap();
        let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

        let config = Config::AZURE(AzConfig {
            address: CORRECT_URL.into(),
            bridge_certfile,
            bridge_keyfile: INCORRECT_PATH.into(),
            ..AzConfig::default()
        });

        assert!(config.validate().is_err());
    }

    #[ignore]
    fn bridge_config_az_create() {
        let mut bridge = AzConfig::default();

        bridge.bridge_cafile = "./test_root.pem".into();
        bridge.address = "test.test.io:8883".into();
        bridge.bridge_certfile = "./test-certificate.pem".into();
        bridge.bridge_keyfile = "./test-private-key.pem".into();

        let expected = AzConfig {
            bridge_cafile: "./test_root.pem".into(),
            address: "test.test.io:8883".into(),
            remote_username: r#"test.test.io:8883".into()/"alpha".into()/?api-version=2018-06-30"#.into(),
            bridge_certfile: "./test-certificate.pem".into(),
            bridge_keyfile: "./test-private-key.pem".into(),
            connection: "edge_to_az".into(),
            remote_clientid: "alpha".into(),
            try_private: false,
            start_type: "automatic".into(),
            topics: vec![
                // az JSON
                r#"messages/events/ out 1 az/ """#.into(),
            ],
        };

        assert_eq!(bridge, expected);
    }
}
*/
