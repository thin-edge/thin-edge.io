# Plugin Conventions

`tedge-agent` supports the following kinds of plugins which are cli tools invoked at runtime using `sudo`:
- Software management plugins (e.g: `tedge_apt_plugin`)
- Configuration management plugins (e.g: `tedge_file_config_plugin`)
- Log management plugins (e.g: `tedge_file_log_plugin`)

They all follow different CLI contracts:

## Software management plugins 

Installed at `/etc/tedge/sm-plugins`.
The agent calls these to handle `software_update` operations.

### CLI Contract

Plugins must support these subcommands:
- `list` — list installed modules
- `install` — install a module
- `remove` — remove a module
- `prepare` — pre-operation setup
- `finalize` — post-operation cleanup
- `update-list` — batch install/remove from a file

### Exit Codes

Exit codes are used to communicate the success or failure of the command.

- `0` — success
- `1` - failure
- `2` — plugin-level failure (operation failed but plugin is healthy)

## Configuration management plugins

Installed at `/usr/share/tedge/config-plugins`.
The agent calls these to handle `config_snapshot` and `config_update` operations.

### CLI Contract

Plugins must support these subcommands:
- `list` — print all supported configuration types, one per line
- `get <config_type>` — write the current configuration for the type to stdout (used for snapshots)
- `prepare <config_type> <downloaded_path> --work-dir <dir>` — called before `set`; may validate the new config or save a backup to the work directory
- `set <config_type> <downloaded_path> --work-dir <dir>` — apply the new configuration; if this fails the agent calls `rollback`
- `verify <config_type> --work-dir <dir>` — confirm the update was applied correctly; if this fails the agent calls `rollback`
- `rollback <config_type> --work-dir <dir>` — restore the previous configuration from the work directory

### Exit Codes

- `0` — success
- `1` - failure

### Log management plugins

Installed at `/usr/share/tedge/log-plugins`.
The agent calls these to handle `log_upload` operations.

Plugins must support these subcommands via clap:
- `list` — list all available log types
- `get <log_type> [--since <date>] [--until <date>]` — retrieve log content for a type, optionally filtered by date range; writes it to stdout

### Exit Codes

- `0` — success
- `1` - failure

## Output

All output captured from the plugins are logged into the operation workflow log.
Any JSON excerpts between `:::begin-tedge:::` are `:::end-tedge:::` are appended to the workflow state payload as well.

## Workspace

Plugins are listed explicitly in the root `Cargo.toml` (not globbed). Remember to add new plugins to the `members` list.
