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
    color: bool,
    glyphs: &'static Glyphs,
    /// Where the log may be written, in order of preference
    log_candidates: Vec<PathBuf>,
    log: Mutex<Option<(std::fs::File, PathBuf)>>,
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
    pub fn new(log_dir: Option<PathBuf>, force_ascii: bool) -> Self {
        let name = format!("tedge-bootstrap-{}.log", std::process::id());
        let log_candidates = log_dir
            .into_iter()
            .chain([std::env::temp_dir()])
            .map(|dir| dir.join(&name))
            .collect();
        let dumb = std::env::var("TERM").is_ok_and(|term| term == "dumb");
        let glyphs = if force_ascii || dumb || !locale_is_utf8() {
            &ASCII
        } else {
            &UNICODE
        };
        let color =
            std::env::var_os("NO_COLOR").is_none() && !dumb && std::io::stderr().is_terminal();
        Self {
            color,
            glyphs,
            log_candidates,
            log: Mutex::new(None),
            state: Mutex::new(State {
                started: Instant::now(),
                phase: None,
                failed: None,
            }),
        }
    }

    /// The log file of the run, opened on first use so that a run
    /// aborted before it starts leaves no empty log behind
    fn log_path(&self) -> Option<PathBuf> {
        self.log
            .lock()
            .unwrap()
            .as_ref()
            .map(|(_, path)| path.clone())
    }

    fn open_log(&self) {
        let mut log = self.log.lock().unwrap();
        if log.is_some() {
            return;
        }
        // The log carries hook diagnostics and config values:
        // private to the user running bootstrap
        let create = |path: &PathBuf| {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)
                .ok()
        };
        *log = self
            .log_candidates
            .iter()
            .find_map(|path| create(path).map(|file| (file, path.clone())));
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
        if let Some((log, _)) = self.log.lock().unwrap().as_mut() {
            let elapsed = self.state.lock().unwrap().started.elapsed().as_secs_f64();
            let stamp = format!("{:02}:{:04.1}", (elapsed / 60.0) as u64, elapsed % 60.0);
            let _ = writeln!(log, "{stamp} {mark} {message}");
        }
    }

    // ---------------------------------------------------------- console

    /// The opening line of the run: the clock starts here,
    /// after any interactive prompting
    pub fn begin(&self, title: &str) {
        self.open_log();
        let mut state = self.state.lock().unwrap();
        state.started = Instant::now();
        state.phase = None;
        state.failed = None;
        drop(state);
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

    /// Detail: logged only, kept off the console
    /// (the summary card points at the log file)
    pub fn debug(&self, message: &str) {
        self.to_log("|", message);
    }

    /// The final report of the run: the verdict, a rule, and a summary
    /// card of key facts (on a failure, the error itself follows)
    pub fn finish(&self, ok: bool, summary: &[(&str, String)]) {
        self.finish_phase(ok);
        let elapsed = self.state.lock().unwrap().started.elapsed();
        let duration = fmt_duration(elapsed);
        let (outcome, verdict, mark) = match ok {
            true => (
                "completed successfully",
                self.green("completed successfully") + " in",
                "*",
            ),
            false => ("failed", self.red("failed") + " after", "x"),
        };
        eprintln!("{}", self.glyphs.rail);
        eprintln!("{}  Bootstrap {verdict} {duration}", self.glyphs.bottom);
        self.to_log(mark, &format!("{outcome} ({duration})"));
        let mut rows: Vec<(&str, String)> = Vec::new();
        if let Some(failed) = self.state.lock().unwrap().failed {
            rows.push(("failed step", failed.to_owned()));
        }
        rows.extend(summary.iter().map(|(key, value)| (*key, value.clone())));
        self.summary_card(&rows);
    }

    fn summary_card(&self, rows: &[(&str, String)]) {
        let mut rows: Vec<(&str, String)> = rows.to_vec();
        if let Some(path) = self.log_path() {
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
