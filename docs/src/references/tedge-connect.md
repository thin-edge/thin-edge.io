# The `tedge connect` command

```
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

## Azure

```
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

```
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
