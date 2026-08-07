---
title: "tedge service"
tags: [Reference, CLI, Services]
sidebar_position: 12
---

# The tedge service command

```text command="tedge service --help" title="tedge service"
Run an action on a system service, whatever manages it

The init system configured in system.toml runs the action for the default service type. Any other service type is handled by the service plugin named after it.

Usage: tedge service [OPTIONS] <ACTION> <SERVICE_NAME>

Arguments:
  <ACTION>
          The action to run on the service, e.g. start, stop or restart
          
          Which actions are supported is decided by the backend running them: the init system for the default service type, a service plugin for any other type.

  <SERVICE_NAME>
          The name of the service to act on, as the backend knows it

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --service-type <SERVICE_TYPE>
          The type of the service, selecting the backend that runs the action
          
          The default type is handled by the init system configured in system.toml. Any other type is handled by the service plugin named after it.
          
          [default: service]

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
