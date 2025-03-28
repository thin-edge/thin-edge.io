use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Context;
use camino::Utf8PathBuf;
use certificate::PemCertificate;
use certificate::ValidityStatus;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use yansi::Paint;

macro_rules! print_async {
    ($out:expr, $fmt:literal) => (
        let _ = $out.write_all($fmt.as_bytes()).await;
    );
    ($out:expr, $fmt:literal, $($arg:tt)*) => (
        let _ = $out.write_all(format!($fmt, $($arg)*).as_bytes()).await;
    );
}

/// Show the device certificate, if any
pub struct ShowCertCmd {
    /// The path where the device certificate is stored
    pub cert_path: Utf8PathBuf,

    /// Minimum validity duration bellow which a new certificate should be requested
    pub minimum: Duration,

    /// Only check the certificate validity
    pub validity_check_only: bool,
}

#[async_trait::async_trait]
impl Command for ShowCertCmd {
    fn description(&self) -> String {
        if self.validity_check_only {
            "check device validity".into()
        } else {
            "show the device certificate".into()
        }
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        if self.validity_check_only {
            let need_renewal = self.check_validity().await;
            match need_renewal {
                Ok(true) => Ok(()),
                Ok(false) => {
                    std::process::exit(1);
                }
                Err(err) => {
                    let mut stderr = tokio::io::stderr();
                    print_async!(stderr, "Cannot check the certificate: {:?}\n", err);
                    let _ = stderr.flush().await;
                    std::process::exit(2);
                }
            }
        } else {
            self.show_certificate().await?;
            Ok(())
        }
    }
}

impl ShowCertCmd {
    async fn read_certificate(&self) -> Result<PemCertificate, anyhow::Error> {
        let cert_path = &self.cert_path;
        let cert = tokio::fs::read_to_string(cert_path)
            .await
            .with_context(|| format!("reading certificate from {cert_path}"))?;
        let pem = PemCertificate::from_pem_string(&cert)
            .with_context(|| format!("decoding certificate from {cert_path}"))?;
        Ok(pem)
    }

    pub async fn show_certificate(&self) -> Result<(), anyhow::Error> {
        let pem = self.read_certificate().await?;
        let validity = pem.still_valid()?;

        let mut stdout = tokio::io::stdout();
        print_async!(stdout, "Certificate:   {}\n", self.cert_path);
        print_async!(stdout, "Subject:       {}\n", pem.subject()?);
        print_async!(stdout, "Issuer:        {}\n", pem.issuer()?);
        print_async!(
            stdout,
            "Status:        {}\n",
            display_status(validity, self.minimum)
        );
        print_async!(stdout, "Valid from:    {}\n", pem.not_before()?);
        print_async!(stdout, "Valid until:   {}\n", pem.not_after()?);
        print_async!(
            stdout,
            "Serial number: {} (0x{})\n",
            pem.serial()?,
            pem.serial_hex()?
        );
        print_async!(stdout, "Thumbprint:    {}\n", pem.thumbprint()?);
        let _ = stdout.flush().await;

        Ok(())
    }

    pub async fn check_validity(&self) -> Result<bool, anyhow::Error> {
        let pem = self.read_certificate().await?;
        let validity = pem.still_valid()?;

        let mut stderr = tokio::io::stderr();
        print_async!(
            stderr,
            "Status:        {}\n",
            display_status(validity, self.minimum)
        );
        let _ = stderr.flush().await;

        Ok(need_renewal(validity, self.minimum))
    }
}

fn display_status(status: ValidityStatus, minimum: Duration) -> String {
    let text = match status {
        ValidityStatus::Valid { expired_in } => {
            format!(
                "VALID (expires in: {})",
                humantime::format_duration(expired_in)
            )
        }
        ValidityStatus::Expired { since } => {
            format!("EXPIRED (since: {})", humantime::format_duration(since))
        }
        ValidityStatus::NotValidYet { valid_in } => {
            format!(
                "NOT VALID YET (will be in: {})",
                humantime::format_duration(valid_in)
            )
        }
    };

    match status {
        ValidityStatus::Valid { expired_in } if expired_in > minimum => {
            format!("{}", text.green())
        }
        ValidityStatus::Valid { .. } => format!("{}", text.yellow()),
        ValidityStatus::Expired { .. } => format!("{}", text.red()),
        ValidityStatus::NotValidYet { .. } => format!("{}", text.red()),
    }
}

fn need_renewal(status: ValidityStatus, minimum: Duration) -> bool {
    match status {
        ValidityStatus::Valid { expired_in } => expired_in <= minimum,
        ValidityStatus::Expired { .. } => true,
        ValidityStatus::NotValidYet { .. } => false,
    }
}
