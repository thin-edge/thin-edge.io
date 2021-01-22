// Don't run on arm builds, relevant bug @CIT-160 needs resolution before.
//#[cfg(not(target_arch = "arm"))]
mod tests {

    use assert_cmd::prelude::*; // Add methods on commands
    use predicates::prelude::*; // Used for writing assertions

    // Temporary workaround CIT-160
    const PATH: &'static str = "target/release:target/debug:/target/armv7-unknown-linux-gnueabihf/debug";

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
        let device_id = "test";
        let cert_path = temp_path("test-cert.pem");
        let key_path = temp_path("test-key.pem");

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
        create_cmd.env("PATH", PATH);
        show_cmd.args(&["cert", "show", "--cert-path", &cert_path]);

        let mut remove_cmd = std::process::Command::new("tedge");
        create_cmd.env("PATH", PATH);
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

        Ok(())
    }

    fn temp_path(filename: &str) -> String {
        let mut path = std::env::temp_dir();
        path.push(filename);
        path.to_str().unwrap_or(filename).into()
    }
}
