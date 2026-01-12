---
title: "tedge run"
tags: [Reference, CLI]
sidebar_position: 13
---

# The tedge run command

```text command="tedge run --help" title="tedge run"
Run thin-edge services and plugins

Usage: tedge run [OPTIONS] <COMMAND>

Commands:
  c8y-firmware-plugin       thin-edge.io device firmware management for Cumulocity
  c8y-remote-access-plugin  thin-edge.io plugin for the Cumulocity Cloud Remote Access feature
  tedge-agent               tedge-agent interacts with a Cloud Mapper and one or more Software Plugins
  tedge-apt-plugin          Thin-edge.io plugin for software management using apt
  tedge-file-log-plugin     Thin-edge.io plugin for file-based log management
  tedge-mapper              tedge-mapper translates thin-edge.io data model to c8y/az/aws data model
  tedge-watchdog            tedge-watchdog checks the health of all the thin-edge.io components/services
  tedge-write               tee-like helper for writing to files which `tedge` user does not have write permissions to
  help                      Print this message or the help of the given subcommand(s)

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --debug
          Turn-on the DEBUG log level.
          
          If off only reports ERROR, WARN, and INFO, if on also reports DEBUG

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
          
          Overrides `--debug`

  -h, --help
          Print help (see a summary with '-h')

```
