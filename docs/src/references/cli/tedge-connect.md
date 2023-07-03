---
title: "tedge connect"
tags: [Reference, CLI]
sidebar_position: 3
---

# The tedge connect command

```sh title="tedge connect"
tedge-connect 
Connect to connector provider

USAGE:
    tedge connect <SUBCOMMAND>

OPTIONS:
    -h, --help    Print help information

SUBCOMMANDS:
    aws     Create connection to AWS
    az      Create connection to Azure
    c8y     Create connection to Cumulocity
    help    Print this message or the help of the given subcommand(s)
```

## AWS

```sh title="tedge connect aws"
tedge-connect-aws 
Create connection to AWS

The command will create config and start edge relay from the device to AWS instance

USAGE:
    tedge connect aws [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --test
            Test connection to AWS
```

## Azure

```sh title="tedge connect az"
tedge-connect-az 
Create connection to Azure

The command will create config and start edge relay from the device to az instance

USAGE:
    tedge connect az [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --test
            Test connection to Azure
```

## Cumulocity

```sh title="tedge connect c8y"
tedge-connect-c8y 
Create connection to Cumulocity

The command will create config and start edge relay from the device to c8y instance

USAGE:
    tedge connect c8y [OPTIONS]

OPTIONS:
    -h, --help
            Print help information

        --test
            Test connection to Cumulocity
```
