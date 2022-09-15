mod test_env;

#[cfg(test)]
mod tests {
    use crate::*;
    use assert_matches::*;
    use rexpect::error::*;
    use rexpect::process::signal::Signal;
    use rexpect::process::wait::WaitStatus;
    use rexpect::*;

    macro_rules! execute_cmd {
        ($($arg:tt)*) => {spawn(&format!($($arg)*), TIMEOUT_MS)?.process.wait()?}
    }

    macro_rules! spawn_cmd {
        ($($arg:tt)*) => {spawn(&format!($($arg)*), TIMEOUT_MS)?}
    }

    const TIMEOUT_MS: Option<u64> = Some(5_000);

    #[test]
    fn it_works() -> Result<(), Error> {
        let mut sub = spawn("tedge mqtt sub test/topic", TIMEOUT_MS)?;

        execute(r#"tedge mqtt pub test/topic "hello thin-edge""#)?;
        assert_eq!(sub.read_line()?, "INFO: Connected");
        assert_eq!(sub.read_line()?, "[test/topic] hello thin-edge");

        execute(r#"tedge mqtt pub test/topic bye-bye --qos 2"#)?;
        assert_eq!(sub.read_line()?, "[test/topic] bye-bye");

        sub.process.kill(Signal::SIGTERM)?;
        Ok(())
    }

    #[test]
    fn tedge_cert_upload_request_a_password() -> Result<(), Error> {
        // Create a certificate
        execute("tedge config set device.cert.path /tmp/test-device.cert")?;
        execute("tedge config set device.key.path /tmp/test-device.key")?;
        execute("tedge cert create --device-id test-device")?;

        // Make sure the certificate has been created
        let mut sub = spawn("tedge config get device.id", TIMEOUT_MS)?;
        assert_eq!(sub.read_line()?, "test-device");
        sub.process.wait()?;

        // Upload the certificate
        execute("tedge config set c8y.url didier.latest.stage.c8y.io")?;
        let mut sub = spawn("tedge cert upload c8y --user foo-user", TIMEOUT_MS)?;
        sub.exp_string("Enter password: ")?;
        sub.send_line("foo-password")?;
        assert_eq!(sub.read_line()?, "");

        // Okay we provided a bad password
        assert_eq!(sub.read_line()?, "Error: failed to upload root certificate");
        assert_eq!(sub.read_line()?, "");
        assert_eq!(sub.read_line()?, "Caused by:");
        assert_eq!(sub.read_line()?, "    HTTP status client error (401 Unauthorized) for url (https://didier.latest.stage.c8y.io/tenant/currentTenant)");
        assert_matches!(sub.process.wait()?, WaitStatus::Exited(_, 1));

        Ok(())
    }

    #[test]
    fn tedge_cert_upload_c8y() -> Result<(), Error> {
        let c8y = test_env::C8YTestEnv::default();
        let url = c8y.url;
        let user = c8y.user;
        let pass = c8y.pass;

        // Create a certificate
        execute("tedge config set device.cert.path /tmp/test-device.cert")?;
        execute("tedge config set device.key.path /tmp/test-device.key")?;
        execute("tedge cert create --device-id test-device")?;

        // Configure tedge for c8y
        execute_cmd!("tedge config set c8y.url {url}");

        // Upload the password
        let mut sub = spawn_cmd!("tedge cert upload c8y --user {user}");
        sub.exp_string("Enter password: ")?;
        sub.send_line(&pass)?;
        assert_eq!(sub.read_line()?, "");
        assert_eq!(sub.read_line()?, "Certificate uploaded successfully.");
        assert_matches!(sub.process.wait()?, WaitStatus::Exited(_, 0));

        Ok(())
    }

    fn execute(cmd: &str) -> Result<(), Error> {
        spawn(cmd, TIMEOUT_MS)?.process.wait()?;
        Ok(())
    }
}
