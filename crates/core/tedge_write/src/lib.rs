//! A binary used for writing to files which `tedge` user does not have write permissions for, using
//! sudo.
//!
//! tedge-agent handles a `config_update` operation, during which we need to overwrite configuration
//! files with newer versions. However, we run tedge-agent as `tedge` user, but some of these files
//! are not writeable by `tedge`. For this reason, we make use of sudo, adding a rule that allows
//! `tedge` to call `tedge-write` with root permissions in order to write to these files. When
//! handling operations where we need to write to user-owned files, tedge components will spawn a
//! `tedge-write` process in order to reduce the surface where the root permissions are used.
//!
//! This behaviour can be disabled by setting an `enable.sudo` flag to `false`. `tedge-write` will
//! then be ran with effective permissions of `tedge` user and group. With this configuration, it
//! will be necessary to modify affected files to be writable by `tedge` user or group.
//!
//! This crate consists of 2 parts:
//!
//! - an implementation of the `tedge-write` binary
//! - tedge-write API meant to be called by other tedge-components
//!
//! https://github.com/thin-edge/thin-edge.io/issues/2456

const TEDGE_WRITE_PATH: &str = "/usr/bin/tedge-write";

pub mod bin;

mod api;

pub use api::CopyOptions;
