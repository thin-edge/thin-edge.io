---
title: tedge-write
tags: [Reference, Configuration, CLI]
description: Granting thin-edge write access to protected files
---

**tedge-write** is a `tee`-like helper %%te%% component used by **tedge-agent** for privilege elevation.

tedge-agent spawns a `tedge-write` process when it needs to write to files that `tedge`
user/group has no write permissions to (e.g. system files or files owned by other packages), for
example when writing an updated configuration file as part of [`config_update` operation][1].
`tedge-agent` will first try to write to a file directly and only retry using `tedge-write` if
direct write fails due to `tedge` user/group not having write permissions to either the file itself
or its parent directory.

[1]: agent/tedge-configuration-management.md#handling-config-update-commands

## Permission elevation

`tedge-write` relies on `sudo` to grant the process write and execute permissions to the target
file's parent directory.

For example, when using following `sudoers` entry:

```sudoers title="file: /etc/sudoers.d/tedge"
tedge   ALL = (ALL) NOPASSWD: /usr/bin/tedge-write /etc/*
```

Permissions are granted when:

- calling user is `tedge`
- binary is `/usr/bin/tedge-write`
- the 1st argument is `/etc/*`, i.e. the files `tedge-write` will be writing to are inside `/etc`

The entry grants privileges without authentication, which is required when `tedge-write` is spawned
by `tedge-agent`.


:::note

When %%te%% is installed using any one of the standard installation methods, the sudoers entry is
automatically created under `/etc/sudoers.d/tedge`, but in non-standard setups the sudoers
configuration may need to be updated manually.

Feel free to customise the sudoers entry according to your requirements, but make sure the entry
correctly grants privileges to all required files and a full and valid path to `tedge-write` binary
is used.

See [`sudoers(5)`][2] for additional details.

:::

[2]: https://www.man7.org/linux/man-pages/man5/sudoers.5.html

If you prefer to disable this permission elevation mechanism or don't want to use `sudo`, set the
tedge config `sudo.enable` setting to `false`. `tedge-write` will still get spawned, but without
`sudo`.

```sh
sudo tedge config set sudo.enable false
```

## Details

Write permission to the parent directory is required to guarantee config updates are atomic.


While updating a file, `tedge-write` will perform an atomic write, i.e. it will write to a temporary
file, set file ownership and mode, and finally rename the temporary into the final file.
If the target file already exists, its original ownership/mode will be preserved and optionally
provided new values will be ignored.


## Command help

```text command="tedge-write --help" title="tedge-write --help"
tee-like helper for writing to files which `tedge` user does not have write permissions to.

To be used in combination with sudo, passing the file content via standard input.

Usage: tedge-write [OPTIONS] <DESTINATION_PATH>

Arguments:
  <DESTINATION_PATH>
          A canonical path to a file to which standard input will be written.

          If the file does not exist, it will be created with the specified owner/group/permissions. If the file does exist, it will be overwritten, but its owner/group/permissions will remain unchanged.

Options:
      --mode <MODE>
          Permission mode for the file, in octal form

      --user <USER>
          User which will become the new owner of the file

      --group <GROUP>
          Group which will become the new owner of the file

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

  -V, --version
          Print version
```
