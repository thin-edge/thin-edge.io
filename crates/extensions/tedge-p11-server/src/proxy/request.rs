use crate::{
    proxy::{
        frame::{Frame, Frame1},
        frame2,
    },
    service::{ChooseSchemeRequest, CreateKeyRequest, SignRequest, SignRequestWithSigScheme},
};

// struct Request<T> {
//     request: T,
// }

#[expect(clippy::enum_variant_names)]
pub enum Request {
    ChooseSchemeRequest(ChooseSchemeRequest),
    SignRequest(SignRequest),
    SignRequestWithSigScheme(SignRequestWithSigScheme),
    GetPublicKeyPemRequest(Option<String>),
    Ping,
    CreateKeyRequest(CreateKeyRequest),
    GetTokensUrisRequest,
}

// trait IntoRequest {
//     type Response;
// }

impl TryFrom<Frame> for Request {
    type Error = ();

    fn try_from(frame: Frame) -> Result<Self, Self::Error> {
        match frame {
            Frame::Version1(frame1) => match frame1 {
                Frame1::ChooseSchemeRequest(request) => Ok(Request::ChooseSchemeRequest(request)),
                Frame1::SignRequest(request) => Ok(Request::SignRequest(request)),
                Frame1::SignRequestWithSigScheme(request) => {
                    Ok(Request::SignRequestWithSigScheme(request))
                }
                Frame1::GetPublicKeyPemRequest(uri) => Ok(Request::GetPublicKeyPemRequest(uri)),
                Frame1::Ping => Ok(Request::Ping),
                Frame1::CreateKeyRequest(request) => Ok(Request::CreateKeyRequest(request)),
                Frame1::GetTokensUrisRequest => Ok(Request::GetTokensUrisRequest),
                _ => Err(()),
            },
            Frame::Version2(frame2) => match frame2 {
                frame2::Frame2::ChooseSchemeRequest(request) => {
                    Ok(Request::ChooseSchemeRequest(request))
                }
                frame2::Frame2::SignRequest(request) => Ok(Request::SignRequest(request)),
                frame2::Frame2::SignRequestWithSigScheme(request) => {
                    Ok(Request::SignRequestWithSigScheme(request))
                }
                frame2::Frame2::GetPublicKeyPemRequest(uri) => {
                    Ok(Request::GetPublicKeyPemRequest(uri))
                }
                frame2::Frame2::Ping => Ok(Request::Ping),
                frame2::Frame2::CreateKeyRequest(request) => Ok(Request::CreateKeyRequest(request)),
                frame2::Frame2::GetTokensUrisRequest => Ok(Request::GetTokensUrisRequest),
                _ => Err(()),
            },
        }
    }
}
