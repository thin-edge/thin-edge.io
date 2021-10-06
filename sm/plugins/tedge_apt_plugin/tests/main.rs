#[cfg(test)]
mod apt_plugin_integration_tests {
    use curl::easy::Easy;
    use hamcrest2::prelude::*;
    use serial_test::serial;
    use std::error;
    use std::fs::File;
    use std::io::prelude::*;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::str::FromStr;
    use std::sync::Once;
    use test_case::test_case;

    type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

    const ROLLDICE: &str = "rolldice";
    const TEDGE_APT_COMMAND: &str = "/etc/tedge/sm-plugins/apt";
    const APT_COMMAND: &str = "/usr/bin/apt-get";
    const ROLLDICE_URL: &str =
        "http://ftp.br.debian.org/debian/pool/main/r/rolldice/rolldice_1.16-1+b3_amd64.deb";
    const ROLLDICE_VERSION: &str = "1.16-1+b3";
    const ROLLDICE_FILE_PATH: &str = "/tmp/rolldice_1.16-1+b3_amd64.deb";
    static DOWNLOAD_ROLLDICE_BINARY: Once = Once::new();

    pub fn download_rolldice_binary_once() {
        DOWNLOAD_ROLLDICE_BINARY.call_once(|| {
            let _ = fetch_url(ROLLDICE_URL).unwrap();
        });
    }
    /// converts a vector of u8 integers into utf8 String.
    /// if the vec of u8 == "" (as a String) => None.
    /// else Some
    fn u8_to_string(vec: Vec<u8>) -> Option<String> {
        let output = String::from_utf8(vec).unwrap();
        if output == "" {
            return None;
        }
        return Some(output);
    }

    /// executes a `cmd` with `args`
    /// returns the stdout, stderr and exit code
    fn run_cmd(cmd: &str, args: &str) -> Result<(Option<String>, Option<String>, i32)> {
        let args: Vec<&str> = args.split_whitespace().collect();
        let output = Command::new(cmd)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;

        let stdout = u8_to_string(output.stdout);
        let stderr = u8_to_string(output.stderr);

        let status_code = output.status.code().unwrap();
        Ok((stdout, stderr, status_code))
    }

    /// https://gist.github.com/BennyFranco/608765f2d3aee69c207c490045ac093c
    fn fetch_url(url: &str) -> Result<PathBuf> {
        let mut dst = Vec::new();
        let mut easy = Easy::new();
        let mut path = PathBuf::from_str("/tmp")?;
        let filename = &url.split("/").last().unwrap();
        path = path.join(&filename);
        easy.url(url).unwrap();
        let _redirect = easy.follow_location(true);

        {
            let mut transfer = easy.transfer();
            transfer
                .write_function(|data| {
                    dst.extend_from_slice(data);
                    Ok(data.len())
                })
                .unwrap();
            transfer.perform().unwrap();
        }
        {
            let mut file = File::create(&path)?;
            file.write_all(dst.as_slice())?;
        }
        Ok(path)
    }

    #[test]
    #[serial]
    fn tedge_apt_install_command() -> Result<()> {
        // setup
        let _value = run_cmd(APT_COMMAND, &format!("remove {} -y", ROLLDICE))?;

        // installing rolldice
        let (stdout, stderr, exit_code) =
            run_cmd(TEDGE_APT_COMMAND, &format!("install {}", ROLLDICE))?;

        // asserting rolldice installation worked
        assert_that!(
            stdout.unwrap(),
            match_regex(&format!(
                "The following NEW packages will be installed\n  {}",
                ROLLDICE
            ))
        );
        assert_that!(stderr, is(none()));
        assert_that!(exit_code, eq(0));
        Ok(())
    }

    #[test]
    #[serial]
    fn tedge_remove_command() -> Result<()> {
        // setup
        let _ = run_cmd(APT_COMMAND, &format!("install {}", ROLLDICE))?;

        // removing rolldice
        let (stdout, stderr, exit_code) =
            run_cmd(TEDGE_APT_COMMAND, &format!("remove {}", ROLLDICE))?;

        // asserting rolldice removal worked
        assert_that!(
            stdout.unwrap(),
            match_regex(&format!(
                "The following packages will be REMOVED\n  {}",
                ROLLDICE
            ))
        );
        assert_that!(stderr, is(none()));
        assert_that!(exit_code, eq(0));
        Ok(())
    }

