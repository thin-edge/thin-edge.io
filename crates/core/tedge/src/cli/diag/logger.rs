use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[macro_export]
macro_rules! diag_log {
    ($log_path:expr, $($arg:tt)+) => {{
        let msg = format!($($arg)+);
        eprintln!("{}", msg);
        super::log_to_file($log_path, &msg);
    }};
}

#[macro_export]
macro_rules! diag_warning {
    ($log_path:expr, $($arg:tt)+) => {{
        use yansi::Paint as _;
        let msg = format!($($arg)+);
        eprintln!("{} {}", "warning:".yellow().bold(), msg);
        super::log_to_file($log_path, &format!("warning: {msg}"));
    }};
}

pub fn log_to_file<P: AsRef<Path>>(path: P, message: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}
