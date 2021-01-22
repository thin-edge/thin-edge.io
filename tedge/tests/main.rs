mod tests {

    use assert_cmd::prelude::*; // Add methods on commands
    use predicates::prelude::*; // Used for writing assertions

    // Temporary workaround CIT-160
    const PATH: &'static str =
        "target/release:target/debug:/home/runner/work/thin-edge/thin-edge/target/debug";

    #[test]
    fn run_help() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = std::process::Command::new("tedge");
        cmd.env("PATH", PATH);

        cmd.arg("--help");
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("USAGE"));

        Ok(())
    }

    #[test]
    fn run_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = std::process::Command::new("tedge");
        cmd.env("PATH", PATH);

        let version_string = format!("tedge {}", env!("CARGO_PKG_VERSION"));
        cmd.arg("-V");
        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with(version_string));

        Ok(())
    }

    #[test]
    fn run_create_certificate() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempfile::tempdir().unwrap();
        let device_id = "test";
        let cert_path = temp_path(&tempdir, "test-cert.pem");
        let key_path = temp_path(&tempdir, "test-key.pem");

        let mut create_cmd = std::process::Command::new("tedge");
        create_cmd.env("PATH", PATH);
        create_cmd.args(&[
            "cert",
            "create",
            "--id",
            device_id,
            "--cert-path",
            &cert_path,
            "--key-path",
            &key_path,
        ]);

        let mut show_cmd = std::process::Command::new("tedge");
        show_cmd.env("PATH", PATH);
        show_cmd.args(&["cert", "show", "--cert-path", &cert_path]);

        let mut remove_cmd = std::process::Command::new("tedge");
        remove_cmd.env("PATH", PATH);
        remove_cmd.args(&[
            "cert",
            "remove",
            "--cert-path",
            &cert_path,
            "--key-path",
            &key_path,
        ]);

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
