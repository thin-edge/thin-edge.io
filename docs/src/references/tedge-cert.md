# The `tedge cert` command
The `tedge cert` command is used to create and manage device certificate.

```
tedge-cert 0.1.0
Create and manage device certificate

USAGE:
    tedge cert <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    create    Create a self-signed device certificate
    help      Prints this message or the help of the given subcommand(s)
    remove    Remove the device certificate
    show      Show the device certificate, if any
    upload    Upload root certificate

```
## Create

```
tedge-cert-create 0.1.0
Create a self-signed device certificate

USAGE:
    tedge cert create --device-id <id>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --device-id <id>    The device identifier to be used as the common name for the certificate

```

## Show

```
tedge-cert-show 0.1.0
Show the device certificate, if any

USAGE:
    tedge cert show

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

```

## Remove

```
tedge-cert-remove 0.1.0
Remove the device certificate

USAGE:
    tedge cert remove [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

```

## Upload

```
tedge-cert-upload 0.1.0
Upload root certificate

USAGE:
    tedge cert upload <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    c8y     Upload root certificate to Cumulocity
    help    Prints this message or the help of the given subcommand(s)
```
