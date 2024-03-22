---
title: "tedge cert"
tags: [Reference, CLI]
sidebar_position: 1
---

# The tedge cert command

```sh title="tedge cert"
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

```sh title="tedge cert create"
tedge-cert-create 
Create a self-signed device certificate

USAGE:
    tedge cert create --device-id <ID>

OPTIONS:
        --device-id <ID>    The device identifier to be used as the common name for the certificate
    -h, --help              Print help information
```

## Create-csr

```sh title="tedge cert create-csr"
tedge-cert-create-csr 
Create certificate signing request

Usage: tedge cert create-csr [OPTIONS]

Options:
      --device-id <ID>           The device identifier to be used as the common name for the certificate
      --output-path <OUTPUT_PATH>  Path where a Certificate signing request will be stored
      --config-dir <CONFIG_DIR>  [default: /etc/tedge]
  -h, --help                     Print help
```

## Show

```sh title="tedge cert show"
tedge-cert-show 
Show the device certificate, if any

USAGE:
    tedge cert show

OPTIONS:
    -h, --help    Print help information
```

## Remove

```sh title="tedge cert remove"
tedge-cert-remove 
Remove the device certificate

USAGE:
    tedge cert remove

OPTIONS:
    -h, --help    Print help information
```

## Upload

```sh title="tedge cert upload"
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
