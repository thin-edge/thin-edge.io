use serde::Deserialize;
use serde::Serialize;

use crate::service::ChooseSchemeRequest;
use crate::service::ChooseSchemeResponse;
use crate::service::CreateKeyRequest;
use crate::service::CreateKeyResponse;
use crate::service::SignRequest;
use crate::service::SignRequestWithSigScheme;
use crate::service::SignResponse;

/// The frame, which is serialized to a postcard tagged union: a sequence of a discriminant(varint32) and the value
/// matching the discriminant.
///
/// New fields can be added, but only at the end, because the discriminants have to remain stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame1 {
    Error(ProtocolError),
    ChooseSchemeRequest(ChooseSchemeRequest),
    SignRequest(SignRequest),
    ChooseSchemeResponse(ChooseSchemeResponse),
    SignResponse(SignResponse),
    SignRequestWithSigScheme(SignRequestWithSigScheme),
    GetPublicKeyPemRequest(Option<String>),
    GetPublicKeyPemResponse(String),
    Ping,
    Pong,
    CreateKeyRequest(CreateKeyRequest),
    CreateKeyResponse(CreateKeyResponse),
    GetTokensUrisRequest,
    GetTokensUrisResponse(Vec<String>),
}

/// An error that can be returned to the client by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError(pub String);
