mod tests {
    use std::process::{Command, Stdio};
    use std::{thread, time};

    use assert_cmd::prelude::*;
    use predicates::prelude::*;

    #[test]
    /// Tests that only one instance of `tedge_agent` is running.
    /// This is done by spawning/running two instances of `tedge_agent`
    /// expecting the first one to work and the second to fail.
    fn tedge_agent_check_no_multiple_instances_running() -> Result<(), Box<dyn std::error::Error>> {
        let _ignore_errors = std::fs::remove_file("/run/lock/tedge_agent.lock");
        // running first `tedge_agent` binary
        let mut agent = Command::cargo_bin(env!("CARGO_PKG_NAME"))?.spawn()?;

        // A sleep is required here to be sure that the lock is acquired by the first agent and not the second.
        let ten_millis = time::Duration::from_millis(10);
        thread::sleep(ten_millis);

        // running second `tedge_agent` binary
        let mut agent_2 = Command::cargo_bin(env!("CARGO_PKG_NAME"))?;

        // test
        agent_2.assert().failure().stdout(predicate::str::contains(
            "Another instance of tedge_agent is running.",
        ));

        // cleanup
        agent.kill()?;
        std::fs::remove_file("/run/lock/tedge_agent.lock")?;
        Ok(())
    }
}
