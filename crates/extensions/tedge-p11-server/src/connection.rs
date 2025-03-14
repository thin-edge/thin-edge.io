//! A connection between tedge-p11-server and client that provides a way to send and receive frames.
//!
//! Because connecting to the UNIX socket is very cheap, we can use a simple approach of only
//! sending one request/response per connection.

use std::io::Read;
use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

use serde::Deserialize;
use serde::Serialize;

use crate::service::ChooseSchemeRequest;
use crate::service::ChooseSchemeResponse;
use crate::service::SignRequest;
use crate::service::SignResponse;

pub struct Connection {
    stream: UnixStream,
}

impl Connection {
    pub fn new(stream: UnixStream) -> Self {
        Self { stream }
    }

    /// Reads a frame and closes the reading half of the connection.
    ///
    /// NOTE: can only be called once
    pub fn read_frame(&mut self) -> anyhow::Result<Frame> {
        let mut buf = Vec::new();
        self.stream.read_to_end(&mut buf)?;
        let frame = postcard::from_bytes(&buf)?;
        self.stream.shutdown(Shutdown::Read)?;

        Ok(frame)
    }

    /// Writes a frame and closes the writing half of the connection.
    ///
    /// NOTE: can only be called once
    pub fn write_frame(&mut self, frame: &Frame) -> anyhow::Result<()> {
        let buf = postcard::to_allocvec(&frame)?;
        self.stream.write_all(&buf)?;
        self.stream.flush()?;
        self.stream.shutdown(Shutdown::Write)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// A version tag for possible future versions, currently ignored.
    // although i'm not sure if it's actually necessary now because we can always add it later if need be; it's because
    // we know that 1) we're having one connection/one call, so we read until EOF and know we have a valid message (so
    // don't have use a scheme like TLV) and 2) we can always add new fields
    pub version: Version,
    pub payload: Payload,
}

impl Frame {
    pub fn new(payload: Payload) -> Self {
        Self {
            version: Version::Version1,
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Version {
    Version1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Payload {
    ChooseSchemeRequest(ChooseSchemeRequest),
    SignRequest(SignRequest),
    ChooseSchemeResponse(ChooseSchemeResponse),
    SignResponse(SignResponse),
}
