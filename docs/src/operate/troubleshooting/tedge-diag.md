---
title: Collecting diagnostic logs
tags: [Operate]
sidebar_position: 1
description: How to collect logs and system diagnostics using plugin-based automation
---

%%te%% diagnostic collector helps you collect diagnostic information about your device and services with a single command,
which can be further extended with user-defined diagnostic plugins.
The collected data is packaged into a single compressed archive (`.tar.gz`) for easy analysis or sharing with support.

This guide explains how to use the diagnostic collection command and how to add or customize diagnostic plugins.

## Quick Start

To collect a snapshot of diagnostic data, run:

```sh
tedge diag collect
```

This command performs the following steps:
* Discovers all available diagnostic plugins (default location: `/usr/share/tedge/diag-plugins`)
* Executes each plugin in alphabetical order
* Captures their logs and outputs
* Package all collected data into a `.tar.gz` archive (e.g., `/tmp/tedge-diag-2025-06-12_13-38-53.tar.gz`)

You can also customize the directory paths and archive name using command-line options or the `tedge config` command.

To view all available options, run:

```sh
tedge diag collect --help
```

## Diagnostic Plugins

A diagnostic plugin is an executable that collects a diagnostic data such as configuration, logs, and statuses for a service or process.
The plugin itself, decides what information should be collected or not.
It is invoked by `tedge diag collect` (referred to as the _runner_).

Each plugin runs as a child process, and its `stdout` and `stderr` are automatically captured by the runner.
Furthermore, each plugin is executed with command-line arguments that includes a plugin specific output directory path, where it can store any additional files under.
These files are then packaged into the final tarball archive.

Plugins are intended to be customized according to your system and diagnostic needs.

### Plugin locations

If %%te%% is installed via a [package](../../install/index.md#installupdate),
pre-defined diagnostic plugins are installed in `/usr/share/tedge/diag-plugins`.

You can also include any additional plugin directories when collecting the diagnostic information by specifying multiple `--plugin-path` options:

```sh
tedge diag collect --plugin-path /usr/share/tedge/diag-plugins --plugin-path /your/own/dir/path
```

Alternatively, you can permanently add a custom directory path via the `tedge config add` command.
In this case, you don't need to give specific path by command-line.

```sh
sudo tedge config add diag.plugin_paths "your/own/dir/path"
tedge diag collect
```

### Predefined plugins

The [predefined plugins](https://github.com/thin-edge/thin-edge.io/tree/main/configuration/contrib/diag-plugins) include:

| Plugin Name        | Description                                                                             |
| ------------------ | --------------------------------------------------------------------------------------- |
| 01_tedge.sh        | Collects logs from `tedge-mapper` and `tedge-agent`, along with `tedge config` settings |
| 02_os.sh           | Collects system information                                                             |
| 03_mqtt.sh         | Collects all messages on the broker with `tedge mqtt sub #`                             |
| 04_workflow.sh     | Copies workflow logs                                                                    |
| 05_entities.sh     | Collects the metadata of all registered entities                                        |
| 06_internal.sh     | Copies internal state files                                                             |
| 07_mosquitto.sh    | Collects mosquitto logs (skipped when using the built-in bridge)                        |
| template.sh.ignore | A template for creating custom diagnostic plugin                                        |

### Disabling plugins

To disable a plugin, add the `.ignore` extension to its filename (e.g., `10_test.sh.ignore`).

### Writing your own diagnostic plugin

The easiest way to create a custom plugin is to start from the [template script](https://github.com/thin-edge/thin-edge.io/blob/main/configuration/contrib/diag-plugins/template.sh.ignore).

First, rename the file, removing the `.ignore` extension, and copy it to your desired location:

```sh
mkdir -p /etc/tedge/diag-plugins
cp /usr/share/tedge/diag-plugins/template.sh.ignore /etc/tedge/diag-plugins/100_my-plugin.sh
```

Then, modify the `collect()` function to gather all relevant diagnostic data for your service,
by printing relevant information to Standard Output (stdout) and Standard Error (stderr) and collecting log files.

Here’s an example that outputs system information to `stdout` and a log file:

```sh title="file: /etc/tedge/diag-plugins/100_my-plugin.sh"
collect() {
    # output to stdout (captured in `100_my-plugin/output.log`)
    echo "system data"
    uname -a

    # output to a file (saved as `100_my-plugin/system.log`)
    uname -a > "$OUTPUT_DIR"/system.log 2>&1
}
```

The `$OUTPUT_DIR` variable is the path of the subdirectory that the runner creates for a plugin.
All files from the plugin should be stored in the directory, so that the runner can package them in the end.

To test your plugin, specify your custom plugin directory:

```sh
tedge diag collect --plugin-dir /etc/tedge/diag-plugins
```

```text title="Output"
Executing /etc/tedge/diag-plugins/100_my-plugin.sh... ✓

Total 1 executed: 1 completed, 0 failed, 0 skipped
Diagnostic information saved to /tmp/tedge-diag-2025-06-12_14-25-35.tar.gz
```

After decompressing the archive, you will see the following directory structure.
Check the contents of the files.

```text title="Directory tree"
tedge-diag-2025-06-12_14-25-35
`-- 100_my-plugin
    |-- output.log
    `-- system.log
```

### Reference

For more details about diagnostic plugins, please see the [specification](../../references/diagnostic-plugin.md).
