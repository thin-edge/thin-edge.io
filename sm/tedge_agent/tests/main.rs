#[cfg(test)]
#[cfg(feature = "mosquitto-available")]
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
        let mut agent = Command::cargo_bin(env!("CARGO_PKG_NAME"))?
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // running second `tedge_agent` binary
        let mut agent_2 = Command::cargo_bin(env!("CARGO_PKG_NAME"))?
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // trying up to 10 times before breaking out.
        for _ in 0..10 {
            if let Ok(Some(code)) = agent.try_wait() {
                agent.wait_with_output().unwrap().assert().failure().stdout(
                    predicate::str::contains("Another instance of tedge_agent is running."),
                );
                agent_2.kill();
                let _ignore_error = std::fs::remove_file("/run/lock/tedge_agent.lock");
                return Ok(());
            } else if let Ok(Some(code)) = agent_2.try_wait() {
                agent_2
                    .wait_with_output()
                    .unwrap()
                    .assert()
                    .failure()
                    .stdout(predicate::str::contains(
                        "Another instance of tedge_agent is running.",
                    ));
                agent.kill();
                let _ignore_error = std::fs::remove_file("/run/lock/tedge_agent.lock");
                return Ok(());
            }
            thread::sleep(time::Duration::from_millis(200));
        }

        // cleanup before panic
        agent.kill();
        agent_2.kill();
        let _ignore_error = std::fs::remove_file("/run/lock/tedge_agent.lock");
        panic!("Agent failed to stop.")
    }
}
