use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Context;
use camino::Utf8PathBuf;
use certificate::PemCertificate;
use certificate::ValidityStatus;
use std::time::Duration;
use tedge_config::TEdgeConfig;
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

    async fn execute(&self, _: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
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
    pub async fn show(cert_path: &Utf8PathBuf) -> Result<(), anyhow::Error> {
        let cmd = ShowCertCmd {
            cert_path: cert_path.clone(),
            minimum: humantime::parse_duration("30d")?,
            validity_check_only: false,
        };
        cmd.show_certificate().await
    }

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

/// Formats duration into a human-readable string shown
/// in years, days, hours, minutes and seconds.
/// Months are explicitly not used when due to the irregular
/// number of days per month where any estimation to calculate
/// how many months there are, skews other units such as hours.
///
/// Examples:
/// 364d 8h 42m 43s
/// 5y 219d 2h 59m
/// 2y
/// 2h 1m 3s
///
fn format_duration_ydhms(duration: Duration) -> String {
    let total_seconds = duration.as_secs();

    if total_seconds == 0 {
        return "0s".to_string();
    }

    const SECS_IN_MINUTE: u64 = 60;
    const SECS_IN_HOUR: u64 = 60 * SECS_IN_MINUTE;
    const SECS_IN_DAY: u64 = 24 * SECS_IN_HOUR;
    const SECS_IN_YEAR: u64 = 365 * SECS_IN_DAY;

    let years = total_seconds / SECS_IN_YEAR;
    let remaining_secs = total_seconds % SECS_IN_YEAR;
    let days = remaining_secs / SECS_IN_DAY;
    let remaining_secs = remaining_secs % SECS_IN_DAY;
    let hours = remaining_secs / SECS_IN_HOUR;
    let remaining_secs = remaining_secs % SECS_IN_HOUR;
    let minutes = remaining_secs / SECS_IN_MINUTE;
    let seconds = remaining_secs % SECS_IN_MINUTE;

    let mut parts = Vec::new();
    if years > 0 {
        parts.push(format!("{}y", years));
    }
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 {
        parts.push(format!("{}s", seconds));
    }
    parts.join(" ")
}

fn display_status(status: ValidityStatus, minimum: Duration) -> String {
    let text = match status {
        ValidityStatus::Valid { expired_in } if expired_in > minimum => {
            format!("VALID (expires in: {})", format_duration_ydhms(expired_in))
        }
        ValidityStatus::Valid { expired_in } => {
            format!(
                "EXPIRES SOON (expires in: {})",
                format_duration_ydhms(expired_in)
            )
        }
        ValidityStatus::Expired { since } => {
            format!("EXPIRED (since: {})", format_duration_ydhms(since))
        }
        ValidityStatus::NotValidYet { valid_in } => {
            format!(
                "NOT VALID YET (will be in: {})",
                format_duration_ydhms(valid_in)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_format_duration_dhms_zero() {
        assert_eq!(format_duration_ydhms(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn test_format_duration_dhms_milliseconds() {
        assert_eq!(format_duration_ydhms(Duration::from_millis(500)), "0s");
        assert_eq!(format_duration_ydhms(Duration::from_millis(1500)), "1s");
    }

    #[test]
    fn test_format_duration_dhms_seconds_only() {
        assert_eq!(format_duration_ydhms(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration_ydhms(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn test_format_duration_dhms_minutes_and_seconds() {
        assert_eq!(format_duration_ydhms(Duration::from_secs(60)), "1m");
        assert_eq!(format_duration_ydhms(Duration::from_secs(65)), "1m 5s");
        assert_eq!(format_duration_ydhms(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn test_format_duration_dhms_hours_minutes_seconds() {
        assert_eq!(format_duration_ydhms(Duration::from_secs(3600)), "1h");
        assert_eq!(format_duration_ydhms(Duration::from_secs(3660)), "1h 1m");
        assert_eq!(format_duration_ydhms(Duration::from_secs(3665)), "1h 1m 5s");
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(3600 + 60 * 2 + 5)),
            "1h 2m 5s"
        );
    }

    #[test]
    fn test_format_duration_dhms_days_hours_minutes_seconds() {
        assert_eq!(format_duration_ydhms(Duration::from_secs(86400)), "1d");
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(86400 + 3600)),
            "1d 1h"
        );
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(86400 + 3600 + 60)),
            "1d 1h 1m"
        );
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(2 * 86400 + 3 * 3600 + 4 * 60 + 5)),
            "2d 3h 4m 5s"
        );
    }

    #[test]
    fn test_format_duration_dhms_many_days() {
        // 35 days, 5 hours, 30 minutes
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(35 * 86400 + 5 * 3600 + 30 * 60)),
            "35d 5h 30m"
        );
        // Exactly 30 days
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(30 * 86400)),
            "30d"
        );
    }

    #[test]
    fn test_format_duration_dhms_many_years() {
        // 5 years, 219 days, 2 hours, 59 minutes
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(
                (5 * 365 * 86400) + 219 * 86400 + 2 * 3600 + 59 * 60
            )),
            "5y 219d 2h 59m"
        );
        // Exactly 2 years
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(2 * 365 * 86400)),
            "2y"
        );
    }

    #[test]
    fn test_format_duration_dhms_less_than_a_day() {
        assert_eq!(
            format_duration_ydhms(Duration::from_secs(23 * 3600 + 59 * 60 + 59)),
            "23h 59m 59s"
        );
    }
}
