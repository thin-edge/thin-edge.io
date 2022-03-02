# The `tedge cert` command

```
tedge-cert 
Create and manage device certificate

USAGE:
    tedge cert <SUBCOMMAND>

OPTIONS:
    -h, --help    Print help information

SUBCOMMANDS:
    create    Create a self-signed device certificate
    help      Print this message or the help of the given subcommand(s)
    remove    Remove the device certificate
    show      Show the device certificate, if any
    upload    Upload root certificate
```

## Create

```
tedge-cert-create 
Create a self-signed device certificate

USAGE:
    tedge cert create --device-id <ID>

OPTIONS:
        --device-id <ID>    The device identifier to be used as the common name for the certificate
    -h, --help              Print help information
```

## Show

```
tedge-cert-show 
Show the device certificate, if any

USAGE:
    tedge cert show

OPTIONS:
    -h, --help    Print help information
```

## Remove

```
tedge-cert-remove 
Remove the device certificate

USAGE:
    tedge cert remove

OPTIONS:
    -h, --help    Print help information
```

## Upload

```
tedge-cert-upload 
Upload root certificate

USAGE:
    tedge cert upload <SUBCOMMAND>

OPTIONS:
    -h, --help    Print help information

SUBCOMMANDS:
    c8y     Upload root certificate to Cumulocity
    help    Print this message or the help of the given subcommand(s)
```
