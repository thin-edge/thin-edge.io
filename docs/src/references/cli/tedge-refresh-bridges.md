---
title: "tedge refresh-bridges"
tags: [Reference, CLI]
sidebar_position: 8
---

# The tedge run command

```text command="tedge refresh-bridges --help" title="tedge refresh-bridges"
Refresh all currently active mosquitto bridges

Usage: tedge refresh-bridges [OPTIONS]

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
