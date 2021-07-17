//! A library to create [ThinEdgeJson][1] from bytes of json data by validating it.
//! [1]: https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/architecture/thin-edge-json.md

pub mod builder;
pub mod data;
pub mod group;
pub mod measurement;
pub mod parser;
pub mod serialize;
pub mod utils;
