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
    Ok(parse_csr_contents(csr))
}

fn parse_csr_contents(csr: String) -> String {
    let pem_header = if csr.contains("-----BEGIN NEW CERTIFICATE REQUEST-----\n") {
        "NEW CERTIFICATE REQUEST"
    } else {
        "CERTIFICATE REQUEST"
    };
    // Don't assume that the CSR starts with the BEGIN block, as some tools
    // like gnutls certtool add details about the CSR in plain text by default.
    let csr = csr
        .split(format!("-----BEGIN {pem_header}-----\n").as_str())
        .last()
        .unwrap_or(&csr);
    let csr = csr
        .split(format!("-----END {pem_header}-----\n").as_str())
        .next()
        .unwrap_or(csr)
        .to_string();
    csr
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

    #[test]
    fn parse_csr_contents_from_tools() {
        // Computed using `$ gnutls-certtool --generate-request --template cert.template --load-privkey '<pkcs_url>' --load-pubkey tedge.pub`
        let gntls_certtool_csr = r#"
PKCS #10 Certificate Request Information:
	Version: 1
	Subject: CN=test001,OU=Test Device,O=Thin Edge
	Subject Public Key Algorithm: EC/ECDSA
	Algorithm Security Level: High (256 bits)
		Curve:	SECP256R1
		X:
			00:d3:18:d2:54:e4:1c:d8:d0:38:46:01:b3:e0:89:2c
			39:ce:09:b3:8f:23:4c:20:f6:b8:4d:4d:e1:1a:1e:b5
			a8
		Y:
			4f:84:a9:df:bf:97:f5:f6:8d:81:f4:1e:13:71:4b:6d
			40:52:e9:40:81:ba:e5:84:b6:38:0c:e4:90:cf:b4:3f
	Signature Algorithm: ECDSA-SHA256
	Attributes:
		Extensions:
			Basic Constraints (critical):
				Certificate Authority (CA): TRUE
			Key Usage (critical):
				Digital signature.
Other Information:
	Public Key ID:
		sha1:8b6aa6928b774d16fcb2bf967072b09ff68cd521
		sha256:4bae00d825d04602c4fed9fcec2887e5ae4a8b97d7f42580fbe24f9c72ef67ef
	Public Key PIN:
		pin-sha256:S64A2CXQRgLE/tn87CiH5a5Ki5fX9CWA++JPnHLvZ+8=

Self signature: verified

-----BEGIN NEW CERTIFICATE REQUEST-----
MIIBKTCB0AIBADA8MRIwEAYDVQQKEwlUaGluIEVkZ2UxFDASBgNVBAsTC1Rlc3Qg
RGV2aWNlMRAwDgYDVQQDEwd0ZXN0MDAxMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcD
QgAE0xjSVOQc2NA4RgGz4IksOc4Js48jTCD2uE1N4RoetahPhKnfv5f19o2B9B4T
cUttQFLpQIG65YS2OAzkkM+0P6AyMDAGCSqGSIb3DQEJDjEjMCEwDwYDVR0TAQH/
BAUwAwEB/zAOBgNVHQ8BAf8EBAMCB4AwCgYIKoZIzj0EAwIDSAAwRQIhALxYCCHa
9ZdaZCd7YhhWmVcq+/KSLPK/PUvfV83PDy5TAiAA/e9yrH6rrLGhkhEPtTbyBbBe
yzaWmqSb64bH/x0TjQ==
-----END NEW CERTIFICATE REQUEST-----
"#
        .to_string();

        // Computed using `$ gnutls-certtool --generate-request --no-text --template cert.template --load-privkey '<pkcs_url>' --load-pubkey tedge.pub`
        let gntls_certtool_csr_without_text = r#"
-----BEGIN NEW CERTIFICATE REQUEST-----
MIIBKTCB0AIBADA8MRIwEAYDVQQKEwlUaGluIEVkZ2UxFDASBgNVBAsTC1Rlc3Qg
RGV2aWNlMRAwDgYDVQQDEwd0ZXN0MDAxMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcD
QgAE0xjSVOQc2NA4RgGz4IksOc4Js48jTCD2uE1N4RoetahPhKnfv5f19o2B9B4T
cUttQFLpQIG65YS2OAzkkM+0P6AyMDAGCSqGSIb3DQEJDjEjMCEwDwYDVR0TAQH/
BAUwAwEB/zAOBgNVHQ8BAf8EBAMCB4AwCgYIKoZIzj0EAwIDSAAwRQIhALxYCCHa
9ZdaZCd7YhhWmVcq+/KSLPK/PUvfV83PDy5TAiAA/e9yrH6rrLGhkhEPtTbyBbBe
yzaWmqSb64bH/x0TjQ==
-----END NEW CERTIFICATE REQUEST-----
        "#
        .to_string();

        // Computed using `$ tedge cert renew --self-signed`
        let tedge_csr = r#"
-----BEGIN CERTIFICATE REQUEST-----
MIIBKTCB0AIBADA8MRIwEAYDVQQKEwlUaGluIEVkZ2UxFDASBgNVBAsTC1Rlc3Qg
RGV2aWNlMRAwDgYDVQQDEwd0ZXN0MDAxMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcD
QgAE0xjSVOQc2NA4RgGz4IksOc4Js48jTCD2uE1N4RoetahPhKnfv5f19o2B9B4T
cUttQFLpQIG65YS2OAzkkM+0P6AyMDAGCSqGSIb3DQEJDjEjMCEwDwYDVR0TAQH/
BAUwAwEB/zAOBgNVHQ8BAf8EBAMCB4AwCgYIKoZIzj0EAwIDSAAwRQIhALxYCCHa
9ZdaZCd7YhhWmVcq+/KSLPK/PUvfV83PDy5TAiAA/e9yrH6rrLGhkhEPtTbyBbBe
yzaWmqSb64bH/x0TjQ==
-----END CERTIFICATE REQUEST-----
        "#
        .to_string();

        let expected_contents = r#"
MIIBKTCB0AIBADA8MRIwEAYDVQQKEwlUaGluIEVkZ2UxFDASBgNVBAsTC1Rlc3Qg
RGV2aWNlMRAwDgYDVQQDEwd0ZXN0MDAxMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcD
QgAE0xjSVOQc2NA4RgGz4IksOc4Js48jTCD2uE1N4RoetahPhKnfv5f19o2B9B4T
cUttQFLpQIG65YS2OAzkkM+0P6AyMDAGCSqGSIb3DQEJDjEjMCEwDwYDVR0TAQH/
BAUwAwEB/zAOBgNVHQ8BAf8EBAMCB4AwCgYIKoZIzj0EAwIDSAAwRQIhALxYCCHa
9ZdaZCd7YhhWmVcq+/KSLPK/PUvfV83PDy5TAiAA/e9yrH6rrLGhkhEPtTbyBbBe
yzaWmqSb64bH/x0TjQ==
"#
        .to_string();

        assert_eq!(
            parse_csr_contents(gntls_certtool_csr).replace(['\n', '\r'], ""),
            expected_contents.replace(['\n', '\r'], "")
        );

        assert_eq!(
            parse_csr_contents(gntls_certtool_csr_without_text).replace(['\n', '\r'], ""),
            expected_contents.replace(['\n', '\r'], "")
        );

        assert_eq!(
            parse_csr_contents(tedge_csr).replace(['\n', '\r'], ""),
            expected_contents.replace(['\n', '\r'], "")
        );
    }
}
