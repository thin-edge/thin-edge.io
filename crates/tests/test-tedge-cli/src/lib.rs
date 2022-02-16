#[cfg(test)]
mod tests {
    use rexpect::*;
    use rexpect::errors::*;
    use rexpect::process::signal::Signal;

    const TIMEOUT_MS: Option<u64> = Some(5_000);

    #[test]
    fn it_works() -> Result<()> {
        let mut sub = spawn("tedge mqtt sub test/topic", TIMEOUT_MS)?;

        execute(r#"tedge mqtt pub test/topic hello-thin-edge"#)?;
        assert_eq!(sub.read_line()?, "INFO: Connected");
        assert_eq!(sub.read_line()?, "[test/topic] hello-thin-edge");

        execute(r#"tedge mqtt pub test/topic bye-bye --qos 2"#)?;
        assert_eq!(sub.read_line()?, "[test/topic] bye-bye");

        sub.process.kill(Signal::SIGTERM)?;
        Ok(())
    }

    fn execute(cmd: &str) -> Result<()> {
        spawn(cmd, TIMEOUT_MS)?.process.wait()?;
        Ok(())
    }
}
