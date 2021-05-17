# The `tedge connect` command
```
tedge-connect 0.1.0
Connect to connector provider

USAGE:
    tedge connect <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    az      Create connection to Azure
    c8y     Create connection to Cumulocity
    help    Prints this message or the help of the given subcommand(s)
```

## az

```
tedge-connect-az 0.1.0
Create connection to Azure

The command will create config and start edge relay from the device to az instance

USAGE:
    tedge connect az [FLAGS]

FLAGS:
    -h, --help       
            Prints help information

        --test       
            Test connection to Azure

    -V, --version    
            Prints version information

```

## c8y

```
tedge-connect-c8y 0.1.0
Create connection to Cumulocity

The command will create config and start edge relay from the device to c8y instance

USAGE:
    tedge connect c8y [FLAGS]

FLAGS:
    -h, --help       
            Prints help information

        --test       
            Test connection to Cumulocity

    -V, --version    
            Prints version information

```
