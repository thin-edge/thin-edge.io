---
title: Flows
tags: [Extend, Flows]
sidebar_position: 5
description: Write custom IoT data logic on the device using JavaScript
---

This tutorial walks through how to write your own custom logic using %%te%% flows; from a simple data mapping example to more advanced topics like aggregation and cross-flow communication. If you're looking for just the technical details, see the [reference](../references/mappers/flows.md) page.

## Overview {#overview}

A **flow** is a small piece of logic that runs inside one of %%te%%'s mappers. Each flow is triggered by an input source — such as incoming MQTT messages, a file, or output from a process — processes the data using a JavaScript function, and optionally produces output — for example, publishing to the local MQTT broker or appending to a file.

Flows are well-suited for use-cases like:

- Remapping data between formats — for example, converting a custom sensor payload into the %%te%% format
- Filtering or dropping unwanted messages before they reach the cloud
- Running local analytics on the device, such as triggering alarms based on sensor data
- Aggregating high-frequency sensor data to reduce bandwidth usage
- Monitoring cloud message rates and raising local alerts

Flows are designed to be lightweight and self-contained. Compared to writing a standalone application, they offer several practical advantages:

- No service configuration (systemd, OpenRC, etc.) needed — flows run inside an existing mapper
- No MQTT client to manage — connection, reconnection and QoS handling are provided automatically
- Sandboxed execution with restricted CPU and memory usage
- Packaged as a simple gzip tarball and hot-reloaded without any service restarts

## Anatomy of a flow {#anatomy}

A flow is defined by a TOML definition file (typically a `flow.toml`) that declares its inputs, outputs, and the steps that process messages. Steps can be JavaScript scripts (ES2020) or built-in functions implemented in Rust. The runtime loads each JavaScript script as an [ECMAScript Module](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Modules) and calls the exported handler functions when events occur.

The example below shows a typical flow package where all related files are kept together in a single directory. This is a convention rather than a requirement, but it helps make clear which files belong to which flow:

```text
my-flow/
├── flow.toml             # Flow definition: inputs, steps, configuration
├── main.js               # JavaScript logic
└── params.toml.template  # (optional) Configuration defaults; copy to params.toml to customize
```

**flow.toml** declares what messages the flow receives, which script steps process them, and any configuration values.

**main.js** exports handler functions that are called when messages arrive or timers fire.

**params.toml.template** is the flow author's way of communicating what can be configured by users. The %%te%% runtime reads `params.toml.template` directly as a source of default values — if a `params.toml` file does not yet exist, the template values are used automatically. Users only need to create a `params.toml` (by copying the template) if they want to override specific values. When the flow package is updated, any existing `params.toml` is preserved so user-configured values are not overwritten.

### Handlers {#handlers}

JavaScript steps export one or more handler functions that the runtime calls as events occur. Three handlers are available:

| Handler | When it is called | Typical use |
|---------|------------------|-------------|
| `onMessage(message, context)` | Once per incoming message | Data mapping, filtering, threshold checks |
| `onInterval(time, context)` | At a fixed interval | Aggregation, periodic reporting, rate checks |
| `onStartup(time, context)` | When the flow is first loaded | Initialization |

Each handler returns an array of output messages. Return an empty array (`[]`) to emit nothing. Where each message is sent — MQTT topic or file — is determined by the flow's output connector configuration.

The `message` object provides:
- `message.topic` — source topic (an MQTT topic, file path, or process name depending on the input connector)
- `message.payload` — raw bytes; use `new TextDecoder().decode(message.payload)` to get a string
- `message.time` — message timestamp as a JavaScript `Date`

The `context` object provides:
- `context.config` — configuration values declared in `flow.toml`, and typically mapped to values from the `params.toml` file
- `context.mapper` — key-value store shared across **all flows** in the same mapper instance
- `context.flow` — key-value store shared across scripts within the **same flow**
- `context.script` — key-value store private to a single script instance; persisted across hot-reloads of the script

:::note
`context.mapper` is held in memory and is **not persisted** across mapper restarts. Use it for short-lived coordination between flows such as counters, flags, and caching; not for durable storage.
:::

## Examples {#examples}

