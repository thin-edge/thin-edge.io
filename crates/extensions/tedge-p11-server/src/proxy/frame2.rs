use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::{
    proxy::{error::Error, frame::ProtocolError, frame1::VersionInfo},
    service::{
        ChooseSchemeRequest, ChooseSchemeResponse, CreateKeyRequest, CreateKeyResponse,
        SignRequest, SignRequestWithSigScheme, SignResponse,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frame2 {
    Error(Error),
    ChooseSchemeRequest(ChooseSchemeRequest),
    SignRequest(SignRequest),
    ChooseSchemeResponse(ChooseSchemeResponse),
    SignResponse(SignResponse),
    SignRequestWithSigScheme(SignRequestWithSigScheme),
    GetPublicKeyPemRequest(Option<String>),
    GetPublicKeyPemResponse(String),
    Ping,
    Pong(Option<VersionInfo>),
    CreateKeyRequest(CreateKeyRequest),
    CreateKeyResponse(CreateKeyResponse),
    GetTokensUrisRequest,
    GetTokensUrisResponse(Vec<String>),
}

impl Frame2 {
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        serde_json::from_slice(bytes).context("failed to deserialize")
    }
}
