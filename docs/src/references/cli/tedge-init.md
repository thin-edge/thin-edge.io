---
title: "tedge init"
tags: [Reference, CLI]
sidebar_position: 1
---

# The tedge init command

```text command="tedge init --help" title="tedge init"
Initialize Thin Edge

Usage: tedge init [OPTIONS]

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --user <USER>
          The user who will own the directories created
          
          [default: tedge]

      --debug
          Turn-on the DEBUG log level.
          
          If off only reports ERROR, WARN, and INFO, if on also reports DEBUG

      --group <GROUP>
          The group who will own the directories created
          
          [default: tedge]

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
          
          Overrides `--debug`

      --relative-links
          Create symlinks to the tedge binary using a relative path (e.g. ./tedge) instead of an absolute path (e.g. /usr/bin/tedge)

  -h, --help
          Print help (see a summary with '-h')
```
