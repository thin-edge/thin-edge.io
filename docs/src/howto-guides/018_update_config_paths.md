# How to change default paths

The `tedge config set` command can be used to change various file system paths used by tedge components.
The following table captures the paths that can be changed, with their default locations.

| Config Key | Description | Default Value |
|------------|-------------|---------------|
| tmp.path | Directory where temporary files are created/stored. E.g: while downloading files | `/tmp` |
| logs.path | Directory where log files are created | `/var/log` |
| run.path | Directory where runtime information are stored | `/run` |
| data.path | Directory where data files are stored. E.g: Cached binary files, operation metadata etc | `/var/tedge` |

For e.g: to change the temp path, run:

```shell
sudo tedge config set tmp.path /path/to/directory
```

Note that the directory must be available to `tedge` user and `tedge` group.

For example:

```shell
# create a directory (with/without sudo)

mkdir ~/tedge_tmp_dir

# give ownership to tedge user and group

sudo chown tedge:tedge ~/tedge_tmp_dir 

```

You must restart the `tedge` daemons after any of these paths are updated, for it to take effect.
But, when the `data.path` is updated, the daemon must be re-initialized as well with its `--init` option.

For example, the `tedge-agent` must be re-initialized as follows:

```shell
# Stop the service
sudo systemctl stop tedge-agent

# Re-initialize the service
sudo tedge-agent --init

# Restart the service
sudo systemctl stop tedge-agent
```

The following daemons also need to be re-initialized after `data.path` is updated:
* `c8y-configuration-plugin`
* `c8y-firmware-plugin`

To revert any of these paths back to their default locations, just `unset` that config as follows:

```shell
sudo tedge config unset tmp.path
```
