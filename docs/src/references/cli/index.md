---
title: "The tedge command"
tags: [Reference, CLI]
sidebar_position: 4
---

# The tedge command

```text command="tedge --help" title="tedge"
Command line interface to interact with thin-edge.io

Usage: tedge [OPTIONS] <COMMAND>

Commands:
  init             Initialize Thin Edge
  cert             Create and manage device certificate
  config           Configure Thin Edge
  connect          Connect to cloud provider
  disconnect       Remove bridge connection for a provider
  reconnect        Reconnect command, calls disconnect followed by connect
  refresh-bridges  Refresh all currently active mosquitto bridges
  upload           Upload files to the cloud
  mqtt             Publish a message on a topic and subscribe a topic
  run              Run thin-edge services and plugins
  help             Print this message or the help of the given subcommand(s)

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --debug
          Turn-on the DEBUG log level.
          
          If off only reports ERROR, WARN, and INFO, if on also reports DEBUG

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace.
          Logs with verbosity lower or equal to the selected level will be printed,
          i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
          
          Overrides `--debug`

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
