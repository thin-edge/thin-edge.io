---
title: Path Configuration
tags: [Operate, Configuration, Unix]
description: Customize %%te%% file/folder paths
---

## %%te%% directories

The `tedge config set` command can be used to change various file system paths used by the %%te%% components.
The following table captures the paths that can be changed along with their default locations.

| Config Key | Description | Default Value |
|------------|-------------|---------------|
| tmp.path | Directory where temporary files are created/stored. E.g: while downloading files | `/tmp` |
| logs.path | Directory where log files are created | `/var/log/tedge` |
| run.path | Directory where runtime information are stored | `/run` |
| data.path | Directory where data files are stored. E.g: Cached binary files, operation metadata etc | `/var/tedge` |


The following daemons also need to be re-started after `data.path` is updated:

* `tedge-agent`
* `c8y-firmware-plugin`
