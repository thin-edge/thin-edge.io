use std::collections::HashMap;

use nu_ansi_term::Color;
use pretty::{Arena, Doc, DocAllocator, Pretty, RefDoc};
use serde::Serialize;
use termimad::MadSkin;

/// Generic config that represents what kind of config a plugin wishes to accept
#[derive(Debug, Serialize, PartialEq)]
pub struct ConfigDescription {
    name: String,
    kind: ConfigKind,
    doc: Option<&'static str>,
}

impl ConfigDescription {
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

/// How an enum is represented
#[derive(Debug, Serialize, PartialEq)]
pub enum EnumVariantRepresentation {
    /// The enum is represented by a string
    ///
    /// This is the case with unit variants for example
    String(&'static str),
    /// The enum is represented by the value presented here
    Wrapped(Box<ConfigDescription>),
}

/// The kind of enum tagging used by the [`ConfigKind`]
#[derive(Debug, Serialize, PartialEq)]
pub enum ConfigEnumKind {
    /// An internal tag with the given tag name
    Tagged(&'static str),
    /// An untagged enum variant
    Untagged,
}

/// The specific kind a [`Config`] represents
#[derive(Debug, Serialize, PartialEq)]
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

    /// Wrap another config
    ///
    /// This is particularly useful if you want to restrict another kind. The common example is a
    /// `Port` config object which is represented as a `u16` but with an explanation of what it is
    /// meant to represent.
    Wrapped(Box<ConfigDescription>),

    /// Config represents an array of values of the given [`ConfigKind`]
    Array(Box<ConfigDescription>),

    /// Config represents a hashmap of named configurations of the same type
    ///
    /// # Note
    ///
    /// The key is always a [`String`] so this only holds the value config
    HashMap(Box<ConfigDescription>),

    /// Config represents a map of different configurations
    ///
    /// The tuple represent `(field_name, documentation, config_description)`
    Struct(Vec<(&'static str, Option<&'static str>, ConfigDescription)>),

    /// Config represents multiple choice of configurations
    Enum(
        ConfigEnumKind,
        Vec<(
            &'static str,
            Option<&'static str>,
            EnumVariantRepresentation,
        )>,
    ),
}

/// Turn a plugin configuration into a [`Config`] object
///
/// Plugin authors are expected to implement this for their configurations to give users
pub trait AsConfig {
    /// Get a [`Config`] object from the type
    fn as_config() -> ConfigDescription;
}

impl<T: AsConfig> AsConfig for Vec<T> {
    fn as_config() -> ConfigDescription {
        ConfigDescription::new(
            format!("Array of '{}'s", T::as_config().name()),
            ConfigKind::Array(Box::new(T::as_config())),
            None,
        )
    }
}

impl<V: AsConfig> AsConfig for HashMap<String, V> {
    fn as_config() -> ConfigDescription {
        ConfigDescription::new(
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
                fn as_config() -> ConfigDescription {
                    ConfigDescription::new({$name}.into(), $kind, Some($doc))
                }
            }
        )+
    };
}

impl_config_kind!(ConfigKind::Integer; "Integer"; "A signed integer with 64 bits" => i64);
impl_config_kind!(ConfigKind::Integer; "Integer"; "An unsigned integer with 64 bits" => u64);

impl_config_kind!(ConfigKind::Integer; "Integer"; "A signed integer with 32 bits" => i32);
impl_config_kind!(ConfigKind::Integer; "Integer"; "An unsigned integer with 32 bits" => u32);

impl_config_kind!(ConfigKind::Integer; "Integer"; "A signed integer with 16 bits" => i16);
impl_config_kind!(ConfigKind::Integer; "Integer"; "An unsigned integer with 16 bits" => u16);

impl_config_kind!(ConfigKind::Integer; "Integer"; "A signed integer with 8 bits" => i8);
impl_config_kind!(ConfigKind::Integer; "Integer"; "An unsigned integer with 8 bits" => u8);

impl_config_kind!(ConfigKind::Float; "Float"; "A floating point value with 64 bits" => f64);
impl_config_kind!(ConfigKind::Float; "Float"; "A floating point value with 32 bits" => f32);

impl_config_kind!(ConfigKind::Bool; "Boolean"; "A boolean" => bool);
impl_config_kind!(ConfigKind::String; "String"; "An UTF-8 string" => String);

/******Pretty Printing of Configs******/

impl ConfigDescription {
    /// Get a [`RcDoc`](pretty::RcDoc) which can be used to write the documentation of this
    pub fn as_terminal_doc<'a>(&'a self, arena: &'a Arena<'a>) -> RefDoc<'a> {
        let mut doc = arena.nil();

