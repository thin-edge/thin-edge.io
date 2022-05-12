#![allow(unused, dead_code)]

use pretty_assertions::assert_eq;
use tedge_api::{
    config::EnumVariantRepresentation, AsConfig, Config, ConfigDescription, ConfigKind,
};

/// Some Config
#[derive(Debug, Config)]
struct SimpleConfig {
    ///  The port to connect to
    port: Port,
    name: String,
    /// A nested configuration
    ///
    /// # This also includes markdown
    ///
    /// And can go over several _lines_
    nested: NestedConfig,
}

#[derive(Debug, Config)]
/// Nested configuration can have its own documentation
struct NestedConfig {
    num: EnumConfig,
}

#[derive(Debug, Config)]
struct Port(u16);

#[derive(Debug, Config)]
#[config(tag = "type")]
/// An enum configuration
enum EnumConfig {
    String(String),
    Num(u64),
    /// Some docs on the complex type
    Complex {
        /// The port of the inner complex type
        port: Port,
        other: String,
    },
}

#[derive(Debug, Config)]
#[config(untagged)]
enum UntaggedEnumConfig {
    One,
    Two,
}

#[test]
fn check_derive_config() {
    let conf = SimpleConfig::as_config();

    println!("{:#?}", conf);

    assert!(matches!(conf.kind(), ConfigKind::Struct(_)));
    assert_eq!(conf.doc().unwrap(), "Some Config");

    assert_eq!(Port::as_config().doc(), None);

    if let ConfigKind::Enum(_, variants) = EnumConfig::as_config().kind() {
        if let EnumVariantRepresentation::Wrapped(kind) = &variants[2].2 {
            if let ConfigKind::Struct(fields) = kind.kind() {
                assert_eq!(fields[0].1, Some("The port of the inner complex type"))
            } else {
                panic!()
            }
        } else {
            panic!()
        }
    } else {
        panic!();
    }
}
