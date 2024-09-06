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

```sh command="tedge-write --help" title="tedge-write --help"
```
