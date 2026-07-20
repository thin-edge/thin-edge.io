//! Supervisor for thin-edge components.
//!
//! Serves two execution modes:
//!
//! - **Multi-unit** (`tedge run all`): hosts the agent and one or more mappers inside
//!   one process so no external init system is required.
//! - **Single-unit** (standalone `tedge-agent`, `tedge-mapper`): wraps one component
//!   with the same signal handling and SIGHUP log-level reloading, but leaves crash
//!   recovery to the init system: the first unit failure exits the process, exactly
//!   as the component behaved before it ran under the supervisor.
//!
//! Each component is a *unit*: a rebuildable factory plus a restart policy. The
//! supervisor owns process-wide signal handling, isolates a crashing unit from its
//! co-hosted peers (rebuilding it under a bounded backoff), and drains everything
//! cleanly on termination.
//!
//! A unit that finishes with [`RuntimeError::RestartRequired`] (self-update, or a
//! configuration update to the component's own config) needs the *process* to be
//! re-executed — a self-update only takes effect by running the new binary. In both
//! modes the supervisor drains every unit and exits non-zero so the init system
//! restarts the process.

// Crash isolation depends on tokio catching a panicking actor task per-task, which
// only happens with the unwinding panic runtime. Refuse to build the supervisor if
// the release profile is ever switched to `panic = "abort"`, which would let one
// component's panic abort every co-hosted component.
#[cfg(panic = "abort")]
compile_error!(
    "the supervisor requires `panic = \"unwind\"` for per-component crash isolation; \
     `panic = \"abort\"` would let one component's panic abort the whole process"
);

use anyhow::Context;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use futures::FutureExt;
use futures::StreamExt;
use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;
use tedge_actors::Runtime;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeHandle;
use tedge_config::LogLevelReloadHandle;
use tokio::signal::unix;
use tokio::sync::mpsc;
use tracing::error;
use tracing::info;
use tracing::warn;
use tracing::Instrument;

/// Kind of a supervised unit. Drives start ordering (agent before mappers) and
/// which signals target it (SIGUSR1 restarts only mappers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    Agent,
    Mapper,
}

/// How the supervisor manages its units' lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SupervisorMode {
    /// Multi-unit mode (`tedge run all`): crashed units are rebuilt in-process
    /// under a bounded backoff, and SIGUSR1 restarts all mapper units.
    MultiUnit,
    /// Standalone mode (`tedge-agent`, `tedge-mapper`): the first unit failure
    /// exits the process so the init system can restart it. SIGUSR1 is ignored
    /// because restarts are the init system's responsibility.
    Standalone,
}

#[derive(Clone, Copy, Debug)]
pub struct RestartPolicy {
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub max_restarts: usize,
    pub window: Duration,
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            initial_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            max_restarts: 5,
            window: Duration::from_secs(60),
        }
    }
}

/// Produces a fresh component runtime on each call (the rebuildable factory).
pub type RuntimeFactory = Box<dyn Fn() -> BoxFuture<'static, anyhow::Result<Runtime>> + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitStatus {
    /// A runtime is built and its completion task is being awaited.
    Running,
    /// Asked to stop so it can be rebuilt (on-demand restart): rebuild immediately
    /// once it finishes draining.
    Restarting,
    /// Crashed and waiting for its backoff timer before being rebuilt.
    BackingOff,
    /// Exceeded the restart cap; left down without exiting the process.
    GaveUp,
    /// Finished and not to be restarted (clean exit, or supervisor shutdown).
    Stopped,
}

pub struct Unit {
    pub name: String,
    pub kind: UnitKind,
    pub factory: RuntimeFactory,
    pub policy: RestartPolicy,
    status: UnitStatus,
    /// `Some` while a runtime is live — used to request a graceful drain.
    handle: Option<RuntimeHandle>,
    /// Timestamps of recent restarts, pruned to the policy window.
    restarts: VecDeque<Instant>,
    /// Single-instance lock, held for the unit's whole lifetime (across restarts).
    _lock: Option<flockfile::Flockfile>,
}

impl Unit {
    pub fn new(
        name: String,
        kind: UnitKind,
        factory: RuntimeFactory,
        lock: Option<flockfile::Flockfile>,
    ) -> Self {
        Self {
            name,
            kind,
            factory,
            policy: RestartPolicy::default(),
            status: UnitStatus::Stopped,
            handle: None,
            restarts: VecDeque::new(),
            _lock: lock,
        }
    }
}

/// Message sent back to the supervisor when a unit's runtime finishes.
struct UnitFinished {
    id: usize,
    result: Result<(), tedge_actors::RuntimeError>,
}

