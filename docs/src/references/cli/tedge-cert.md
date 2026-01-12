---
title: "tedge cert"
tags: [Reference, CLI]
sidebar_position: 2
---

# The tedge cert command

```text command="tedge cert --help" title="tedge cert"
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

```text command="tedge cert create --help" title="tedge cert create"
tedge-cert-create 
Create a self-signed device certificate

USAGE:
    tedge cert create --device-id <ID>

OPTIONS:
        --device-id <ID>    The device identifier to be used as the common name for the certificate
    -h, --help              Print help information
```

## Create-csr

```text command="tedge cert create-csr --help" title="tedge cert create-csr"
tedge-cert-create-csr 
Create certificate signing request

Usage: tedge cert create-csr [OPTIONS]

Options:
      --device-id <ID>           The device identifier to be used as the common name for the certificate
      --output-path <OUTPUT_PATH>  Path where a Certificate signing request will be stored
      --config-dir <CONFIG_DIR>  [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
  -h, --help                     Print help
```

## Show

```text command="tedge cert show --help" title="tedge cert show"
tedge-cert-show 
Show the device certificate, if any

USAGE:
    tedge cert show

OPTIONS:
    -h, --help    Print help information
```

## Remove

```text command="tedge cert remove --help" title="tedge cert remove"
tedge-cert-remove 
Remove the device certificate

USAGE:
    tedge cert remove

OPTIONS:
    -h, --help    Print help information
```

## Upload

```text command="tedge cert upload --help" title="tedge cert upload"
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
