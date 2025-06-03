---
title: Diagnostic Collection
tags: [Operate]
sidebar_position: 1
description: How to collect logs and system diagnostics using plugin-based automation
---

%%te%% helps you collect useful diagnostic information about your device and services by executing diagnostic plugins.
These diagnostics are bundled into a single compressed archive (`.tar.gz`) for easy analysis or sharing with support.

This guide explains how to use the diagnostic collection tool and how plugins work.

## Getting Started

To collect a snapshot of diagnostic data, run:

```sh
tedge diag collect
```

This command performs the following steps:
* Discovers all available diagnostic plugins (default location: `/usr/share/tedge/diag-plugins`)
* Executes each plugin in sequence
* Captures their logs and outputs
* Bundles all collected data into a .`tar.gz` archive (e.g., `/tmp/tedge-diag-YYYY-MM-DD_HH-MM-SS.tar.gz`)

All directory paths and the package name can be customized using command-line arguments or the `tedge config` command.

To see all available options, run:

```sh
tedge diag collect --help
```

## Diagnostic Plugins

Diagnostic plugins are executable scripts or binaries invoked by `tedge diag collect` (referred to as the "runner") to gather diagnostic data.

Each plugin is called with the collect subcommand, along with two options:
* `--output-dir`: where the plugin should store generated files
* `--config-dir:` the directory containing the configuration (`tedge.toml`)

Some diagnostic plugins are included with the %%te%% package and installed by default in `/usr/share/tedge/diag-plugins`.
You can also create your own plugins and store them in any directory.

Each plugin is expected to finish within a limited time. If it runs too long, the runner will terminate it.

The plugin may print to `stdout` and `stderr`, which will be captured into an `output.log` file by the runner.
Additionally, it can write its own files into the specified output directory.

### Plugin Exit Codes

* `0`: for successful completion
* `2`: if the plugin chooses to skip itself (for example, if it's not relevant for the current system)
* any other non-zero value to indicate an error

To disable a plugin temporarily, simply rename it to include a `.ignore` extension—the runner will skip such files.

For more details, see the [specification](../../references/diagnostic-plugin.md).
