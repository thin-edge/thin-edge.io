---
title: Diagnostic API
tags: [ Reference, API ]
sidebar_position: 100
description: Diagnostic Plugin API reference
---

# Diagnostic Runner and Plugin API

The `tedge diag collect` command is used to gather diagnostics by executing a series of plugins.
Each plugin outputs logs or relevant system data, and everything is bundled into a single `.tar.gz` archive.

The design consists of two parts, the **runner** and the **plugins**.
The runner is the `tedge diag collect` command, which calls plugins that must be executable files.

## Runner

* The runner supports multiple plugin directory paths, which are resolved in the following order of precedence:
    1. Command-line arguments (e.g. `--plugin-dir /pathA --plugin-dir /pathB` or `--plugin-dir "/pathA,/pathB"`)
    2. Environmental variable (`TEDGE_DIAG_PLUGIN_PATHS`)
    3. `tedge config` value (`diag.plugin_paths`)
* The runner creates a temporary output directory at `<OUTPUT_DIR>/<TARBALL_NAME>` (e.g. `/tmp/tedge-diag-now`) and
  subdirectories for each plugin at
  `<OUTPUT_DIR>/<TARBALL_NAME>/<PLUGIN_NAME>` (e.g. `/tmp/tedge-diag-now/01_tedge`) to collect output files.
* The runner does not need to understand the content of each plugin script.
  It blindly executes all plugin scripts located under `<PLUGIN_DIR>` (e.g. `/usr/share/tedge/diag-plugins`) using the
  following arguments:
    * arg1: `collect`
    * option1 `--output-dir <PATH>`: the path to the temporary output subdirectory for the plugin (e.g.
      `/tmp/tedge-diag-now/01_tedge`)
    * option2 `--config-dir <PATH>`: the directory where the `tedge.toml` file is stored (e.g. `/etc/tedge`)
* The runner logs both stdout and stderr from the plugin's execution and stores them in `output.log`.
* The runner makes a tarball archive of the output directory as `<OUTPUT_DIR>/<TARBALL_NAME>.tar.gz` (e.g.
  `/tmp/tedge-diag-now.tar.gz`)
* The runner determines its own exit codes based on the exit codes of the plugins:
    * `0`: all plugins returned either `0` or `2`
    * `1`: at least one plugin returned a code other than `0` or `2`
    * `2`: no valid plugins were found in the specified plugin directory
* If a plugin runs too long, the runner sends a `SIGTERM` after a grace period. If the plugin does not terminate,
  it is forcibly terminated with `SIGKILL`.

### Directory Hierarchy Example

#### Plugin Directory

```
/usr/share/tedge/diag-plugins/
├─ 01_tedge.sh
├─ 02_os.sh
├─ 03_mqtt.sh
├─ ...
```

#### Temporary Output Directory

```
/tmp/
├─ tedge-diag-2025-05-20_15-08-42.tar.gz
├─ tedge-diag-2025-05-20_15-08-42
│  ├─ 01_tedge/
│  │  ├─ output.log
│  │  ├─ tedge-agent.log
│  │  ├─ tedge-mapper-c8y.log
│  ├─ 02_os/
│  │  ├─ output.log
│  ├─ 03_mqtt/
│  │  ├─ output.log
│  │  ├─ tedge-mqtt-sub-retained-only.log
│  │  ├─ ...
```

## Plugin

* A plugin must be an executable file.
* It is called by the runner with the specified arguments (see the [Runner](#runner) section).
* The plugin can print diagnostic information on its standard output and error stream, both will be collected by the
  runner.
* The plugin can also create files in the output directory provided as `--output-dir` by the runner.
* The plugin must complete execution within the timeout period and return an appropriate exit code:
    * `0`: successful execution
    * `2`: plugin skipped (not applicable)
    * any other non-zero value: error occurred
* Plugins with the `.ignore` extension will be ignored.

### Example Plugin Execution

The 01_tedge.sh plugin is invoked by the runner as follows:

```shell
/usr/share/tedge/diag-plugins/01_tedge.sh collect \
  --output-dir "/tmp/tedge-diag-now/01_tedge" \
  --config-dir "/etc/tedge"
```
