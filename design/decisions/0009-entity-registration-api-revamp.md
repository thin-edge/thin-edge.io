# Entity registration API revamp

* Date: __2026-06-24__
* Status: __Draft__

## Background

%%te%% exposes two APIs to manage entities (devices and services):

* an **MQTT API**
  ([docs](../../docs/src/operate/entity-management/mqtt_api.md),
  [reference](../../docs/src/references/mqtt-api.md)),
  where an entity is registered by publishing a **retained** message
  to its entity-entity metadata topic (e.g. `te/device/child01//`), and
* an **HTTP API** ([docs](../../docs/src/operate/entity-management/rest_api.md)) served by `tedge-agent`,
  which validates the request and, on success, publishes the same retained registration message to the broker.
  With this, the `tedge-agent` was made the central owner of the entity store;

The original MQTT API remains the one most users rely on.
The two APIs have drifted slightly in both behaviour and validation,
and the underlying retained-message model has shown structural problems that this document captures,
before proposing a way forward.

This document is concerned with the **registration / update / delete** lifecycle
and the **twin data** that rides along with it.

## Root cause: the broker plays three incompatible roles

In the current design the MQTT broker is, at the same time:

1. the **ingest channel** — any client may publish a retained registration message to any entity metadata topic;
2. the **persistence layer** — the retained messages are what components like the mappers rely on for persistence,
   despite the agent maintaining a persistent entity store.
3. the **queryable source of truth** —
   components treat the retained messages on `te/+/+/+/+` as the list of registered entities.
   They don't query the `tedge-agent`, which has an authoritative entity store.

There is **no validation gate** between role 1 and role 3.
A client write becomes "the truth" the instant it is retained,
regardless of whether `tedge-agent` accepts it.
The agent validates *after the fact* and has no authority to correct what is already on the broker.
Almost every issue below is a symptom of collapsing these three roles into one writable, unvalidated namespace.

The HTTP API does not escape this:
a successful HTTP registration is still published to the broker as a retained message
so an HTTP-created entity can be overwritten by any MQTT client afterwards.
HTTP buys validated *writes*, not a protected *source of truth*.

## Catalogue of issues

### A. The entity data on the broker is easily corrupted

* **A1 — Rejected registrations leave broken retained messages.**
   A bad registration message, rejected by the agent, stays with the broker even after the rejection.
   There is no compensating publish.
   The broker keeps serving a message the agent refuses to honour,
   so the broker's view and the agent's view disagree permanently and never reconverge.

* **A2 — A valid retained message can be replaced by an invalid one.**
   Because the entity metadata topic is writable by anyone,
   a second publisher can clobber a good registration with a bad one.
   The agent rejects the new message but, per issue A1, the bad message stays retained.

* **A3 — Registration and update share one channel, with surprising replace semantics.**
   An MQTT "update" is a **full struct replace**, not a merge:
   the code builds `EntityMetadata { twin_data: merged, ..entity_metadata }`,
   so `@id`, `@type`, `@parent` and `@health` all come from the new message.
   * An update that omits `@id` **silently drops** the external id (`@id` becomes `None`).
   * This contradicts the documentation, which states `@type` and `@id` "cannot be updated after the registration".
     Nothing enforces that — both can be silently changed or dropped on re-publish.
   * The MQTT update example in the docs republishes only `@type`+`@parent` (no `@id`),
     which would wipe the id of an entity originally registered with one.
   * Twin data is *merged* while entity metadata is *replaced* —
     two different merge semantics in one message, undocumented.

* **A4 — Empty payload is overloaded as "delete".**
   An entity can never have an intentionally empty registration,
   and any accidental empty retained publish (a common scripting mistake)
   deletes the entity and its entire subtree, with no confirmation.
   Combined with issue A2 (anyone can publish), this is a sharp footgun.

### B. Twin data carried in the registration message