/// A control action for the supervisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    /// Graceful shutdown of all units (SIGINT/SIGTERM/SIGQUIT). A second one while
    /// already draining forces an immediate exit.
    ShutdownAll,
    /// Restart all mapper units, leaving the agent running (SIGUSR1).
    RestartMappers,
    /// Re-read `system.toml` and apply the log levels live, without restarting
    /// any component (SIGHUP).
    ReloadLogLevels,
}

pub struct Supervisor {
    units: Vec<Unit>,
    events_tx: mpsc::Sender<UnitFinished>,
    events_rx: mpsc::Receiver<UnitFinished>,
    commands_tx: mpsc::Sender<Command>,
    commands_rx: mpsc::Receiver<Command>,
    /// Pending backoff timers; each yields the id of the unit to rebuild.
    backoffs: FuturesUnordered<BoxFuture<'static, usize>>,
    /// Number of units with a live runtime completion task.
    running: usize,
    shutting_down: bool,
    /// Deadline by which a drain must complete before the supervisor force-exits.
    drain_deadline: Option<tokio::time::Instant>,
    drain_timeout: Duration,
    /// Refreshes log levels on SIGHUP; `None` when an override fixes them (see
    /// [`Command::ReloadLogLevels`]).
    log_reload: Option<LogLevelReloadHandle>,
    mode: SupervisorMode,
    /// The failure the run loop exits with once every unit has drained.
    failure: Option<anyhow::Error>,
}

impl Supervisor {
    pub fn new(units: Vec<Unit>) -> Self {
        let (events_tx, events_rx) = mpsc::channel(16);
        let (commands_tx, commands_rx) = mpsc::channel(16);
        Self {
            units,
            events_tx,
            events_rx,
            commands_tx,
            commands_rx,
            backoffs: FuturesUnordered::new(),
            running: 0,
            shutting_down: false,
            drain_deadline: None,
            // Generous bound on the collective drain; each unit's own runtime has a
            // shorter internal cleanup timeout.
            drain_timeout: Duration::from_secs(75),
            log_reload: None,
            mode: SupervisorMode::MultiUnit,
            failure: None,
        }
    }

    pub fn with_mode(mut self, mode: SupervisorMode) -> Self {
        self.mode = mode;
        self
    }

    /// Attaches the handle used to refresh log levels live on SIGHUP.
    pub fn with_log_reload(mut self, log_reload: Option<LogLevelReloadHandle>) -> Self {
        self.log_reload = log_reload;
        self
    }

    /// Constructs and runs a single-unit supervisor for a standalone process.
    ///
    /// Provides the same signal handling and SIGHUP log-level reloading as
    /// `tedge run all`, but for a single component. Crash recovery stays with the
    /// init system: the first unit failure exits the process with an error, just
    /// as the component behaved before it ran under the supervisor.
    pub async fn run_standalone(
        name: String,
        kind: UnitKind,
        factory: RuntimeFactory,
        lock: Option<flockfile::Flockfile>,
        log_reload: Option<LogLevelReloadHandle>,
    ) -> anyhow::Result<()> {
        Supervisor::new(vec![Unit::new(name, kind, factory, lock)])
            .with_log_reload(log_reload)
            .with_mode(SupervisorMode::Standalone)
            .run()
            .await
    }

    /// A handle for injecting [`Command`]s (used by the signal listener, and tests).
    fn commands(&self) -> mpsc::Sender<Command> {
        self.commands_tx.clone()
    }

    /// Builds a unit's runtime and spawns the task that awaits its completion.
    ///
    /// A build failure is treated like a crash: the restart policy applies while
    /// in-process crash recovery is enabled, otherwise it ends the process.
    async fn spawn_unit(&mut self, id: usize) {
        if self.shutting_down {
            return;
        }
        // In multi-unit mode, build (and thereby spawn the unit's actors) inside a
        // `component` span so every log record the unit emits is attributed to it in
        // the shared process log stream. The runtime captures this span at
        // construction. A standalone unit owns the whole log stream, so the span
        // would only prefix every record with noise and change the log format the
        // component had before it ran under the supervisor.
        let span = if self.units.len() > 1 {
            tracing::info_span!("component", name = %self.units[id].name)
        } else {
            tracing::Span::none()
        };
        let build = (self.units[id].factory)().instrument(span);
        match build.await {
            Ok(runtime) => {
                let handle = runtime.get_handle();
                let name = self.units[id].name.clone();
                let tx = self.events_tx.clone();
                tokio::spawn(async move {
                    let result = runtime.run_to_completion_supervised().await;
                    let _ = tx.send(UnitFinished { id, result }).await;
                });
                self.units[id].handle = Some(handle);
                self.units[id].status = UnitStatus::Running;
                self.running += 1;
                info!(component = %name, "started");
            }
            Err(err) if self.mode == SupervisorMode::MultiUnit => {
                error!(component = %self.units[id].name, "failed to build: {err:#}");
                self.schedule_restart_or_give_up(id);
            }
            Err(err) => {
                error!(component = %self.units[id].name, "failed to build: {err:#}");
                let name = self.units[id].name.clone();
                self.exit_with(err.context(format!("failed to build {name}")))
                    .await;
            }
        }
    }

