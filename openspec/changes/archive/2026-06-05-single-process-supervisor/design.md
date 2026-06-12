## Goal

Allow the core tedge components (the agent and a mapper) to run inside a single
process under a built-in supervisor, so tedge can run on devices or in containers
that have no init system. The supervisor restarts a component after it crashes and
can restart all mapper components on demand. The existing multi-process workflow
(one process per component, managed by systemd) is preserved unchanged.

## Decisions

### Additive refactor: extract `build()`, keep two callers

Each component's entry point is split into a shared assembly function — roughly
`build(opt, config) -> (RuntimeHandle, completion_future)` — and two thin callers:

- the **standalone runner** (`run()`), preserved bit-for-bit, including its
  `std::process::exit(1)` on runtime error, its own `SignalActor`, its own
  flockfile, and its own MQTT connection. The existing `main.rs` multicall arms
  (`tedge-mapper c8y`, `tedge-agent`, …) are untouched.
- the **supervisor**, a second consumer of `build()` that owns signals centrally
  and applies a restart policy instead of exiting.

`build()` doubles as the rebuildable unit factory: calling it again yields a fresh
incarnation. The risk surface is contained to "extract `build()` without changing
what `run()` does." This is what lets the single-process mode be *tried out*
without touching the proven multi-process path.

### `tedge run all` is a new multicall arm

The supervisor is reached through a new run mode. Existing arms are unchanged; this
is purely additive.

### Coarse units = whole components

A supervised unit is an entire component (the agent, a mapper) — the unit a user
recognises. Individual actors remain implementation details. "Restart the mapper"
means tearing down and rebuilding that component's `Runtime`, which the builder
model already supports (everything is wired at build time, then spawned).

### Crash isolation: no `process::exit` in the supervised path

Today `Runtime::run_to_completion()` calls `std::process::exit(1)` on a runtime
error — lethal in a shared process. In the supervised path the assembled runtime
instead returns its error to the supervisor, which restarts that unit. This,
combined with `panic = "unwind"` (so a panicking actor task is caught per-task by
tokio rather than aborting the process), is what buys per-unit crash isolation.

### Restart policy: restart-on-crash with bounded backoff

A crashed unit is rebuilt via its factory after a backoff, up to a maximum number
of attempts within a window, after which the supervisor gives up on that unit (and
logs it) rather than hot-looping. MVP defaults; values tunable later.

### The supervisor owns all signals — one handler in the process

`tokio::signal::unix` registration is process-global and broadcasts to every
stream, so per-unit `SignalActor`s are removed from the supervised path. The single
handler translates signals into supervisor actions:

- `SIGINT` / `SIGTERM` / `SIGQUIT` → graceful shutdown of all units (fan out
  `RuntimeRequest::Shutdown`, await drain bounded by the runtime's existing cleanup
  timeout, then exit). A second `SIGTERM` (or timeout expiry) forces abort.
- `SIGUSR1` → restart all units of kind *mapper*; the agent is left running.
- `SIGHUP` / `SIGUSR2` → reserved as documented future hooks (reload / restart-all).

A restart request for a unit that is already restarting or in backoff is coalesced
(ignored), so repeated `SIGUSR1`s do not stack.

### Restart targeting: SIGUSR1 now, unix socket + CLI later

The MVP control plane is the untargeted-ish `SIGUSR1` (all mappers). The long-term
plane is a unix domain socket plus a `tedge service restart <name>` verb, which
feeds the *same* internal "restart units matching X" action — the signal is just
the dumb version of that command, and both converge on one API. The socket is
preferred long-term because it works even when MQTT is unavailable.

### Best-effort start ordering, not a dependency gate

The supervisor *spawns* the agent before mappers (and stops in reverse), but does
**not** wait for the agent to be ready and does **not** block or fail a mapper if
the agent is slow or down. Ordering is the order `build()` is called, nothing more.
The any-order startup correctness that exists today (agent and mapper may start in
either order) is fully preserved.

### Per-unit MQTT connections — kept long-term, not just for the MVP

Each unit keeps its own MQTT connection/session. This is deliberate, not a
shortcut. Non-clean sessions give broker-side offline queueing only at the
connection boundary: when a unit restarts, its connection drops, the broker queues
its QoS≥1 messages, and replays them on reconnect — exactly today's behaviour under
systemd, with no message loss.

A single shared MQTT actor (the obvious dedup target) would *defeat* this: the
shared connection never drops across a unit restart, so the broker keeps delivering
to a live connection whose in-process consumer has gone away, and those messages
are silently lost during the restart window. Reintroducing the queue inside the
actor means bounded, non-persistent in-process buffering — strictly weaker than
what mosquitto already provides for free. If actor deduplication is pursued later,
it should target lightweight actors (health/timer), not the MQTT connection.

### MVP runs a single mapper instance

