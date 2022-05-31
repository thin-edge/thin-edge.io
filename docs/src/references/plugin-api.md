# Software Management Plugin API

Thin-edge uses plugins to delegate to the appropriate package managers and installers
all the software management operations: installation of packages, uninstallations and queries.

* A package manager plugin acts as a facade for a specific package manager.
* A plugin is an executable that follows the [plugin API](#plugin-api).
* On a device, several plugins can be installed to deal with different kinds of software modules.
* The filename of a plugin is used by thin-edge to determine the appropriate plugin for a software module.
* All the actions on a software module are directed to the plugin bearing the name that matches the module type name.
* The plugins are loaded and invoked by the sm-agent in a systematic order (in practice the alphanumerical order of their names in the file system).
* The software modules to be installed/removed are also passed to the plugins in a consistent order.
* Among all the plugins, one can be marked as the default plugin using `tedge config` cli.
* The default plugin is invoked when an incoming software module in the cloud request doesn't contain any explicit type annotation.
* Several plugins can co-exist for a given package manager as long as they are given different names.
  Each can implement a specific software management policy.
  For instance, for a debian package manager, several plugins can concurrently be installed, say one named `apt` to handle regular packages from the public apt repository and another named `company-apt` to install packages from a company's private package repository.

## Plugin repository

* To be used by thin-edge, a plugin has to stored in the directory `/etc/tedge/sm-plugins`.
* A plugin must be named after the software module type as specified in the cloud request.
  That is, a plugin named `apt` handles software modules that are defined with type `apt` in the cloud request.
  Consequently a plugin to handle software module defined for `docker` must be named `docker`.
* The same plugin can be given different names, using virtual links.
* When there are multiple plugins on a device, one can be marked as the default plugin using the command
  `tedge config set software.plugin.default <plugin-name>`
* If there's one and only one plugin available on a device, that's treated as the default, even without an explicit configuration.

On start-up and sighup, the sm-agent registers the plugins as follow:
1. Iterate over the executable file of the directory `/etc/tedge/sm-plugins`.
2. Check the executable is indeed a plugin, calling the [`list`](#the-list-command) command.

## Plugin API

* A plugin must implement all the commands used by the sm-agent of thin-edge,
  and support all the options for these commands.
* A plugin should not support extra command or option.
* A plugin might have a configuration file.
  * It can be a list of remote repositories, or a list of software modules to be excluded.
  * These configuration files can be managed from the cloud via the sm-agent (TODO: how).

### Input, Output and Errors

* The plugins are called by the sm-agent using a child process for each action.
* Beside command `update-list` there is no input beyond the command arguments, and a plugin that does not
  implement `update-list` can close its `stdin`.
* The `stdout` and `stderr` of the process running a plugin command are captured by the sm-agent.
  * These streams don't have to be the streams returned by the underlying package manager.
    It can be a one sentence summary of the error, redirecting the administrator to the package manager logs.
* A plugin must return the appropriate exit status after each command.
  * In no cases, the error status of the underlying package manager should be reported.
* The exit status are interpreted by sm-agent as follows:
  * __`0`__: success.
  * __`1`__: usage. The command arguments cannot be interpreted, and the command has not been launched.
  * __`2`__: failure. The command failed and there is no point to retry.
  * __`3`__: retry. The command failed but might be successful later (for instance, when the network will be back).
* If the command fails to return within 5 minutes, the sm-agent reports a timeout error:
  * __`4`__: timeout.

### The `list` command

When called with the `list` command, a plugin returns the list of software modules that have been installed with this plugin,
using tab separated values.

```shell
$ debian-plugin list
...
collectd-core  5.8.1-1.3
mosquitto   1.5.7-1+deb10u1
...
```

Contract:
* This command take no arguments.
* If an error status is returned, the executable is removed from the list of plugins.
* The list is returned using [CSV with tabulations as separators](https://en.wikipedia.org/wiki/Tab-separated_values).
  Each line has two values separated by a tab: the name of the module then the version of that module.
  If there is no version for a module, then the trailing tabulation is not required and be skipped.
### The `prepare` command

The `prepare` command is invoked by the sm-agent before a sequence of install and remove commands

```shell
$ /etc/tedge/sm-plugins/debian prepare
$ /etc/tedge/sm-plugins/debian install x
$ /etc/tedge/sm-plugins/debian install y
$ /etc/tedge/sm-plugins/debian remove z
$ /etc/tedge/sm-plugins/debian finalize
```

For many plugins this command will do nothing. However, It gives an opportunity to the plugin to:
* Update the dependencies before an operation, *i.e. a sequence of actions.
  Notably, a debian plugin can update the `apt` cache issuing an `apt-get update`.
* Start a transaction, in case the plugin is able to manage rollbacks.

Contract:
* This command take no arguments.
* No output is expected.
* If the `prepare` command fails, then the planned sequences of actions (.i.e the whole sm operation) is cancelled.

### The `finalize` command

The `finalize` command closes a sequence of install and remove commands started by a `prepare` command.

This can be a no-op, but this is also an opportunity to:
* Remove any unnecessary software module after a sequence of actions.
* Commit or rollback the sequence of actions.
* Restart any processes using the modules, e.g. restart the analytics engines if the modules have changed

Contract:
* This command take no arguments.
* No output is expected.
* This command might check (but doesn't have to) that the list of install and remove command has been consistent.
  * For instance, a plugin might raise an error after the sequence `prepare;install a; remove a-dependency; finalize`.
* If the `finalize` command fails, then the planned sequences of actions (.i.e the whole sm operation) is reported as failed,
  even if all the atomic actions has been successfully completed.

### The `install` command

The `install` command installs a software module, possibly of some expected version.

```shell
$ plugin install NAME [--module-version VERSION] [--file FILE]
```

Contract:
* The command requires a single mandatory argument: the software module name.
  * This module name is meaningful only to the plugin.
* An optional version string can be provided.
  * This version string is meaningful only to the plugin
    and is transmitted unchanged from the cloud to the plugin.
  * The version string can include constraints (as at least that version),
    from the sm-agent viewpoint this is no more than a string.
  * If no version is provided the plugin is free to install the more appropriate version.
* An optional file path can be provided.
  * When the device administrator provides an url,
    the sm-agent downloads the software module on the device,
    then invoke the install command with a path to that file.
  * If no file is provided, the plugin has to derive the appropriate location from its repository
    and to download the software module accordingly.
* The command installs the requested software module and any dependencies that might be required.
  * It is up to the plugin to define if this command triggers an installation or an upgrade.
    It depends on the presence of a previous version on the device and
    of the ability of the package manager to deal with concurrent versions for a module.
  * A plugin might not be able to install dependencies.
    In that case, the device administrator will have to request explicitly the dependencies to be installed first.
  * After a successful sequence `prepare; install foo; finalize` the module `foo` must be reported by the `list` command.
  * After a successful sequence `prepare; install foo --module-version v; finalize` the module `foo` must be reported by the `list` command with the version `v`.
    If the plugin manage concurrent versions, the module `foo` might also be reported with versions already installed before the operation.
  * A plugin is not required to detect inconsistent actions as `prepare; install a; remove a-dependency; finalize`.
  * This is not an error to run this command twice or when the module is already installed.
* An error must be reported if:
  * The module name is unknown.
  * There is no version for the module that matches the constraint provided by the `--version` option.
  * The file content provided by `--file` option:
    * is not in the expected format,
    * doesn't correspond to the software module name,
    * has a version that doesn't match the constraint provided by the `--module-version` option (if any).
  * The module cannot be downloaded.
  * The module cannot be installed.

### The `remove` command

The `remove` command uninstalls a software module, and possibly its dependencies if no other modules are dependent on those.

```shell
$ plugin remove NAME [--module-version VERSION]
```

Contract:
* The command requires a single mandatory argument: the module name.
  * This module name is meaningful only to the plugin
    and is transmitted unchanged from the cloud to the plugin.
* An optional version string can be provided.
  * This version string is meaningful only to the plugin
    and is transmitted unchanged from the cloud to the plugin.
* The command uninstall the requested module and possibly any dependencies that are no more required.
  * If a version is provided, only the module of that version is removed.
    This is in-practice useful only for a package manager that is able to install concurrent versions of a module.
  * After a successful sequence `prepare; remove foo; finalize` the module `foo` must no more be reported by the `list` command.
  * After a successful sequence `prepare; remove foo --module-version v; finalize` the module `foo` no more be reported by the `list` command with the version `v`.
    If the plugin manage concurrent versions, the module `foo` might still be reported with versions already installed before the operation.
  * A plugin is not required to detect inconsistent actions as `prepare; remove a; install a-reverse-dependency; finalize`.
  * This is not an error to run this command twice or when the module is not installed.
* An error must be reported if:
  * The module name is unknown.
  * The module cannot be uninstalled.

### The `update-list` command

The `update-list` command accepts a list of software modules and associated operations as `install` or `remove`.

This basically achieves same purpose as original commands `install` and `remove`, but gets passed all software modules to be processed in one command.
This can be needed when order of processing software modules is relevant - e.g. when dependencies between packages inside the software list do occur.

```shell
# building list of software modules and operations, 
# and passing to plugin's stdin via pipe:
# NOTE that each argument is tab separated:

$ echo '\
  install	name1	version1
  install	name2		path2
  remove	name3	version3
  remove	name4'\
 | plugin update-list
```

Contract:
* This command is optional for a plugin. It can be implemented alternatively to original commands `install` and `remove` as both are specified above.
  * If a plugin does not implement this command it must return exit status `1`. In that case sm-agent will call the plugin again
    package-by-package using original commands `install` and `remove`.
  * If a plugin implements this command sm-agent uses it instead of original commands `install` and `remove`.
* This command takes no commandline arguments, but expects a software list sent from sm-agent to plugin's `stdin`.
* In the software list each software module is represented by exactly one line, using tab separated values.
* The position of each argument in the argument list has it's defined meaning:
  * 1st argument: Is the operation and can be `install` or `remove`
  * 2nd argument: Is the software module's name.
  * 3rd argument: Is the software module's version. That argument is optional and can be empty (then empty string "" is used).
  * 4th argument: Is the software module's path. That argument is optional and can be empty (then empty string "" is used). For operation `remove` that argument does not exist.
* Behaviour of operations `install` and `remove` is same as for original commands `install` and `remove` as specified above.
  * For details about operations' arguments "name", "version" and "path", see specification of original command `install` or `remove`.
  * For details about `exitstatus` see accoring specification of original command `install` or `remove`.
* An overall error must be reported (via process's exit status) when at least one software module operation has failed.

Example how to invoke that plugin command `update-list`. Note that each argument is tab separated:

```shell
$ plugin update-list <<EOF
  install	name1	version1
  install	name2		path2
  remove	name3	version3
  remove	name4
EOF
```

That is equivalent to use of original commands (`install` and `remove`):

```shell
$ plugin install name1 --module-version version1
$ plugin install name2 --module-path path2
$ plugin remove "name 3" --module-version version3
$ plugin remove name4
```

Exemplary implementation of a shell script for parsing software list from `stdin`:

Note that this example works only in bash.
```shell
#!/bin/bash

echo ""
echo "---+++ reading software list +++---"
while IFS=$'\t' read -r ACTION MODULE VERSION FILE
do
    echo "$0 $ACTION $MODULE $VERSION"
done
```
