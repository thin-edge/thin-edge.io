---
title: "tedge bootstrap"
tags: [Reference, CLI]
sidebar_position: 12
---

# The tedge bootstrap command

See [Connecting to Cumulocity](../../start/connect-c8y.md) for a walkthrough,
and [Extend: Bootstrap](../../extend/bootstrap.md) for the hooks and cloud descriptors
by which the command is customized.

```text command="tedge bootstrap --help" title="tedge bootstrap"
Bootstrap the device and onboard it to a cloud (experimental)

Configures the cloud endpoints, obtains device credentials, and connects the device, in a single command.

Usage: tedge bootstrap [OPTIONS] [CLOUD]

Arguments:
  [CLOUD]
          The cloud to bootstrap: c8y, az, aws (optionally with a profile, e.g. c8y.prod), or a custom cloud mapper name (e.g. thingsboard).
          
          When omitted, an interactive wizard guides through the available options

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --url <URL>
          Cloud URL to connect to. This should be the HTTP/S address used to talk to the platform.
          
          For Cumulocity, the MQTT endpoint is discovered automatically; if it differs from the HTTP endpoint, c8y.http and c8y.mqtt are configured separately instead of c8y.url.

      --debug
          Turn-on the DEBUG log level.
          
          If off only reports ERROR, WARN, and INFO, if on also reports DEBUG

      --register <REGISTER>
          How the device obtains its credentials.
          
          The available methods depend on the cloud (declared by its cloud descriptor): c8y offers c8y-ca (default), self-signed, basic and basic-preregistered; other clouds offer the methods of their register.d hooks

      --device-id <DEVICE_ID>
          The device identifier to be used as the certificate common name

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
          
          Overrides `--debug`

      --profile <PROFILE>
          The cloud profile (when the device connects to several instances of a cloud)

      --set <KEY=VALUE>
          Set additional configuration keys before registering. Can be repeated.
          
          For built-in clouds these are tedge config keys, e.g. --set c8y.software_management.api=advanced; for custom cloud mappers they are mapper config keys prefixed with the mapper name, e.g. --set thingsboard.transport.port=8883

      --type <CLOUD_TYPE>
          The cloud type of a custom-named mapper instance, e.g. c8y.
          
          Enables the named cloud's registration methods and wizard options for an instance with a non-default name (e.g. a second Cumulocity instance: `tedge bootstrap c8y-second --type c8y`), and is persisted as the instance's cloud_type. When omitted, the cloud_type already in the instance's mapper.toml or the `type` declared by the cloud's descriptor applies

      --interactive
          Run the interactive wizard even when stdin is not a terminal

      --timeout <TIMEOUT>
          Maximum time to wait for a custom cloud mapper to report a healthy connection.
          
          The first connection can be slow (service start, DNS, TLS), or depend on an operator action (e.g. registering a certificate in the cloud's UI), so the connection check is retried until then. Built-in clouds use the connect flow's own retries; registration waits for an operator for up to 10 minutes
          
          [default: 5m]

      --no-wait
          Only try the connection check once instead of retrying until --timeout

      --dry-run
          Only print what would be done, without changing anything

      --ascii
          Force the plain ASCII output profile (used automatically when the locale does not advertise UTF-8, or when TERM=dumb)

      --plugin-dir <DIR>
          Directory to search for bootstrap hooks (<phase>.d) and cloud descriptors (clouds.d). Can be repeated; earlier directories take precedence per file name.
          
          Overrides the configured bootstrap.plugin_paths (and the TEDGE_BOOTSTRAP_PLUGIN_PATHS environment variable)

      --from <FILE>
          Bootstrap from an invocation file: a JSON array of invocations (the format --save writes), run in order — e.g. two Cumulocity instances.
          
          Environment variables are captured by name only: the listed variables must be set when replaying

      --save <FILE>
          Save the effective invocation(s) as a declarative JSON array file, replayable with --from; append further instances by editing the array.
          
          Combined with --dry-run: walk the wizard, save the answers, apply nothing — then apply here or on another device with --from

      --re-register
          Remove the instance's existing registration artifacts first, so registration re-runs against the kept configuration: its credentials file, its certificate and CSR (for the default instance this is the shared device certificate and private key, removed with a warning as other cloud connections may use them). Hooks receive --re-register so they can re-register too

      --clean
          Unwind the instance before bootstrapping: everything --re-register removes, plus the configuration bootstrap manages for the instance (its endpoints, auth method, credentials location, per-instance defaults, and the settings this run applies); other settings of the cloud's section and device-global settings are kept. The run then needs its inputs supplied afresh. Hooks receive --clean so they can remove their own state too

      --offline
          Provision without network access: apply the configuration, run the hooks (which receive --offline), and stage the services, deferring everything that needs the cloud.
          
          Registration is deferred for the built-in methods needing the cloud (their inputs are not collected, and no registration URL is printed - its one-time password would not survive to the online run); basic-preregistered stores its credentials offline, and register hooks still run and may fulfil registration offline (e.g. a local PKI). The staged services connect by themselves when the network appears; re-running the same command online performs the remaining steps. Not captured by --save

      --describe
          Describe the resolved cloud descriptors instead of bootstrapping: each cloud's registration methods with their inputs (as environment variable names) and its settings.
          
          Rendered from the same descriptors that drive the wizard and validation - packaged clouds and clouds.d overrides included - so it documents exactly what this device would ask for

  -h, --help
          Print help (see a summary with '-h')
```
