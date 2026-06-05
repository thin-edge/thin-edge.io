use super::SystemConfig;
use super::SystemTomlError;
use crate::cli::LogConfigArgs;
use camino::Utf8Path;
use std::collections::HashMap;
use std::fmt;
use std::io::IsTerminal;
use std::str::FromStr;
use std::sync::Arc;
use tracing::field::Visit;
use tracing::Subscriber;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::layer::Context;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

const DEFAULT_LEVEL: tracing::Level = tracing::Level::INFO;

#[macro_export]
/// The basic subscriber
macro_rules! subscriber_builder {
    () => {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_ansi(std::io::stderr().is_terminal() && yansi::Condition::no_color())
            .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
    };
}

/// Configures and enables logging taking into account flags, env variables and file config.
///
/// Priority order (highest to lowest):
/// 1. `--debug` or `--log-level` flags
/// 2. `RUST_LOG` environment variable
/// 3. File configuration (`system.toml`)
/// 4. Default level (INFO)
///
/// Reports all the log events sent either with the `log` crate or the `tracing`
/// crate.
pub fn log_init(
    sname: &str,
    flags: &LogConfigArgs,
    config_dir: &Utf8Path,
) -> Result<(), SystemTomlError> {
    log_init_with_default_level(sname, flags, config_dir, DEFAULT_LEVEL)
}

pub fn log_init_for_services(
    service_names: &[&str],
    flags: &LogConfigArgs,
    config_dir: &Utf8Path,
) -> Result<(), SystemTomlError> {
    let logger = logger_for_services(service_names, flags, Some(config_dir), DEFAULT_LEVEL)?;
    logger.init();
    Ok(())
}

pub fn log_init_with_default_level(
    sname: &str,
    flags: &LogConfigArgs,
    config_dir: &Utf8Path,
    default_level: tracing::Level,
) -> Result<(), SystemTomlError> {
    let logger = logger_for_services(&[sname], flags, Some(config_dir), default_level)?;
    logger.init();
    Ok(())
}

pub fn unconfigured_logger() -> Arc<dyn tracing::Subscriber + Send + Sync> {
    logger_for_services(
        &["tedge"],
        &LogConfigArgs {
            debug: false,
            log_level: None,
        },
        None,
        DEFAULT_LEVEL,
    )
    .unwrap()
}

fn logger_for_services(
    service_names: &[&str],
    flags: &LogConfigArgs,
    config_dir: Option<&Utf8Path>,
    default_level: tracing::Level,
) -> Result<Arc<dyn tracing::Subscriber + Send + Sync>, SystemTomlError> {
    let subscriber = subscriber_builder!();

    // Added by systemd if the process is running as systemd unit
    // https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#%24INVOCATION_ID
    let is_running_as_systemd_unit = std::env::var("INVOCATION_ID").is_ok();

    // disable time formatting if journald because journald already records the time
    let subscriber = if is_running_as_systemd_unit {
        subscriber.with_timer(TimeFormat::Disabled)
    } else {
        subscriber.with_timer(TimeFormat::Enabled)
    };

    // first use log level from flags
    let log_level = flags
        .log_level
        .or(flags.debug.then_some(tracing::Level::DEBUG));

    if let Some(log_level) = log_level {
        return Ok(Arc::new(subscriber.with_max_level(log_level).finish()));
    }

    // if no flags used, use EnvFilter
    if std::env::var("RUST_LOG").is_ok() {
        return Ok(Arc::new(
            subscriber
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_file(true)
                .with_line_number(true)
                .finish(),
        ));
    }

    // if no EnvFilter, get log level from config file
    if let Some(config_dir) = config_dir {
        if service_names.len() == 1 {
            if let Some(log_level) = log_level_for_service(service_names[0], config_dir) {
                return Ok(Arc::new(subscriber.with_max_level(log_level).finish()));
            }
        }

        if let Some(filter) = log_filter_for_services(service_names, config_dir, default_level) {
            let fmt_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(std::io::stderr().is_terminal() && yansi::Condition::no_color())
                .with_timer(if is_running_as_systemd_unit {
                    TimeFormat::Disabled
                } else {
                    TimeFormat::Enabled
                });
            return Ok(Arc::new(
                tracing_subscriber::registry().with(filter).with(fmt_layer),
            ));
        }
    }

    // otherwise, use the default log level
    Ok(Arc::new(subscriber.with_max_level(default_level).finish()))
}

/// Return the log level for a given service, if it's defined in the config file. Otherwise return `None`.
pub fn get_log_level_from_config_file(
    sname: &str,
    config_dir: &Utf8Path,
) -> Result<Option<tracing::Level>, SystemTomlError> {
    log_level_from_config(&SystemConfig::try_new(config_dir)?, sname)
}

