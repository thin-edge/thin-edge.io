use serde::Deserialize;
use std::ops::Range;
use toml::de::ValueDeserializer;
use toml::ser::ValueSerializer;

pub fn toml_spanned(s: &str) -> toml::Spanned<String> {
    let mut value = String::new();
    serde::Serialize::serialize(&s, ValueSerializer::new(&mut value)).unwrap();
    let deser = ValueDeserializer::parse(&value).unwrap();
    <_>::deserialize(deser).unwrap()
}

pub fn extract_toml_span(value: &toml::Spanned<String>, span: Range<usize>) -> String {
    let mut output = String::new();
    serde::Serialize::serialize(&value, ValueSerializer::new(&mut output)).unwrap();
    output[span].to_owned()
}
