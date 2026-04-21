//! Single-process supervisor reached through `tedge run all`.
//!
//! Hosts the core thin-edge components (the agent and a mapper) inside one process
//! so no external init system is required. Each component is a *unit*: a rebuildable
//! factory plus a restart policy. The supervisor owns process-wide signal handling,
//! isolates a crashing unit from its co-hosted peers (rebuilding it under a bounded
//! backoff), and drains everything cleanly on termination.
//!
//! Each component's `build()` factory is also used by the standalone per-component
//! path (`tedge-agent`, `tedge-mapper c8y`, …); only the way the resulting runtime
//! is driven differs.

// Crash isolation depends on tokio catching a panicking actor task per-task, which
// only happens with the unwinding panic runtime. Refuse to build the supervisor if
// the release profile is ever switched to `panic = "abort"`, which would let one
// component's panic abort every co-hosted component.
#[cfg(panic = "abort")]
compile_error!(
    "the single-process supervisor (`tedge run all`) requires `panic = \"unwind\"` for \
     per-component crash isolation; `panic = \"abort\"` would let one component's panic \
     abort the whole process"
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
use tedge_actors::RuntimeHandle;
use tedge_agent::AgentOpt;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init_for_services;
use tedge_config::TEdgeConfig;
use tedge_mapper::MapperName;
use tokio::signal::unix;
use tokio::sync::mpsc;
use tracing::error;
use tracing::info;
use tracing::warn;
use tracing::Instrument;

/// `tedge run all` — run the agent and (optionally) a mapper under one supervisor.
#[derive(Debug, clap::Parser)]
pub struct RunAllOpt {
    /// The mapper to run alongside the agent (e.g. `c8y`, `aws`, `az`).
    #[clap(subcommand)]
    pub mapper: Option<MapperName>,

    #[command(flatten)]
    pub common: CommonArgs,
}

/// Kind of a supervised unit. Drives start ordering (agent before mappers) and
/// which signals target it (SIGUSR1 restarts only mappers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitKind {
    Agent,
    Mapper,
}

#[derive(Clone, Copy, Debug)]
struct RestartPolicy {
    initial_backoff: Duration,
    max_backoff: Duration,
    max_restarts: usize,
    window: Duration,
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
type RuntimeFactory = Box<dyn Fn() -> BoxFuture<'static, anyhow::Result<Runtime>> + Send>;

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

struct Unit {
    name: String,
    kind: UnitKind,
    factory: RuntimeFactory,
    policy: RestartPolicy,
    status: UnitStatus,
    /// `Some` while a runtime is live — used to request a graceful drain.
    handle: Option<RuntimeHandle>,
    /// Timestamps of recent restarts, pruned to the policy window.
    restarts: VecDeque<Instant>,
    /// Single-instance lock, held for the unit's whole lifetime (across restarts).
    _lock: Option<flockfile::Flockfile>,
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
}

struct Supervisor {
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
}

impl Supervisor {
    fn new(units: Vec<Unit>) -> Self {
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
        }
    }

    /// A handle for injecting [`Command`]s (used by the signal listener, and tests).
    fn commands(&self) -> mpsc::Sender<Command> {
        self.commands_tx.clone()
    }

