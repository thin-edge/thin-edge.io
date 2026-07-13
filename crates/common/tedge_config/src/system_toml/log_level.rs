use super::SystemConfig;
use super::SystemTomlError;
use crate::cli::LogConfigArgs;
use camino::Utf8Path;
use camino::Utf8PathBuf;
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
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;

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

/// Initialises logging for several co-hosted services, returning a handle to
/// refresh their log levels from `system.toml` at runtime.
///
/// Returns `None` when an explicit override (`--debug`/`--log-level`/`RUST_LOG`,
/// see [`log_init`]) takes priority, as those are fixed for the process lifetime.
pub fn log_init_reloadable_for_services(
    service_names: &[&str],
    flags: &LogConfigArgs,
    config_dir: &Utf8Path,
) -> Result<Option<LogLevelReloadHandle>, SystemTomlError> {
    let default_level = DEFAULT_LEVEL;
    let is_running_as_systemd_unit = std::env::var("INVOCATION_ID").is_ok();

    // Explicit flags / RUST_LOG take priority and opt out of runtime reload.
    if let Some(subscriber) = override_subscriber(flags, is_running_as_systemd_unit) {
        subscriber.init();
        return Ok(None);
    }

    // File-configured levels in a reload layer, so SIGHUP can swap them in later.
    // As with the standalone agent/mapper, a malformed `system.toml` falls back to
    // defaults rather than blocking startup; we warn once logging is up.
    let (filter, parse_error) =
        match build_component_log_filter(service_names, config_dir, default_level) {
            Ok(filter) => (filter, None),
            Err(err) => (
                ComponentLogFilter {
                    default_level,
                    component_levels: HashMap::new(),
                },
                Some(err),
            ),
        };
    let (filter, handle) = reload::Layer::new(filter);
    tracing_subscriber::registry()
        .with(filter)
        .with(stderr_fmt_layer(is_running_as_systemd_unit))
        .init();

    if let Some(err) = parse_error {
        tracing::warn!("could not read log levels from system.toml; using defaults: {err}");
    }

    Ok(Some(LogLevelReloadHandle {
        handle,
        service_names: service_names.iter().map(|s| s.to_string()).collect(),
        config_dir: config_dir.to_path_buf(),
        default_level,
    }))
}

/// Refreshes the live log levels of co-hosted services from `system.toml`.
///
/// Returned by [`log_init_reloadable_for_services`]; [`reload`] swaps the active
/// filter in place without restarting the process.
///
/// [`reload`]: LogLevelReloadHandle::reload
pub struct LogLevelReloadHandle {
    handle: reload::Handle<ComponentLogFilter, Registry>,
    service_names: Vec<String>,
    config_dir: Utf8PathBuf,
    default_level: tracing::Level,
}

impl LogLevelReloadHandle {
    /// Re-reads `system.toml` and applies the configured log levels live.
    ///
    /// On a syntax error the previous levels are kept and the error is returned.
    /// Levels removed from the file revert to the default.
    pub fn reload(&self) -> Result<(), SystemTomlError> {
        let service_names: Vec<&str> = self.service_names.iter().map(String::as_str).collect();
        let filter =
            build_component_log_filter(&service_names, &self.config_dir, self.default_level)?;
        // `reload` only fails if the subscriber has been dropped, which cannot
        // happen while the process is running.
        let _ = self.handle.reload(filter);
        Ok(())
    }
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
    let is_running_as_systemd_unit = std::env::var("INVOCATION_ID").is_ok();

    // Flags / RUST_LOG take priority over file config.
    if let Some(subscriber) = override_subscriber(flags, is_running_as_systemd_unit) {
        return Ok(subscriber);
    }

    if let Some(config_dir) = config_dir {
        if service_names.len() == 1 {
            if let Some(log_level) = log_level_for_service(service_names[0], config_dir) {
                return Ok(max_level_subscriber(is_running_as_systemd_unit, log_level));
            }
        }

        if let Some(filter) = log_filter_for_services(service_names, config_dir, default_level) {
            return Ok(Arc::new(
                tracing_subscriber::registry()
                    .with(filter)
                    .with(stderr_fmt_layer(is_running_as_systemd_unit)),
            ));
        }
    }

    Ok(max_level_subscriber(
        is_running_as_systemd_unit,
        default_level,
    ))
}

/// `TimeFormat::Disabled` under systemd (journald already timestamps records),
/// `Enabled` otherwise.
fn time_format(is_running_as_systemd_unit: bool) -> TimeFormat {
    // INVOCATION_ID is set by systemd: see
    // https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html#%24INVOCATION_ID
    if is_running_as_systemd_unit {
        TimeFormat::Disabled
    } else {
        TimeFormat::Enabled
    }
}