* **B1 — Inline twin data creates a second, never-maintained copy.**
   The twin fragments inside the retained registration message are republished to `twin/<key>`.
   The fragment now lives in two places.
   Twin-topic updates (HTTP or MQTT) update only `twin/<key>` —
   they never rewrite the inline copy in the registration message.

* **B2 — On restart the stale inline copy wins.**
   `load_from_message_log` deliberately **skips** replaying `twin/` topics
   ("Twin fragments are intentionally not restored from the entity store log")
   but **does** replay the registration message,
   so the only twin value the rebuilt store sees is the stale inline copy, and it overwrites the newer value.
   This is exactly the family of bugs documented by the `Skip`-ped tests in
   [`entity_store_persistence.robot`](../../tests/RobotFramework/tests/tedge_agent/entity_store_persistence.robot):
   * a twin updated via the `twin/` topic is lost on restart;
   * a twin updated while the agent is offline is lost on restart;
   * a twin **deleted** via an empty `twin/` payload is **resurrected** on restart by the inline copy.

### C. MQTT vs HTTP behavioural divergence (same operation, different result)

* **C1 — `@`-prefixed keys are rejected in one path and silently absorbed in another.**
    The twin HTTP API rejects any fragment key starting with `@` (`400`).
    But the registration payload has no `deny_unknown_fields`
    and `#[serde(flatten)]`s everything else into `twin_data`,
    so a `@whatever` key in a registration message is silently accepted
    *and then published to a `twin/@whatever` topic* — the very thing the twin API forbids.
    (`EntityUpdateMessage` *does* set `deny_unknown_fields`, so the three payload types validate inconsistently.)
    The MQTT docs only "discourage" `@` twin keys; the HTTP docs hard-reject them.

* **C2 — Parent-dependency model differs.**
   HTTP registration is *strict*: an unknown `@parent` returns `400 NoParent`.
   MQTT registration is *eventual*:
   a child-before-parent is cached in `pending_entity_store` and silently held
   (`update()` catches `NoParent` and returns `Ok(vec![])`).
   The same registration succeeds over MQTT and fails over HTTP.

### D. No message ordering across topics

* **D1 — No ordering between registration and data messages; every consumer must reorder.**
   An entity's lifecycle is spread across topics:
   registration on `te/device/<id>//`, twin fragments on `.../twin/<key>`,
   telemetry on `.../m/<type>`, and so on.
   MQTT guarantees ordered delivery only per topic (and only within one client session per QoS flow),
   so a consumer can observe data for an entity before its registration.
   Compounding this, MQTT registration is fire-and-forget:
   a publisher that *wants* to order its publishes correctly
   has no confirmation signal to wait on before sending data.

   Today this is worked around inside %%te%%, per consumer:
   both `tedge-agent` and the `c8y-mapper` embed a `PendingEntityStore`
   that caches child registrations arriving before their parent
   and early data messages of unregistered entities.
   The workaround is duplicated between the two components,
   and — most importantly — is not available to anyone else:
   every third-party consumer of `te/#` must reimplement the same machinery.

* **D2 — The ordering problem on the cloud.**
    Even though the c8y mapper processes the messages in the right order,
    it fans out the converted messages to independent bridge topics:
    a child device is created with SmartREST `101` on the *parent's* topic
    (`c8y/s/us`, or `c8y/s/us/<parent-xid>` for nested children),
    while the child's own telemetry goes to `c8y/s/us/<child-xid>`
    or `c8y/measurement/measurements/create`.
    Neither the MQTT bridge nor the cloud-side guarantees ordering *across* these topics,
    so the cloud can process data for a child
    before the child's managed object exists.
    And since `101` is itself fire-and-forget,
    the mapper has no confirmation to sequence against —
    the same missing-barrier problem as issue D1, one hop further.

### E. Service status at registration

