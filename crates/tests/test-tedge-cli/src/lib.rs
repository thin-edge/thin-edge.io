#[cfg(test)]
mod tests {
    use rexpect::errors::*;
    use rexpect::*;

    const TIMEOUT_MS: Option<u64> = Some(5_000);

    #[test]
    fn it_works() -> Result<()> {
        let mut sub = spawn("tedge mqtt sub test/topic", TIMEOUT_MS)?;

        execute(r#"tedge mqtt pub test/topic "hello thin-edge""#)?;
        sub.exp_string("INFO: Connected\n")?;
        sub.exp_string("[test/topic] hello thin-edge\n")?;

        execute(r#"tedge mqtt pub test/topic "bye bye" --qos 2"#)?;
        sub.exp_string("[test/topic] bye bye\n")?;

        sub.process.wait()?;
        Ok(())
    }

    fn execute(cmd: &str) -> Result<()> {
        spawn(cmd, TIMEOUT_MS)?.process.wait()?;
        Ok(())
    }
}
