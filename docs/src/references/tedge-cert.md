# The `tedge cert` command

```
tedge-cert 0.5.3
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
tedge-cert-create 0.5.3
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
tedge-cert-show 0.5.3
Show the device certificate, if any

USAGE:
    tedge cert show

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
```

## Remove

```
tedge-cert-remove 0.5.3
Remove the device certificate

USAGE:
    tedge cert remove

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
```

## Upload

```
tedge-cert-upload 0.5.3
Upload root certificate

USAGE:
    tedge cert upload <SUBCOMMAND>

FLAGS:
    -h, --help       
            Prints help information

    -V, --version    
            Prints version information


SUBCOMMANDS:
    c8y     Upload root certificate to Cumulocity
    help    Prints this message or the help of the given subcommand(s)
```
