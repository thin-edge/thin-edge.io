# Apama Software Management Plugin

The Apama plugin can be used to install Apama projects using the Cumulocity software management feature.

> Note: This plugin expects an Apama installation on the device.

## Install Apama artifacts from Cumulocity

Before an Apama project can be installed on the device using the software management feature in Cumulocity, the project files need to be added to the Cumulocity software repository.

There is a naming convention that you need to follow while creating software entries for Apama artifacts in the software repository.

For Apama projects:

1. The name must be suffixed with `::project` as in `my-demo-project::project`.
2. The version must be suffixed with `::Apama` as in `1.0::Apama` or just `::Apama` if no  version number is necessary.
3. The uploaded binary must be a `zip` file that contains the `project` directory. If a directory named `project` is not found by the plugin at the root level in the zip, it is considered invalid.

![Add new apama project in Software Repository](./images/apama-plugin/apama-project-c8y-software-repository.png)

Once the software modules have been added to the software repository, these can be installed on the device just like any other software from the `Software` tab of the device in the Cumulocity device UI.

### Testing Apama Plugin

Here is an Apama project that you can use to test this plugin.

[project zip](https://github.com/thin-edge/thin-edge.io/raw/main/tests/PySys/plugin_apama/Input/quickstart.zip)

Add these binaries as software packages in the Cumulocity software repository by following the instructions in the previous section.
Once added, this Apama project can be installed on any target device.
You can test if the project was successfully installed by running the following Apama command:

```shell
/opt/softwareag/Apama/bin/Apama_env engine_inspect -m
```

You can expect an output like this:

```console
Monitors
========
Name                                               Num Sub Monitors
----                                               ----------------
TedgeDemoMonitor                                             1
```

You can find more information on this test Apama project [here](https://github.com/thin-edge/thin-edge.io_examples/tree/main/StreamingAnalytics#testing-a-project).
