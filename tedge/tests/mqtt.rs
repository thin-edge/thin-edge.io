#[cfg(test)]
mod tests {

    // These test cases need mosquitto on localhost on GH hosted machine.

    #[test]
    #[cfg(feature = "mosquitto-available")]
    fn test_cli_pub_basic() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let assert = cmd
            .args(&["mqtt", "pub", "topic", "message"])
            .unwrap()
            .assert();

        assert.success().code(predicate::eq(0));
        Ok(())
    }

    #[test]
    #[cfg(feature = "mosquitto-available")]
    fn test_cli_pub_qos() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let assert = cmd
            .args(&["mqtt", "pub", "topic", "message"])
            .args(&["--qos", "1"])
            .unwrap()
            .assert();

        assert.success().code(predicate::eq(0));
        Ok(())
    }

    #[test]
    #[cfg(feature = "mosquitto-available")]
    fn test_cli_sub_basic() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let err = cmd
            .args(&["mqtt", "sub", "topic"])
            .timeout(std::time::Duration::from_secs(1))
            .unwrap_err();

        let output = err.as_output().unwrap();
        assert_eq!(None, output.status.code());

        Ok(())
    }

    #[test]
    #[cfg(feature = "mosquitto-available")]
    fn test_cli_sub_qos() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = Command::cargo_bin("tedge")?;
        let err = cmd
            .args(&["mqtt", "sub", "topic"])
            .args(&["--qos", "1"])
            .timeout(std::time::Duration::from_secs(1))
            .unwrap_err();

        let output = err.as_output().unwrap();
        assert_eq!(None, output.status.code());

        Ok(())
    }
}
