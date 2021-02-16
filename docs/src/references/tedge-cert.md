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
```

## Create

```
tedge-cert-create 0.1.0
Create a self-signed device certificate

USAGE:
    tedge cert create [OPTIONS] --id <id>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --cert-path <cert-path>    The path where the device certificate will be stored [default: ./tedge-
                                   certificate.pem]
        --id <id>                  The device identifier
        --key-path <key-path>      The path where the device private key will be stored [default: ./tedge-private-
                                   key.pem]
```

## Show

```
tedge-cert-show 0.1.0
Show the device certificate, if any

USAGE:
    tedge cert show [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --cert-path <cert-path>    The path where the device certificate will be stored [default: ./tedge-
                                   certificate.pem]
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

OPTIONS:
        --cert-path <cert-path>    The path of the certificate to be removed [default: ./tedge-certificate.pem]
        --key-path <key-path>      The path of the private key to be removed [default: ./tedge-private-key.pem]
```
