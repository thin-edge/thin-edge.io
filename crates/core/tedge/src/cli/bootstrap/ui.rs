//! The bootstrap command's console output
//!
//! Bootstrap is aimed at people, so the console renders a prompt-kit
//! style checklist: a continuous rail down the left, operator-facing
//! content carried inside it, one glyph per phase as it completes
//! (with its duration), and a final report with the outcome,
//! the total time, and the log location.
//!
//! The output degrades gracefully for limited consoles
//! (serial terminals, legacy codepages, CI logs):
//! colors only on a terminal (`NO_COLOR` and `TERM=dumb` respected),
//! Unicode glyphs only when the locale advertises UTF-8
//! (`--ascii` forces the plain profile).
//!
//! Everything — plus hook diagnostics and the composed steps' output —
//! goes to a timeline-formatted log file
//! (each line stamped with the elapsed time),
//! written to the device's log directory when writable.

use std::io::IsTerminal;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

/// The console glyph profile
struct Glyphs {
    top: &'static str,
    rail: &'static str,
    done: &'static str,
    fail: &'static str,
    bottom: &'static str,
    rule: &'static str,
}

const UNICODE: Glyphs = Glyphs {
    top: "┌",
    rail: "│",
    done: "◇",
    fail: "■",
    bottom: "└",
    rule: "─",
};

const ASCII: Glyphs = Glyphs {
    top: "+",
    rail: "|",
    done: "o",
    fail: "x",
    bottom: "+",
    rule: "-",
};

pub struct Ui {
    pub verbose: bool,
    color: bool,
    glyphs: &'static Glyphs,
    log: Option<Mutex<std::fs::File>>,
    pub log_path: Option<PathBuf>,
    state: Mutex<State>,
}

struct State {
    started: Instant,
    /// The phase in progress: its completion label and start time
    phase: Option<(&'static str, Instant)>,
    /// The phase that failed, for the summary
    failed: Option<&'static str>,
}

impl Ui {
    /// `log_dir` is the device's log directory (`logs.path`,
    /// normally /var/log/tedge): the bootstrap log is written there
    /// when writable, falling back to the system temp directory
    pub fn new(verbose: bool, log_dir: Option<PathBuf>, force_ascii: bool) -> Self {
        let name = format!("tedge-bootstrap-{}.log", std::process::id());
        let candidates = [
            log_dir.map(|dir| dir.join(&name)),
            Some(std::env::temp_dir().join(&name)),
        ];
        let (log, log_path) = candidates
            .into_iter()
            .flatten()
            .find_map(|path| std::fs::File::create(&path).ok().map(|file| (file, path)))
            .map(|(file, path)| (Some(file), Some(path)))
            .unwrap_or((None, None));
        let dumb = std::env::var("TERM").is_ok_and(|term| term == "dumb");
        let glyphs = if force_ascii || dumb || !locale_is_utf8() {
            &ASCII
        } else {
            &UNICODE
        };
        let color =
            std::env::var_os("NO_COLOR").is_none() && !dumb && std::io::stderr().is_terminal();
        Self {
            verbose,
            color,
            glyphs,
            log_path,
            log: log.map(Mutex::new),
            state: Mutex::new(State {
                started: Instant::now(),
                phase: None,
                failed: None,
            }),
        }
    }

    // ---------------------------------------------------------- styling

    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    }

    fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    fn dim(&self, text: &str) -> String {
        self.paint("2", text)
    }

    fn green(&self, text: &str) -> String {
        self.paint("32", text)
    }

    fn red(&self, text: &str) -> String {
        self.paint("31", text)
    }