/// Return the log level configured for `sname` in an already-parsed [`SystemConfig`], if any.
fn log_level_from_config(
    config: &SystemConfig,
    sname: &str,
) -> Result<Option<tracing::Level>, SystemTomlError> {
    match config.log.get(sname) {
        Some(ll) => {
            let ll = tracing::Level::from_str(&ll.to_uppercase()).map_err(|_| {
                SystemTomlError::InvalidLogLevel {
                    name: ll.to_string(),
                }
            })?;
            Ok(Some(ll))
        }
        None => Ok(None),
    }
}

fn log_level_for_service(sname: &str, config_dir: &Utf8Path) -> Option<tracing::Level> {
    let config = SystemConfig::try_new(config_dir).ok()?;
    log_level_from_config(&config, sname).ok().flatten()
}

fn log_filter_for_services(
    service_names: &[&str],
    config_dir: &Utf8Path,
    default_level: tracing::Level,
) -> Option<ComponentLogFilter> {
    let config = SystemConfig::try_new(config_dir).ok()?;
    let fallback_level = log_level_from_config(&config, "tedge")
        .ok()
        .flatten()
        .unwrap_or(default_level);
    let mut levels = HashMap::new();

    for service_name in service_names {
        if *service_name == "tedge" {
            continue;
        }

        if let Some(service_level) = log_level_from_config(&config, service_name)
            .ok()
            .flatten()
            .or_else(|| {
                service_name
                    .starts_with("tedge-mapper-")
                    .then(|| {
                        log_level_from_config(&config, "tedge-mapper")
                            .ok()
                            .flatten()
                    })
                    .flatten()
            })
        {
            levels.insert((*service_name).to_string(), service_level);
        }
    }

    if !levels.is_empty()
        || log_level_from_config(&config, "tedge")
            .ok()
            .flatten()
            .is_some()
    {
        Some(ComponentLogFilter {
            default_level: fallback_level,
            component_levels: levels,
        })
    } else {
        None
    }
}

#[derive(Debug)]
struct ComponentLogFilter {
    default_level: tracing::Level,
    component_levels: HashMap<String, tracing::Level>,
}

impl<S> Layer<S> for ComponentLogFilter
where
    S: Subscriber,
    for<'lookup> S: LookupSpan<'lookup>,
{
    fn enabled(&self, metadata: &tracing::Metadata<'_>, ctx: Context<'_, S>) -> bool {
        let _ = ctx;
        !metadata.is_event() || *metadata.level() <= self.max_level()
    }

    fn event_enabled(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) -> bool {
        let current_span = if let Some(id) = event.parent() {
            ctx.span(id)
        } else {
            ctx.lookup_current()
        };

        let level = current_span
            .and_then(|span| span.extensions().get::<ComponentName>().cloned())
            .and_then(|component| self.component_levels.get(component.0.as_str()).copied())
            .unwrap_or(self.default_level);

        *event.metadata().level() <= level
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        if attrs.metadata().name() != "component" {
            return;
        }

        let mut visitor = ComponentNameVisitor::default();
        attrs.record(&mut visitor);

        if let (Some(span), Some(component)) = (ctx.span(id), visitor.name) {
            span.extensions_mut().insert(ComponentName(component));
        }
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::from_level(self.max_level()))
    }
}

impl ComponentLogFilter {
    fn max_level(&self) -> tracing::Level {
        self.component_levels
            .values()
            .copied()
            .chain(std::iter::once(self.default_level))
            .max_by_key(|level| verbosity_rank(*level))
            .unwrap_or(self.default_level)
    }
}

#[derive(Clone)]
struct ComponentName(String);

#[derive(Default)]
struct ComponentNameVisitor {
    name: Option<String>,
}

impl Visit for ComponentNameVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "name" {
            self.name = Some(value.to_string());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "name" {
            self.name = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
}

fn verbosity_rank(level: tracing::Level) -> u8 {
    match level {
        tracing::Level::ERROR => 1,
        tracing::Level::WARN => 2,
        tracing::Level::INFO => 3,
        tracing::Level::DEBUG => 4,
        tracing::Level::TRACE => 5,
    }
}

/// Initializes a tracing subscriber with a given log level if environment
/// variable `RUST_LOG` is not present.
///
/// Reports all the log events sent either with the `log` crate or the `tracing`
/// crate.
pub fn set_log_level(log_level: tracing::Level) {
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal() && yansi::Condition::no_color())
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339());

    if std::env::var("RUST_LOG").is_ok() {
        subscriber
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();
    } else {
        subscriber.with_max_level(log_level).init();
    }
}

enum TimeFormat {
    Enabled,
    Disabled,
}

impl FormatTime for TimeFormat {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        match self {
            Self::Enabled => tracing_subscriber::fmt::time::UtcTime::rfc_3339().format_time(w),
            Self::Disabled => ().format_time(w),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::io;
    use std::sync::Mutex;
    use tempfile::TempDir;
    use tracing::Level;