    /// Builds a unit's runtime and spawns the task that awaits its completion.
    ///
    /// A build failure is treated like a crash so the restart policy still applies
    /// (and the process is not taken down).
    async fn spawn_unit(&mut self, id: usize) {
        if self.shutting_down {
            return;
        }
        // Build (and thereby spawn the unit's actors) inside a `component` span so
        // every log record the unit emits is attributed to it in the shared process
        // log stream. The runtime captures this span at construction.
        let span = tracing::info_span!("component", name = %self.units[id].name);
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
            Err(err) => {
                error!(component = %self.units[id].name, "failed to build: {err:#}");
                self.schedule_restart_or_give_up(id);
            }
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

    /// Handles a unit's runtime finishing, reacting to clean exit vs error/panic and
    /// to whether the stop was requested (restart / shutdown) or unexpected.
    async fn on_unit_finished(&mut self, finished: UnitFinished) {
        let UnitFinished { id, result } = finished;
        self.running -= 1;
        self.units[id].handle = None;
        let status = self.units[id].status;
        let name = self.units[id].name.clone();

        match status {
            UnitStatus::Restarting => {
                // On-demand restart (e.g. SIGUSR1): rebuild immediately.
                info!(component = %name, "restarting");
                self.spawn_unit(id).await;
            }
            UnitStatus::Stopped => {
                // Supervisor-initiated graceful shutdown: stay down.
                info!(component = %name, "stopped");
            }
            _ => match result {
                Ok(()) => {
                    // The component decided to exit on its own; honour that.
                    info!(component = %name, "exited cleanly; not restarting");
                    self.units[id].status = UnitStatus::Stopped;
                }
                Err(err) => {
                    error!(component = %name, "crashed: {err}");
                    self.schedule_restart_or_give_up(id);
                }
            },
        }
    }

    /// Restarts every unit matching `selector`, coalescing requests for units already
    /// restarting or in backoff. SIGUSR1 targets the mappers (see
    /// [`Supervisor::restart_mappers`]).
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
                    // Operator intervention: give a unit that gave up a fresh chance.
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

    /// Restarts all mapper units, leaving the agent running (the SIGUSR1 action).
    async fn restart_mappers(&mut self) {
        info!("SIGUSR1: restarting all mapper components");
        self.restart_units(|unit| unit.kind == UnitKind::Mapper)
            .await;
    }

    /// Begins a collective graceful shutdown and arms the drain deadline. Drain
    /// *requests* go out in reverse start order, but the units then drain
    /// concurrently; `run_loop` exits once all have finished or the deadline elapses.
    async fn begin_shutdown(&mut self) {
        info!("shutdown requested; draining all components");
        self.shutting_down = true;
        self.drain_deadline = Some(tokio::time::Instant::now() + self.drain_timeout);
        // Cancel any pending backoff restarts.
        self.backoffs.clear();

        for id in (0..self.units.len()).rev() {
            // Live units drain via their handle and report back through a
            // `UnitFinished` event; units already down (in backoff / gave up) simply
            // become terminal. Either way the unit must not be restarted.
            if let Some(handle) = self.units[id].handle.as_mut() {
                let _ = handle.shutdown().await;
            }
            self.units[id].status = UnitStatus::Stopped;
        }
    }

    /// Registers the process signal handlers and runs the supervisor to completion.
    async fn run(self) -> anyhow::Result<()> {
        // The supervisor owns all signal handling for the process. Each signal is
        // translated into a [`Command`] and fed to the same loop as any future
        // control-plane trigger.
        spawn_signal_listener(self.commands())?;
        self.run_loop().await
    }

    /// The core supervisor loop: starts every unit, then services unit completions,
    /// backoff restarts and control commands until everything has drained (after a
    /// shutdown command) or the drain deadline forces an exit.
    ///
    /// Free of any OS-signal machinery so it can be driven deterministically in tests
    /// via [`Supervisor::commands`].
    async fn run_loop(mut self) -> anyhow::Result<()> {
        // Best-effort start ordering: spawn units in declaration order (agent first),
        // with no readiness gate between them.
        for id in 0..self.units.len() {
            self.spawn_unit(id).await;
        }

        loop {
            if self.shutting_down && self.running == 0 {
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
                            // Second termination signal: abort immediately.
                            warn!("second termination signal; forcing exit");
                            break;
                        }
                        Command::ShutdownAll => self.begin_shutdown().await,
                        Command::RestartMappers => self.restart_mappers().await,
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

        Ok(())
    }
}

/// Translates process signals into supervisor [`Command`]s:
///
/// - SIGINT / SIGTERM / SIGQUIT — graceful shutdown of every component
/// - SIGUSR1 — restart the mappers, leaving the agent running
///
/// The supervisor owns all signal handling, so the per-component `SignalActor` is
/// unused on this path.
fn spawn_signal_listener(commands: mpsc::Sender<Command>) -> anyhow::Result<()> {
    let mut sigint =
        unix::signal(unix::SignalKind::interrupt()).context("registering SIGINT handler")?;
    let mut sigterm =
        unix::signal(unix::SignalKind::terminate()).context("registering SIGTERM handler")?;
    let mut sigquit =
        unix::signal(unix::SignalKind::quit()).context("registering SIGQUIT handler")?;
    let mut sigusr1 =
        unix::signal(unix::SignalKind::user_defined1()).context("registering SIGUSR1 handler")?;

    tokio::spawn(async move {
        loop {
            let command = tokio::select! {
                _ = sigint.recv() => Command::ShutdownAll,
                _ = sigterm.recv() => Command::ShutdownAll,
                _ = sigquit.recv() => Command::ShutdownAll,
                _ = sigusr1.recv() => Command::RestartMappers,
            };
            // The supervisor has exited (loop dropped the receiver): stop listening.
            if commands.send(command).await.is_err() {
                break;
            }
        }
    });

    Ok(())
}

/// Entry point for `tedge run all`: assembles the units and runs the supervisor.
pub async fn run(opt: RunAllOpt) -> anyhow::Result<()> {
    // A single tracing subscriber for the whole process; each unit's logs are
    // attributed to its component via the `component` span field.
    let log_services = log_service_names(opt.mapper.as_ref());
    let log_services: Vec<_> = log_services.iter().map(String::as_str).collect();
    log_init_for_services(&log_services, &opt.common.log_args, &opt.common.config_dir)?;

    let config_dir = opt.common.config_dir.clone();
    let tedge_config = TEdgeConfig::load(&config_dir).await?;

    let mut units: Vec<Unit> = Vec::new();

    // Agent unit — spawned first (best-effort ordering).
    {
        let lock = tedge_agent::acquire_lock(&tedge_config).context("acquiring agent lock")?;
        let agent_opt = AgentOpt {
            common: opt.common.clone(),
            mqtt_device_topic_id: None,
            mqtt_topic_root: None,
        };
        let config_dir = config_dir.clone();
        let factory: RuntimeFactory = Box::new(move || {
            let config_dir = config_dir.clone();
            let agent_opt = agent_opt.clone();
            async move {
                let config = TEdgeConfig::load(&config_dir).await?;
                tedge_agent::build(agent_opt, config).await
            }
            .boxed()
        });
        units.push(Unit {
            name: tedge_agent::AGENT_NAME.to_string(),
            kind: UnitKind::Agent,
            factory,
            policy: RestartPolicy::default(),
            status: UnitStatus::Stopped,
            handle: None,
            restarts: VecDeque::new(),
            _lock: lock,
        });
    }

    // Mapper unit — optional, spawned after the agent.
    if let Some(mapper) = opt.mapper {
        let name = mapper.to_string();
        let lock = tedge_mapper::acquire_lock(&name, &tedge_config)
            .with_context(|| format!("acquiring lock for {name}"))?;
        let config_dir = config_dir.clone();
        let factory: RuntimeFactory = Box::new(move || {
            let config_dir = config_dir.clone();
            let mapper = mapper.clone();
            async move {
                let config = TEdgeConfig::load(&config_dir).await?;
                tedge_mapper::build(mapper, config).await
            }
            .boxed()
        });
        units.push(Unit {
            name,
            kind: UnitKind::Mapper,
            factory,
            policy: RestartPolicy::default(),
            status: UnitStatus::Stopped,
            handle: None,
            restarts: VecDeque::new(),
            _lock: lock,
        });
    }

    Supervisor::new(units).run().await
}

fn log_service_names(mapper: Option<&MapperName>) -> Vec<String> {
    let mut services = vec![
        "tedge".to_string(),
        tedge_agent::AGENT_NAME.to_string(),
        "tedge-mapper".to_string(),
    ];
    if let Some(mapper) = mapper {
        services.push(mapper.log_service_name().to_string());
    }
    services
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

        // Each crash within the window schedules a backoff restart...
        for _ in 0..3 {
            sup.schedule_restart_or_give_up(0);
            assert_eq!(sup.units[0].status, UnitStatus::BackingOff);
        }
        // ...until the cap is exceeded, at which point the unit is left down.
        sup.schedule_restart_or_give_up(0);
        assert_eq!(sup.units[0].status, UnitStatus::GaveUp);
    }

    #[test]
    fn run_all_logging_considers_supervisor_agent_and_mapper_services() {
        assert_eq!(
            log_service_names(Some(&MapperName::UserDefined(vec!["local".to_string()]))),
            vec![
                "tedge".to_string(),
                tedge_agent::AGENT_NAME.to_string(),
                "tedge-mapper".to_string(),
                "tedge-mapper".to_string(),
            ]
        );
    }

    #[test]
    fn crashes_outside_the_window_do_not_count_towards_the_cap() {
        // A zero-length window means every prior restart has aged out, so a unit
        // crashing slowly never exhausts its budget.
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
        // A high cap so the mapper keeps being restarted for the duration of the test.
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

        // The crashing mapper is rebuilt again and again...
        wait_until(
            || mapper_builds.load(Ordering::SeqCst) >= 3,
            "the crashing mapper to be restarted",
        )
        .await;
        // ...while the healthy agent was built exactly once and never disturbed.
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
        // 1 initial build + 3 restarts, then give up.
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

        // After the cap it must stop restarting, and crucially the supervisor process
        // must stay alive rather than exiting.
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

        // The SIGUSR1 action.
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

        // The SIGTERM action: drains every unit, then the loop returns and the
        // process exits.
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
        // 1 initial build attempt + 3 retries, then give up.
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

        // A build failure is treated like a crash, so the unit is retried under the
        // same backoff policy until its budget is exhausted.
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
    async fn a_unit_that_exits_cleanly_is_not_restarted() {
        let builds = Arc::new(AtomicUsize::new(0));
        // A high cap so any restart would be visible rather than a give-up.
        let policy = test_policy(1000);

        let units = vec![make_unit(
            "mapper",
            UnitKind::Mapper,
            clean_exit_factory(builds.clone()),
            policy,
        )];
        let supervisor = Supervisor::new(units);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || builds.load(Ordering::SeqCst) >= 1,
            "the unit to be built once",
        )
        .await;

        // The component returned `Ok` of its own accord, so the supervisor must honour
        // that and leave it down rather than restarting it.
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "a cleanly-exited unit must not be restarted"
        );

        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("supervisor should exit after shutdown")
            .unwrap()
            .unwrap();
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

        // The first termination signal asks the unit to drain, but it never does, so
        // with the (generous) drain timeout the supervisor is still draining a moment
        // later.
        commands.send(Command::ShutdownAll).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(
            !handle.is_finished(),
            "the supervisor must keep draining after a single termination signal"
        );

        // A second termination signal abandons the drain and exits immediately.
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
        // Force the drain deadline to fire quickly; the stubborn unit never drains.
        supervisor.drain_timeout = Duration::from_millis(100);
        let commands = supervisor.commands();
        let handle = tokio::spawn(supervisor.run_loop());

        wait_until(
            || builds.load(Ordering::SeqCst) >= 1,
            "the stubborn unit to start",
        )
        .await;

        // The unit ignores the drain request, so the supervisor exits once the drain
        // deadline elapses rather than hanging forever.
        commands.send(Command::ShutdownAll).await.unwrap();
        timeout(Duration::from_secs(5), handle)
            .await
            .expect("the drain deadline should force the supervisor to exit")
            .unwrap()
            .unwrap();
    }

