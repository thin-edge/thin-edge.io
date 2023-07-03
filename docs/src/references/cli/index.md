---
title: "The tedge command"
tags: [Reference, CLI]
sidebar_position: 4
---

# The tedge command

```sh title="tedge"
tedge is the cli tool for thin-edge.io

USAGE:
    tedge [OPTIONS] [SUBCOMMAND]

OPTIONS:
        --config-dir <CONFIG_DIR>    [default: /etc/tedge]
    -h, --help                       Print help information
        --init                       Initialize the tedge
    -V, --version                    Print version information

SUBCOMMANDS:
    cert          Create and manage device certificate
    config        Configure Thin Edge
    connect       Connect to connector provider
    disconnect    Remove bridge connection for a provider
    help          Print this message or the help of the given subcommand(s)
    init          Initialize Thin Edge
    mqtt          Publish a message on a topic and subscribe a topic
    reconnect     Reconnect command, calls disconnect followed by connect
```
