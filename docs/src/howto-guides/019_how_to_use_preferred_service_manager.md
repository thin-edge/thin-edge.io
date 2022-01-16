# How to use thin-edge.io with your preferred init system

thin-edge.io works seamlessly with `systemd` on the CLI commands `tedge connect` and `tedge disconnect`.
However, not all OS support `systemd`.
You might want to use other init systems like `OpenRC`, `BSD`, `init.d` with thin-edge.io.
For this demand, this guide explains how to give a custom init system configuration to thin-edge.io.

## How to set a custom init system configuration

Create a file `system.toml` owned by `root:root` in `/etc/tedge` directory.

```shell
sudo touch /etc/tedge/system.toml
```

Open your editor and copy and paste the following toml contents.
This is an example how to configure OpenRC as the init system for thin-edge.io.
We have example configurations for BSD, OpenRC, and systemd under [thin-edge.io/configuration/contrib/system](https://github.com/thin-edge/thin-edge.io/tree/main/configuration/contrib/system).


```toml
[init]
name = "OpenRC"
is_available = ["/sbin/rc-service", "-l"]
restart = ["/sbin/rc-service", "{}", "restart"]
stop =  ["/sbin/rc-service", "{}", "stop"]
enable =  ["/sbin/rc-update", "add", "{}"]
disable =  ["/sbin/rc-update", "delete", "{}"]
is_active = ["/sbin/rc-service", "{}", "status"]
```

Then, adjust the values with your preferred init system.
To get to know the rules of the configuration file, please refer to [Init System Configuration File](./../references/system-config.md).

After you finish creating your own configuration file, it's good to limit the file permission to read-only.

```shell
sudo chmod 444 /etc/tedge/system.toml
```

Now `tedge connect` and `tedge disconnect` use the init system that you specified!

## Default settings

If the custom configuration file `/etc/tedge/system.toml` is not placed,
`tedge connect` and `tedge disconnect` will use `/bin/systemctl` as the init system.

## Reference document
- [Init System Configuration File](./../references/system-config.md)
