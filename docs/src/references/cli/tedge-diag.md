---
title: "tedge diag"
tags: [Reference, CLI]
sidebar_position: 6
---

# The tedge diag command

```text command="tedge diag --help" title="tedge diag"
Collect diagnostic information to help with debugging

Usage: tedge diag [OPTIONS] <COMMAND>

Commands:
  collect  Collect diagnostic information by running device-specific scripts
  help     Print this message or the help of the given subcommand(s)

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

## tedge diag collect

```text command="tedge diag collect --help" title="tedge diag collect"
Collect diagnostic information by running device-specific scripts

Usage: tedge diag collect [OPTIONS]

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --plugin-dir <PLUGIN_DIR>
          Directory where diagnostic plugins are stored. The paths from diag.plugin_dir will be used by default

      --debug
          Turn-on the DEBUG log level.
          
          If off only reports ERROR, WARN, and INFO, if on also reports DEBUG

      --output-dir <OUTPUT_DIR>
          Directory where output tarball and temporary output files are stored. The path from tmp.path will be used by default

      --log-level <LOG_LEVEL>
          Configures the logging level.
          
          One of error/warn/info/debug/trace. Logs with verbosity lower or equal to the selected level will be printed, i.e. warn prints ERROR and WARN logs and trace prints logs of all levels.
          
          Overrides `--debug`

      --name <NAME>
          Filename (without .tar.gz) for the output tarball
          
          [default: tedge-diag_<TIMESTAMP>]

      --keep-dir
          Whether to keep intermediate output files after the tarball is created

      --timeout <TIMEOUT>
          Timeout for a graceful plugin shutdown
          
          [default: 60s]

      --forceful-timeout <FORCEFUL_TIMEOUT>
          Timeout for forced termination, starting after a graceful timeout expires
          
          [default: 60s]

  -h, --help
          Print help (see a summary with '-h')

```