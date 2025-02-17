pub use self::cli::TEdgeCertCli;
use std::io::Read;
use std::path::Path;

mod c8y;
mod cli;
mod create;
mod create_csr;
mod error;
mod remove;
mod renew;
mod show;

pub use self::cli::*;
pub use self::create::*;
pub use self::error::*;

pub(crate) fn read_cert_to_string(path: impl AsRef<Path>) -> Result<String, CertError> {
    let mut file = std::fs::File::open(path.as_ref()).map_err(|err| {
        let path = path.as_ref().display().to_string();
        CertError::CertificateReadFailed(err, path)
    })?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    Ok(content)
}
