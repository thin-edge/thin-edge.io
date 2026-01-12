use anyhow::Context;
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
    Pong(Option<VersionInfo>),
    CreateKeyRequest(CreateKeyRequest),
    CreateKeyResponse(CreateKeyResponse),
    GetTokensUrisRequest,
    GetTokensUrisResponse(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionInfo {
    pub version: u16,
}

impl Frame1 {
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        match bytes.first() {
            Some(0x09) => {
                let version_info: Option<VersionInfo> = match serde_json::from_slice(&bytes[1..]) {
                    Ok(v) => v,
                    Err(e) if e.is_eof() => None,
                    Err(e) => return Err(e).context("failed to deserialize"),
                };
                Ok(Self::Pong(version_info))
            }

            Some(_) | None => postcard::from_bytes(bytes).context("failed to deserialize"),
        }
    }
}

/// An error that can be returned to the client by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError(pub String);

/// Snapshot tests for `Frame1` Serialize/Deserialize implementations.
///
/// This is to make sure that recent versions of TedgeP11Client can work with older versions of TedgeP11Server and vice
/// versa. The `input` bytes should not be updated when `Frame1` is updated: these are reference snapshots.
#[cfg(test)]
mod tests {
    use crate::pkcs11::CreateKeyParams;
    use crate::pkcs11::KeyTypeParams;
    use crate::pkcs11::SigScheme;
    use crate::service::SignatureAlgorithm;
    use crate::service::SignatureScheme;
    use crate::SecretString;

    use super::*;

    #[test]
    fn test_deserialize_error() {
        let input = vec![0, 1, 97];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(request, Frame1::Error(ProtocolError("a".to_string())));
    }

    #[test]
    fn test_deserialize_choose_scheme_request() {
        let input = vec![1, 0, 0, 0];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::ChooseSchemeRequest(ChooseSchemeRequest {
                offered: vec![],
                uri: None,
                pin: None,
            })
        );
    }

    #[test]
    fn test_deserialize_choose_scheme_request_2() {
        let input = vec![1, 1, 131, 8, 1, 3, 117, 114, 105, 1, 3, 112, 105, 110];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::ChooseSchemeRequest(ChooseSchemeRequest {
                offered: vec![SignatureScheme(
                    rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                )],
                uri: Some("uri".to_string()),
                pin: Some(SecretString::new("pin".to_string())),
            })
        );
    }

    #[test]
    fn test_deserialize_sign_request() {
        let input = vec![
            2, 4, 100, 97, 116, 97, 1, 3, 107, 101, 121, 1, 4, 100, 97, 116, 97,
        ];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::SignRequest(SignRequest {
                to_sign: b"data".to_vec(),
                uri: Some("key".to_string()),
                pin: Some(SecretString::new("data".to_string())),
            })
        );
    }

    #[test]
    fn test_deserialize_choose_scheme_response() {
        let input = vec![3, 1, 132, 16, 1];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::ChooseSchemeResponse(ChooseSchemeResponse {
                scheme: Some(SignatureScheme(rustls::SignatureScheme::RSA_PSS_SHA256)),
                algorithm: SignatureAlgorithm(rustls::SignatureAlgorithm::RSA),
            })
        );
    }

    #[test]
    fn test_deserialize_sign_response() {
        let input = vec![4, 6, 115, 105, 103, 110, 101, 100];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::SignResponse(SignResponse(b"signed".to_vec()))
        );
    }

    #[test]
    fn test_deserialize_sign_request_with_sigscheme() {
        let input = vec![
            5, 4, 100, 97, 116, 97, 1, 3, 107, 101, 121, 1, 0, 1, 4, 100, 97, 116, 97,
        ];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::SignRequestWithSigScheme(SignRequestWithSigScheme {
                to_sign: b"data".to_vec(),
                uri: Some("key".to_string()),
                sigscheme: Some(SigScheme::EcdsaNistp256Sha256),
                pin: Some(SecretString::new("data".to_string())),
            })
        );
    }

    #[test]
    fn test_deserialize_get_public_key_pem_request_none() {
        let input = vec![6, 0];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(request, Frame1::GetPublicKeyPemRequest(None));
    }

    #[test]
    fn test_deserialize_get_public_key_pem_request_some() {
        let input = vec![6, 1, 7, 112, 117, 98, 95, 117, 114, 105];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::GetPublicKeyPemRequest(Some("pub_uri".to_string()))
        );
    }

    #[test]
    fn test_deserialize_get_public_key_pem_response() {
        let input = vec![7, 3, 112, 101, 109];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(request, Frame1::GetPublicKeyPemResponse("pem".to_string()));
    }

    #[test]
    fn test_deserialize_ping() {
        let input = vec![8];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(request, Frame1::Ping);
    }

    #[test]
    fn test_deserialize_pong() {
        let input = vec![9];
        let request: Frame1 = Frame1::from_bytes(&input).unwrap();
        assert_eq!(request, Frame1::Pong(None));

        let version_info = br#"{"version": 1}"#;
        let mut input = input;
        input.extend(version_info);
        dbg!(&input);
        let request: Frame1 = Frame1::from_bytes(&input).unwrap();
        assert_eq!(request, Frame1::Pong(Some(VersionInfo { version: 1 })));
    }

    #[test]
    fn test_deserialize_create_key_request() {
        let input = vec![
            10, 4, 97, 115, 100, 102, 1, 128, 2, 6, 109, 121, 32, 107, 101, 121, 1, 3, 1, 2, 3, 1,
            3, 112, 105, 110,
        ];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::CreateKeyRequest(CreateKeyRequest {
                uri: "asdf".to_string(),
                params: CreateKeyParams {
                    key: KeyTypeParams::Ec { curve: 256 },
                    id: Some(vec![1, 2, 3]),
                    label: "my key".to_string(),
                },
                pin: Some(SecretString::new("pin".to_string())),
            })
        );
    }

    #[test]
    fn test_deserialize_create_key_response() {
        let input = vec![
            11, 7, 107, 101, 121, 32, 112, 101, 109, 7, 107, 101, 121, 32, 117, 114, 105,
        ];
        let frame: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            frame,
            Frame1::CreateKeyResponse(CreateKeyResponse {
                pem: "key pem".to_string(),
                uri: "key uri".to_string(),
            })
        );
    }

    #[test]
    fn test_deserialize_get_tokens_uris_request() {
        let input = vec![12];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(request, Frame1::GetTokensUrisRequest);
    }

    #[test]
    fn test_deserialize_get_tokens_uris_response() {
        let input = vec![13, 2, 1, 97, 1, 98];
        let request: Frame1 = postcard::from_bytes(&input).unwrap();
        assert_eq!(
            request,
            Frame1::GetTokensUrisResponse(vec!["a".to_string(), "b".to_string()])
        );
    }
}