    fn dummy_unit(name: &str, kind: UnitKind, policy: RestartPolicy) -> Unit {
        Unit {
            name: name.to_string(),
            kind,
            factory: Box::new(|| async { Err(anyhow::anyhow!("dummy")) }.boxed()),
            policy,
            status: UnitStatus::Stopped,
            handle: None,
            restarts: VecDeque::new(),
            _lock: None,
        }
    }

    fn policy(max_restarts: usize, window: Duration) -> RestartPolicy {
        RestartPolicy {
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(1),
            max_restarts,
            window,
        }
    }

    /// A policy with tiny backoffs so restart behaviour plays out quickly in tests.
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
        Unit {
            name: name.to_string(),
            kind,
            factory,
            policy,
            status: UnitStatus::Stopped,
            handle: None,
            restarts: VecDeque::new(),
            _lock: None,
        }
    }

    /// A factory whose runtime stays alive until the supervisor drains it (a healthy
    /// component). It records each build so tests can tell when, and how often, it
    /// was (re)started.
    fn healthy_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            // An empty runtime parks on its actions channel until it receives a
            // shutdown request via its handle, so it models a running component.
            async { Ok(Runtime::new()) }.boxed()
        })
    }

    /// A factory whose runtime crashes shortly after starting (an actor that returns
    /// an error), exercising the crash-restart path. Records each build.
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

    /// A factory that always fails to build its runtime, exercising the build-failure
    /// path (which the supervisor treats like a crash). Records each attempt.
    fn failing_build_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async { Err(anyhow::anyhow!("simulated build failure")) }.boxed()
        })
    }

    /// A factory whose runtime shuts itself down immediately, so it completes with
    /// `Ok` — modelling a component that exits of its own accord. Records each build.
    fn clean_exit_factory(builds: Arc<AtomicUsize>) -> RuntimeFactory {
        Box::new(move || {
            builds.fetch_add(1, Ordering::SeqCst);
            async {
                let runtime = Runtime::new();
                // Ask the runtime to stop, so `run_to_completion_supervised` returns
                // `Ok` and the supervisor sees a clean exit.
                let mut handle = runtime.get_handle();
                handle.shutdown().await.ok();
                Ok(runtime)
            }
            .boxed()
        })
    }

    /// A factory whose runtime never drains: it hosts an actor that parks forever and
    /// ignores shutdown requests, used to exercise the forced-exit paths. Records each
    /// build.
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

    /// A factory whose actor stays alive after shutdown is requested. This models an
    /// actor that has not actually released its external resource (e.g. an MQTT
    /// client id) by the time the runtime gives up waiting for cleanup.
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

    /// Polls `cond` until it holds, failing the test if it does not within 5s.
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

    /// An actor that fails immediately, so the runtime hosting it reports a crash.
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

    /// An actor that parks forever and never responds to a shutdown request, so the
    /// runtime hosting it cannot drain.
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
