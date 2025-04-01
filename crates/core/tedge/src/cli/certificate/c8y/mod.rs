mod download;
mod renew;
mod upload;

use crate::cli::certificate::create_csr::CreateCsrCmd;
use crate::override_public_key;
use crate::read_cert_to_string;
use crate::CertError;
use camino::Utf8PathBuf;
use certificate::CsrTemplate;
pub use download::DownloadCertCmd;
pub use renew::RenewCertCmd;
pub use upload::UploadCertCmd;

/// Create a device private key and CSR
///
/// Return the CSR in the format expected by c8y CA
async fn create_device_csr(
    common_name: String,
    key_path: Utf8PathBuf,
    csr_path: Utf8PathBuf,
    csr_template: CsrTemplate,
) -> Result<(), CertError> {
    let create_cmd = CreateCsrCmd {
        id: common_name,
        csr_path: csr_path.clone(),
        key_path,
        user: "tedge".to_string(),
        group: "tedge".to_string(),
        csr_template,
    };
    create_cmd.create_certificate_signing_request().await?;
    Ok(())
}

/// Return the CSR in the format expected by c8y CA
async fn read_csr_from_file(csr_path: &Utf8PathBuf) -> Result<String, CertError> {
    let csr = read_cert_to_string(csr_path).await?;
    let csr = csr
        .strip_prefix("-----BEGIN CERTIFICATE REQUEST-----\n")
        .unwrap_or(&csr);
    let csr = csr
        .strip_suffix("-----END CERTIFICATE REQUEST-----\n")
        .unwrap_or(csr)
        .to_string();
    Ok(csr)
}

/// Store the certificate received from c8y CA
///
/// The c8y CA being EST compliant, its response is encoded along PKCS#7
/// and the x509 certificate has to be extracted from this envelop.
async fn store_device_cert(cert_path: &Utf8PathBuf, pk7_base64: String) -> Result<(), CertError> {
    let x509_pem = pk7_to_x509(pk7_base64)?;

    override_public_key(cert_path, x509_pem).await
}

/// Extract the x509 certificates from a pkcs7 pem
///
/// EST returns certificates using
/// [application/pkcs7-mime;smime-type=certs-only](https://datatracker.ietf.org/doc/html/rfc5273.html#page-3).
/// Meaning the content is a:
/// - base64-encoded
/// - BER [SignedData object](https://datatracker.ietf.org/doc/html/rfc2315.html#section-9.1)
fn pk7_to_x509(pk7_base64: String) -> Result<String, IllFormedPk7Cert> {
    use base64::prelude::*;
    use rasn::ber;
    use rasn::der;
    use rasn_cms::ContentInfo;
    use rasn_cms::SignedData;

    let pk7_ber = BASE64_STANDARD.decode(pk7_base64.replace(['\n', '\r'], ""))?;
    let content_info = ber::decode::<ContentInfo>(&pk7_ber)?;
    let pk7 = ber::decode::<SignedData>(content_info.content.as_bytes())?;
    let x509_pem: Result<Vec<_>, IllFormedPk7Cert> = if let Some(certificates) = pk7.certificates {
        certificates
            .to_vec()
            .iter()
            .map(|cert| {
                der::encode(cert)
                    .map_err(|err| IllFormedPk7Cert::IllFormedCMS(format!("{err}")))
                    .map(|x509_der| pem::encode(&pem::Pem::new("CERTIFICATE", x509_der)))
            })
            .collect()
    } else {
        Err(IllFormedPk7Cert::MissingCertificate)
    };

    Ok(x509_pem?.join("\r\n"))
}