* **E1 — Service registration fabricates an initial status.**
    A service's registration message declares its *existence* but carries no *liveness* information;
    the actual status lives on the separate `status/health` channel.
    The c8y mapper, however, must supply a status in the service creation message
    (SmartREST `102`), and it hardcodes **`up`** —
    so every service appears as "up" in the cloud from the moment of registration,
    even if it has never started, crashed before publishing,
    or was registered on its behalf by another component.
    The true status only converges when (and if)
    the service publishes its first health message,
    and a service that never does stays falsely "up" forever.
    This is also a special case of issue D1:
    registration and health status ride different topics,
    with no ordering between them and no way to register both facts atomically.

## Design constraints

Any solution must respect these constraints (mostly for backward compatibility):

1. **The retained registration message on the entity metadata topic is sacred.**
   The overwhelming majority of users register entities
   by publishing a retained message to `te/device/<id>//`.
   This behaviour cannot be broken.
   It is the one hard constraint.

2. **Inline twin data in the registration message is expendable if there is no other way.**
   Its only purpose was convenience:
   register a device with all its initial twin data in one shot,
   instead of publishing each twin fragment separately afterwards.
   It is rarely used and is the direct cause of the category-B issues.
   Breaking or constraining it is acceptable as a last resort,
   ideally while preserving the one-shot convenience.

3. **Mappers validate eventually, independently, and possibly differently from each other.**
   A registration that is valid for the agent may be invalid for a mapper
   (e.g. an external id that conflicts in the cloud),
   and one that is valid for the c8y mapper may be invalid for the Azure mapper.
   Mappers may not even be running at registration time —
   they can come up much later
   and must be able to fetch and replicate everything registered so far.
   Therefore **mapper-level validation can never gate local registration**;
   it happens asynchronously and reports back out-of-band.

4. **The agent must eventually reconcile state from both the device side and the cloud side.**
   Local registrations/twins originate on the device;
   some twin values and managed objects originate in the cloud.
   The agent needs a defined reconciliation with explicit precedence
   so the two sources converge instead of clobbering each other
   (a generalisation of the B2 restart problem to the cloud dimension).

5. **The parent dependency model difference between HTTP and MQTT APIs was an intentional design choice.**
   Both APIs are designed differently as per their strenths/limitations.
   The HTTP API can immediately reject a child without a parent with feedback while MQTT can not.
   There is no desire to align the behaviour on this aspect.

