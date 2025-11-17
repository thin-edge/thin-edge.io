use serde::Deserialize;
use serde::Serialize;

pub use super::frame1::Frame1;
pub use super::frame1::ProtocolError;

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
pub enum Frame {
    Version1(Frame1),
}