#[derive(thiserror::Error, Debug)]
pub enum IllFormedPk7Cert {
    #[error(transparent)]
    NotBase64(#[from] base64::DecodeError),

    #[error("Invalid pkcs#7 certificate: {0}")]
    IllFormedCMS(String),

    #[error("No certificate found in pkcs#7 content")]
    MissingCertificate,
}

impl From<rasn::error::DecodeError> for IllFormedPk7Cert {
    fn from(value: rasn::error::DecodeError) -> Self {
        IllFormedPk7Cert::IllFormedCMS(format!("{value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use certificate::PemCertificate;

    #[test]
    fn decode_certificate() {
        let pk7 = r#"
MIAGCSqGSIb3DQEHAqCAMIACAQExADALBgkqhkiG9w0BBwGggDCCAXkwggEgoAMC
AQICBgGVPZIizTAKBggqhkjOPQQDAjBCMRYwFAYDVQQGEw1Vbml0ZWQgU3RhdGVz
MRMwEQYDVQQKEwpDdW11bG9jaXR5MRMwEQYDVQQDEwptYW5hZ2VtZW50MB4XDTI1
MDIyNTE0NDU0MloXDTI2MDIyNDA5NDE0NFowRjEaMBgGA1UEAwwRZGlkaWVyLWRl
dmljZS0wMDExEjAQBgNVBAoMCVRoaW4gRWRnZTEUMBIGA1UECwwLVGVzdCBEZXZp
Y2UwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAATwSjNE/7AJZEtrXW2CP2LSLlcl
wDyh4YwHmpwDhnTCm+ZxeeXBUcUARcFXOtdmxMset9CgMQl1Fjw255dISpqiMAoG
CCqGSM49BAMCA0cAMEQCICapYBWyzrDU36IVEtyOfdlDA0bW9HE3pwHz2X9LAgl1
AiAD0naayxieH0RVE1vJtdD3iCJHrzLNM3Eff2gNOhuzJAAAMQAAAAAAAAA=
"#
        .to_string();

        // Computed using `openssl pkcs7 -print_certs`
        let expected_x509 = r#"
-----BEGIN CERTIFICATE-----
MIIBeTCCASCgAwIBAgIGAZU9kiLNMAoGCCqGSM49BAMCMEIxFjAUBgNVBAYTDVVu
aXRlZCBTdGF0ZXMxEzARBgNVBAoTCkN1bXVsb2NpdHkxEzARBgNVBAMTCm1hbmFn
ZW1lbnQwHhcNMjUwMjI1MTQ0NTQyWhcNMjYwMjI0MDk0MTQ0WjBGMRowGAYDVQQD
DBFkaWRpZXItZGV2aWNlLTAwMTESMBAGA1UECgwJVGhpbiBFZGdlMRQwEgYDVQQL
DAtUZXN0IERldmljZTBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABPBKM0T/sAlk
S2tdbYI/YtIuVyXAPKHhjAeanAOGdMKb5nF55cFRxQBFwVc612bEyx630KAxCXUW
PDbnl0hKmqIwCgYIKoZIzj0EAwIDRwAwRAIgJqlgFbLOsNTfohUS3I592UMDRtb0
cTenAfPZf0sCCXUCIAPSdprLGJ4fRFUTW8m10PeIIkevMs0zcR9/aA06G7Mk
-----END CERTIFICATE-----
"#
        .to_string();

        let x509 = pk7_to_x509(pk7).unwrap();
        let cert = PemCertificate::from_pem_string(&x509).unwrap();

        assert_eq!(
            x509.replace(['\n', '\r'], ""),
            expected_x509.replace(['\n', '\r'], "")
        );

        assert_eq!(
            cert.subject().unwrap(),
            "CN=didier-device-001, O=Thin Edge, OU=Test Device".to_string()
        );
        assert_eq!(
            cert.issuer().unwrap(),
            "C=United States, O=Cumulocity, CN=management".to_string()
        );
        assert_eq!(
            cert.not_before().unwrap(),
            "Tue, 25 Feb 2025 14:45:42 +0000".to_string()
        );
        assert_eq!(
            cert.not_after().unwrap(),
            "Tue, 24 Feb 2026 09:41:44 +0000".to_string()
        );
        assert_eq!(
            cert.thumbprint().unwrap(),
            "9C68C7EC9A860366FB8D2697C53B2543D9EA525C".to_string()
        );
    }
}
