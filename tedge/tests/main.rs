mod tests {

    use predicates::prelude::*; // Used for writing assertions

    fn tedge_command<I, S>(args: I) -> Result<assert_cmd::Command, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let path: &str = "tedge";
        let mut cmd = assert_cmd::Command::cargo_bin(path)?;
        cmd.args(args);
        Ok(cmd)
    }

    #[test]
    fn run_help() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tedge_command(&["--help"])?;

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("USAGE"));

        Ok(())
    }

    #[test]
    fn run_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tedge_command(&["-V"])?;

        let version_string = format!("tedge {}", env!("CARGO_PKG_VERSION"));

        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with(version_string));

        Ok(())
    }

    #[test]
    fn run_create_certificate() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempfile::tempdir()?;
        let device_id = "test";
        let cert_path = temp_path(&tempdir, "test-cert.pem");
        let key_path = temp_path(&tempdir, "test-key.pem");

        let mut create_cmd = tedge_command(&[
            "cert",
            "create",
            "--id",
            device_id,
            "--cert-path",
            &cert_path,
            "--key-path",
            &key_path,
        ])?;

        let mut show_cmd = tedge_command(&["cert", "show", "--cert-path", &cert_path])?;

        let mut remove_cmd = tedge_command(&[
            "cert",
            "remove",
            "--cert-path",
            &cert_path,
            "--key-path",
            &key_path,
        ])?;

        // The remove command can be run when there is no certificate
        remove_cmd.assert().success();

        // The create command create a certificate
        create_cmd.assert().success();

        // The certificate use the device id as CN
        show_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("CN={},", device_id)));

        // When a certificate exists, it is not over-written by the create command
        create_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("A certificate already exists"));

        // The remove command remove the certificate
        remove_cmd.assert().success();

        // which can no more be displayed
        show_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("Missing file"));

        // The a new certificate can then be created.
        create_cmd.assert().success();

        Ok(())
    }

    fn temp_path(dir: &tempfile::TempDir, filename: &str) -> String {
        String::from(dir.path().join(filename).to_str().unwrap())
    }
}
