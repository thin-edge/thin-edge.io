# The `tedge config` command

```
tedge-config 0.2.0
Configure Thin Edge

USAGE:
    tedge config <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    get      Get the value of the provided configuration key
    help     Prints this message or the help of the given subcommand(s)
    list     Print the configuration keys and their values
    set      Set or update the provided configuration key with the given value
    unset    Unset the provided configuration key
```

## Get

```
tedge-config-get 0.2.0
Get the value of the provided configuration key

USAGE:
    tedge config get <key>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <key>    Configuration key. Run `tedge config list --doc` for available keys
```

## Set

```
tedge-config-set 0.2.0
Set or update the provided configuration key with the given value

USAGE:
    tedge config set <key> <value>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <key>      Configuration key. Run `tedge config list --doc` for available keys
    <value>    Configuration value
```

## List

```
tedge-config-list 0.2.0
Print the configuration keys and their values

USAGE:
    tedge config list [FLAGS]

FLAGS:
    -h, --help       Prints help information
        --all        Prints all the configuration keys, even those without a configured value
        --doc        Prints all keys and descriptions with example values
    -V, --version    Prints version information
```

## Unset

```
tedge-config-unset 0.2.0
Unset the provided configuration key

USAGE:
    tedge config unset <key>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <key>    Configuration key. Run `tedge config list --doc` for available keys
```
