pub mod restart_operation {

    use crate::error::AgentError;
    use std::{fs::File, fs::OpenOptions, io::Read, io::Write, path::Path};
    use time::OffsetDateTime;
    use tracing::error;

    const SLASH_RUN_PATH_TEDGE_AGENT_RESTART: &str = "/run/tedge_agent_restart";
    const SLASH_PROC_UPTIME: &str = "/proc/uptime";
    const MAX_ALLOWED_DURATION: usize = 5;
    /// creates an empty file in /run
    /// the file name defined by `SLASH_RUN_PATH_TEDGE_AGENT_RESTART`
    ///
    /// # Example
    /// ```
    /// let () = RestartOperationHelper::create_slash_run_file()?;
    /// ```
    pub fn create_slash_run_file() -> Result<(), AgentError> {
        let path = Path::new(SLASH_RUN_PATH_TEDGE_AGENT_RESTART);

        let mut file = match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(err) => {
                return Err(AgentError::FromIo(err));
            }
        };
        let date_utc = OffsetDateTime::now_utc().unix_timestamp();
        file.write_all(date_utc.to_string().as_bytes())?;
        Ok(())
    }

    pub fn slash_run_file_exists() -> bool {
        std::path::Path::new(&SLASH_RUN_PATH_TEDGE_AGENT_RESTART.to_string()).exists()
    }

    /// returns the datetime of `SLASH_RUN_PATH_TEDGE_AGENT_RESTART` "modified at".
    fn get_restart_file_datetime() -> Result<time::OffsetDateTime, AgentError> {
        let mut file = File::open(&SLASH_RUN_PATH_TEDGE_AGENT_RESTART.to_string())?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let dt = OffsetDateTime::from_unix_timestamp(
            contents
                .parse::<i64>()
                .expect("Could not parse unix timestamp"),
        )
        .unwrap();
        Ok(dt)
    }

    /// computes the time of last reboot.
    ///
    /// where "time of last reboot" is defined as:
    ///     current datetime - number of seconds the system has been up.
    ///
    /// number of seconds the system has been up are obtained from /proc/uptime
    fn get_system_uptime() -> Result<time::OffsetDateTime, AgentError> {
        // reading uptime
        let uptime_file = std::fs::File::open(std::path::Path::new(SLASH_PROC_UPTIME))?;
        let mut buf_reader = std::io::BufReader::new(uptime_file);
        let mut buffer = String::new();
        buf_reader.read_to_string(&mut buffer)?;

        // system uptime is the first value of the /proc/uptime file content
        let maybe_uptime = buffer.split(' ').nth(0);

        if let Some(uptime) = maybe_uptime {
            match uptime.parse::<f64>() {
                Ok(result) => {
                    let local = OffsetDateTime::now_utc();
                    let uptime_secs = result as i64;
                    let duration = time::Duration::seconds(uptime_secs);
                    let reboot_time = local - duration;
                    Ok(reboot_time)
                }
                Err(_err) => Err(AgentError::FloatCastingError),
            }
        } else {
            Err(AgentError::UptimeParserError)
        }
    }

    /// computes the difference between system reboot datetime and tedge_agent_restart datetime
    fn compute_uptime_duration() -> Result<time::Duration, AgentError> {
        let system_reboot_dt = get_system_uptime()?;
        let tedge_restart_file_dt = get_restart_file_datetime()?;
        Ok(tedge_restart_file_dt - system_reboot_dt)
    }

    /// checks if system rebooted
    pub fn has_rebooted() -> Result<bool, AgentError> {
        // there is no slash run file after the reboot, so we assume success.
        // this is true for most of the cases as "/run/" is normally cleared after a reboot.
        if !slash_run_file_exists() {
            return Ok(true);
        }
        // if the file is still present, we compute the difference between system uptime and the
        // unix timestamp of /run/tedge_agent_restart
        let duration = compute_uptime_duration()?;

        if duration < time::Duration::minutes(MAX_ALLOWED_DURATION as i64) {
            Ok(true)
        } else {
            error!("Restart failed, duration > 5 minutes.");
            Ok(false)
        }
    }
}
