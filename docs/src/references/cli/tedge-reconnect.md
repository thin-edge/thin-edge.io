---
title: "tedge reconnect"
tags: [Reference, CLI]
sidebar_position: 7
---

# The tedge reconnect command

```text command="tedge reconnect --help" title="tedge reconnect"
Reconnect command, calls disconnect followed by connect

Usage: tedge reconnect [OPTIONS] <COMMAND>

Commands:
  c8y   
  az    
  aws   
  help  Print this message or the help of the given subcommand(s)

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --offline
          Ignore connection registration and connection check

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

## AWS

```text command="tedge reconnect aws --help" title="tedge reconnect aws"
Usage: tedge reconnect aws [OPTIONS]

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --offline
          Ignore connection registration and connection check

      --profile <PROFILE>
          The cloud profile you wish to use
          
          [env: TEDGE_CLOUD_PROFILE]

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

## Azure

```text command="tedge reconnect az --help" title="tedge reconnect az"
Usage: tedge reconnect az [OPTIONS]

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --offline
          Ignore connection registration and connection check

      --profile <PROFILE>
          The cloud profile you wish to use
          
          [env: TEDGE_CLOUD_PROFILE]

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

## Cumulocity

```text command="tedge reconnect c8y --help" title="tedge reconnect c8y"
Usage: tedge reconnect c8y [OPTIONS]

Options:
      --config-dir <CONFIG_DIR>
          [env: TEDGE_CONFIG_DIR, default: /etc/tedge]

      --offline
          Ignore connection registration and connection check

      --profile <PROFILE>
          The cloud profile you wish to use
          
          [env: TEDGE_CLOUD_PROFILE]

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