    #[test]
    fn valid_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = "debug"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir)?;
        assert_eq!(Some(Level::DEBUG), res);
        Ok(())
    }

    #[test]
    fn invalid_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = "other"
    "#;
        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir).unwrap_err();
        assert_eq!(
            "Invalid log level: \"other\", supported levels are info, warn, error and debug",
            res.to_string()
        );
        Ok(())
    }

    #[test]
    fn empty_log_level() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge_mapper = ""
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir).unwrap_err();

        assert_eq!(
            "Invalid log level: \"\", supported levels are info, warn, error and debug",
            res.to_string()
        );
        Ok(())
    }

    #[test]
    fn log_level_not_configured_for_the_service() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        some_mapper = "other"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = get_log_level_from_config_file("tedge_mapper", &config_dir).unwrap();
        assert_eq!(None, res);
        Ok(())
    }

    #[test]
    fn single_service_log_level_is_global() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge-agent = "trace"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = log_level_for_service("tedge-agent", &config_dir);
        assert_eq!(Some(Level::TRACE), res);
        Ok(())
    }

    #[test]
    fn multi_service_log_levels_are_scoped_to_component_spans() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge = "info"
        tedge-agent = "trace"
        tedge-mapper = "debug"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = log_filter_for_services(
            &["tedge", "tedge-agent", "tedge-mapper"],
            &config_dir,
            DEFAULT_LEVEL,
        )
        .unwrap();
        assert_eq!(Level::INFO, res.default_level);
        assert_eq!(Some(&Level::TRACE), res.component_levels.get("tedge-agent"));
        assert_eq!(
            Some(&Level::DEBUG),
            res.component_levels.get("tedge-mapper")
        );
        Ok(())
    }

    #[test]
    fn missing_services_do_not_override_configured_multi_service_log_filter() -> anyhow::Result<()>
    {
        let toml_conf = r#"
        [log]
        tedge-agent = "debug"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res =
            log_filter_for_services(&["tedge", "tedge-agent"], &config_dir, Level::INFO).unwrap();
        assert_eq!(Level::INFO, res.default_level);
        assert_eq!(Some(&Level::DEBUG), res.component_levels.get("tedge-agent"));
        assert_eq!(None, res.component_levels.get("tedge"));
        Ok(())
    }

    #[test]
    fn generic_mapper_log_level_applies_to_specific_mapper_component() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge-mapper = "debug"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = log_filter_for_services(
            &["tedge", "tedge-agent", "tedge-mapper", "tedge-mapper-c8y"],
            &config_dir,
            Level::INFO,
        )
        .unwrap();
        assert_eq!(Level::INFO, res.default_level);
        assert_eq!(
            Some(&Level::DEBUG),
            res.component_levels.get("tedge-mapper")
        );
        assert_eq!(
            Some(&Level::DEBUG),
            res.component_levels.get("tedge-mapper-c8y")
        );
        Ok(())
    }

    #[test]
    fn component_filter_uses_current_component_span() {
        #[derive(Clone)]
        struct Buffer(Arc<Mutex<Vec<u8>>>);

        impl io::Write for Buffer {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = Buffer(output.clone());
        let filter = ComponentLogFilter {
            default_level: Level::INFO,
            component_levels: HashMap::from([
                ("tedge-agent".to_string(), Level::TRACE),
                ("tedge-mapper-c8y".to_string(), Level::INFO),
            ]),
        };
        let fmt_layer = tracing_subscriber::fmt::layer()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone());
        let subscriber = tracing_subscriber::registry().with(filter).with(fmt_layer);

        tracing::subscriber::with_default(subscriber, || {
            let agent = tracing::info_span!("component", name = "tedge-agent");
            let _guard = agent.enter();
            tracing::trace!("agent_trace");
            drop(_guard);

            let mapper = tracing::info_span!("component", name = "tedge-mapper-c8y");
            let _guard = mapper.enter();
            tracing::trace!("mapper_trace");
        });

        let output = String::from_utf8(output.lock().unwrap().clone()).unwrap();
        assert!(output.contains("agent_trace"));
        assert!(!output.contains("mapper_trace"));
    }

    // Need to return TempDir, otherwise the dir will be deleted when this function ends.
    fn create_temp_system_config(content: &str) -> std::io::Result<(TempDir, Utf8PathBuf)> {
        let temp_dir = TempDir::new()?;
        let config_root = temp_dir.path().to_path_buf();
        let config_file_path = config_root.join("system.toml");
        std::fs::write(config_file_path.as_path(), content.as_bytes())?;
        Ok((temp_dir, config_root.try_into().unwrap()))
    }
}