    /// Records the failure the run loop exits with and drains every unit; once the
    /// drain completes the process exits non-zero so the init system takes over.
    async fn exit_with(&mut self, failure: anyhow::Error) {
        if self.failure.is_none() {
            self.failure = Some(failure);
        }
        if !self.shutting_down {
            self.begin_shutdown().await;
        }
    }

    /// Records a crash and either schedules a backoff restart or gives up if the
    /// unit has crashed too often within its window.
    fn schedule_restart_or_give_up(&mut self, id: usize) {
        let now = Instant::now();
        let policy = self.units[id].policy;

        // Drop restart records older than the window.
        while let Some(&front) = self.units[id].restarts.front() {
            if now.duration_since(front) > policy.window {
                self.units[id].restarts.pop_front();
            } else {
                break;
            }
        }

        if self.units[id].restarts.len() >= policy.max_restarts {
            self.units[id].status = UnitStatus::GaveUp;
            error!(
                component = %self.units[id].name,
                "crashed {} times within {:?}; giving up (process stays up, other components keep running)",
                policy.max_restarts, policy.window
            );
            return;
        }

        let attempt = self.units[id].restarts.len() as u32;
        let backoff = policy
            .initial_backoff
            .saturating_mul(2u32.saturating_pow(attempt))
            .min(policy.max_backoff);
        self.units[id].restarts.push_back(now);
        self.units[id].status = UnitStatus::BackingOff;
        info!(
            component = %self.units[id].name,
            "restarting in {:?} (attempt {})", backoff, attempt + 1
        );
        self.backoffs.push(
            async move {
                tokio::time::sleep(backoff).await;
                id
            }
            .boxed(),
        );
    }

    async fn on_unit_finished(&mut self, finished: UnitFinished) {
        let UnitFinished { id, result } = finished;
        self.running -= 1;
        self.units[id].handle = None;
        let status = self.units[id].status;
        let name = self.units[id].name.clone();

        match status {
            UnitStatus::Restarting => {
                info!(component = %name, "restarting");
                self.spawn_unit(id).await;
            }
            UnitStatus::Stopped => {
                info!(component = %name, "stopped");
            }
            _ => match result {
                Ok(()) => {
                    info!(component = %name, "exited cleanly; not restarting");
                    self.units[id].status = UnitStatus::Stopped;
                }
                // A self-update (or an update of the component's own configuration)
                // only takes effect by re-executing the binary: rebuilding the unit
                // in-process would keep running the old code, so exit and let the
                // init system restart the process.
                Err(err @ RuntimeError::RestartRequired) => {
                    info!(component = %name, "requested a process restart; draining and exiting");
                    self.units[id].status = UnitStatus::Stopped;
                    self.exit_with(
                        anyhow::Error::from(err)
                            .context(format!("{name} requested a process restart")),
                    )
                    .await;
                }
                Err(err) if self.mode == SupervisorMode::MultiUnit => {
                    error!(component = %name, "crashed: {err}");
                    self.schedule_restart_or_give_up(id);
                }
                Err(err) => {
                    error!(component = %name, "crashed: {err}");
                    self.units[id].status = UnitStatus::Stopped;
                    self.exit_with(anyhow::Error::from(err).context(format!("{name} crashed")))
                        .await;
                }
            },
        }
    }

    async fn restart_units(&mut self, selector: impl Fn(&Unit) -> bool) {
        if self.shutting_down {
            return;
        }
        for id in 0..self.units.len() {
            if !selector(&self.units[id]) {
                continue;
            }
            match self.units[id].status {
                UnitStatus::Running => {
                    self.units[id].status = UnitStatus::Restarting;
                    if let Some(handle) = self.units[id].handle.as_mut() {
                        let _ = handle.shutdown().await;
                    }
                }
                UnitStatus::GaveUp => {
                    self.units[id].restarts.clear();
                    self.spawn_unit(id).await;
                }
                _ => {
                    info!(
                        component = %self.units[id].name,
                        "restart ignored (already restarting or in backoff)"
                    );
                }
            }
        }
    }

    fn reload_log_levels(&mut self) {
        let Some(handle) = self.log_reload.as_ref() else {
            warn!("SIGHUP: log levels are fixed by --log-level/--debug/RUST_LOG; ignoring");
            return;
        };
        info!("SIGHUP: reloading log levels from system.toml");
        match handle.reload() {
            Ok(()) => info!("log levels reloaded"),
            Err(err) => warn!("failed to reload log levels; keeping current levels: {err}"),
        }
    }

