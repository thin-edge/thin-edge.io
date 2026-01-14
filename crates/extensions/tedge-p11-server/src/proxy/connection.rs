//! A connection between tedge-p11-server and client that provides a way to send and receive frames.
//!
//! Because connecting to the UNIX socket is very cheap, we can use a simple approach of only
//! sending one request/response per connection.

use std::io::Read;
use std::io::Write;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;

use anyhow::Context;
use tracing::warn;

use crate::proxy::frame::Frame2;
use crate::proxy::frame1::VersionInfo;
use crate::proxy::request::Request;
use crate::proxy::response::Response;

pub use super::frame::Frame;
pub use super::frame::Frame1;
pub use super::frame::ProtocolError;

pub struct Connection {
    stream: UnixStream,
    version_info: Option<VersionInfo>,
}

impl Connection {
    pub fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            version_info: None,
        }
    }

    /// Reads a frame and closes the reading half of the connection.
    ///
    /// NOTE: can only be called once
    pub fn read_frame(&mut self) -> anyhow::Result<Frame> {
        let mut buf = Vec::new();
        self.stream.read_to_end(&mut buf)?;

        let frame = match buf.first() {
            Some(0) => {
                self.version_info = Some(VersionInfo { version: 1 });
                Frame::Version1(postcard::from_bytes(&buf[1..])?)
            }
            Some(1) => {
                self.version_info = Some(VersionInfo { version: 2 });
                Frame::Version2(serde_json::from_slice(&buf[1..])?)
            }
            _ => anyhow::bail!("reeee"),
        };

        // by that time the sender should've already closed this connection half, so we ignore
        // ENOTCONN that can possibly be returned on some platforms (MacOS?)
        let _ = self.stream.shutdown(Shutdown::Read);

        Ok(frame)
    }

    pub fn write_error(&mut self, error: super::error::Error) -> anyhow::Result<()> {
        match self.version_info {
            None | Some(VersionInfo { version: 0..=1 }) => self.write_frame(&Frame::Version1(
                Frame1::Error(ProtocolError(error.to_string())),
            )),
            Some(VersionInfo { version: 2.. }) => self.write_frame2(&Frame2::Error(error)),
        }
    }

    pub fn write_response(&mut self, response: &Response) -> anyhow::Result<()> {
        match self.version_info {
            None | Some(VersionInfo { version: 0..=1 }) => {
                self.write_frame(&Frame::Version1(response.clone().into()))
            }
            Some(VersionInfo { version: 2.. }) => self.write_frame2(&response.clone().into()),
        }
    }

    pub fn write_request(&mut self, request: &Request) -> anyhow::Result<()> {
        match self.version_info {
            None | Some(VersionInfo { version: 0..=1 }) => {
                self.write_frame(&Frame::Version1(request.clone().into()))
            }
            Some(VersionInfo { version: 2.. }) => self.write_frame2(&request.clone().into()),
        }
    }

    /// Writes a frame and closes the writing half of the connection.
    ///
    /// NOTE: can only be called once
    pub fn write_frame(&mut self, frame: &Frame) -> anyhow::Result<()> {
        let buf = postcard::to_allocvec(&frame).context("Failed to serialize message")?;

        self.stream.write_all(&buf)?;
        self.stream.flush()?;

        // shutdown sends an EOF, which is important
        if let Err(err) = self.stream.shutdown(Shutdown::Write) {
            warn!("Failed to shutdown connection writing half: {err:?}");
        }

        Ok(())
    }

    pub fn write_frame2(&mut self, frame: &Frame2) -> anyhow::Result<()> {
        let mut buf = serde_json::to_vec(&frame).context("Failed to serialize message")?;
        // frame version 2
        buf.insert(0, 1);

        self.stream.write_all(&buf)?;
        self.stream.flush()?;

        // shutdown sends an EOF, which is important
        if let Err(err) = self.stream.shutdown(Shutdown::Write) {
            warn!("Failed to shutdown connection writing half: {err:?}");
        }

        Ok(())
    }
}
