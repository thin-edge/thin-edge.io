---
title: "tedge http"
tags: [Reference, CLI]
sidebar_position: 11
---

# The tedge http command

A `tedge` sub command to interact with the HTTP services hosted on the device by the Cumulocity mapper and the agent:

- the [Cumulocity Proxy](../../cumulocity-proxy/)
- the [File Transfer Service](../../file-transfer-service/)
- the [Entity Store Service](../../../operate/entity-management/).

This command uses `tedge config` to get the appropriate host, port and credentials to reach these local HTTP services.
So the same command can be used unchanged from the main device or a child device, with TLS or mTLS enabled or not.

```text command="tedge http --help" title="tedge http"
Send HTTP requests to local thin-edge HTTP servers

Usage: tedge http [OPTIONS] <COMMAND>

Commands:
  get     GET content from thin-edge local HTTP servers
  post    POST content to thin-edge local HTTP servers
  put     PUT content to thin-edge local HTTP servers
  patch   PATCH content to thin-edge local HTTP servers
  delete  DELETE resource from thin-edge local HTTP servers
  help    Print this message or the help of the given subcommand(s)

Options:
      --config-dir <CONFIG_DIR>  [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
      --debug                    Turn-on the DEBUG log level
      --log-level <LOG_LEVEL>    Configures the logging level
  -h, --help                     Print help (see more with '--help')
```

## %%te%% HTTP services {#http-services}


`tedge http` forwards requests to the appropriate %%te%% HTTP service using the URL prefixes.

The requests are forwarded to the appropriate service depending on the URL prefix.

- URIs prefixed by `/c8y/` are forwarded to the [Cumulocity Proxy](../../cumulocity-proxy/)

   ```sh title="Interacting with Cumulocity"
   tedge http get /c8y/inventory/managedObjects
   ```

- URIs starting with `/te/v1/files/` are directed to the [File Transfer Service](../../file-transfer-service)

   ```sh title="Transferring files to/from the main device"
   tedge http put /te/v1/files/target.txt --file source.txt
   ```
  
- URIs starting with `/te/v1/entities` are directed to the [Entity Store Service](../../../operate/entity-management/)

   ```sh title="Listing all entities"
   tedge http get /te/v1/entities
   ```


## Configuration

For `tedge http` to be used from the main device or any client device, with TLS or mTLS enabled or not,
the host, port and credentials of the local %%te%% HTTP services
have to be properly configured on the main device as well as the child devices.

### On the host running the main agent

The following `tedge config` settings control the access granted to child devices
on the HTTP services provided by the main agent
([file transfer](../../file-transfer-service) and entity registration).
This can be done along three security levels.

```sh title="Listening HTTP requests"
            http.bind.port  The port number of the File Transfer Service HTTP server binds to for internal use. 
                            Example: 8000
         http.bind.address  The address of the File Transfer Service HTTP server binds to for internal use. 
                            Examples: 127.0.0.1, 192.168.1.2, 0.0.0.0
```

```sh title="Enabling TLS aka HTTPS"
            http.cert_path  The file that will be used as the server certificate for the File Transfer Service. 
                            Example: /etc/tedge/device-certs/file_transfer_certificate.pem
             http.key_path  The file that will be used as the server private key for the File Transfer Service. 
                            Example: /etc/tedge/device-certs/file_transfer_key.pem
```

```sh title="Enforcing mTLS"
              http.ca_path  Path to a directory containing the PEM encoded CA certificates that are trusted when checking incoming client certificates for the File Transfer Service. 
                            Example: /etc/ssl/certs
```

### On the host running the cumulocity mapper

The following `tedge config` settings control the access granted to child devices
on the HTTP services provided by the Cumulocity mapper ([Cumulocity proxy](../../cumulocity-proxy/)).
This can be done along three security levels.

```sh title="Listening HTTP requests"
    c8y.proxy.bind.address  The IP address local Cumulocity HTTP proxy binds to. 
                            Example: 127.0.0.1
       c8y.proxy.bind.port  The port local Cumulocity HTTP proxy binds to. 
                            Example: 8001
```

```sh title="Enabling TLS aka HTTPS"
       c8y.proxy.cert_path  The file that will be used as the server certificate for the Cumulocity proxy. 
                            Example: /etc/tedge/device-certs/c8y_proxy_certificate.pem
        c8y.proxy.key_path  The file that will be used as the server private key for the Cumulocity proxy. 
                            Example: /etc/tedge/device-certs/c8y_proxy_key.pem
```

```sh title="Enforcing mTLS"
         c8y.proxy.ca_path  Path to a file containing the PEM encoded CA certificates that are trusted when checking incoming client certificates for the Cumulocity Proxy. 
                            Example: /etc/ssl/certs
```

### On all client hosts

The following `tedge config` settings control how client devices access the local HTTP services.
This has to be done in consistent way with the main agent and Cumulocity mapper settings.

