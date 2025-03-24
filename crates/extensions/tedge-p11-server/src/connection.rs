//! A connection between tedge-p11-server and client that provides a way to send and receive frames.
//!
//! Because connecting to the UNIX socket is very cheap, we can use a simple approach of only
//! sending one request/response per connection.

use std::io::Read;
use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

use anyhow::Context;
use serde::Deserialize;
use serde::Serialize;
use tracing::warn;

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
    pub fn read_frame(&mut self) -> anyhow::Result<Frame1> {
        let mut buf = Vec::new();
        self.stream.read_to_end(&mut buf)?;

        // will fail if not version 1
        let frame: Frame =
            postcard::from_bytes(&buf).context("Failed to parse the received frame")?;

        let Frame::Version1(frame) = frame;

        // by that time the sender should've already closed this connection half, so we ignore
        // ENOTCONN that can possibly be returned on some platforms (MacOS?)
        let _ = self.stream.shutdown(Shutdown::Read);

        Ok(frame)
    }

    /// Writes a frame and closes the writing half of the connection.
    ///
    /// NOTE: can only be called once
    pub fn write_frame(&mut self, frame: &Frame1) -> anyhow::Result<()> {
        let frame = Frame::Version1(frame.clone());

        let buf = postcard::to_allocvec(&frame).context("Failed to serialize message")?;

        self.stream.write_all(&buf)?;
        self.stream.flush()?;

        // shutdown sends an EOF, which is important
        if let Err(err) = self.stream.shutdown(Shutdown::Write) {
            warn!("Failed to shutdown connection writing half: {err:?}");
        }

        Ok(())
    }
}

/// The actual frame that we serialize and send/receive.
///
/// This essentially just adds a version tag and should deal with cases when non-backwards
/// compatible changes are added to new versions.
///
/// For example, current connection semantics is one request/response per connection (client
/// connects, sends request and closes sending half, server reads, sends response and closes sending
/// half, etc.) but if we wanted to move away from that model, we can very easily because the
/// version is the first byte sent by the client so maintaining compatibility should be easy.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum Frame {
    Version1(Frame1),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame1 {
    Error(ProtocolError),
    ChooseSchemeRequest(ChooseSchemeRequest),
    SignRequest(SignRequest),
    ChooseSchemeResponse(ChooseSchemeResponse),
    SignResponse(SignResponse),
}

/// An error that can be returned to the client by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError(pub String);
