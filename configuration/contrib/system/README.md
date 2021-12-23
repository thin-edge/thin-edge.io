# System Configuration File

To support multiple system and service manager, `tedge` requires the `/etc/tedge/system.toml` file.
The file configures which system manager executes which command to do some actions. 

The format of the file is:

```toml
name = "systemd"
is_available = ["/bin/systemctl", "--version"]
restart = ["/bin/systemctl", "restart", "{}"]
stop =  ["/bin/systemctl", "stop", "{}"]
enable =  ["/bin/systemctl", "enable", "{}"]
disable =  ["/bin/systemctl", "disable", "{}"]
is_active = ["/bin/systemctl", "is-active", "{}"]
```

## Placeholder

`{}` will be replaced by a service name (`mosquitto`, `tedge-mapper-c8y`, `tedge-mapper-az`, etc.).
For example,

```toml
restart = ["/bin/systemctl", "restart", "{}"]
```

will be interpreted to

```shell
/bin/systemctl restart mosquitto
```

## Keys

- **name**: An identifier of the system manager. 
  It is used in the output of `tedge connect` and `tedge disconnect`.
- **is_available**: The command to check if the system manager is available on your system.
- **restart**: The command to restart a service by the system manager.
- **stop**: The command to stop a service by the system manager.
- **enable**: The command to enable a service by the system manager.
- **disable**: The command to disable a service by the system manager.
- **is_active**: The command to check if the service is running by the system manager.