```sh title="Reaching local HTTP services"
          http.client.port  The port number on the remote host on which the File Transfer Service HTTP server is running. 
                            Example: 8000
          http.client.host  The address of the host on which the File Transfer Service HTTP server is running. 
                            Examples: 127.0.0.1, 192.168.1.2, tedge-hostname
     c8y.proxy.client.host  The address of the host on which the local Cumulocity HTTP Proxy is running, used by the Cumulocity mapper. 
                            Examples: 127.0.0.1, 192.168.1.2, tedge-hostname
     c8y.proxy.client.port  The port number on the remote host on which the local Cumulocity HTTP Proxy is running, used by the Cumulocity mapper. 
                            Example: 8001
```

```sh title="Using TLS aka HTTPS"
              http.ca_path  Path to a directory containing the PEM encoded CA certificates that are trusted when checking incoming client certificates for the File Transfer Service. 
                            Example: /etc/ssl/certs
         c8y.proxy.ca_path  Path to a file containing the PEM encoded CA certificates that are trusted when checking incoming client certificates for the Cumulocity Proxy. 
                            Example: /etc/ssl/certs
```

```sh title="Using mTLS"
http.client.auth.cert_file  Path to the certificate which is used by the agent when connecting to external services. 
 http.client.auth.key_file  Path to the private key which is used by the agent when connecting to external services. 
```

## tedge http get

```text command="tedge http get --help" title="tedge http get"
GET content from thin-edge local HTTP servers

Usage: tedge http get [OPTIONS] <URI>

Arguments:
  <URI>  Source URI

Options:
      --accept-type <ACCEPT_TYPE>  MIME type of the expected content
      --config-dir <CONFIG_DIR>    [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
      --debug                      Turn-on the DEBUG log level
      --profile <PROFILE>          Optional c8y cloud profile
      --log-level <LOG_LEVEL>      Configures the logging level
  -h, --help                       Print help (see more with '--help')
```

## tedge http post

```text command="tedge http post --help" title="tedge http post"
POST content to thin-edge local HTTP servers

Usage: tedge http post [OPTIONS] <content|--data <DATA>|--file <FILE>> <URI>

Arguments:
  <URI>      Target URI
  [content]  Content to send

Options:
      --config-dir <CONFIG_DIR>      [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
      --data <DATA>                  Content to send
      --debug                        Turn-on the DEBUG log level
      --file <FILE>                  File which content is sent
      --content-type <CONTENT_TYPE>  MIME type of the content
      --log-level <LOG_LEVEL>        Configures the logging level
      --accept-type <ACCEPT_TYPE>    MIME type of the expected content
      --profile <PROFILE>            Optional c8y cloud profile
  -h, --help                         Print help (see more with '--help')
```

## tedge http put

```text command="tedge http put --help" title="tedge http put"
PUT content to thin-edge local HTTP servers

Usage: tedge http put [OPTIONS] <content|--data <DATA>|--file <FILE>> <URI>

Arguments:
  <URI>      Target URI
  [content]  Content to send

Options:
      --config-dir <CONFIG_DIR>      [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
      --data <DATA>                  Content to send
      --debug                        Turn-on the DEBUG log level
      --file <FILE>                  File which content is sent
      --content-type <CONTENT_TYPE>  MIME type of the content
      --log-level <LOG_LEVEL>        Configures the logging level
      --accept-type <ACCEPT_TYPE>    MIME type of the expected content
      --profile <PROFILE>            Optional c8y cloud profile
  -h, --help                         Print help (see more with '--help')
```

## tedge http patch

```text command="tedge http patch --help" title="tedge http patch"
PATCH content to thin-edge local HTTP servers

Usage: tedge http patch [OPTIONS] <content|--data <DATA>|--file <FILE>> <URI>

Arguments:
  <URI>      Target URI
  [content]  Content to send

Options:
      --config-dir <CONFIG_DIR>      [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
      --data <DATA>                  Content to send
      --debug                        Turn-on the DEBUG log level
      --file <FILE>                  File which content is sent
      --content-type <CONTENT_TYPE>  MIME type of the content
      --log-level <LOG_LEVEL>        Configures the logging level
      --accept-type <ACCEPT_TYPE>    MIME type of the expected content
      --profile <PROFILE>            Optional c8y cloud profile
  -h, --help                         Print help (see more with '--help')
```

## tedge http delete

```text command="tedge http delete --help" title="tedge http delete"
DELETE resource from thin-edge local HTTP servers

Usage: tedge http delete [OPTIONS] <URI>

Arguments:
  <URI>  Source URI

Options:
      --config-dir <CONFIG_DIR>  [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
      --profile <PROFILE>        Optional c8y cloud profile
      --debug                    Turn-on the DEBUG log level
      --log-level <LOG_LEVEL>    Configures the logging level
  -h, --help                     Print help (see more with '--help')
```
