use certificate::CertificateError;
use certificate::PemCertificate;

/// Returns the subject common name of the PEM certificate at `cert_path`, or
/// `None` if there is no certificate there yet
pub fn certificate_common_name(cert_path: &str) -> Result<Option<String>, String> {
    let cert = match PemCertificate::from_pem_file(cert_path) {
        Ok(cert) => cert,
        // Before `tedge cert create` runs there is no certificate, which leaves
        // the device id unset rather than broken
        Err(CertificateError::IoError { error, .. })
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(None)
        }
        Err(e) => return Err(e.to_string()),
    };
    cert.subject_common_name()
        .map(Some)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn a_certificate_path_that_cannot_be_opened_is_an_error() {
        let dir = TempTedgeDir::new();
        dir.file("not-a-dir");
        let cert_path = dir.path().join("not-a-dir/cert.pem");

        assert!(certificate_common_name(cert_path.as_str()).is_err());
    }
}