/// The subscriber that takes priority over file config: `--log-level`/`--debug`,
/// then `RUST_LOG`. Returns `None` when neither applies, leaving the caller to
/// fall back to file or default levels.
fn override_subscriber(
    flags: &LogConfigArgs,
    is_running_as_systemd_unit: bool,
) -> Option<Arc<dyn tracing::Subscriber + Send + Sync>> {
    let subscriber = subscriber_builder!().with_timer(time_format(is_running_as_systemd_unit));

    let log_level = flags
        .log_level
        .or(flags.debug.then_some(tracing::Level::DEBUG));
    if let Some(log_level) = log_level {
        return Some(Arc::new(subscriber.with_max_level(log_level).finish()));
    }

    if std::env::var("RUST_LOG").is_ok() {
        return Some(Arc::new(
            subscriber
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_file(true)
                .with_line_number(true)
                .finish(),
        ));
    }

    None
}

/// A subscriber applying a single max level to every component.
fn max_level_subscriber(
    is_running_as_systemd_unit: bool,
    level: tracing::Level,
) -> Arc<dyn tracing::Subscriber + Send + Sync> {
    Arc::new(
        subscriber_builder!()
            .with_timer(time_format(is_running_as_systemd_unit))
            .with_max_level(level)
            .finish(),
    )
}

/// The shared stderr `fmt` layer used alongside a [`ComponentLogFilter`].
fn stderr_fmt_layer<S>(is_running_as_systemd_unit: bool) -> impl Layer<S>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal() && yansi::Condition::no_color())
        .with_timer(time_format(is_running_as_systemd_unit))
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

/// A component filter built only when `system.toml` actually configures levels,
/// so callers can otherwise fall back to a plain default-level subscriber. A
/// malformed file is treated as "nothing configured".
fn log_filter_for_services(
    service_names: &[&str],
    config_dir: &Utf8Path,
    default_level: tracing::Level,
) -> Option<ComponentLogFilter> {
    let config = SystemConfig::try_new(config_dir).ok()?;
    let filter = component_log_filter(&config, service_names, default_level).ok()?;
    let tedge_default_configured = log_level_from_config(&config, "tedge")
        .ok()
        .flatten()
        .is_some();
    (!filter.component_levels.is_empty() || tedge_default_configured).then_some(filter)
}

/// Builds a [`ComponentLogFilter`] from `system.toml`, propagating parse errors.
///
/// Unlike [`log_filter_for_services`], always yields a filter (falling back to
/// `default_level` when nothing is configured), so it can drive a reload layer.
fn build_component_log_filter(
    service_names: &[&str],
    config_dir: &Utf8Path,
    default_level: tracing::Level,
) -> Result<ComponentLogFilter, SystemTomlError> {
    let config = SystemConfig::try_new(config_dir)?;
    let mut filter = component_log_filter(&config, service_names, default_level)?;

    // A single-service process is entirely that component: its level applies
    // process-wide, so events need no span-based attribution and the process
    // does not have to run inside a `component` span.
    if let [service_name] = service_names {
        if let Some(level) = filter.component_levels.remove(*service_name) {
            filter.default_level = level;
        }
    }

    Ok(filter)
}

