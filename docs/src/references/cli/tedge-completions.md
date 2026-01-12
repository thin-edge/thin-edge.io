---
title: "tedge completions"
tags: [Reference, CLI]
sidebar_position: 14
---

# The tedge completions command

```text command="tedge completions --help" title="tedge completions"
Usage: tedge completions [OPTIONS] <SHELL>

Arguments:
  <SHELL>
          [possible values: bash, zsh, fish]

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