Each example below includes a `tedge flows test` command that runs the flow against sample input without making any live MQTT connections and without deploying anything to a device — making it safe and convenient to validate logic during development. See the [Testing flows](#testing-flows) section for a full description of the test command and its options. Of course, all flows shown here can equally be deployed to a running %%te%% mapper on a real device.

### Example 1: Remapping sensor data {#example-1}

**Goal:** A sensor publishes temperature readings to a custom topic `sensors/factory/+`. Remap these messages into the %%te%% measurement format so they are handled by the standard mapper flow.

This is the simplest possible flow: one input topic, one step, and a transformation in `onMessage`.

```toml title="file: sensor-remap/flow.toml"
input.mqtt.topics = ["sensors/factory/+"]

steps = [
    { script = "main.js" },
]
```

```js title="file: sensor-remap/main.js"
const decoder = new TextDecoder();

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const temperature = payload?.temp;

    if (typeof temperature !== "number") {
        // Skip messages that do not carry a numeric temperature
        return [];
    }

    // Re-publish in the thin-edge measurement format
    return [{
        topic: "te/device/main///m/environment",
        payload: JSON.stringify({ temperature }),
    }];
}
```

Test the flow locally without deploying to a device:

```sh
echo '[sensors/factory/zone1] {"temp": 22.4}' \
    | tedge flows test --flows-dir ./sensor-remap/
```

```text title="Output"
[te/device/main///m/environment] {"temperature":22.4}
```

---

### Example 2: Local temperature alarm {#example-2}

**Goal:** Raise a %%te%% alarm when a temperature measurement exceeds 70°C, and clear it automatically when the temperature returns to a safe level.

The input topic uses the wildcard `te/+/+/+/+/m/environment`, which matches both the main device and any connected child devices. The alarm topic is derived from the incoming topic's device prefix so the alarm is always created on the correct device.

```toml title="file: high-temp-alert/flow.toml"
input.mqtt.topics = ["te/+/+/+/+/m/environment"]

steps = [
    { script = "main.js" },
]
```

```js title="file: high-temp-alert/main.js"
const decoder = new TextDecoder();

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const temperature = payload?.temperature;

    if (typeof temperature !== "number") {
        return [];
    }

    // Derive the alarm topic from the incoming message's device prefix
    // e.g. "te/device/main///m/environment" → "te/device/main///a/temp_high"
    const alarmTopic = message.topic.split("/").slice(0, 5).join("/") + "/a/temp_high";

    if (temperature >= 70.0) {
        return [{
            topic: alarmTopic,
            payload: JSON.stringify({
                severity: "major",
                text: `Temperature is ${temperature}°C, exceeding the 70°C limit`,
            }),
            mqtt: { retain: true, qos: 1 },
        }];
    }

    // Clear the alarm by publishing an empty retained message
    return [{
        topic: alarmTopic,
        payload: "",
        mqtt: { retain: true, qos: 1 },
    }];
}
```

Test this flow locally without deploying to a device:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --flows-dir ./high-temp-alert/
```

```text title="Output"
[te/device/main///a/temp_high] {"severity":"major","text":"Temperature is 75°C, exceeding the 70°C limit"}
```

---

### Example 3: Configurable alarm threshold {#example-3}

**Goal:** Extend the alarm flow from Example 2 so the threshold is user-configurable rather than hardcoded. The same flow package can then be deployed across many different devices, each with its own threshold value defined locally.

This example builds directly on the `high-temp-alert` flow from Example 2 — only the changed files are shown.

#### Step 1: Declare configuration in flow.toml

Add a `[config]` section. The `${params.xxx}` syntax binds the value from `params.toml` at runtime — the mapper substitutes it automatically.

```toml title="file: high-temp-alert/flow.toml"
input.mqtt.topics = ["te/+/+/+/+/m/environment"]

[config]
threshold = "${params.threshold_degc}"

[[steps]]
script = "main.js"
```

#### Step 2: Provide a parameter template

`params.toml.template` documents all user-configurable parameters with comments explaining their purpose and providing sensible default values.

```toml title="file: high-temp-alert/params.toml.template"
# Temperature alarm threshold in degrees Celsius.
# An alarm is raised when the measured temperature reaches or exceeds this value.
threshold_degc = 70.0
```

Users configure the flow by copying the template and editing the values:

```sh
cp params.toml.template params.toml
# Edit params.toml and set threshold_degc to the desired value
```

When the flow package is updated, `params.toml` is preserved so user-defined values are not overwritten.

#### Step 3: Read configuration in the script

Access configuration values via `context.config`. Always provide a fallback default in case `params.toml` has not been created yet.

```js title="file: high-temp-alert/main.js"
const decoder = new TextDecoder();

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const temperature = payload?.temperature;

    if (typeof temperature !== "number") {
        return [];
    }

    // Read the threshold from config, with a safe fallback
    const { threshold = 70.0 } = context.config;

    const alarmTopic = message.topic.split("/").slice(0, 5).join("/") + "/a/temp_high";

    if (temperature >= threshold) {
        return [{
            topic: alarmTopic,
            payload: JSON.stringify({
                severity: "major",
                text: `Temperature is ${temperature}°C, exceeding the ${threshold}°C limit`,
            }),
            mqtt: { retain: true, qos: 1 },
        }];
    }

    return [{
        topic: alarmTopic,
        payload: "",
        mqtt: { retain: true, qos: 1 },
    }];
}
```

Test with the default threshold from `params.toml.template`:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --flows-dir ./high-temp-alert/
```

```text title="Output"
[te/device/main///a/temp_high] {"severity":"major","text":"Temperature is 75°C, exceeding the 70°C limit"}
```

---

### Example 4: Stateful alarm with hysteresis {#example-4}

**Goal:** Improve the alarm from Example 3 in two ways:
1. **Hysteresis**: instead of clearing the alarm the moment temperature drops below the threshold, require it to drop to a configurable band below the threshold before clearing. This prevents rapid toggling when temperature hovers near the threshold.
2. **State tracking**: use `context.script` to remember whether the alarm is currently active, and only publish when the state actually changes — avoiding a redundant retain publish on every incoming reading.

This example builds directly on the `high-temp-alert` flow from Example 3 — only the changed files are shown.

With a threshold of 70°C and a hysteresis of 5°C:
- The alarm **raises** when temperature reaches or exceeds 70°C
- The alarm **clears** only when temperature drops below 65°C (= 70 − 5)

Alarm state is tracked in `context.script`. The parameter `assume_alarm_active` controls what the script assumes when no state has been recorded yet — either after first startup or after a mapper restart. The default `false` assumes no alarm is active. Set it to `true` on devices where a stale retained alarm may already be present on the broker — this ensures it is cleared as soon as temperature enters the safe zone.

```toml title="file: high-temp-alert/flow.toml"
input.mqtt.topics = ["te/+/+/+/+/m/environment"]

[config]
threshold           = "${params.threshold_degc}"
hysteresis          = "${params.hysteresis_degc}"
assume_alarm_active = "${params.assume_alarm_active}"

[[steps]]
script = "main.js"
```

```toml title="file: high-temp-alert/params.toml.template"
# Temperature alarm threshold in degrees Celsius.
# An alarm is raised when the measured temperature reaches or exceeds this value.
threshold_degc = 70.0

# Hysteresis band in degrees Celsius.
# The alarm clears only when temperature drops below (threshold - hysteresis).
# Increase this value to reduce alarm toggling on sensors with noisy readings.
hysteresis_degc = 5.0

# Assumed alarm state on first start or after a mapper restart, before any reading
# has been processed. Set to true on devices where a stale retained alarm may already
# be present on the broker, so it is cleared as soon as temperature enters the safe zone.
assume_alarm_active = false
```

```js title="file: high-temp-alert/main.js"
const decoder = new TextDecoder();

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const temperature = payload?.temperature;

    if (typeof temperature !== "number") {
        return [];
    }

    const { threshold = 70.0, hysteresis = 5.0, assume_alarm_active = false } = context.config;
    const clearBelow = threshold - hysteresis;

    // Use a per-device key so child devices are tracked independently
    const deviceKey = message.topic.split("/").slice(0, 5).join("/");
    const alarmTopic = deviceKey + "/a/temp_high";

    // Fall back to the configured assumption when no state has been recorded yet
    const alarmActive = context.script.get(deviceKey) ?? assume_alarm_active;

    if (temperature >= threshold) {
        if (!alarmActive) {
            context.script.set(deviceKey, true);
            console.log(`Raising alarm ${deviceKey}: ${temperature}°C >= ${threshold}°C`);
            return [{
                topic: alarmTopic,
                payload: JSON.stringify({
                    severity: "major",
                    text: `Temperature is ${temperature}°C, exceeding the ${threshold}°C limit`,
                }),
                mqtt: { retain: true, qos: 1 },
            }];
        }
        // Already active — no redundant publish
        return [];
    }

    if (temperature < clearBelow) {
        if (alarmActive) {
            context.script.set(deviceKey, false);
            console.log(`Clearing alarm for ${deviceKey}: ${temperature}°C < ${clearBelow}°C`);
            return [{
                topic: alarmTopic,
                payload: "",
                mqtt: { retain: true, qos: 1 },
            }];
        }
        // Already cleared — no redundant publish
        return [];
    }

    // Temperature is in the hysteresis zone [clearBelow, threshold) — no state change
    return [];
}
```

**Raising the alarm.** After a restart the state is unknown — the first reading (60°C) is below the clear threshold, so any stale retained alarm is immediately cleared. The second reading (75°C) exceeds the threshold and raises the alarm:

```sh
tedge flows test --flows-dir ./high-temp-alert/ <<'EOF'
[te/device/main///m/environment] {"temperature": 60.0}
[te/device/main///m/environment] {"temperature": 75.0}
EOF
```

```text title="Output"
JavaScript.Console: "Raising alarm te/device/main//: 75°C >= 70°C"
[te/device/main///a/temp_high] {"severity":"major","text":"Temperature is 75°C, exceeding the 70°C limit"}
```

**Clearing the alarm.** Start by sending a reading that raises the alarm (75°C), then show the hysteresis band in action (67°C produces no output), then the final clear when temperature drops to 62°C:

```sh
tedge flows test --flows-dir ./high-temp-alert/ <<'EOF'
[te/device/main///m/environment] {"temperature": 75.0}
[te/device/main///m/environment] {"temperature": 67.0}
[te/device/main///m/environment] {"temperature": 62.0}
EOF
```

```text title="Output"
JavaScript.Console: "Raising alarm te/device/main//: 75°C >= 70°C"
[te/device/main///a/temp_high] {"severity":"major","text":"Temperature is 75°C, exceeding the 70°C limit"}
JavaScript.Console: "Clearing alarm for te/device/main//: 62°C < 65°C"
[te/device/main///a/temp_high]
```

---

### Example 5: Aggregating measurements {#example-5}


**Goal:** Collect temperature readings over a 30-second window and publish one aggregated message containing the minimum, maximum, and average values, instead of forwarding every individual reading. This reduces bandwidth usage, which is important on cellular connections with limited data plans.

This example introduces two handlers working together:
- `onMessage()` accumulates incoming samples without emitting anything
- `onInterval()` fires every 30 seconds, computes the aggregates, emits one message, then resets the buffer

```toml title="file: temp-aggregator/flow.toml"
input.mqtt.topics = ["te/+/+/+/+/m/environment"]

[config]
output_topic = "${params.output_topic}"

[[steps]]
script = "main.js"
interval = "30s"
```

```toml title="file: temp-aggregator/params.toml.template"
# Topic where aggregated results are published
output_topic = "te/device/main///m/environment_agg"
```

```js title="file: temp-aggregator/main.js"
const decoder = new TextDecoder();

// Buffer holding samples for the current time window
let samples = [];

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const temperature = payload?.temperature;

    if (typeof temperature === "number") {
        samples.push(temperature);
    }

    // Accumulate only — do not emit individual messages
    return [];
}

export function onInterval(time, context) {
    if (samples.length === 0) {
        return [];
    }

    const min = Math.min(...samples);
    const max = Math.max(...samples);
    const avg = samples.reduce((sum, v) => sum + v, 0) / samples.length;

    const { output_topic = "te/device/main///m/environment_agg" } = context.config;

    // Reset the buffer for the next window
    samples = [];

    return [{
        topic: output_topic,
        payload: JSON.stringify({
            temperature: {
                min: parseFloat(min.toFixed(2)),
                max: parseFloat(max.toFixed(2)),
                avg: parseFloat(avg.toFixed(2)),
            },
        }),
    }];
}
```

Because `onInterval` only fires on a timer, the test runner would produce no output unless the timer is manually triggered. The `--final-on-interval` flag tells `tedge flows test` to fire one additional `onInterval` event after all input messages have been processed, which flushes the aggregation buffer and produces the final output:

```sh
tedge flows test --flows-dir ./temp-aggregator/ --final-on-interval <<'EOF'
[te/device/main///m/environment] {"temperature": 75.1}
[te/device/main///m/environment] {"temperature": 87.3}
[te/device/main///m/environment] {"temperature": 84.9}
EOF
```

```text title="Output"
[te/device/main///m/environment_agg] {"temperature":{"min":75.1,"max":87.3,"avg":82.43}}
```

:::tip Keeping raw data alongside aggregates
To publish both individual readings and aggregated summaries simultaneously, return the original message from `onMessage()` instead of an empty array. Both outputs are then active at the same time.
:::

---

## Testing flows {#testing-flows}

The `tedge flows test` command runs flow scripts against test input without making any live MQTT connections. It is safe to run on a production device, no messages are published to the broker.

### Testing a flow during development

Use `--flows-dir` to point at a local flow directory before it is deployed. Input messages are piped via stdin in the format `[topic] payload`, one per line:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --flows-dir ./high-temp-alert/
```

To test a sequence of messages, use a heredoc:

```sh
tedge flows test --flows-dir ./debounce-alarm/ <<'EOF'
[te/device/main///m/environment] {"temperature": 75.0}
[te/device/main///m/environment] {"temperature": 76.0}
[te/device/main///m/environment] {"temperature": 71.0}
EOF
```

### Testing deployed flows

Once a flow is installed (under `/etc/tedge/mappers/<mapper>/flows/`), you can test it without `--flows-dir`. The command uses flows already loaded by the running mapper:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --mapper "<mapper>"
```

For example, if the flow has been installed in the `local` mapper, then you would run:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --mapper local
```

### Injecting context

Some flows read values from `context.mapper` that are normally set by other flows at runtime. Use `--context` to provide those values during testing:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --flows-dir ./data-adaptive/ \
        --context '{"data_mode": "aggregated"}'
```

### Testing Cumulocity mapper flows

Flows that run inside the Cumulocity mapper can be tested with `--mapper c8y`. Many built-in steps require device registration data to be present in the context:

```sh
CONTEXT='{
    "device/main//": {
        "@id": "example01",
        "@type": "device",
        "@topic-id": "device/main//",
        "@name": "example01",
        "@type-name": "thin-edge"
    }
}'

echo '[te/device/main///m/foo] {"pressure": 1024}' \
    | tedge flows test --mapper c8y --context "$CONTEXT"
```

:::note
If a `--mapper c8y` test produces no output, check that the device registration entry is present in `--context`. Built-in steps such as `cache-early-messages` silently discard messages until a device registration is seen. See [Builtin mapping rules](../references/mappers/builtin-flows.md) for the full list of steps that require this context.
:::

Add `--log-level debug` to see step-by-step processing details:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --flows-dir ./high-temp-alert/ --log-level debug
```

---

## Creating flow packages {#creating-packages}

A flow package is a gzip-compressed tar archive (`*.tar.gz`) containing the files described in the [Anatomy of a flow](#anatomy-of-a-flow) section — typically `flow.toml`, the script file(s), and an optional `params.toml.template`.

A few practical points to keep in mind when building packages:

- **Include version metadata.** Adding a version string and description to `flow.toml` makes it easy to identify which release is deployed on a device.
- **Keep flows self-contained.** A flow can reference files from a sibling flow using relative paths such as `../other-flow/main.js`, but there is no dependency manager to enforce this — you are responsible for ensuring both flows are installed together. Self-contained packages are simpler to reason about and easier to update independently.

### Uploading to the Cumulocity software repository

To deploy a flow via Cumulocity software management, upload the package to the software repository with the following fields:

| Field | Value |
|-------|-------|
| Name | `<mapper>/<flow_name>` (e.g. `c8y/high-temp-alert`) |
| Software type | `flow` |
| Version | The flow version (e.g. `1.2.0`) |
| URL | A direct download link, or upload the file directly |

The mapper prefix in the name controls where the flow is installed — `c8y/` puts it under the Cumulocity mapper, `local/` under the local mapper.

## Installing flows {#installing-flows}

### During development

When iterating on a flow locally, you do not need to create a package. Copy the flow directory directly into the mapper's flows folder and the mapper picks it up immediately — any subsequent edits to the files are hot-reloaded at runtime without restarting the mapper:

```sh
cp -Ra dev/myflow /etc/tedge/mappers/c8y/flows/
```

Before deploying, validate the flow logic with `tedge flows test`. It catches syntax errors and lets you confirm that the output is correct before anything is published to a live broker.

### On a device via Cumulocity

Install a packaged flow through Cumulocity software management. The software name must use the `<mapper>/<flow>` format — for example, `c8y/high-temp-alert` installs the flow under the Cumulocity mapper's flows directory, while `local/monitor` targets the local mapper. If no mapper prefix is provided, then the installation will fail.

To debug installation problems, copy the package to the device directly and run the software plugin manually to see the full error output:

```sh
sudo -u tedge /etc/tedge/sm-plugins/flow install c8y/myflow --module-version 1.2.0 --file ./myflow_1.2.0.tar.gz
```


---

## Tips {#tips}

### Packaging on macOS

On macOS, `tar` is the BSD variant and includes hidden `._` metadata files by default. These cause %%te%% to reject the package as invalid. Supply the `COPYFILE_DISABLE` environment variable to prevent this:

```sh
COPYFILE_DISABLE=1 tar czf my-flow.tar.gz -C my-flow/ .
```

### TOML array-of-tables syntax

TOML supports two equivalent ways to define an array of step objects in `flow.toml`. Choose whichever you find more readable:

```toml tab={"label":"Inline"}
steps = [
    { script = "step1.js" },
    { script = "step2.js" },
]
```

```toml tab={"label":"Table array"}
[[steps]]
script = "step1.js"

[[steps]]
script = "step2.js"
```

---

### Using TypeScript

The %%te%% runtime executes scripts as ECMAScript Modules (ES2020). You can write flows in TypeScript and use a bundler like esbuild, Rollup, webpack to transpile and bundle them into a single `.js` file / ECMAScript Module.

A good example of this is the [`tedge-flows-examples`](https://github.com/thin-edge/tedge-flows-examples) repository which is a multi-workspace npm project that uses **esbuild** to transpile and package the flows. Each flow is written in TypeScript with Jest their own unit tests, and the build step produces a bundled ECMAScript Module, and also some tasks to even create the flow package that can be installed on a device.

A minimal esbuild setup for a flow looks like this:

```json title="file: my-flow/package.json"
{
  "name": "my-flow",
  "scripts": {
    "build": "esbuild src/main.ts --bundle --platform=neutral --format=esm --outfile=dist/main.js",
    "test": "jest"
  },
  "devDependencies": {
    "esbuild": "*",
    "typescript": "*",
    "jest": "*",
    "@types/jest": "*"
  }
}
```

The `--platform=neutral --format=esm` flags tell esbuild to produce a standards-compliant ESM bundle with no Node.js-specific globals, which is what the %%te%% runtime expects.

The bundled output at `dist/main.js` (along with `flow.toml` and an optional `params.toml.template`) is then packaged into an archive:

```sh
cd dist
COPYFILE_DISABLE=1 tar czf ../my-flow.tar.gz flow.toml dist/main.js params.toml.template
```

:::tip
See the [`tedge-flows-examples`](https://github.com/thin-edge/tedge-flows-examples) repository for complete TypeScript flow examples with build scripts, unit tests, and packaging.
:::

---

## Advanced Examples {#advanced-examples}

The examples below build on the foundations from the previous section. They cover patterns that come up once basic flows are working: letting flows talk to each other through shared state, exposing a simple MQTT control interface so operators can change behaviour at runtime without redeploying, and chaining multiple processing steps within a single flow. These techniques can be combined freely — the goal is to show the building blocks rather than prescribe a fixed structure.

### Example 6: Monitoring cloud message rate {#example-6}

**Goal:** Watch outgoing Cumulocity messages and publish a local alert when the rate exceeds a configurable limit — for example, 1000 messages per hour or 10000 per day. This helps catch runaway processes or misconfigured services that might exhaust the data allowance of a SIM card.

The alert is published as a %%te%% event on the local MQTT broker. If a cloud mapper is active, the event will also be forwarded to the cloud. If not, it remains available locally for other flows or monitoring tools.

This example also introduces `context.mapper`. By storing the counters there instead of in script-local variables, other flows in the same mapper can read the current message counts if needed — for example, to include the counts in a status report.

```toml title="file: cloud-rate-monitor/flow.toml"
input.mqtt.topics = ["c8y/#"]

[config]
max_per_hour = "${params.max_per_hour}"
max_per_day  = "${params.max_per_day}"

[[steps]]
script = "main.js"
interval = "60s"
```

```toml title="file: cloud-rate-monitor/params.toml.template"
# Maximum Cumulocity messages allowed per hour before an alert is raised
max_per_hour = 1000

# Maximum Cumulocity messages allowed per day before an alert is raised
max_per_day = 10000
```

```js title="file: cloud-rate-monitor/main.js"
const HOUR_MS = 60 * 60 * 1000;
const DAY_MS  = 24 * HOUR_MS;

export function onStartup(time, context) {
    context.mapper.set("rate.hourly_count", 0);
    context.mapper.set("rate.daily_count",  0);
    context.mapper.set("rate.hourly_start", Date.now());
    context.mapper.set("rate.daily_start",  Date.now());
}

export function onMessage(message, context) {
    const hourly = (context.mapper.get("rate.hourly_count") ?? 0) + 1;
    const daily  = (context.mapper.get("rate.daily_count")  ?? 0) + 1;
    context.mapper.set("rate.hourly_count", hourly);
    context.mapper.set("rate.daily_count",  daily);
    return [];
}

export function onInterval(time, context) {
    const { max_per_hour = 1000, max_per_day = 10000 } = context.config;
    const now = time.getTime();
    const messages = [];

    const hourly_count = context.mapper.get("rate.hourly_count") ?? 0;
    const daily_count  = context.mapper.get("rate.daily_count")  ?? 0;
    const hourly_start = context.mapper.get("rate.hourly_start") ?? now;
    const daily_start  = context.mapper.get("rate.daily_start")  ?? now;

    if (hourly_count >= max_per_hour) {
        messages.push({
            topic: "te/device/main///e/cloud_rate_exceeded",
            payload: JSON.stringify({
                text: `Hourly cloud message limit reached: ${hourly_count} messages (limit: ${max_per_hour})`,
            }),
        });
    }

    if (daily_count >= max_per_day) {
        messages.push({
            topic: "te/device/main///e/cloud_rate_exceeded",
            payload: JSON.stringify({
                text: `Daily cloud message limit reached: ${daily_count} messages (limit: ${max_per_day})`,
            }),
        });
    }

    // Reset hourly counter after one hour
    if (now - hourly_start >= HOUR_MS) {
        context.mapper.set("rate.hourly_count", 0);
        context.mapper.set("rate.hourly_start", now);
    }

    // Reset daily counter after one day
    if (now - daily_start >= DAY_MS) {
        context.mapper.set("rate.daily_count", 0);
        context.mapper.set("rate.daily_start", now);
    }

    return messages;
}
```

To trigger both thresholds in the test, we need to send more than 10000 messages — enough to exceed both the hourly (1000) and daily (10000) limits. The `yes` command generates an infinite stream of identical lines; `head -n 10001` limits it to 10001. The `--final-on-interval` flag fires `onInterval` after all messages are consumed, which is when the threshold check and counter reset logic runs:

```sh
yes '[c8y/measurement/measurements/create] {"temp":{"temp":{"value":37.9}},"time":"2026-03-31T21:06:34.898Z","type":"temperature"}' \
| head -n 10001 \
| tedge flows test --flows-dir ./cloud-rate-monitor/ --final-on-interval
```

```text title="Output"
[te/device/main///e/cloud_rate_exceeded] {"text":"Hourly cloud message limit reached: 10001 messages (limit: 1000)"}
[te/device/main///e/cloud_rate_exceeded] {"text":"Daily cloud message limit reached: 10001 messages (limit: 10000)"}
```

:::note Context is not persisted
`context.mapper` values are in-memory only. If the mapper process restarts, all counters reset to zero. For this monitoring use case that is acceptable — the rate monitor resumes counting from restart.
:::

---

### Example 7: Adaptive data mode {#example-7}

**Goal:** Automatically switch between publishing raw (full-frequency) measurements and aggregated measurements based on the incoming message rate, while also allowing a user to override the mode at any time via MQTT.

This example uses **two flows communicating through `context.mapper`**. The shared key `"data_mode"` acts as a signal:
- `"auto"` — the measurement flow selects the mode based on measured rate (default)
- `"raw"` — always forward individual measurements to the cloud
- `"aggregated"` — always aggregate, regardless of rate

:::note
Because `context.mapper` is shared across all flows within the same mapper instance, both flows must be deployed to the **same mapper**.
:::

#### Flow 1: Mode controller

Listens for user-requested mode changes on the local broker and updates the shared state. Sending a message to this topic lets an operator switch between full-fidelity diagnostic mode and bandwidth-conserving aggregate mode without restarting anything.

```toml title="file: data-mode-controller/flow.toml"
input.mqtt.topics = ["control/data_mode"]

steps = [
    { script = "main.js" },
]
```

```js title="file: data-mode-controller/main.js"
const decoder = new TextDecoder();

const VALID_MODES = new Set(["raw", "aggregated", "auto"]);

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const mode = payload?.mode;

    if (VALID_MODES.has(mode)) {
        context.mapper.set("data_mode", mode);
        console.log(`Data mode changed to: ${mode}`);
    }

    return [];
}
```

Trigger a mode change from the command line. The topic `control/data_mode` is a plain user-defined MQTT topic — it deliberately does not use the `te/.../cmd/` schema, which is reserved for %%te%%'s structured command workflow:

```sh
tedge mqtt pub control/data_mode '{"mode": "aggregated"}'
```

#### Flow 2: Adaptive measurement publisher

Reads the current mode from `context.mapper` on each message and interval. In `"auto"` mode it counts messages within the current window — if the rate exceeds the threshold the flow starts aggregating; once the rate drops it reverts to passing raw data through.

```toml title="file: data-adaptive/flow.toml"
input.mqtt.topics = ["te/+/+/+/+/m/environment"]

[config]
high_freq_threshold = "${params.high_freq_threshold}"

[[steps]]
script = "main.js"
interval = "30s"
```

```toml title="file: data-adaptive/params.toml.template"
# Number of messages per interval above which aggregation is activated in "auto" mode.
# Tune this value based on the expected sensor publish rate.
high_freq_threshold = 20
```

```js title="file: data-adaptive/main.js"
const decoder = new TextDecoder();

let samples = [];
let window_count = 0;

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const temperature = payload?.temperature;

    if (typeof temperature !== "number") {
        return [];
    }

    window_count += 1;
    samples.push(temperature);

    const mode = context.mapper.get("data_mode") ?? "auto";
    const { high_freq_threshold = 20 } = context.config;

    const isHighFreq = window_count > high_freq_threshold;

    // Pass through raw data when in "raw" mode or when auto-mode detects a low rate
    if (mode === "raw" || (mode === "auto" && !isHighFreq)) {
        return [{
            topic: "te/device/main///m/environment",
            payload: decoder.decode(message.payload),
        }];
    }

    // In "aggregated" mode or auto+high-frequency: collect only, publish on interval
    return [];
}

export function onInterval(time, context) {
    const mode = context.mapper.get("data_mode") ?? "auto";
    const { high_freq_threshold = 20 } = context.config;

    const isHighFreq = window_count > high_freq_threshold;
    const shouldAggregate = mode === "aggregated" || (mode === "auto" && isHighFreq);

    // Swap out the buffers before any early returns
    const current_samples = samples;
    samples = [];
    window_count = 0;

    if (!shouldAggregate || current_samples.length === 0) {
        return [];
    }

    const min = Math.min(...current_samples);
    const max = Math.max(...current_samples);
    const avg = current_samples.reduce((sum, v) => sum + v, 0) / current_samples.length;

    return [{
        topic: "te/device/main///m/environment_agg",
        payload: JSON.stringify({
            temperature_min: parseFloat(min.toFixed(2)),
            temperature_max: parseFloat(max.toFixed(2)),
            temperature_avg: parseFloat(avg.toFixed(2)),
            sample_count: current_samples.length,
        }),
    }];
}
```

The two flows together allow the device to conserve bandwidth during normal operation, while an operator can request full-fidelity data at any time by publishing a single MQTT message.

To test the adaptive publisher in isolation, use `--context` to pre-seed the `data_mode` value that the mode controller would normally set at runtime:

```sh
echo '[te/device/main///m/environment] {"temperature": 75.0}' \
    | tedge flows test --flows-dir ./data-adaptive/ \
        --context '{"data_mode": "aggregated"}' \
        --final-on-interval
```

```text title="Output"
[te/device/main///m/environment_agg] {"temperature_min":75.0,"temperature_max":75.0,"temperature_avg":75.0,"sample_count":1}
```

---

### Example 8: Multi-step debounce alarm {#example-8}

**Goal:** Raise a temperature alarm only after a configurable number of _consecutive_ above-threshold readings. A single spike should not trigger an alarm — only sustained elevated temperatures are actionable.

This example introduces two new concepts:
- **Multi-step pipelines** — a flow can chain multiple scripts, where each step's output becomes the next step's input
- **`context.script`** — a key-value store used by individual scripts to store additional state that does not need to be shared with other scripts or flows

```toml title="file: debounce-alarm/flow.toml"
input.mqtt.topics = ["te/+/+/+/+/m/environment"]

[config]
threshold       = "${params.threshold_degc}"
min_consecutive = "${params.min_consecutive}"

[[steps]]
script = "validate.js"

[[steps]]
script = "debounce.js"
```

```toml title="file: debounce-alarm/params.toml.template"
# Temperature (°C) at or above which a reading is considered high
threshold_degc = 70.0

# Number of consecutive high readings required before raising an alarm
min_consecutive = 3
```

**validate.js** checks only whether the temperature exceeds the threshold and annotates the message. It does not decide whether to raise an alarm — that logic belongs in the next step.

```js title="file: debounce-alarm/validate.js"
const decoder = new TextDecoder();

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const temperature = payload?.temperature;

    if (typeof temperature !== "number") {
        return [];
    }

    const { threshold = 70.0 } = context.config;

    // Tag the message and pass it downstream to debounce.js
    return [{
        topic: message.topic,
        payload: JSON.stringify({
            ...payload,
            _above_threshold: temperature >= threshold,
        }),
    }];
}
```

**debounce.js** maintains a counter in `context.script`. The counter is incremented on each above-threshold reading and reset on a normal reading. An alarm is emitted only once the counter reaches `min_consecutive`.

```js title="file: debounce-alarm/debounce.js"
const decoder = new TextDecoder();

export function onMessage(message, context) {
    const payload = JSON.parse(decoder.decode(message.payload));
    const { min_consecutive = 3 } = context.config;

    const alarmTopic = message.topic.split("/").slice(0, 5).join("/") + "/a/temp_high";

    if (payload._above_threshold) {
        const count = (context.script.get("consecutive_count") ?? 0) + 1;
        context.script.set("consecutive_count", count);

        if (count >= min_consecutive) {
            return [{
                topic: alarmTopic,
                payload: JSON.stringify({
                    severity: "major",
                    text: `High temperature for ${count} consecutive readings`,
                }),
                mqtt: { retain: true, qos: 1 },
            }];
        }
    } else {
        // Temperature returned to normal — reset counter and clear the alarm
        context.script.set("consecutive_count", 0);
        return [{
            topic: alarmTopic,
            payload: "",
            mqtt: { retain: true, qos: 1 },
        }];
    }

    return [];
}
```

Test by sending three consecutive above-threshold readings. Only the third produces output:

```sh
tedge flows test --flows-dir ./debounce-alarm/ <<'EOF'
[te/device/main///m/environment] {"temperature": 75.0}
[te/device/main///m/environment] {"temperature": 76.0}
[te/device/main///m/environment] {"temperature": 71.0}
EOF
```

```text title="Output"
[te/device/main///a/temp_high] {"severity":"major","text":"High temperature for 3 consecutive readings"}
```

The first two readings accumulate the counter silently. A reading below the threshold resets the counter and publishes an alarm-clear message.

---

## References {#references}

### [tedge-flows-examples](https://github.com/thin-edge/tedge-flows-examples)

A repository of maintained example flows, each with TypeScript source, npm build tooling, Jest unit tests, and packaging scripts. The repository covers a wide range of use cases, some of which are described below:

| Flow | Description |
|------|-------------|
| [measurement-aggregator](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/measurement-aggregator) | Collects individual per-topic MQTT datapoints within a configurable time window and publishes them as a single combined %%te%% measurement; supports flat values and nested sub-series |
| [log-surge](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/log-surge) | Monitors journald system logs for surges in error, warning, or info messages and raises alarms when counts exceed configurable thresholds |
| [certificate-alert](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/certificate-alert) | Monitors the device certificate for approaching expiry; raises major or warning alarms at configurable lead times and publishes certificate metadata to the device digital twin |
| [uptime](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/uptime) | Tracks service availability over a configurable time window and publishes uptime percentage as a %%te%% measurement |
| [thingsboard-registration](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/thingsboard-registration) | Handles device and child-device registration with ThingsBoard, mapping %%te%% entity registration messages to ThingsBoard attribute format |
| [thingsboard-telemetry](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/thingsboard-telemetry) | Maps %%te%% measurements, events, alarms, twin data, and service health to ThingsBoard telemetry and attribute topics |
| [thingsboard-server-rpc](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/thingsboard-server-rpc) | Handles ThingsBoard server-side RPC calls, translating them to %%te%% commands for execution on the device |
| [x509-cert-issuer](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/x509-cert-issuer) | An MQTT-based X.509 Certificate Authority; devices that present a trusted factory certificate receive a TLS client certificate for authenticating to an MQTT broker's TLS endpoint |
| [jsonata-xform](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/jsonata-xform) | Applies JSONata expressions to transform message payloads; supports the same substitution rules as the Cumulocity Dynamic Mapper, enabling flexible schema remapping without custom JavaScript |
| [tedge-events](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/tedge-events) | Demonstrates forwarding %%te%% events to the Cumulocity MQTT service, enriching each event with a device identifier and a sequence counter |
| [tedge-config-context](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/tedge-config-context) | Reads %%te%% configuration values at startup and publishes them to `context.mapper`, making device-specific settings available to all flows without repeated config reads |
| [cloud-mapper-telemetry](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/cloud-mapper-telemetry) | A minimal template for building a custom cloud mapper; demonstrates the basic input-transform-output pattern for routing %%te%% telemetry to a non-built-in cloud platform |
| [protobuf-xform](https://github.com/thin-edge/tedge-flows-examples/tree/main/flows/protobuf-xform) | Encodes outgoing %%te%% measurements as Protocol Buffers for the Cumulocity MQTT Service; demonstrates TypeScript with external npm library bundling |

### Reference documentation

| Document | Description |
|----------|-------------|
| [User-defined mapping rules](../references/mappers/flows.md) | Full API reference: flow configuration, step API, context scopes, and built-in transformations |
| [Builtin mapping rules](../references/mappers/builtin-flows.md) | The built-in c8y/az/aws mapper flows that can be customized or extended |
| [User-defined mappers](../references/mappers/user-defined-mappers.md) | Deploying a custom mapper for a non-built-in cloud platform |