/// Maps an already-parsed [`SystemConfig`] to per-component levels. The `tedge`
/// key sets the fallback level; a `tedge-mapper-*` service inherits `tedge-mapper`
/// when it has no level of its own.
fn component_log_filter(
    config: &SystemConfig,
    service_names: &[&str],
    default_level: tracing::Level,
) -> Result<ComponentLogFilter, SystemTomlError> {
    let fallback_level = log_level_from_config(config, "tedge")?.unwrap_or(default_level);
    let mut component_levels = HashMap::new();

    for service_name in service_names {
        if *service_name == "tedge" {
            continue;
        }

        let level = match log_level_from_config(config, service_name)? {
            Some(level) => Some(level),
            None if service_name.starts_with("tedge-mapper-") => {
                log_level_from_config(config, "tedge-mapper")?
            }
            None => None,
        };

        if let Some(level) = level {
            component_levels.insert((*service_name).to_string(), level);
        }
    }

    Ok(ComponentLogFilter {
        default_level: fallback_level,
        component_levels,
    })
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
        // Walk up the span ancestry from the event to find the nearest enclosing
        // `component` span. Dependencies a component calls into open their own
        // child spans, which carry no `ComponentName`; only looking at the
        // innermost span would attribute those events to the default level rather
        // than to the component that called them.
        let level = ctx
            .event_scope(event)
            .into_iter()
            .flatten()
            .find_map(|span| {
                span.extensions()
                    .get::<ComponentName>()
                    .map(|c| c.0.clone())
            })
            .and_then(|component| self.component_levels.get(component.as_str()).copied())
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
    fn standalone_service_log_level_applies_without_component_spans() -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge-agent = "debug"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = build_component_log_filter(&["tedge-agent"], &config_dir, DEFAULT_LEVEL)?;
        assert_eq!(Level::DEBUG, res.default_level);
        assert!(
            res.component_levels.is_empty(),
            "a single-service process must not depend on span-based attribution"
        );
        Ok(())
    }

    #[test]
    fn standalone_mapper_inherits_generic_mapper_level_without_component_spans(
    ) -> anyhow::Result<()> {
        let toml_conf = r#"
        [log]
        tedge-mapper = "debug"
    "#;

        let (_dir, config_dir) = create_temp_system_config(toml_conf)?;
        let res = build_component_log_filter(&["tedge-mapper-c8y"], &config_dir, DEFAULT_LEVEL)?;
        assert_eq!(Level::DEBUG, res.default_level);
        assert!(
            res.component_levels.is_empty(),
            "a single-service process must not depend on span-based attribution"
        );
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

        assert!(buffer_contains(&output, "agent_trace"));
        assert!(!buffer_contains(&output, "mapper_trace"));
    }

    #[test]
    fn build_filter_falls_back_to_default_when_nothing_configured() -> anyhow::Result<()> {
        let (_dir, config_dir) = create_temp_system_config("")?;
        let filter =
            build_component_log_filter(&["tedge", "tedge-agent"], &config_dir, Level::WARN)?;
        assert_eq!(Level::WARN, filter.default_level);
        assert!(filter.component_levels.is_empty());
        Ok(())
    }

    #[test]
    fn build_filter_propagates_syntax_errors() -> anyhow::Result<()> {
        let (_dir, config_dir) = create_temp_system_config("[log\nnot = valid")?;
        let err = build_component_log_filter(&["tedge"], &config_dir, Level::INFO).unwrap_err();
        assert!(matches!(err, SystemTomlError::InvalidSyntax { .. }));
        Ok(())
    }

    #[test]
    fn reload_applies_updated_levels_without_reinit() -> anyhow::Result<()> {
        // Start with the default (INFO) for the agent component.
        let (dir, config_dir) = create_temp_system_config("")?;
        let initial =
            build_component_log_filter(&["tedge", "tedge-agent"], &config_dir, Level::INFO)?;
        let (filter, handle) = reload::Layer::new(initial);

        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = Buffer(output.clone());
        let fmt_layer = tracing_subscriber::fmt::layer()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone());
        let subscriber = tracing_subscriber::registry().with(filter).with(fmt_layer);

        let reloader = LogLevelReloadHandle {
            handle,
            service_names: vec!["tedge".to_string(), "tedge-agent".to_string()],
            config_dir: config_dir.clone(),
            default_level: Level::INFO,
        };

        tracing::subscriber::with_default(subscriber, || {
            let emit_debug = || {
                let agent = tracing::info_span!("component", name = "tedge-agent");
                let _guard = agent.enter();
                tracing::debug!("agent_debug");
            };

            // Before reload: agent debug is below the INFO default and is dropped.
            emit_debug();
            assert!(!buffer_contains(&output, "agent_debug"));

            // Operator raises the agent to debug and signals a reload.
            std::fs::write(
                dir.path().join("system.toml"),
                "[log]\ntedge-agent = \"debug\"\n",
            )
            .unwrap();
            reloader.reload().unwrap();

            // After reload: the same debug event is now recorded, no restart needed.
            emit_debug();
            assert!(buffer_contains(&output, "agent_debug"));
        });

        Ok(())
    }

    #[test]
    fn component_filter_applies_to_nested_dependency_spans() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = Buffer(output.clone());
        let filter = ComponentLogFilter {
            default_level: Level::INFO,
            component_levels: HashMap::from([("tedge-agent".to_string(), Level::TRACE)]),
        };
        let fmt_layer = tracing_subscriber::fmt::layer()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone());
        let subscriber = tracing_subscriber::registry().with(filter).with(fmt_layer);

        tracing::subscriber::with_default(subscriber, || {
            let agent = tracing::info_span!("component", name = "tedge-agent");
            let _guard = agent.enter();
            // A dependency the component calls into opens its own span, which does
            // not carry a component name. Events emitted within it must still be
            // filtered at the enclosing component's level.
            let dependency = tracing::info_span!("dependency_call");
            let _dependency_guard = dependency.enter();
            tracing::trace!("dependency_trace");
        });

        assert!(buffer_contains(&output, "dependency_trace"));
    }

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

    fn buffer_contains(output: &Arc<Mutex<Vec<u8>>>, needle: &str) -> bool {
        String::from_utf8(output.lock().unwrap().clone())
            .unwrap()
            .contains(needle)
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
