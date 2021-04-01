fn main() -> anyhow::Result<()> {
    eprintln!("Privileged tedge process");
    // assert!(users::get_current_uid() == 0);

    loop {
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line)?;

        eprintln!("Received command: {}", line);

        match line.trim() {
            "command1" => {
                // ...
                // if you need to run command1 as different user, switch to that user now.
                eprintln!("Running command1");
                println!("OK");
            }
            "quit" => {
                eprintln!("Terminating");
                return Ok(());
            }
            _ => {
                anyhow::bail!("Invalid command");
            }
        }
    }
}
