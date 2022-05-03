use std::collections::HashMap;

use nu_ansi_term::Color;
use pretty::{Arena, Doc, DocAllocator, Pretty, RefDoc};
use serde::Serialize;
use termimad::MadSkin;

use crate::message::MessageType;

/// Generic config that represents what kind of config a plugin wishes to accept
#[derive(Debug, Serialize)]
pub struct Config {
    name: String,
    kind: ConfigKind,
    doc: Option<&'static str>,
}

impl Config {
    /// Construct a new generic config explanation
    #[must_use]
    pub fn new(name: String, kind: ConfigKind, doc: Option<&'static str>) -> Self {
        Self { name, kind, doc }
    }

    /// Get a reference to the config's documentation.
    #[must_use]
    pub fn doc(&self) -> Option<&'static str> {
        self.doc
    }

    /// Get a reference to the config's kind.
    #[must_use]
    pub fn kind(&self) -> &ConfigKind {
        &self.kind
    }

    /// Set or replace the documentation of this [`Config`]
    #[must_use]
    pub fn with_doc(mut self, doc: Option<&'static str>) -> Self {
        self.doc = doc;
        self
    }

    /// Get the config's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The specific kind a [`Config`] represents
#[derive(Debug, Serialize)]
pub enum ConfigKind {
    /// Config represents a boolean `true`/`false`
    Bool,

    /// Config represents an integer `1, 10, 200, 10_000, ...`
    ///
    /// # Note
    ///
    /// The maximum value that can be represented is between [`i64::MIN`] and [`i64::MAX`]
    Integer,

    /// Config represents a floating point value `1.0, 20.235, 3.1419`
    ///
    /// # Note
    /// Integers are also accepted and converted to their floating point variant
    ///
    /// The maximum value that can be represented is between [`f64::MIN`] and [`f64::MAX`]
    Float,

    /// Config represents a string
    String,

    /// Config represents an array of values of the given [`ConfigKind`]
    Array(Box<Config>),

    /// Config represents a map of different configurations
    Struct(HashMap<String, Config>),

    /// Config represents a hashmap of named configurations of the same type
    ///
    /// # Note
    ///
    /// The key is always a [`String`] so this only holds the value config
    HashMap(Box<Config>),
}

/// Turn a plugin configuration into a [`Config`] object
///
/// Plugin authors are expected to implement this for their configurations to give users
pub trait AsConfig {
    /// Get a [`Config`] object from the type
    fn as_config() -> Config;
}

impl<T: AsConfig> AsConfig for Vec<T> {
    fn as_config() -> Config {
        Config::new(
            format!("Array of '{}'s", T::as_config().name()),
            ConfigKind::Array(Box::new(T::as_config())),
            None,
        )
    }
}

impl<V: AsConfig> AsConfig for HashMap<String, V> {
    fn as_config() -> Config {
        Config::new(
            format!("Table of '{}'s", V::as_config().name()),
            ConfigKind::HashMap(Box::new(V::as_config())),
            None,
        )
    }
}

macro_rules! impl_config_kind {
    ($kind:expr; $name:expr; $doc:expr => $($typ:ty),+) => {
        $(
            impl AsConfig for $typ {
                fn as_config() -> Config {
                    Config::new({$name}.into(), $kind, Some($doc))
                }
            }
        )+
    };
}

impl_config_kind!(ConfigKind::Integer; "Integer"; "A signed integer with 64 bits" => u64, i64);
impl_config_kind!(ConfigKind::Float; "Float"; "A floating point value with 64 bits" => f64);
impl_config_kind!(ConfigKind::Bool; "Boolean"; "A boolean representing either true or false" => bool);
impl_config_kind!(ConfigKind::String; "String"; "An UTF-8 encoded string of characters" => String);

/******Pretty Printing of Configs******/

impl Config {
    /// Get a [`RcDoc`](pretty::RcDoc) which can be used to write the documentation of this
    pub fn as_terminal_doc<'a>(&'a self, arena: &'a Arena<'a>) -> RefDoc<'a> {
        let mut doc = arena
            .nil()
            .append(Color::LightBlue.bold().paint(self.name()).to_string())
            .append(arena.hardline());

        if let Some(conf_doc) = self.doc() {
            let skin = MadSkin::default_dark();
            let rendered = skin.text(&conf_doc, None).to_string();
            doc = doc.append(arena.intersperse(
                rendered.split("\n").map(|t| {
                    arena.intersperse(
                        t.split(char::is_whitespace).map(|t| t.to_string()),
                        arena.softline(),
                    )
                }),
                arena.hardline(),
            ));
        }

        match self.kind() {
            ConfigKind::Array(conf) => {
                doc = doc.append(Pretty::pretty(conf.as_terminal_doc(arena), arena))
            }
            ConfigKind::Struct(stc) => {
                doc = doc
                    .append(arena.hardline())
                    .append(Color::Blue.paint("[Members]").to_string())
                    .append(arena.hardline())
                    .append(arena.intersperse(
                        stc.iter().map(|(member_name, member_conf)| {
                            arena
                                .text(Color::Blue.bold().paint(member_name).to_string())
                                .append(": ")
                                .append(
                                    Pretty::pretty(member_conf.as_terminal_doc(arena), arena)
                                        .nest(4),
                                )
                        }),
                        Doc::hardline(),
                    ))
            }
            ConfigKind::HashMap(conf) => {
                doc = doc.append(Pretty::pretty(conf.as_terminal_doc(arena), arena))
            }
            _ => (),
        };

        doc.into_doc()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use nu_ansi_term::Color;
    use pretty::Arena;

    use crate::config::{AsConfig, Config, ConfigKind};

    #[test]
    fn verify_correct_config_kinds() {
        assert!(matches!(
            Vec::<f64>::as_config(),
            Config {
                doc: None,
                kind: ConfigKind::Array(x),
                ..
            } if matches!(x.kind(), ConfigKind::Float)
        ));

        let complex_config = HashMap::<String, Vec<HashMap<String, String>>>::as_config();
        println!("Complex config: {:#?}", complex_config);

        assert!(
            matches!(complex_config.kind(), ConfigKind::HashMap(map) if matches!(map.kind(), ConfigKind::Array(arr) if matches!(arr.kind(), ConfigKind::HashMap(inner_map) if matches!(inner_map.kind(), ConfigKind::String))))
        );
    }

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

    #[test]
    fn check_terminal_output() {
        let arena = Arena::new();

        let doc = Vec::<String>::as_config();
        let rendered_doc = doc.as_terminal_doc(&arena);

        let mut output = String::new();

        rendered_doc.render_fmt(80, &mut output).unwrap();

        println!("{}", output);
        println!("-------");

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
        println!("{}", output);
        println!("-------");
    }
}
