---
title: "tedge config"
tags: [Reference, CLI]
sidebar_position: 2
---

# The tedge config command

```sh title="tedge config"
tedge-config 
Configure Thin Edge

USAGE:
    tedge config <SUBCOMMAND>

OPTIONS:
    -h, --help    Print help information

SUBCOMMANDS:
    get      Get the value of the provided configuration key
    help     Print this message or the help of the given subcommand(s)
    list     Print the configuration keys and their values
    set      Set or update the provided configuration key with the given value
    unset    Unset the provided configuration key
```

## Get

```sh title="tedge config get"
tedge-config-get 
Get the value of the provided configuration key

USAGE:
    tedge config get <KEY>

ARGS:
    <KEY>    Configuration key. Run `tedge config list --doc` for available keys

OPTIONS:
    -h, --help    Print help information
```

## Set

```sh title="tedge config set"
tedge-config-set 
Set or update the provided configuration key with the given value

USAGE:
    tedge config set <KEY> <VALUE>

ARGS:
    <KEY>      Configuration key. Run `tedge config list --doc` for available keys
    <VALUE>    Configuration value

OPTIONS:
    -h, --help    Print help information
```

## List

```sh title="tedge config list"
tedge-config-list 
Print the configuration keys and their values

USAGE:
    tedge config list [OPTIONS]

OPTIONS:
        --all     Prints all the configuration keys, even those without a configured value
        --doc     Prints all keys and descriptions with example values
    -h, --help    Print help information
```

## Unset

```sh title="tedge config unset"
tedge-config-unset 
Unset the provided configuration key

USAGE:
    tedge config unset <KEY>

ARGS:
    <KEY>    Configuration key. Run `tedge config list --doc` for available keys

OPTIONS:
    -h, --help    Print help information
```