    async fn restart_mappers(&mut self) {
        if self.mode == SupervisorMode::Standalone {
            info!("SIGUSR1: ignored (standalone process; restart via the service manager instead)");
            return;
        }
        info!("SIGUSR1: restarting all mapper components");
        self.restart_units(|unit| unit.kind == UnitKind::Mapper)
            .await;
    }

    async fn begin_shutdown(&mut self) {
        info!("shutdown requested; draining all components");
        self.shutting_down = true;
        self.drain_deadline = Some(tokio::time::Instant::now() + self.drain_timeout);
        self.backoffs.clear();

        for id in (0..self.units.len()).rev() {
            if let Some(handle) = self.units[id].handle.as_mut() {
                let _ = handle.shutdown().await;
            }
            self.units[id].status = UnitStatus::Stopped;
        }
    }

    /// Registers the process signal handlers and runs the supervisor to completion.
    pub async fn run(self) -> anyhow::Result<()> {
        spawn_signal_listener(self.commands())?;
        self.run_loop().await
    }

    /// The core supervisor loop, free of OS-signal machinery so it can be driven
    /// deterministically in tests via [`Supervisor::commands`].
    ///
    /// Ends once every unit has stopped: after a drain (termination signal, restart
    /// request, or a failure with in-process crash recovery disabled), or when every
    /// unit has exited cleanly of its own accord — a unit that gave up restarting
    /// keeps the process (and its co-hosted peers) up instead.
    ///
    /// Returns an error when a unit requested a process restart, or when a unit
    /// failed while in-process crash recovery is disabled — the non-zero process
    /// exit is what hands recovery over to the init system.
    async fn run_loop(mut self) -> anyhow::Result<()> {
        for id in 0..self.units.len() {
            self.spawn_unit(id).await;
        }

        loop {
            let all_stopped = self
                .units
                .iter()
                .all(|unit| unit.status == UnitStatus::Stopped);
            if self.running == 0 && (self.shutting_down || all_stopped) {
                info!("all components stopped; exiting");
                break;
            }

            let drain_deadline = self.drain_deadline;
            let shutting_down = self.shutting_down;

            tokio::select! {
                Some(finished) = self.events_rx.recv() => {
                    self.on_unit_finished(finished).await;
                }
                Some(id) = self.backoffs.next() => {
                    self.spawn_unit(id).await;
                }
                Some(command) = self.commands_rx.recv() => {
                    match command {
                        Command::ShutdownAll if shutting_down => {
                            warn!("second termination signal; forcing exit");
                            break;
                        }
                        Command::ShutdownAll => self.begin_shutdown().await,
                        Command::RestartMappers => self.restart_mappers().await,
                        Command::ReloadLogLevels => self.reload_log_levels(),
                    }
                }
                _ = async {
                    match drain_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                }, if shutting_down => {
                    warn!("drain timed out; forcing exit");
                    break;
                }
            }
        }

        match self.failure.take() {
            None => Ok(()),
            Some(failure) => Err(failure),
        }
    }
}