Multiple mappers in one process is a known must-have but is deferred: the mapper
sets the process-global `TEDGE_CLOUD_PROFILE` env var to pass the profile to
children, so two mappers in one process would clobber each other. The fix is to
thread the profile explicitly rather than via env — done when multi-mapper lands.
Profiles are also being deprecated as the mechanism for running multiple mappers,
so this is not the long-term shape anyway.

### Flockfile retained, per-component, guarding external duplicates

The existing single-instance lock is kept per component. Its job in this mode is to
catch a user who forgot to stop a systemd-managed service before trying
`tedge run all`. The current implementation is sufficient.

### Single log subscriber with per-component attribution

One tracing subscriber is initialised per process. Each unit's logs carry a field
(or span) identifying the originating component, so a single combined log stream is
still clearly attributable.

### No embedded broker

An in-process broker (e.g. compiling in `rumqttd`) was considered and rejected for
now. `rumqttd` does not meet our requirements — notably it has no persistence — and
is not well maintained, and there is no suitable alternative crate. The single-
process supervisor therefore always assumes an externally provided broker
(mosquitto), exactly as the standalone components do today.

The longer-term direction, once thin-edge has its own service management, is to
optionally let the supervisor take control of *mosquitto* (start/stop/supervise the
real broker) rather than embed one. That is out of scope here and is not designed
against in this change.

## Non-goals / deferred

The MVP is deliberately minimal. The items below are out of scope for it but the
skeleton is designed so each attaches cleanly later — see Future extension points.

- **Multiple mapper instances in one process** — MVP runs a single mapper.
- **Unix socket + `tedge service restart <name>` CLI verb** — the targeted control
  plane; MVP uses `SIGUSR1` (all mappers).
- **Shared/deduplicated MQTT and health actors** — rejected for MQTT (see
  decision); may revisit for lightweight actors only.
- **Readiness gating / health-based start ordering** — ordering is best-effort
  spawn sequence only.
- **Embedded / supervised broker** — `tedge run all` assumes an external mosquitto;
  embedding a broker is rejected (see decision) and managing mosquitto is deferred
  to future service management.

## Future extension points

Captured here so the MVP's interfaces are not accidentally closed against them.

- **Multiple mapper instances.** Blocked today only by the mapper setting the
  process-global `TEDGE_CLOUD_PROFILE` env var to pass the profile to children. The
  fix is to thread the profile explicitly through `build()` instead of via env;
  once that is done the supervisor can hold N mapper units with no further change to
  the supervision skeleton. (Profiles are themselves being deprecated as the way to
  run multiple mappers, so the long-term shape may differ — but the unit model is
  unaffected either way.)

- **Targeted restart via unix socket + CLI.** The supervisor already routes
  `SIGUSR1` through a single internal "restart units matching X" action. The future
  unix domain socket and `tedge service restart <name>` / `tedge service status`
  verbs are just a second trigger of that same action, carrying a unit *name* as
  the selector. Preferred long-term over signals because it works when MQTT is
  unavailable and can name an individual unit.

- **Lightweight actor deduplication.** Sharing a health/timer actor across units is
  plausible by hoisting it into a root runtime the units attach to. The MQTT actor
  is explicitly excluded (see the per-unit MQTT decision) because a shared
  connection defeats broker-side offline queueing on restart. Any dedup also
  requires the shared actor to support *runtime* peer attach/detach, which the
  builder model does not do today (wiring happens before `build()`); that is the
  real cost of this path.

- **Readiness-gated ordering.** Ordering is currently best-effort spawn sequence.
  If a unit ever needs another to be ready first — e.g. a future supervised local
  broker before its clients — the supervisor can grow an optional readiness wait per
  unit without changing the best-effort default for the rest.

- **Supervising mosquitto.** Once thin-edge has its own service management, the
  supervisor could optionally take control of the real broker (mosquitto) as a
  supervised unit — started before the agent and mappers — instead of relying on an
  externally managed one. This replaces the previously-considered embedded broker,
  which was rejected (no persistence, poorly maintained, no suitable alternative).

## Risks / trade-offs

- **Refactor risk**: the value of this change depends on `build()` being extracted
  without altering standalone semantics; the standalone path must remain
  bit-for-bit equivalent.
- **Crash isolation relies on `panic = "unwind"`**: if the release profile ever
  switched to `panic = "abort"`, one actor panic would kill every co-hosted unit.
- **No process-level resource isolation**: co-hosting trades the OS isolation of
  separate processes (a runaway unit cannot be cgroup-bounded independently) for a
  smaller footprint. Accepted for the target deployments.
- **`SIGUSR1` restarts *all* mappers**: coarse, but acceptable while the MVP runs a
  single mapper; targeted restart arrives with the socket control plane.

## Capabilities

### New Capabilities
single-process-supervisor

### Modified Capabilities
