#[cfg(test)]
mod tests {
    // use std::process::Command;
    // use std::process::Stdio;
    // use std::thread;
    // use std::time;
    //
    // use assert_cmd::prelude::*;
    // use predicates::prelude::*;
    //
    // #[test]
    // #[ignore] // this test requires sudo
    // /// Tests that only one instance of `tedge-agent` is running.
    // /// This is done by spawning/running two instances of `tedge-agent`
    // /// expecting the first one to work and the second to fail.
    // fn tedge_agent_check_no_multiple_instances_running() -> Result<(), Box<dyn std::error::Error>> {
    //     let _ignore_errors = std::fs::remove_file("/run/lock/tedge-agent.lock");
    //
    //     // running first `tedge-agent` binary
    //     let mut agent = Command::cargo_bin(env!("CARGO_PKG_NAME"))?
    //         .stdout(Stdio::piped())
    //         .stderr(Stdio::piped())
    //         .spawn()?;
    //
    //     // running second `tedge-agent` binary
    //     let mut agent_2 = Command::cargo_bin(env!("CARGO_PKG_NAME"))?
    //         .stdout(Stdio::piped())
    //         .stderr(Stdio::piped())
    //         .spawn()?;
    //
    //     // trying up to 10 times before breaking out.
    //     for _ in 0..10 {
    //         if let Ok(Some(_)) = agent.try_wait() {
    //             agent.wait_with_output().unwrap().assert().failure().stdout(
    //                 predicate::str::contains("Another instance of tedge-agent is running."),
    //             );
    //             let _ = agent_2.kill();
    //             let _ignore_error = std::fs::remove_file("/run/lock/tedge-agent.lock");
    //             return Ok(());
    //         } else if let Ok(Some(_)) = agent_2.try_wait() {
    //             agent_2
    //                 .wait_with_output()
    //                 .unwrap()
    //                 .assert()
    //                 .failure()
    //                 .stdout(predicate::str::contains(
    //                     "Another instance of tedge-agent is running.",
    //                 ));
    //             let _ = agent.kill();
    //             let _ignore_error = std::fs::remove_file("/run/lock/tedge-agent.lock");
    //             return Ok(());
    //         }
    //         thread::sleep(time::Duration::from_millis(200));
    //     }
    //
    //     // cleanup before panic
    //     let _ = agent.kill();
    //     let _ = agent_2.kill();
    //     let _ignore_error = std::fs::remove_file("/run/lock/tedge-agent.lock");
    //     panic!("Agent failed to stop.")
    // }
}
