# Sample init scripts templates

There are a few samples provided, for systemd and for initd based systems.

The files provide simple daemon configuration templates for init scripts along the start, stop, restart and status commands.

## systemd Example

This is provided in the thin-edge.service file which contains basic configuration of the binary startup behaviour.

Most Linux distributions use systemd as a system and service manager, however in case of Yocto (as well as buildroot) and other custom distributions this may not be a case.

The systemctl is the main command in systemd, used to control services.

### Sample Service

Create systemd service file in /etc/systemd/system/ (copy the template):

```sh
    sudo cp systemd/thin-edge.service /etc/systemd/system/thin-edge.service
    sudo chmod 664 /etc/systemd/system/thin-edge.service
```

Once the service file is added, you need to reload systemd daemon:

```sh
    sudo systemctl daemon-reload
```

Now you should be able to start, stop, restart and check the service status

```sh
    sudo systemctl start thin-edge
    sudo systemctl stop thin-edge
    sudo systemctl restart thin-edge
    systemctl status thin-edge
```

All daemons can be configured to start on boot:

```sh
    sudo systemctl enable thin-edge
```

Logs are available via:

```sh
    journalctl -u thin-edge
```

## initd Example

### Getting started

Copy thin-edge to /etc/init.d/thin-edge
```sh
    cp /initd/tedge /etc/init.d/tedge
```

### Script usage

Start the app.

```sh
    /etc/init.d/tedge start
```

Stop the app.

```sh
    /etc/init.d/tedge stop
```

Restart the app.

```sh
    /etc/init.d/tedge restart
```

Print current daemon status.

```sh
    /etc/init.d/tedge status
```

## Logging

By default, standard output goes to `/var/log/tedge.log` and error output to `/var/log/tedge.err`. You can change where the logs will be written by changing `stdout_log` and `stderr_log`.