        if !matches!(self.kind(), ConfigKind::Wrapped(_)) && self.doc().is_none() {
            doc = doc
                .append(Color::LightBlue.bold().paint(self.name()).to_string())
                .append(arena.space())
                .append(match self.kind() {
                    ConfigKind::Bool
                    | ConfigKind::Integer
                    | ConfigKind::Float
                    | ConfigKind::String
                    | ConfigKind::Wrapped(_)
                    | ConfigKind::Array(_)
                    | ConfigKind::HashMap(_) => arena.nil(),
                    ConfigKind::Struct(_) => {
                        arena.text(Color::Blue.dimmed().paint("[Table]").to_string())
                    }
                    ConfigKind::Enum(_, _) => {
                        arena.text(Color::Green.dimmed().paint("[Enum]").to_string())
                    }
                })
                .append(arena.hardline());
        }

        let skin = MadSkin::default_dark();
        let render_markdown = |text: &str| {
            let rendered = skin.text(text, None).to_string();
            arena.intersperse(
                rendered.split("\n").map(|t| {
                    arena.intersperse(
                        t.split(char::is_whitespace).map(|t| t.to_string()),
                        arena.softline(),
                    )
                }),
                arena.hardline(),
            )
        };

        if let Some(conf_doc) = self.doc() {
            doc = doc.append(render_markdown(&conf_doc));
        }

        match self.kind() {
            ConfigKind::Bool | ConfigKind::Integer | ConfigKind::Float | ConfigKind::String => (),
            ConfigKind::Struct(stc) => {
                doc = doc
                    .append(arena.hardline())
                    .append(Color::Blue.paint("[Members]").to_string())
                    .append(arena.hardline())
                    .append(arena.intersperse(
                        stc.iter().map(|(member_name, member_doc, member_conf)| {
                            let mut doc = arena.nil();

                            if let Some(member_doc) = member_doc {
                                doc = doc.append(render_markdown(&member_doc));
                            }
                            doc.append(
                                arena.text(Color::Blue.bold().paint(*member_name).to_string()),
                            )
                            .append(": ")
                            .append(
                                Pretty::pretty(member_conf.as_terminal_doc(arena), arena).nest(4),
                            )
                        }),
                        Doc::hardline(),
                    ))
            }
            ConfigKind::Enum(enum_kind, variants) => {
                doc = doc
                    .append(arena.hardline())
                    .append(Color::Green.paint("One of:").to_string())
                    .append(arena.space())
                    .append(match enum_kind {
                        ConfigEnumKind::Tagged(tag) => arena.text(
                            Color::White
                                .dimmed()
                                .paint(format!(
                                    "[Tagged with {}]",
                                    Color::LightGreen
                                        .italic()
                                        .dimmed()
                                        .paint(format!("'{}'", tag))
                                ))
                                .to_string(),
                        ),
                        ConfigEnumKind::Untagged => {
                            arena.text(Color::White.dimmed().paint("[Untagged]").to_string())
                        }
                    })
                    .append(arena.hardline())
                    .append(
                        arena.intersperse(
                            variants
                                .iter()
                                .map(|(member_name, member_doc, member_conf)| {
                                    arena.text("-").append(arena.space()).append({
                                        let mut doc = arena
                                            .nil()
                                            .append(match member_conf {
                                                EnumVariantRepresentation::String(_) => arena.text(
                                                    Color::Green
                                                        .bold()
                                                        .paint(&format!(
                                                            "{:?}",
                                                            member_name.to_lowercase()
                                                        ))
                                                        .to_string(),
                                                ),
                                                EnumVariantRepresentation::Wrapped(_) => arena
                                                    .text(
                                                        Color::Green
                                                            .bold()
                                                            .paint(*member_name)
                                                            .to_string(),
                                                    ),
                                            })
                                            .append(": ");

                                        if let Some(member_doc) = member_doc {
                                            doc = doc.append(render_markdown(&member_doc));
                                        }

                                        doc.append(
                                            Pretty::pretty(
                                                match member_conf {
                                                    EnumVariantRepresentation::String(_) => {
                                                        arena.nil().into_doc()
                                                    }

                                                    EnumVariantRepresentation::Wrapped(
                                                        member_conf,
                                                    ) => arena
                                                        .text(
                                                            Color::LightRed
                                                                .paint("Is a: ")
                                                                .to_string(),
                                                        )
                                                        .append(member_conf.as_terminal_doc(arena))
                                                        .into_doc(),
                                                },
                                                arena,
                                            )
                                            .nest(4),
                                        )
                                        .nest(2)
                                    })
                                }),
                            Doc::hardline(),
                        ),
                    );
            }
            ConfigKind::Array(conf) => {
                doc = doc
                    .append(Color::LightRed.paint("Many of:").to_string())
                    .append(arena.space())
                    .append(conf.as_terminal_doc(arena));
            }
            ConfigKind::HashMap(conf) | ConfigKind::Wrapped(conf) => {
                doc = doc.append(conf.as_terminal_doc(arena));
            }
        };

        doc.into_doc()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::config::{AsConfig, ConfigDescription, ConfigKind};

    #[test]
    fn verify_correct_config_kinds() {
        assert!(matches!(
            Vec::<f64>::as_config(),
            ConfigDescription {
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
}
