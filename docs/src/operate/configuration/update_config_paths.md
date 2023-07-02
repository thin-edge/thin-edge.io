---
title: Path Configuration
tags: [Operate, Configuration, Unix]
sidebar_position: 4
---

# How to change default paths

## thin-edge.io directories

The `tedge config set` command can be used to change various file system paths used by tedge components.
The following table captures the paths that can be changed, with their default locations.

| Config Key | Description | Default Value |
|------------|-------------|---------------|
| tmp.path | Directory where temporary files are created/stored. E.g: while downloading files | `/tmp` |
| logs.path | Directory where log files are created | `/var/log` |
| run.path | Directory where runtime information are stored | `/run` |
| data.path | Directory where data files are stored. E.g: Cached binary files, operation metadata etc | `/var/tedge` |


The following daemons also need to be re-started after `data.path` is updated:

* `c8y-configuration-plugin`
* `c8y-firmware-plugin`

## Example: Set a custom temporary directory path

The following shows how to change the temp directory used by thin-edge.io and its components.


1. Create a new directory which will be used by thin-edge.io

    ```sh
    # create a directory (with/without sudo)
    mkdir ~/tedge_tmp_dir

    # give ownership to tedge user and group
    sudo chown tedge:tedge ~/tedge_tmp_dir 
    ```

2. Update the tedge configuration to point to the newly created directory

    ```sh title="Example"
    sudo tedge config set tmp.path ~/tedge_tmp_dir
    ```

    :::info
    The directory must be available to `tedge` user and `tedge` group.
    :::

3. Restart the `tedge` daemons after any of these paths are updated, for it to take effect.

    ```sh
    sudo systemctl restart tedge-agent
    ```

## Example: Revert custom path settings

To revert any of these paths back to their default locations, `unset` that config as follows:

```sh
sudo tedge config unset tmp.path
```

Then restart the relevant services.

```sh
sudo systemctl restart tedge-agent
```
