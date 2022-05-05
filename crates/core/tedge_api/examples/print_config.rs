use std::collections::HashMap;

use nu_ansi_term::Color;
use pretty::Arena;
use tedge_api::config::{AsConfig, Config, ConfigKind};
struct Port(u64);

impl AsConfig for Port {
    fn as_config() -> Config {
        Config::new(
            String::from("Integer"),
            ConfigKind::Integer,
            Some("A TCP port number is an integer between 0 and 65535"),
        )
    }
}

struct VHost;

impl AsConfig for VHost {
    fn as_config() -> Config {
        Config::new(
            String::from("VHost"),
            ConfigKind::Struct(HashMap::from([(String::from("name"), String::as_config())])),
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

    let doc = Config::new(
            String::from("ServerConfig"),
            ConfigKind::Struct(HashMap::from([
                (String::from("port"), Port::as_config()),
                (String::from("interface"), String::as_config()),
                (String::from("virtual_hosts"), Vec::<VHost>::as_config()),
                (String::from("headers"), HashMap::<String, String>::as_config()),
            ])),
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
    println!("-------");
}
