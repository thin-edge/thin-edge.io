pub mod restart_operation {

    use crate::error::AgentError;
    use std::{fs::File, fs::OpenOptions, io::Read, io::Write, path::Path};
    use time::OffsetDateTime;

    const SLASH_RUN_PATH_TEDGE_AGENT_RESTART: &str = "tedge_agent/tedge_agent_restart";
    const SLASH_PROC_UPTIME: &str = "/proc/uptime";

    /// creates an empty file in /run
    /// the file name defined by `SLASH_RUN_PATH_TEDGE_AGENT_RESTART`
    ///
    /// # Example
    /// ```
    /// let () = RestartOperationHelper::create_slash_run_file()?;
    /// ```
    pub fn create_slash_run_file(run_dir: &Path) -> Result<(), AgentError> {
        let path = &run_dir.join(SLASH_RUN_PATH_TEDGE_AGENT_RESTART);
        let path = Path::new(path);

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

    pub fn slash_run_file_exists(run_dir: &Path) -> bool {
        let path = &run_dir.join(SLASH_RUN_PATH_TEDGE_AGENT_RESTART);
        std::path::Path::new(path).exists()
    }

    /// returns the datetime of `SLASH_RUN_PATH_TEDGE_AGENT_RESTART` "modified at".
    fn get_restart_file_datetime(run_dir: &Path) -> Result<time::OffsetDateTime, AgentError> {
        let mut file = File::open(&run_dir.join(SLASH_RUN_PATH_TEDGE_AGENT_RESTART))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let unix_timestamp = contents
            .parse::<i64>()
            .expect("Could not parse unix timestamp");

        let dt = match OffsetDateTime::from_unix_timestamp(unix_timestamp) {
            Ok(result) => result,
            Err(error) => {
                return Err(AgentError::TimestampConversionError {
                    timestamp: unix_timestamp,
                    error_msg: error.to_string(),
                });
            }
        };
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
        let maybe_uptime = buffer.split(' ').next();

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

    /// checks if system rebooted by comparing dt of tedge_agent_restart with dt of system restart.
    pub fn has_rebooted(run_dir: &Path) -> Result<bool, AgentError> {
        // there is no slash run file after the reboot, so we assume success.
        // this is true for most of the cases as "/run/" is normally cleared after a reboot.
        if !slash_run_file_exists(run_dir) {
            return Ok(true);
        }

        let system_reboot_dt = get_system_uptime()?;
        let tedge_restart_file_dt = get_restart_file_datetime(run_dir)?;

        Ok(system_reboot_dt > tedge_restart_file_dt)
    }
}