    /// Content styling by shape: links stand out, warnings warn
    fn styled(&self, message: &str) -> String {
        let trimmed = message.trim_start();
        if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
            self.paint("36", message)
        } else if trimmed.starts_with("Warning") {
            self.paint("33", message)
        } else {
            message.to_owned()
        }
    }

    // ---------------------------------------------------------- log (timeline format)

    fn to_log(&self, mark: &str, message: &str) {
        if let Some(log) = &self.log {
            let elapsed = self.state.lock().unwrap().started.elapsed().as_secs_f64();
            let stamp = format!("{:02}:{:04.1}", (elapsed / 60.0) as u64, elapsed % 60.0);
            let _ = writeln!(log.lock().unwrap(), "{stamp} {mark} {message}");
        }
    }

    // ---------------------------------------------------------- console

    /// The opening line of the run
    pub fn begin(&self, title: &str) {
        eprintln!("{}  {}", self.glyphs.top, self.bold(title));
        self.to_log(">", title);
    }

    /// Enter a pipeline phase; `label` is its completion word
    /// ("prepared", "connected", …), ticked off when the next phase
    /// starts or the run finishes
    pub fn phase(&self, label: &'static str) {
        self.finish_phase(true);
        eprintln!("{}", self.glyphs.rail);
        self.state.lock().unwrap().phase = Some((label, Instant::now()));
    }

    fn finish_phase(&self, ok: bool) {
        let phase = self.state.lock().unwrap().phase.take();
        if let Some((label, started)) = phase {
            let duration = fmt_duration(started.elapsed());
            if ok {
                eprintln!(
                    "{}  {label} {}",
                    self.green(self.glyphs.done),
                    self.dim(&duration)
                );
                self.to_log("*", &format!("{label} ({duration})"));
            } else {
                eprintln!(
                    "{}  {label} {}",
                    self.red(self.glyphs.fail),
                    self.dim(&duration)
                );
                self.to_log("x", &format!("{label} ({duration})"));
                self.state.lock().unwrap().failed = Some(label);
            }
        }
    }

    /// Operator-facing content: carried inside the rail, and logged
    pub fn line(&self, message: &str) {
        eprintln!("{}  {}", self.glyphs.rail, self.styled(message));
        self.to_log("|", message);
    }

    /// A failure notice inside the rail
    pub fn fail_line(&self, message: &str) {
        let text = format!("{} {message}", self.glyphs.fail);
        eprintln!("{}  {}", self.glyphs.rail, self.red(&text));
        self.to_log("x", message);
    }

    /// Replay an already-logged line on the console, inside the rail
    /// (used when dumping a failed step's diagnostics)
    pub fn replay_line(&self, message: &str) {
        eprintln!("{}  {}", self.glyphs.rail, message);
    }

    /// Detail: logged, and shown (dimmed) only with --verbose
    pub fn debug(&self, message: &str) {
        self.to_log("|", message);
        if self.verbose {
            eprintln!("{}  {}", self.glyphs.rail, self.dim(message));
        }
    }

    /// The final report of a successful run:
    /// the verdict, a rule, and a summary card of key facts
    pub fn finish_success(&self, summary: &[(&str, String)]) {
        self.finish_phase(true);
        let elapsed = self.state.lock().unwrap().started.elapsed();
        let duration = fmt_duration(elapsed);
        eprintln!("{}", self.glyphs.rail);
        eprintln!(
            "{}  Bootstrap {} in {duration}",
            self.glyphs.bottom,
            self.green("completed successfully")
        );
        self.to_log("*", &format!("completed successfully ({duration})"));
        self.summary_card(summary);
    }

    /// The final report of a failed run (the error itself follows)
    pub fn finish_failure(&self, summary: &[(&str, String)]) {
        self.finish_phase(false);
        let elapsed = self.state.lock().unwrap().started.elapsed();
        let duration = fmt_duration(elapsed);
        eprintln!("{}", self.glyphs.rail);
        eprintln!(
            "{}  Bootstrap {} after {duration}",
            self.glyphs.bottom,
            self.red("failed")
        );
        self.to_log("x", &format!("failed ({duration})"));
        let mut rows: Vec<(&str, String)> = Vec::new();
        if let Some(failed) = self.state.lock().unwrap().failed {
            rows.push(("failed step", failed.to_owned()));
        }
        rows.extend(summary.iter().map(|(key, value)| (*key, value.clone())));
        self.summary_card(&rows);
    }

    fn summary_card(&self, rows: &[(&str, String)]) {
        let mut rows: Vec<(&str, String)> = rows.to_vec();
        if let Some(path) = &self.log_path {
            rows.push(("log", path.display().to_string()));
        }
        if rows.is_empty() {
            return;
        }
        eprintln!("   {}", self.dim(&self.glyphs.rule.repeat(44)));
        let width = rows.iter().map(|(key, _)| key.len()).max().unwrap_or(0);
        for (key, value) in &rows {
            let styled = if *key == "failed step" {
                self.red(value)
            } else {
                self.styled(value)
            };
            eprintln!("   {}  {styled}", self.dim(&format!("{key:<width$}")));
            self.to_log("|", &format!("{key}: {value}"));
        }
    }
}

/// Whether the locale advertises UTF-8 (the standard heuristic:
/// LC_ALL, then LC_CTYPE, then LANG); limited consoles without it
/// get the ASCII glyph profile
fn locale_is_utf8() -> bool {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|value| !value.is_empty()))
        .is_some_and(|value| value.to_uppercase().replace('-', "").contains("UTF8"))
}

fn fmt_duration(duration: Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 10.0 {
        format!("{secs:.1}s")
    } else if secs < 120.0 {
        format!("{}s", secs.round() as u64)
    } else {
        format!("{}m{}s", duration.as_secs() / 60, duration.as_secs() % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_format_for_humans() {
        assert_eq!(fmt_duration(Duration::from_millis(340)), "0.3s");
        assert_eq!(fmt_duration(Duration::from_secs(44)), "44s");
        assert_eq!(fmt_duration(Duration::from_secs(155)), "2m35s");
    }
}
