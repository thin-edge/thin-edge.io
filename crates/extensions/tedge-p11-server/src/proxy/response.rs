// struct Request<T> {
//     request: T,
// }

use crate::{
    proxy::{
        frame::{Frame, Frame1},
        frame1::VersionInfo,
        frame2::Frame2,
    },
    service::{ChooseSchemeResponse, CreateKeyResponse, SignResponse},
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::enum_variant_names)]
pub enum Response {
    ChooseSchemeResponse(ChooseSchemeResponse),
    SignResponse(SignResponse),
    GetPublicKeyPemResponse(String),
    Pong(Option<VersionInfo>),
    CreateKeyResponse(CreateKeyResponse),
    GetTokensUrisResponse(Vec<String>),
}

// trait IntoResponse {
//     type Response;
// }

impl TryFrom<Frame> for Response {
    type Error = ();

    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        match frame {
            Frame::Version1(frame1) => match frame1 {
                Frame1::ChooseSchemeResponse(response) => {
                    Ok(Response::ChooseSchemeResponse(response))
                }
                Frame1::SignResponse(response) => Ok(Response::SignResponse(response)),
                Frame1::GetPublicKeyPemResponse(response) => {
                    Ok(Response::GetPublicKeyPemResponse(response))
                }
                Frame1::Pong(response) => Ok(Response::Pong(response)),
                Frame1::CreateKeyResponse(response) => Ok(Response::CreateKeyResponse(response)),
                Frame1::GetTokensUrisResponse(response) => {
                    Ok(Response::GetTokensUrisResponse(response))
                }
                _ => Err(()),
            },
            Frame::Version2(frame2) => match frame2 {
                Frame2::ChooseSchemeResponse(response) => {
                    Ok(Response::ChooseSchemeResponse(response))
                }
                Frame2::SignResponse(response) => Ok(Response::SignResponse(response)),
                Frame2::GetPublicKeyPemResponse(response) => {
                    Ok(Response::GetPublicKeyPemResponse(response))
                }
                Frame2::Pong(response) => Ok(Response::Pong(response)),
                Frame2::CreateKeyResponse(response) => Ok(Response::CreateKeyResponse(response)),
                Frame2::GetTokensUrisResponse(response) => {
                    Ok(Response::GetTokensUrisResponse(response))
                }
                _ => Err(()),
            },
        }
    }
}

impl From<Response> for Frame1 {
    fn from(response: Response) -> Self {
        match response {
            Response::ChooseSchemeResponse(resp) => Frame1::ChooseSchemeResponse(resp),
            Response::SignResponse(resp) => Frame1::SignResponse(resp),
            Response::GetPublicKeyPemResponse(resp) => Frame1::GetPublicKeyPemResponse(resp),
            Response::Pong(resp) => Frame1::Pong(resp),
            Response::CreateKeyResponse(resp) => Frame1::CreateKeyResponse(resp),
            Response::GetTokensUrisResponse(resp) => Frame1::GetTokensUrisResponse(resp),
        }
    }
}

impl From<Response> for Frame2 {
    fn from(response: Response) -> Self {
        match response {
            Response::ChooseSchemeResponse(resp) => Frame2::ChooseSchemeResponse(resp),
            Response::SignResponse(resp) => Frame2::SignResponse(resp),
            Response::GetPublicKeyPemResponse(resp) => Frame2::GetPublicKeyPemResponse(resp),
            Response::Pong(resp) => Frame2::Pong(resp),
            Response::CreateKeyResponse(resp) => Frame2::CreateKeyResponse(resp),
            Response::GetTokensUrisResponse(resp) => Frame2::GetTokensUrisResponse(resp),
        }
    }
}