    #[test_case(
        &format!("install {} --file {}", ROLLDICE, "wrong_path"),                                               // input
        &format!("ERROR: Parsing Debian package failed"),                                                       // expected stderr
        5                                                                                                       // expected exit code
        ; "wrong path"                                                                               // description
    )]
    #[test_case(
        &format!("install {} --file {} --module-version {}", ROLLDICE, "notapackagepath", ROLLDICE_VERSION),    // input
        &format!("ERROR: Parsing Debian package failed"),                                                       // expected stderr
        5                                                                                                       // expected exit code
        ; "wrong path with right version"                                              // description
    )]
    #[test_case(
        &format!("install {} --file {} --module-version {}", ROLLDICE, ROLLDICE_FILE_PATH, "someversion"),       // input
        &format!("ERROR: Parsing Debian package failed"),                                                       // expected stderr
        5                                                                                                       // expcted exit code
        ; "right path with wrong version"                                               // description
    )]
    #[test_case(
        &format!("install {} --file {} --module-version {}", ROLLDICE, "notapackagepath", "someversion"),       // input
        &format!("ERROR: Parsing Debian package failed"),                                                       // expected stderr
        5                                                                                                       // expcted exit code
        ; "wrong path with wrong version"                                               // description
    )]
    fn install_from_local_file_fail(
        input_command: &str,
        expected_stderr: &str,
        expected_exit_code: i32,
    ) -> Result<()> {
        // no setup needed, wrong arguments are provided to tedge apt plugin
        let (stdout, stderr, exit_code) = run_cmd(TEDGE_APT_COMMAND, input_command)?;

        // asserting command failed
        assert_that!(stdout, is(none()));
        assert_that!(stderr.unwrap(), match_regex(expected_stderr));
        assert_that!(exit_code, eq(expected_exit_code));
        Ok(())
    }

    #[test_case(
        &format!("install {} --file {}", ROLLDICE, ROLLDICE_FILE_PATH),                                         // input
        &format!("The following NEW packages will be installed\n  {}", ROLLDICE),                               // expected stdout
        0                                                                                                      // expected exit code
        ; "path"                                                                             // description
    )]
    #[serial]
    #[test_case(
        &format!("install {} --file {} --module-version {}", ROLLDICE, ROLLDICE_FILE_PATH, ROLLDICE_VERSION),   // input
        &format!("The following NEW packages will be installed\n  {}", ROLLDICE),                               // expected stdout 
        0                                                                                                      // expected exit code
        ; "path with version"                                                         // description
    )]
    #[serial]
    #[test_case(
        &format!("install {} --module-version {} --file {}", ROLLDICE,  ROLLDICE_VERSION, ROLLDICE_FILE_PATH),  // input
        &format!("The following NEW packages will be installed\n  {}", ROLLDICE),                               // expected stdout 
        0                                                                                                      // expected exit code
        ; "version with path"                                           // description
    )]
    #[serial]
    fn install_from_local_file_sucess(
        input_command: &str,
        expected_stdout: &str,
        expected_exit_code: i32,
    ) -> Result<()> {
        // fetching the debian package & removing rolldice in case it is already installed.
        // only executed once.
        download_rolldice_binary_once(); // without 42.07s, with 39.80s
        let _ = run_cmd(APT_COMMAND, &format!("remove {} -y", ROLLDICE))?;

        // execute command to install from local file
        let (stdout, stderr, exit_code) = run_cmd(TEDGE_APT_COMMAND, input_command)?;
        //dbg!(&stdout, &stderr, &exit_code);

        // asserting sucesss
        assert_that!(stdout.unwrap(), match_regex(expected_stdout));
        assert_that!(stderr, is(none()));
        assert_that!(exit_code, eq(expected_exit_code));

        Ok(())
    }
}
