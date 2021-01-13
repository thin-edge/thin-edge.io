// Don't run on arm builds, relevant bug @CIT-160 needs resolution before.
#[cfg(not(target_arch = "arm"))]
mod tests {

    use assert_cmd::prelude::*; // Add methods on commands
    use predicates::prelude::*; // Used for writing assertions
    use std::process::Command; // Run programs

    #[test]
    fn run_help() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;

        cmd.arg("--help");
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("USAGE"));

        Ok(())
    }

    #[test]
    fn run_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;

        let version_string = format!("tedge {}", env!("CARGO_PKG_VERSION"));
        cmd.arg("-V");
        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with(version_string));

        Ok(())
    }
}
