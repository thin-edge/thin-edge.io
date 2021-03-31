# The `tedge config` command
```
tedge-config 0.1.0
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

## get

```
tedge-config-get 0.1.0
Get the value of the provided configuration key

USAGE:
    tedge config get <key>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <key>    [ device.id device.key.path device.cert.path c8y.url c8y.root.cert.path azure.url azure.root.cert.path
             ]
```

## set 

```
tedge-config-set 0.1.0
Set or update the provided configuration key with the given value

USAGE:
    tedge config set <key> <value>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <key>      [ device.key.path device.cert.path c8y.url c8y.root.cert.path azure.url azure.root.cert.path ]
    <value>    Configuration value
```

## list 

```
tedge-config-list 0.1.0
Print the configuration keys and their values

USAGE:
    tedge config list [FLAGS]

FLAGS:
    -h, --help       Prints help information
        --all        Prints all the configuration keys, even those without a configured value
        --doc        Prints all keys and descriptions with example values
    -V, --version    Prints version information
```

## unset

```
tedge-config-unset 0.1.0
Unset the provided configuration key

USAGE:
    tedge config unset <key>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

ARGS:
    <key>    [ device.key.path device.cert.path c8y.url c8y.root.cert.path azure.url azure.root.cert.path ]

```
