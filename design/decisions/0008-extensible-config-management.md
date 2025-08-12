# Extensible configuration management

* Date: __2025-08-12__
* Status: __New__

## Background

The built-in configuration and log management features offered by `tedge-agent` has the following limitations:

- only file based configurations and logs can be retrieved.
- while updating a configuration file, there are no pre/post actions that can be performed.
  For e.g, a user would want to backup the existing configuration before updating the configuration,
  and restart a service after the configuration is updated.
- Though the tedge workflows feature allows overriding the existing behavior,
  when a built-in operation is overridden, you end up losing all existing functionality,
  like uploading/downloading the files from the cloud as well.

The goal is to provide an interface that allows users to extend only the specific aspect of the operation that they want,
like the pre-update or post-update actions, leaving the rest of the behavior unchanged.

## Solution proposal

- The agent supports plugins for log and config management.
- The supported types for the respective operation is gathered from all these plugins.
- The agent maintains a mapping of the types and their corresponding plugins.
- When an an operation is received for a type, it is delegated to the corresponding plugin.

### Log Management

- `tedge-agent` to add support for multiple log management plugins defined under `/etc/tedge/log-plugins`.
- Migrate the existing file-based log functionality into a dedicated `file` plugin which is available by default.
- To retrieve logs from other sources like systemd journal or docker, add respective plugins for these sources.
- Each plugin must support the following sub-commands:
  - `list`: list all the log types that it supports.
    Used to detect if this is a valid plugin if it exits with exit code 0.
    The output is used only when auto-discovery for log types is turned ON for this plugin.
  - `get <type> [--from <timestamp>] [--to <timestamp>] [--filter <filter-text>]`:
    Used to fetch the log for the given type within the provided log range
- The plugin directory would also have a `conf.d` directory where each plugin can store their configuration in a toml file.
  These config files are used to turn type discovery on/off and to define inclusion/exclusion list when type discovery is turned on:
  ```docker.toml
  [docker]
  auto_discover=true

  [docker.include]
  container_name_regex = "nginx"

  [docker.include]
  image_name_regex = ""
  
  [docker.exclude]
  container_name_regex = "kube-*"
  ``` 
- The main config file for a plugin must be named after the plugin itself (e,g: `docker.sh` -> `docker.toml`).
  Key configs like `auto_discover` can only be defined in this main file.
  The inclusion/exclusion list can be defined in extension files as well,
  but the table names with the type prefix (e.g: `[docker.include]` and `[docker.exclude]`) must be used in those as well.
- When new software is installed, their corresponding `include/exclude` entries can be appended to the main plugin config itself,
  or created in an independent extension file.