6. A **malicious actor with write access to the broker is explicitly a non-goal.**
   Such an actor can already delete any device by clearing its topics,
   impersonate any component, or flood the broker —
   none of which any registration-API redesign can prevent.
   This is an **authorization** problem,
   and authorization is only solvable at the transport layer:
   broker ACLs and per-client authentication (mosquitto, tedge's default broker, supports both).
   Because backward compatibility requires that ordinary clients keep publishing to `te/+/+/+/+`,
   nothing at the API layer can distinguish a malicious authorized writer from a legitimate one.
   We therefore do not try to; we point operators at broker ACLs and move on.

## Ideal solution without backward compatibility constraints

Before evaluating backward-compatible proposals,
it is worth stating how the API would be designed today *without* constraint 1,
as a north star to judge the proposals against.
No mechanism inside MQTT is a silver bullet for cross-topic ordering.
The root-cause section identified the broker doing three jobs at once.
A greenfield design splits those jobs instead of collapsing them:

* **Registration is a request, not a retained fact.**
  Entity lifecycle (register / update / delete) is a request/response exchange with the agent:
  a non-retained request carrying a client-chosen request id,
  answered by a single terminal response —
  accepted (with the normalized entity) or rejected (with the reason).
  The response *is* the registration confirmation —
  the ordering barrier a publisher waits on before sending data (issue D1) —
  and the request id resolves the multi-publisher ambiguity for free.
  (With MQTT 5, the response-topic and correlation-data properties
  provide this exchange natively.)
* **Authoritative state is retained on agent-owned entity metadata topics.**
  Only the agent writes to the `te/device/<entity>/<topic>/<id>` topic.
  Consumers never see unvalidated retained input,
  so no marker is needed to tell input apart from truth.
* **The broker is not the queryable source of truth for consumers.**
  Late-starting consumers (e.g. mappers) bootstrap from the agent's HTTP query API —
  a consistent snapshot — and then follow the live topics.
  Retained messages remain a notification/cache layer, not the store,
  which removes the retained-replay ordering problem for late joiners entirely.
* **Data messages are order-tolerant by specification, not by per-consumer improvisation.**
  Data received for an unknown entity creates a *provisional* entity,
  upgraded by merge when its authoritative registration arrives
  (a generalisation of today's auto-registration).
  With this mandated by the spec, out-of-order data stops being every consumer's private problem.

This proposal is rejected only because of backward compatibility constraints.
So, the further proposals are trying to come close to this ideal solution.

## Proposal A — Agent owns the entity metadata topic; user publishes treated as requests

The core idea is to separate the **desired-state input** from the **authoritative-state output**,
and make `tedge-agent` the single writer of the **authoritative state**.

* Clients express *intent* (register / update / delete / set-twin).
* The agent is the only component that validates intent and publishes the resulting authoritative entity state.
* Mappers and external observers read only the authoritative state;
  they never see unvalidated input as truth.
* Mapper/cloud validation is a separate, eventual layer
  that reports status on a dedicated channel and feeds back into reconciliation,
  never blocking local acceptance.

### A.1 Telling user requests apart from validated data

This is the crux of keeping a single shared topic.
If the agent re-publishes validated data to the *same* entity metadata topic the user writes to,
how does a mapper — or any other listener — tell a raw, not-yet-validated user message
apart from agent-validated truth?
On a single topic with last-writer-wins retention, it cannot, for two reasons:

* there is a window between a user's publish and the agent's validation
  in which the retained message is unvalidated input; and
* after the agent re-publishes, nothing in the payload says "this one is the validated version".

A listener that acts on whatever it reads
will act on transient, unvalidated, or about-to-be-rolled-back input.
So yes — distinguishing the two is required not just for mappers but for *every* listener
trying to determine the real truth.

The minimal-breakage answer is a reserved marker that only the agent ever sets on the output.
A bad actor can mimic this marker, but we aren't immune to bad actors anyway as already established.

* On a valid registration message, the agent re-publishes the normalized entity **with** the marker
  like `@source: "tedge-agent"`.
* On a **rejected** message, the agent
  (a) publishes a structured error to `te/errors` describing why, and
  (b) **restores the last-known-good** marked message if one exists,
  or **clears** the topic if the entity never existed.
  This directly fixes issues A1 and A2:
  the broker can no longer be left holding a message the agent rejected.
* **Listeners act only on messages bearing the agent's marker.**
  An unmarked message is treated as "pending validation"
  and ignored until the agent confirms it (or rolls it back).

The same mechanism solves the agent's own echo problem:
when the agent's re-published message comes back to it
(the agent is subscribed to the same topics),
the marker plus a match against its store identifies it as its own echo,
so it is a no-op and there is no republish loop.

**Legacy Clients**

Third-party components that reads `te/+/+/+/+` today would get notified twice on those topics.
They should have expected it anyway, as upates via the same topics were allowed earlier as well.
The clients publishing the registrations wouldn't feel any difference,
as they are less likely to be subscribed to the same topic.
They can subscribe to them now, to get notified about the agent accepting/rejecting them.

:::note
Restoring or clearing a retained message that the agent did not author is a fundamental change.
It must be documented and comunicated heavily on roll-out.
:::

### A.2 Twin data in the registration message: accepted, but never stored twice

* The agent keeps **accepting** twin fragments inside a registration message
  (the one-shot convenience is preserved).
* On intake it **splits** them out:
  identity is retained on the entity metadata topic;
  each twin fragment is published to its `twin/<key>` topic.
* The **normalized registration message the agent republishes contains identity only — no twin.**
  So there is never a second retained copy of a twin fragment,
  and `load_from_message_log` can no longer resurrect a stale value.
* Restart behaviour changes so the `twin/` topics are authoritative for twin values
  (the registration message no longer carries any), fixing both category-B issues.

**Breakage.**
A client that re-reads its *own* registration retained message
and expects to find the twin fields it sent will no longer find them there
(they are on the `twin/` topics instead).
This is the acceptable last-resort breakage the constraints permit.
The convenience of one-shot registration-with-twin itself is retained.

### A.3 Reject changes to fixed fields; validate the same way everywhere

* Reject changes to `@id`/`@type` on re-publish (update).
  The only update that is allowed is the updation of `@parent` and `@health` just like the HTTP API.
* Apply the same twin-key validation (no leading `@`, no `/`, non-empty)
  at registration intake as the twin API already applies,
  so `@`-keys can no longer sneak onto `twin/` topics via registration (issue C1).
  Unknown `@`-keys in a registration are rejected rather than absorbed.
* These restrictions can't be considered breaking changes,
  as we are just tightening previously undefined behaviour.

### A.4 Wait for the registration confirmation before publishing data

Issue D1 has two sides; this addresses the producer side.

Until now, MQTT registration offered nothing to synchronize on.
The agent-marked republish of A.1 is the first confirmation signal
the MQTT API has ever had,
and it enables a documented **producer contract**:

1. Subscribe to your own entity metadata topic (and optionally `te/errors`).
2. Publish the registration message (retained, QoS 1).
3. Wait for a message on that topic bearing the agent marker
   whose identity fields (`@type`, `@id`, `@parent`) match what was sent.
   On an error on `te/errors`, handle the rejection;
   on timeout, re-publish
   (re-publishing is idempotent —
   the agent treats a matching re-publish as a no-op echo).
4. Only then publish twin and telemetry data for that entity.

Clients that can use HTTP should simply prefer the HTTP API,
whose synchronous `201` response is the same barrier without the subscription dance.

Caveats that must accompany this contract:

* **A.1 alone makes the ordering window worse, not better.**
  The marked republish necessarily lands *after* the raw user publish,
  so a producer that publishes registration-then-data without waiting
  now almost guarantees that marker-trusting consumers
  see the data before the trusted registration.
  The producer wait defined here is what closes the window;
  A.1 only provides the signal.
* **An MQTT confirmation is state, not a response.**
  When several clients write the same topic,
  a marked message means "the authoritative state is now X",
  not "*your* request was accepted".
  Request-level accept/reject feedback exists only on the HTTP API
  (and best-effort via `te/errors`).
* Even for fully compliant producers,
  cross-topic delivery order to a subscriber
  is broker implementation behaviour, not an MQTT guarantee.
  The barrier shrinks the consumer-side problem to a rare residue —
  it cannot erase it, which is why A.5 remains necessary.

### A.5 Handling data that arrives before its entity is registered

The consumer side of issue D1.
Legacy producers will keep publishing registration and data back-to-back without waiting,
so every consumer will keep receiving data for entities it does not know yet.
This cannot be solved once, centrally, in the agent:
data messages are not routed through the agent —
the broker delivers them directly to every subscriber.
The agent can clean up the registration topics (A.1),
but it cannot hold back or re-order anyone else's data subscriptions.
(For the mapper specifically, this is why marked registrations alone are not enough:
it trusts them alone for *truth*,
but a marked registration is published only after validation —
strictly later than the raw input —
so back-to-back publishes practically guarantee
that the data arrives before the marked registration does.)

Every consumer therefore needs a strategy for early data.
That strategy does **not** have to be today's pending-entity store.
The right response differs by channel, because the channels differ in kind:

* **Retained state channels — twin, alarms, health, command metadata — need no caching at all.**
  These messages are retained,
  and a broker re-delivers retained messages on every new subscription.
  A consumer that learns of a new entity can open a short-lived,
  entity-specific subscription (`te/device/<id>/...`)
  and receive all of that entity's retained state again —
  workable from any programming language, with no state to manage.
  (Twin data can equally be fetched from the agent's HTTP API.)
  The duplicate deliveries this causes are harmless:
  state messages are idempotent by nature.
* **Measurements are not worth caching — dropping them is acceptable.**
  A measurement stream supersedes itself; the next sample is moments away.
  More fundamentally, the loss boundary is arbitrary:
  hardware typically starts emitting at boot,
  so everything published before the consumer started,
  or before anyone sent the registration,
  is already lost — and already accepted as lost.
  Caching a brief pre-registration window
  pretends to a completeness the channel never had.
  (Today's implementation also shares one telemetry ring buffer across all entities,
  so a single noisy unregistered entity evicts everyone else's cached data.)
* **Events are the one channel where a small buffer earns its keep.**
  Events are one-shot, not retained, and unrecoverable if missed —
  and the pre-registration window is exactly when the most meaningful ones fire
  (boot, service start).
  A small, bounded buffer is justified here,
  with drops logged and counted (ideally reported on `te/errors`), never silent.

For %%te%%'s own components,
this slims the shared pending store down to
orphan-registration handling plus a bounded event buffer,
with observable drops and configurable bounds.

For third-party consumers, the per-channel strategy *is* the point.
Other mappers are written by other teams, in other languages —
a shared Rust crate helps none of them.
What they get instead is a behavioural contract in plain language:
act only on marked registrations;
on discovering a new entity, re-subscribe to its topics to recover retained state
(or query the agent's HTTP API);
buffer events briefly if events matter to you;
let pre-registration measurements go.
Nothing to port.

**Startup ordering of the registrations themselves.**
Even the marked registrations ride one topic per entity,
so they have no cross-topic ordering guarantee among themselves:
on (re)subscribe in particular, the broker replays retained messages in no defined order,
and a child's marked message can arrive before its parent's.
The orphan-registration handling therefore remains necessary at startup.

### A.6 c8y mapper: hold back data until the entity is created in the cloud

Issue D2 cannot be fixed by ordering publishes better —
a `101` on `c8y/s/us` and a measurement on `c8y/measurement/measurements/create`
are never ordered relative to each other,
no matter when they are published.
The fix is the same barrier pattern applied one hop further:
**do not release an entity's data to the cloud
until its creation there is confirmed.**

* The mapper keeps a per-entity gate:
  data converted for a newly registered entity is held (bounded queue)
  until the entity's managed object is confirmed to exist,
  then flushed in order.
* Preferred confirmation mechanism:
  create child devices/services via the **HTTP inventory API**
  through the mapper's existing HTTP proxy,
  instead of fire-and-forget SmartREST `101`.
  The HTTP response confirms the creation,
  returns the internal id immediately
  (removing today's separate identity lookup),
  and surfaces real errors like external-id conflicts —
  feeding the out-of-band mapper validation channel that constraint 3 calls for.
* Trade-offs to accept:
  the first data of a freshly registered entity
  is delayed by the creation round-trip;
  during a cloud/HTTP outage the hold queues are bounded,
  so the overflow policy must be defined (and observable, per A.5);
  and the gate must cover *every* outbound channel for the entity —
  a single ungated channel reintroduces the race.

This change is internal to the c8y mapper
and is orthogonal to which proposal is chosen;
it is listed here to complete the end-to-end story for issue D2.

### A.7 Registering a service does not mean the service is up

Issue E1 disappears once nothing is fabricated:

* The health channel (`status/health`, retained, Last-Will-backed)
  remains the **single authoritative source** of a service's status —
  the same one-fact-one-channel principle as A.2.
* An **initial status may ride inside the registration message**,
  handled with the same split-at-intake semantics as inline twin data (A.2):
  the agent publishes it to the service's `status/health` topic,
  and the normalized registration it republishes carries no status —
  so no second retained copy exists,
  and nothing stale can be resurrected on restart
  (the B1/B2 dual-copy problem does not apply).
  Two rules keep the derived publish harmless:
  * The agent emits the inline status **only on first registration**
    (entity previously unknown), never on updates or replays —
    the same idempotency that protects inline twin data.
  * A **real status always outranks the derived one**.
    The derived publish *can* transiently overwrite a fresher real status:
    a client that publishes registration (inline status X)
    and a real status Y back-to-back
    wins the race against the agent,
    whose derived X only lands after it processed the registration —
    topic history `[Y, X]`, retained value: the stale X.
    But unlike every other race in this document,
    this one is confined to a *single* topic,
    and on a single topic MQTT *does* guarantee
    that every subscriber sees the same message order.
    The agent subscribes to `status/health` itself,
    so its own inbox shows it the proof of displacement —
    a real Y received *before* its own derived echo X —
    and it repairs by republishing Y:
    history `[Y, X, Y]`, retained state converged to the real value
    (a brief flap for consumers, correct final state).
    In the opposite interleaving `[X, Y]` the real status is already
    the last writer and no repair is needed.
    MQTT messages carry no publisher identity,
    so the agent tells its own messages apart
    the same way A.1 already does on the entity topic:
    derived status messages are stamped with the `@source` marker,
    and incoming messages are additionally matched
    against the agent's record of its own in-flight publishes.
    This also covers the nastiest overlap:
    a service that registers with inline "up" and crashes immediately,
    whose Last-Will "down" would otherwise be overwritten
    by the agent's slightly later derived "up".

  Conflicting values are unlikely in legitimate use anyway:
  a self-registering service inlines the same status it would publish itself,
  and a third-party registrant cannot know the service's liveness,
  so it should inline nothing and leave the default —
  inlining "up" on someone else's behalf
  is exactly the E1 fabrication being eliminated.
* Until a status is known — inlined or published —
  the status of a registered service is **`unknown`**, the honest value.
  The c8y mapper creates services with `unknown` instead of the hardcoded `up`
  (Cumulocity models exactly this tri-state: up / down / unknown).
* A service that wants to appear "up" immediately
  either inlines its initial status in the registration message,
  or follows the A.4 producer contract:
  publish registration, await the confirmation,
  then publish its retained health status message.
  If the health message arrives *before* the registration (legacy publishers),
  the A.5 recipe recovers it without caching:
  health status is retained,
  so the entity-specific re-subscription made after the registration
  delivers it again.
* Towards the cloud, service creation (`102`, on the parent's topic)
  and the later status update (`104`, on the service's own topic)
  again ride different `c8y/s/us/...` topics;
  the A.6 gate covers their ordering like any other data channel.

The visible behavioural change:
a freshly registered service that neither inlined an initial status
nor reported one yet shows as "unknown" in the cloud,
where today it falsely shows "up".
That is a correction, not a regression —
monitoring built on the fabricated "up" was never telling the truth.

### How Proposal A addresses each issue

How Proposal A (with A.1–A.7) addresses each issue from the list above:

| Issue                                                                         | Addressed by                                                | Effect                                                                                                                                                                 |
|-------------------------------------------------------------------------------|-------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| A1 — Rejected registrations leave broken retained messages                    | A.1 — restore last-known-good, or clear                     | **Fixed**                                                                                                                                                              |
| A2 — A valid retained message can be replaced by an invalid one               | A.1 — agent restores the last-known-good message            | **Fixed**                                                                                                                                                              |
| A3 — Update is a silent full replace (`@id` dropped, immutability unenforced) | A.1 normalization + A.3 immutability rules                  | **Fixed** — updates limited to `@parent`/`@health`, like HTTP                                                                                                          |
| A4 — Empty payload overloaded as "delete"                                     | A.1 — validation gate                                       | **Mitigated** — the gate is a place to add delete safeguards, but an authorized client clearing the topic still deletes (constraint 6)                                 |
| B1 — Inline twin data creates a second, never-maintained copy                 | A.2 — split at intake; republished identity carries no twin | **Fixed**                                                                                                                                                              |
| B2 — Stale inline twin copy wins on restart                                   | A.2 — `twin/` topics become authoritative                   | **Fixed**                                                                                                                                                              |
| C1 — `@`-keys absorbed via registration but rejected by the twin API          | A.3 — unified validation at intake                          | **Fixed**                                                                                                                                                              |
| C2 — Parent-dependency model differs between HTTP and MQTT                    | —                                                           | **Unchanged by design** (constraint 5: intentional)                                                                                                                    |
| D1 — No registration/data ordering; every consumer must reorder               | A.4 (producer barrier) + A.5 (per-channel consumer recipe)  | **Mitigated** — compliant producers get a reliable barrier; consumers recover retained state via re-subscription, buffer only events, drop early measurements          |
| D2 — Ordering problem re-exported to the cloud                                | A.6 — confirmation-gated forwarding                         | **Fixed** for gated channels — at the cost of first-data latency, and bounded hold queues during cloud outages                                                         |
| E1 — Service registration fabricates an initial `up` status                   | A.7 — `unknown` by default; optional inline initial status split onto the health channel | **Fixed** — services show their inlined/reported status, or the honest `unknown`, never a fabricated `up`                                                             |

### What changes for existing users

* Re-publishing of entity metadata messages with `@source` marker leading to duplicate notifications.
* Stripping twin fields from a "user owned" registration* message.
* Previously-silent malformed inputs
  (changing `@id`/`@type`, `@`-prefixed twin keys, unknown `@`-keys)
  now get rejected with feedback instead of being silently mishandled.
* A recommended producer contract:
  await the agent-marked confirmation (or the HTTP `201`)
  before publishing the first data message of a newly registered entity (A.4).
* Measurements published before an entity's registration is processed
  are no longer cached and replayed — they are dropped,
  like all other pre-registration measurements already are;
  the remaining event buffer reports its drops instead of staying silent (A.5).
* The c8y mapper delays the first data of a newly registered entity
  until its cloud creation is confirmed (A.6).
* Newly registered services appear in the cloud with `unknown` status
  until their first (inlined or published) health status,
  instead of a fabricated `up` (A.7).

## Proposal B - User publishes to entity metadata topic; Agent owns separate entity metadata channel

Not refined further as the duplication of the same data across multiple topic trees isn't ideal.

The duplication could be mitigated by clearing the retained message on the user channel
once the agent has replicated it to its own channel,
turning `te/+/+/+/+` into a pure ingest channel.
But the evaluation still doesn't favour it:

* **No significant ordering advantage over Proposal A.**
  The ordering problem is registration-vs-*data*,
  and data stays on its own topics either way;
  the confirmation barrier (wait for the agent-channel message)
  and the consumer-side tolerance
  are identical in shape to A.4/A.5.
* Its genuine advantage is a cleaner trust model:
  a channel that only ever carries validated truth,
  with no `@source` marker inspection
  and no window in which a retained message is unvalidated input.
* The clearing mitigation, however, breaks the most relied-upon *consumer* behaviour:
  entity discovery via retained replay on `te/+/+/+/+`
  (explicitly advertised in the docs as the entity-store view)
  would come up empty —
  violating the spirit of constraint 1 on the read side.
  Without clearing, the duplication drawback stands.
* Every consumer must migrate to the new channel to benefit,
  whereas Proposal A upgrades the semantics of the topics they already watch.

Same ordering outcome as Proposal A,
at a higher migration and compatibility cost.