/// Translates process signals into supervisor [`Command`]s.
fn spawn_signal_listener(commands: mpsc::Sender<Command>) -> anyhow::Result<()> {
    let mut sigint =
        unix::signal(unix::SignalKind::interrupt()).context("registering SIGINT handler")?;
    let mut sigterm =
        unix::signal(unix::SignalKind::terminate()).context("registering SIGTERM handler")?;
    let mut sigquit =
        unix::signal(unix::SignalKind::quit()).context("registering SIGQUIT handler")?;
    let mut sigusr1 =
        unix::signal(unix::SignalKind::user_defined1()).context("registering SIGUSR1 handler")?;
    let mut sighup =
        unix::signal(unix::SignalKind::hangup()).context("registering SIGHUP handler")?;

    tokio::spawn(async move {
        loop {
            let command = tokio::select! {
                _ = sigint.recv() => Command::ShutdownAll,
                _ = sigterm.recv() => Command::ShutdownAll,
                _ = sigquit.recv() => Command::ShutdownAll,
                _ = sigusr1.recv() => Command::RestartMappers,
                _ = sighup.recv() => Command::ReloadLogLevels,
            };
            if commands.send(command).await.is_err() {
                break;
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use tedge_actors::Actor;
    use tedge_actors::Builder;
    use tedge_actors::DynSender;
    use tedge_actors::NullSender;
    use tedge_actors::RuntimeError;
    use tedge_actors::RuntimeRequest;
    use tedge_actors::RuntimeRequestSink;
    use tokio::time::timeout;

    #[test]
    fn gives_up_after_max_restarts_within_window() {
        let unit = dummy_unit("x", UnitKind::Mapper, policy(3, Duration::from_secs(60)));
        let mut sup = Supervisor::new(vec![unit]);

        for _ in 0..3 {
            sup.schedule_restart_or_give_up(0);
            assert_eq!(sup.units[0].status, UnitStatus::BackingOff);
        }
        sup.schedule_restart_or_give_up(0);
        assert_eq!(sup.units[0].status, UnitStatus::GaveUp);
    }

    #[test]
    fn crashes_outside_the_window_do_not_count_towards_the_cap() {
        let unit = dummy_unit("x", UnitKind::Mapper, policy(3, Duration::from_millis(0)));
        let mut sup = Supervisor::new(vec![unit]);

        for _ in 0..10 {
            sup.schedule_restart_or_give_up(0);
            assert_eq!(sup.units[0].status, UnitStatus::BackingOff);
        }
    }

    #[test]
    fn scheduling_a_restart_queues_a_backoff_timer() {
        let unit = dummy_unit("x", UnitKind::Agent, policy(3, Duration::from_secs(60)));
        let mut sup = Supervisor::new(vec![unit]);

        assert!(sup.backoffs.is_empty());
        sup.schedule_restart_or_give_up(0);
        assert_eq!(sup.backoffs.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_crashing_unit_is_restarted_while_others_keep_running() {
        let agent_builds = Arc::new(AtomicUsize::new(0));
        let mapper_builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![
            make_unit(
                "agent",
                UnitKind::Agent,
                healthy_factory(agent_builds.clone()),
                policy,
            ),
            make_unit(
                "mapper",
                UnitKind::Mapper,
                crashing_factory(mapper_builds.clone()),
                policy,
            ),
        ];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || mapper_builds.load(Ordering::SeqCst) >= 3,
            "the crashing mapper to be restarted",
        )
        .await;
        assert_eq!(
            agent_builds.load(Ordering::SeqCst),
            1,
            "the agent must keep running, isolated from the mapper crashing"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn repeated_crashes_back_off_and_eventually_give_up_without_exiting() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(3);

        let units = vec![make_unit(
            "mapper",
            UnitKind::Mapper,
            crashing_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || builds.load(Ordering::SeqCst) >= 4,
            "the mapper to exhaust its restart budget",
        )
        .await;

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            builds.load(Ordering::SeqCst),
            4,
            "the unit must not be restarted after giving up"
        );
        assert!(
            !handle.is_finished(),
            "the process must stay up after a unit gives up"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restart_mappers_restarts_only_mappers() {
        let agent_builds = Arc::new(AtomicUsize::new(0));
        let mapper_builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![
            make_unit(
                "agent",
                UnitKind::Agent,
                healthy_factory(agent_builds.clone()),
                policy,
            ),
            make_unit(
                "mapper",
                UnitKind::Mapper,
                healthy_factory(mapper_builds.clone()),
                policy,
            ),
        ];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || {
                agent_builds.load(Ordering::SeqCst) == 1
                    && mapper_builds.load(Ordering::SeqCst) == 1
            },
            "both units to start",
        )
        .await;

        commands.send(Command::RestartMappers).await.unwrap();

        wait_until(
            || mapper_builds.load(Ordering::SeqCst) >= 2,
            "the mapper to be restarted",
        )
        .await;
        assert_eq!(
            agent_builds.load(Ordering::SeqCst),
            1,
            "RestartMappers must leave the agent running untouched"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn rapid_mapper_restart_requests_do_not_overlap_live_actors() {
        let mapper_builds = Arc::new(AtomicUsize::new(0));
        let live_actors = Arc::new(AtomicUsize::new(0));
        let max_live_actors = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "mapper",
            UnitKind::Mapper,
            leaky_shutdown_factory(
                mapper_builds.clone(),
                live_actors.clone(),
                max_live_actors.clone(),
            ),
            policy,
        )];
        let mut supervisor = Supervisor::new(units);
        supervisor.drain_timeout = Duration::from_millis(100);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || mapper_builds.load(Ordering::SeqCst) == 1,
            "the mapper to start",
        )
        .await;

        commands.send(Command::RestartMappers).await.unwrap();
        commands.send(Command::RestartMappers).await.unwrap();

        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(61)).await;
        tokio::task::yield_now().await;
        tokio::task::yield_now().await;
        wait_until(
            || mapper_builds.load(Ordering::SeqCst) >= 2,
            "the mapper to be restarted",
        )
        .await;

        assert_eq!(
            max_live_actors.load(Ordering::SeqCst),
            1,
            "a mapper restart must not start a replacement while the previous actor is still alive"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_drains_all_units_and_exits() {
        let agent_builds = Arc::new(AtomicUsize::new(0));
        let mapper_builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![
            make_unit(
                "agent",
                UnitKind::Agent,
                healthy_factory(agent_builds.clone()),
                policy,
            ),
            make_unit(
                "mapper",
                UnitKind::Mapper,
                healthy_factory(mapper_builds.clone()),
                policy,
            ),
        ];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || {
                agent_builds.load(Ordering::SeqCst) == 1
                    && mapper_builds.load(Ordering::SeqCst) == 1
            },
            "both units to start",
        )
        .await;

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should drain and exit on shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_unit_that_fails_to_build_backs_off_and_eventually_gives_up() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(3);

        let units = vec![make_unit(
            "mapper",
            UnitKind::Mapper,
            failing_build_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || builds.load(Ordering::SeqCst) >= 4,
            "the unit to exhaust its build-retry budget",
        )
        .await;

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            builds.load(Ordering::SeqCst),
            4,
            "a unit that gave up must not be rebuilt"
        );
        assert!(
            !handle.is_finished(),
            "a failed build must not take the process down"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_unit_that_exits_cleanly_is_not_restarted_while_others_keep_running() {
        let agent_builds = Arc::new(AtomicUsize::new(0));
        let mapper_builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![
            make_unit(
                "agent",
                UnitKind::Agent,
                healthy_factory(agent_builds.clone()),
                policy,
            ),
            make_unit(
                "mapper",
                UnitKind::Mapper,
                clean_exit_factory(mapper_builds.clone()),
                policy,
            ),
        ];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || mapper_builds.load(Ordering::SeqCst) >= 1,
            "the mapper to be built once",
        )
        .await;

        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            mapper_builds.load(Ordering::SeqCst),
            1,
            "a cleanly-exited unit must not be restarted"
        );
        assert!(
            !handle.is_finished(),
            "the process must stay up for the still-running unit"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_standalone_unit_that_exits_cleanly_ends_the_process() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "agent",
            UnitKind::Agent,
            clean_exit_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units).with_mode(SupervisorMode::Standalone);
        let handle = tokio::spawn(supervisor.run_loop());

        timeout(Duration::from_secs(5), handle)
            .await
            .expect("a standalone clean exit should end the process")
            .unwrap()
            .expect("a clean exit must end the process without an error");
        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "a cleanly-exited unit must not be rebuilt"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_process_exits_cleanly_once_every_unit_has_stopped() {
        let agent_builds = Arc::new(AtomicUsize::new(0));
        let mapper_builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![
            make_unit(
                "agent",
                UnitKind::Agent,
                clean_exit_factory(agent_builds.clone()),
                policy,
            ),
            make_unit(
                "mapper",
                UnitKind::Mapper,
                clean_exit_factory(mapper_builds.clone()),
                policy,
            ),
        ];
        let supervisor = Supervisor::new(units);
        let handle = tokio::spawn(supervisor.run_loop());

        timeout(Duration::from_secs(5), handle)
            .await
            .expect("the supervisor should exit once every unit has stopped")
            .unwrap()
            .expect("clean unit exits must end the process without an error");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_second_termination_signal_forces_an_immediate_exit() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "stubborn",
            UnitKind::Mapper,
            stubborn_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || builds.load(Ordering::SeqCst) >= 1,
            "the stubborn unit to start",
        )
        .await;

        commands.send(Command::ShutdownAll).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !handle.is_finished(),
            "the supervisor must keep draining after a single termination signal"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("a second termination signal should force an immediate exit")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_drain_that_overruns_its_deadline_forces_an_exit() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "stubborn",
            UnitKind::Mapper,
            stubborn_factory(builds.clone()),
            policy,
        )];
        let mut supervisor = Supervisor::new(units);
        supervisor.drain_timeout = Duration::from_millis(100);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || builds.load(Ordering::SeqCst) >= 1,
            "the stubborn unit to start",
        )
        .await;

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("the drain deadline should force the supervisor to exit")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn standalone_supervisor_shuts_down_cleanly() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "agent",
            UnitKind::Agent,
            healthy_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || builds.load(Ordering::SeqCst) == 1,
            "the standalone unit to start",
        )
        .await;

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("standalone supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_standalone_unit_crash_exits_the_process_with_an_error() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "agent",
            UnitKind::Agent,
            crashing_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units).with_mode(SupervisorMode::Standalone);
        let handle = tokio::spawn(supervisor.run_loop());

        let result = timeout(Duration::from_secs(5), handle)
            .await
            .expect("a standalone crash should stop the supervisor")
            .unwrap();
        assert!(
            result.is_err(),
            "a standalone crash must exit the process with an error, for the init system to restart it"
        );
        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "a standalone unit must not be rebuilt in-process after a crash"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_standalone_build_failure_exits_the_process_with_an_error() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "agent",
            UnitKind::Agent,
            failing_build_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units).with_mode(SupervisorMode::Standalone);
        let handle = tokio::spawn(supervisor.run_loop());

        let result = timeout(Duration::from_secs(5), handle)
            .await
            .expect("a standalone build failure should stop the supervisor")
            .unwrap();
        assert!(
            result.is_err(),
            "a standalone build failure must exit the process with an error"
        );
        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "a standalone unit must not be rebuilt in-process after a build failure"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_restart_request_exits_a_standalone_process_instead_of_rebuilding_the_unit() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "agent",
            UnitKind::Agent,
            restart_required_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units).with_mode(SupervisorMode::Standalone);
        let handle = tokio::spawn(supervisor.run_loop());

        let result = timeout(Duration::from_secs(5), handle)
            .await
            .expect("a restart request should stop the supervisor")
            .unwrap();
        assert!(
            result.is_err(),
            "a restart request must exit the process with an error, for the init system to re-execute the binary"
        );
        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "a restart request must not rebuild the unit in-process"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_restart_request_drains_co_hosted_units_and_exits_the_process() {
        let agent_builds = Arc::new(AtomicUsize::new(0));
        let mapper_builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![
            make_unit(
                "agent",
                UnitKind::Agent,
                restart_required_factory(agent_builds.clone()),
                policy,
            ),
            make_unit(
                "mapper",
                UnitKind::Mapper,
                healthy_factory(mapper_builds.clone()),
                policy,
            ),
        ];
        let supervisor = Supervisor::new(units);
        let handle = tokio::spawn(supervisor.run_loop());

        let result = timeout(Duration::from_secs(5), handle)
            .await
            .expect("a restart request should drain the co-hosted units and stop the supervisor")
            .unwrap();
        assert!(
            result.is_err(),
            "a restart request must exit the whole process with an error, even with crash recovery enabled"
        );
        assert_eq!(
            agent_builds.load(Ordering::SeqCst),
            1,
            "a restart request must not rebuild the unit in-process"
        );
        assert_eq!(
            mapper_builds.load(Ordering::SeqCst),
            1,
            "the co-hosted unit must be drained, not restarted"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reload_log_levels_invokes_the_handle() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "agent",
            UnitKind::Agent,
            healthy_factory(builds.clone()),
            policy,
        )];
        let mut supervisor = Supervisor::new(units);
        // No log reload handle — the command should log a warning but not crash.
        supervisor.log_reload = None;
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(|| builds.load(Ordering::SeqCst) == 1, "the unit to start").await;

        commands.send(Command::ReloadLogLevels).await.unwrap();
        // If we get here without panic/hang, the command was handled.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "ReloadLogLevels must not stop the supervisor"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn restart_mappers_is_a_noop_for_a_single_agent_unit() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "agent",
            UnitKind::Agent,
            healthy_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(|| builds.load(Ordering::SeqCst) == 1, "the agent to start").await;

        commands.send(Command::RestartMappers).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "RestartMappers must not restart an agent-only supervisor"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn restart_mappers_is_ignored_in_standalone_mode() {
        let builds = Arc::new(AtomicUsize::new(0));
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "mapper",
            UnitKind::Mapper,
            healthy_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units).with_mode(SupervisorMode::Standalone);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(|| builds.load(Ordering::SeqCst) == 1, "the mapper to start").await;

        commands.send(Command::RestartMappers).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "RestartMappers must be ignored in standalone mode"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
    }

    fn dummy_unit(name: &str, kind: UnitKind, policy: RestartPolicy) -> Unit {
        let mut unit = Unit::new(
            name.to_string(),
            kind,
            Box::new(|| async { Err(anyhow::anyhow!("dummy")) }.boxed()),
            None,
        );
        unit.policy = policy;
        unit
    }

    fn policy(max_restarts: usize, window: Duration) -> RestartPolicy {
        RestartPolicy {
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
            max_restarts,
            window,
        }
    }

    fn test_policy(max_restarts: usize) -> RestartPolicy {
        RestartPolicy {
            initial_backoff: Duration::from_millis(2),
            max_backoff: Duration::from_millis(5),
            max_restarts,
            window: Duration::from_secs(60),
        }
    }

    fn make_unit(
        name: &str,
        kind: UnitKind,
        factory: RuntimeFactory,
        policy: RestartPolicy,
    ) -> Unit {
        let mut unit = Unit::new(name.to_string(), kind, factory, None);
        unit.policy = policy;
        unit
    }

    fn healthy_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async { Ok(Runtime::new()) }.boxed()
        })
    }

    fn crashing_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async {
                let mut runtime = Runtime::new();
                runtime.spawn(CrashActorBuilder).await?;
                Ok(runtime)
            }
            .boxed()
        })
    }

    fn restart_required_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async {
                let mut runtime = Runtime::new();
                runtime.spawn(RestartRequiredActorBuilder).await?;
                Ok(runtime)
            }
            .boxed()
        })
    }

    fn failing_build_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async { Err(anyhow::anyhow!("simulated build failure")) }.boxed()
        })
    }

    fn clean_exit_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async {
                let runtime = Runtime::new();
                let mut handle = runtime.get_handle();
                handle.shutdown().await.ok();
                Ok(runtime)
            }
            .boxed()
        })
    }

    fn stubborn_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async {
                let mut runtime = Runtime::new();
                runtime.spawn(StubbornActorBuilder).await?;
                Ok(runtime)
            }
            .boxed()
        })
    }

    fn leaky_shutdown_factory(
        builds: Arc<AtomicUsize>,
        live_actors: Arc<AtomicUsize>,
        max_live_actors: Arc<AtomicUsize>,
    ) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            let live_actors = live_actors.clone();
            let max_live_actors = max_live_actors.clone();
            async move {
                let mut runtime = Runtime::new();
                runtime
                    .spawn(LeakyShutdownActorBuilder {
                        live_actors,
                        max_live_actors,
                    })
                    .await?;
                Ok(runtime)
            }
            .boxed()
        })
    }

    async fn wait_until(mut cond: impl FnMut() -> bool, what: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !cond() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    struct CrashActor;

    #[async_trait::async_trait]
    impl Actor for CrashActor {
        fn name(&self) -> &str {
            "crash"
        }

        async fn run(self) -> Result<(), RuntimeError> {
            Err(RuntimeError::ActorError("simulated crash".into()))
        }
    }

    struct CrashActorBuilder;

    impl Builder<CrashActor> for CrashActorBuilder {
        type Error = std::convert::Infallible;

        fn try_build(self) -> Result<CrashActor, Self::Error> {
            Ok(CrashActor)
        }

        fn build(self) -> CrashActor {
            CrashActor
        }
    }

    impl RuntimeRequestSink for CrashActorBuilder {
        fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
            NullSender.into()
        }
    }

    struct RestartRequiredActor;

    #[async_trait::async_trait]
    impl Actor for RestartRequiredActor {
        fn name(&self) -> &str {
            "restart-required"
        }

        async fn run(self) -> Result<(), RuntimeError> {
            Err(RuntimeError::RestartRequired)
        }
    }

    struct RestartRequiredActorBuilder;

    impl Builder<RestartRequiredActor> for RestartRequiredActorBuilder {
        type Error = std::convert::Infallible;

        fn try_build(self) -> Result<RestartRequiredActor, Self::Error> {
            Ok(RestartRequiredActor)
        }

        fn build(self) -> RestartRequiredActor {
            RestartRequiredActor
        }
    }

    impl RuntimeRequestSink for RestartRequiredActorBuilder {
        fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
            NullSender.into()
        }
    }

    struct StubbornActor;

    #[async_trait::async_trait]
    impl Actor for StubbornActor {
        fn name(&self) -> &str {
            "stubborn"
        }

        async fn run(self) -> Result<(), RuntimeError> {
            std::future::pending::<()>().await;
            Ok(())
        }
    }

    struct StubbornActorBuilder;

    impl Builder<StubbornActor> for StubbornActorBuilder {
        type Error = std::convert::Infallible;

        fn try_build(self) -> Result<StubbornActor, Self::Error> {
            Ok(StubbornActor)
        }

        fn build(self) -> StubbornActor {
            StubbornActor
        }
    }

    impl RuntimeRequestSink for StubbornActorBuilder {
        fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
            NullSender.into()
        }
    }

    struct LeakyShutdownActor {
        live_actors: Arc<AtomicUsize>,
        max_live_actors: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Actor for LeakyShutdownActor {
        fn name(&self) -> &str {
            "leaky-shutdown"
        }

        async fn run(self) -> Result<(), RuntimeError> {
            let live = self.live_actors.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_live_actors.fetch_max(live, Ordering::SeqCst);
            let _guard = LiveActorGuard(self.live_actors.clone());
            std::future::pending::<()>().await;
            #[allow(unreachable_code)]
            Ok(())
        }
    }

    struct LiveActorGuard(Arc<AtomicUsize>);

    impl Drop for LiveActorGuard {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    struct LeakyShutdownActorBuilder {
        live_actors: Arc<AtomicUsize>,
        max_live_actors: Arc<AtomicUsize>,
    }

    impl Builder<LeakyShutdownActor> for LeakyShutdownActorBuilder {
        type Error = std::convert::Infallible;

        fn try_build(self) -> Result<LeakyShutdownActor, Self::Error> {
            Ok(self.build())
        }

        fn build(self) -> LeakyShutdownActor {
            LeakyShutdownActor {
                live_actors: self.live_actors,
                max_live_actors: self.max_live_actors,
            }
        }
    }

    impl RuntimeRequestSink for LeakyShutdownActorBuilder {
        fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
            NullSender.into()
        }
    }
}
