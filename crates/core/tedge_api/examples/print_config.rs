use std::collections::HashMap;

use nu_ansi_term::Color;
use pretty::Arena;
use tedge_api::{
    config::{AsConfig, ConfigDescription, ConfigKind},
    Config,
};
struct Port(u64);

impl AsConfig for Port {
    fn as_config() -> ConfigDescription {
        ConfigDescription::new(
            String::from("Integer"),
            ConfigKind::Integer,
            Some("A TCP port number is an integer between 0 and 65535"),
        )
    }
}

struct VHost;

impl AsConfig for VHost {
    fn as_config() -> ConfigDescription {
        ConfigDescription::new(
            String::from("VHost"),
            ConfigKind::Struct(vec![("name", None, String::as_config())]),
            Some("A virtual host definition"),
        )
    }
}

fn main() {
    let arena = Arena::new();

    let doc = Vec::<String>::as_config();
    let rendered_doc = doc.as_terminal_doc(&arena);

    let mut output = String::new();

    rendered_doc.render_fmt(80, &mut output).unwrap();

    println!(
        "------- Output for {}",
        std::any::type_name::<Vec<String>>()
    );
    println!("{}", output);

    let arena = Arena::new();

    let doc = ConfigDescription::new(
            String::from("ServerConfig"),
            ConfigKind::Struct(vec![
                ("port", None, Port::as_config()),
                ("interface", None, String::as_config()),
                ("virtual_hosts", None, Vec::<VHost>::as_config()),
                ("headers", None, HashMap::<String, String>::as_config()),
            ]),
            Some("Specify how the server should be started\n\n## Note\n\nThis is a reallly really loooooooooooooooooong loooooooooooooooooooong new *line*."),
        );
    let rendered_doc = doc.as_terminal_doc(&arena);

    let mut output = String::new();

    rendered_doc.render_fmt(80, &mut output).unwrap();

    println!(
        "Configuration for {} plugin kinds",
        Color::White.bold().paint(doc.name())
    );
    println!(
        "{}",
        Color::White.dimmed().bold().paint(format!(
            "=================={}=============",
            std::iter::repeat('=')
                .take(doc.name().len())
                .collect::<String>()
        ))
    );
    println!("------- Output for ServerConfig");
    println!("{}", output);
    let arena = Arena::new();

    #[derive(Config)]
    #[config(tag = "type")]
    /// An Nginx virtual host
    ///
    /// # Note
    ///
    /// This is an example and as such is nonsense
    enum NginxVHost {
        /// A simple host consisting of a string
        Simple(String),
        /// A more complex host that can also specify its port
        Complex {
            /// the name of the VHost
            name: String,
            port: Port,
        },
        UndocumentedComplex {
            num: u16,
            foo: f32,
        },
    }

    #[derive(Config)]
    #[config(untagged)]
    enum DebugLevel {
        /// Enables debug output
        ///
        /// And info of course
        Debug,
        /// Only pertinent information will be logged
        Info,
        /// A custom debug level
        Custom(String),
    }

    #[derive(Config)]
    struct NginxConfig {
        vhosts: Vec<NginxVHost>,
        debug_level: DebugLevel,
        allow_priv_ports: bool,
    }

    let doc = NginxConfig::as_config();
    let rendered_doc = doc.as_terminal_doc(&arena);

    let mut output = String::new();

    rendered_doc.render_fmt(80, &mut output).unwrap();

    println!("------- Output for NginxConfig");
    println!(
        "Configuration for {} plugin kinds",
        Color::White.bold().paint(doc.name())
    );
    println!(
        "{}",
        Color::White.dimmed().bold().paint(format!(
            "=================={}=============",
            std::iter::repeat('=')
                .take(doc.name().len())
                .collect::<String>()
        ))
    );
    println!("{}", output);
    println!("-------");
}
