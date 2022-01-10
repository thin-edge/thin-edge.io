# How to change temp path

The `tedge` command can be used to change the temp path. By default the directory used is `/tmp`. 

To change the temp path, run:

```shell
tedge config set tmp.path /path/to/directory
```

Note that the directory must be available to `tedge-agent` user and `tedge-agent` group.

For example:

```shell
# create a directory (with/without sudo)

mkdir ~/tedge_tmp_dir

# give ownership to tedge-agent

sudo chown tedge-agent:tedge-agent ~/tedge_tmp_dir 

# reconnect to cloud.
```
