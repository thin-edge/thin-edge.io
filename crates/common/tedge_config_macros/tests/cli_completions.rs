use std::ffi::OsString;

use camino::Utf8PathBuf;
use clap::CommandFactory;
use clap_complete::ArgValueCandidates;
use clap_complete::CompletionCandidate;
use tedge_config_macros::*;

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),
}

pub trait AppendRemoveItem {
    type Item;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item>;

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item>;
}

impl<T> AppendRemoveItem for T {
    type Item = T;

    fn append(_current_value: Option<Self::Item>, _new_value: Self::Item) -> Option<Self::Item> {
        unimplemented!()
    }

    fn remove(_current_value: Option<Self::Item>, _remove_value: Self::Item) -> Option<Self::Item> {
        unimplemented!()
    }
}

define_tedge_config! {
    #[tedge_config(deprecated_name = "azure")]
    az: {
        mapper: {
            /// The azure mapper timestamp
            timestamp: bool,
        }
    },
    device: {
        #[tedge_config(rename = "type")]
        /// The device type
        ty: bool,

        #[doku(as = "String")]
        /// The device type
        cert_path: Utf8PathBuf,
    }
}

#[derive(clap::Parser)]
enum ExampleCli {
    Read {
        #[clap(add = ArgValueCandidates::new(ReadableKey::completions))]
        key: ReadableKey,
    },
    Write {
        #[clap(add = ArgValueCandidates::new(WritableKey::completions))]
        key: WritableKey,
    },
}

#[test]
fn completion_returns_name_and_help_text() {
    let completions = completions_for("example read a");
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].get_value(), "az.mapper.timestamp");
    assert_eq!(
        completions[0].get_help().unwrap().to_string().trim(),
        "The azure mapper timestamp"
    );
}

#[test]
fn completion_returns_all_possible_matches() {
    let completions = completions_for("example read dev");
    let completion_values = completions
        .iter()
        .map(|c| c.get_value())
        .collect::<Vec<_>>();
    assert_eq!(completion_values, ["device.type", "device.cert_path"]);
}

#[test]
fn completions_are_generated_for_writable_keys() {
    let completions = completions_for("example write dev");
    let completion_values = completions
        .iter()
        .map(|c| c.get_value())
        .collect::<Vec<_>>();
    assert_eq!(completion_values, ["device.type", "device.cert_path"]);
}

/// Generates the [clap_complete] dynamic completions for an example cli invocation
///
/// The input looks like a shell string, e.g. `example read device.`
///
/// The output is the matching completions for the last argument
fn completions_for(args: &str) -> Vec<CompletionCandidate> {
    let args: Vec<_> = args.split_whitespace().map(OsString::from).collect();
    let index = args.len() - 1;
    clap_complete::engine::complete(&mut ExampleCli::command(), args, index, None).unwrap()
}
