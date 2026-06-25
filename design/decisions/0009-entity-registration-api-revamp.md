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

### A. Source-of-truth integrity

1. **Rejected registrations leave broken retained messages.**
   A bad registration message, rejected by the agent, stays with the broker even after the rejection.
   There is no compensating publish.
   The broker keeps serving a message the agent refuses to honour,
   so the broker's view and the agent's view disagree permanently and never reconverge.

2. **A valid retained message can be replaced by an invalid one.**
   Because the entity metadata topic is writable by anyone,
   a second publisher can clobber a good registration with a bad one.
   The agent rejects the new message but, per issue 1, the bad message stays retained.

3. **Registration and update share one channel, with surprising replace semantics.**
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

4. **Empty payload is overloaded as "delete".**
   An entity can never have an intentionally empty registration,
   and any accidental empty retained publish (a common scripting mistake)
   deletes the entity and its entire subtree, with no confirmation.
   Combined with issue 2 (anyone can publish), this is a sharp footgun.

### B. Twin data carried in the registration message

5. **Inline twin data creates a second, never-maintained copy.**
   The twin fragments inside the retained registration message are republished to `twin/<key>`.
   The fragment now lives in two places.
   Twin-topic updates (HTTP or MQTT) update only `twin/<key>` —
   they never rewrite the inline copy in the registration message.

6. **On restart the stale inline copy wins.**
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

7. **`@`-prefixed keys are rejected in one path and silently absorbed in another.**
    The twin HTTP API rejects any fragment key starting with `@` (`400`).
    But the registration payload has no `deny_unknown_fields`
    and `#[serde(flatten)]`s everything else into `twin_data`,
    so a `@whatever` key in a registration message is silently accepted
    *and then published to a `twin/@whatever` topic* — the very thing the twin API forbids.
    (`EntityUpdateMessage` *does* set `deny_unknown_fields`, so the three payload types validate inconsistently.)
    The MQTT docs only "discourage" `@` twin keys; the HTTP docs hard-reject them.

8. **Parent-dependency model differs.**
   HTTP registration is *strict*: an unknown `@parent` returns `400 NoParent`.
   MQTT registration is *eventual*:
   a child-before-parent is cached in `pending_entity_store` and silently held
   (`update()` catches `NoParent` and returns `Ok(vec![])`).
   The same registration succeeds over MQTT and fails over HTTP.

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
   It is rarely used and is the direct cause of the issue-B family.
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
   (a generalisation of the issue-B restart problem to the cloud dimension).

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

### A.1 Distinguishing intent from validated truth

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
  This directly fixes issues 1 and 2:
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

### A.2 Inline twin: accept at intake, never retain a second copy

* The agent keeps **accepting** twin fragments inside a registration message
  (the one-shot convenience is preserved).
* On intake it **splits** them out:
  identity is retained on the entity metadata topic;
  each twin fragment is published to its `twin/<key>` topic.
* The **normalized registration message the agent republishes contains identity only — no twin.**
  So there is never a second retained copy of a twin fragment,
  and `load_from_message_log` can no longer resurrect a stale value.
* Restart behaviour changes so the `twin/` topics are authoritative for twin values
  (the registration message no longer carries any), fixing the entire issue-B family.

**Breakage.**
A client that re-reads its *own* registration retained message
and expects to find the twin fields it sent will no longer find them there
(they are on the `twin/` topics instead).
This is the acceptable last-resort breakage the constraints permit.
The convenience of one-shot registration-with-twin itself is retained.

### A.3 Enforce immutability and unify validation

* Reject changes to `@id`/`@type` on re-publish (update).
  The only update that is allowed is the updation of `@parent` and `@health` just like the HTTP API.
* Apply the same twin-key validation (no leading `@`, no `/`, non-empty)
  at registration intake as the twin API already applies,
  so `@`-keys can no longer sneak onto `twin/` topics via registration (issue 10).
  Unknown `@`-keys in a registration are rejected rather than absorbed.
* These restrictions can't be considered breaking changes,
  as we are just tightening previously undefined behaviour.


### Summary of behavioural/breaking changes

* Re-publishing of entity metadata messages with `@source` marker leading to duplicate notifications.
* Stripping twin fields from a "user owned" registration* message.
* Previously-silent malformed inputs
  (changing `@id`/`@type`, `@`-prefixed twin keys, unknown `@`-keys)
  now get rejected with feedback instead of being silently mishandled.

## Proposal B - Agent owns the entity metadata topic; separate channel for registration requests

Not refined further as the primary backward compatibility requirement itself is not met.

## Proposal C - User publishes to entity metadata topic; Agent owns separate entity metedata channel

Not refined further as the duplication of the same data across multiple topic trees isn't ideal.