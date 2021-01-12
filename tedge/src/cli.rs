use structopt::clap;
use structopt::StructOpt;

mod mqtt;

#[derive(StructOpt, Debug)]
#[structopt(
    name = clap::crate_name!(),
    version = clap::crate_version!(),
    about = clap::crate_description!()
)]
pub struct Opt {
    // The number of occurrences of the `v` flag
    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short, parse(from_occurrences))]
    verbose: u8,

    #[structopt(subcommand)]
    tedge_cmd: TEdgeCmd,
}

#[derive(StructOpt, Debug)]
pub enum TEdgeCmd {
    /// Configure Thin Edge.
    Config(ConfigCmd),

    /// Publish a message on a topic and subscribe a topic.
    Mqtt(MqttCmd),
}

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// List all.
    List,

    /// Add new value (overwrite the value if the key exists).
    Set { key: String, value: String },

    /// Remove value.
    Unset { key: String },

    /// Get value.
    Get { key: String },
}

#[derive(StructOpt, Debug)]
pub enum MqttCmd {
    /// Publish a MQTT message on a topic.
    Pub {
        /// Topic to publish
        topic: String,
        /// Message to publish
        message: String,
        ///  QoS level (0, 1, 2)
        #[structopt(short, long, parse(try_from_str = mqtt::parse_qos), default_value = "0")]
        qos: rumqttc::QoS,
    },

    /// Subscribe a MQTT topic.
    Sub {
        /// Topic to publish
        topic: String,
        /// QoS level (0, 1, 2)
        #[structopt(short, long, parse(try_from_str = mqtt::parse_qos), default_value = "0")]
        qos: rumqttc::QoS,
    },
}
