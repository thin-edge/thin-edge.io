/// Extracts the subject common name from the PEM certificate at `cert_path`
///
/// Returns `Ok(None)` when the certificate does not exist, leaving the
/// derived key unset. Any other failure reports why the value could not
/// be derived.
pub fn certificate_common_name(cert_path: &str) -> Result<Option<String>, String> {
    let pem_data = match std::fs::read(cert_path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read certificate: {e}")),
    };
    let (_, pem) = x509_parser::pem::parse_x509_pem(&pem_data)
        .map_err(|_| "the file is not a PEM certificate".to_owned())?;
    let cert = pem
        .parse_x509()
        .map_err(|e| format!("the file is not a valid X.509 certificate: {e}"))?;
    let cn = cert
        .subject()
        .iter_common_name()
        .next()
        .ok_or("the certificate subject has no common name")?;
    let cn = cn
        .as_str()
        .map_err(|e| format!("the certificate common name is not a valid string: {e}"))?;
    Ok(Some(cn.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_certificate_yields_no_value() {
        assert_eq!(certificate_common_name("/nonexistent/cert.pem"), Ok(None));
    }

    #[test]
    fn invalid_certificate_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cert.pem");
        std::fs::write(&path, "not a certificate").unwrap();
        assert_eq!(
            certificate_common_name(path.to_str().unwrap()),
            Err("the file is not a PEM certificate".to_owned())
        );
    }
}
