# tedge-p11-server

A thin-edge server that allows thin-edge service possibly working in a container to access host's cryptographic tokens.

## Building and using

1. Compile on the host
    ```sh
    $ cargo build --bin tedge-p11-server
    ```

2. Run on the host, passing necessary configuration via CLI or tedge config
    ```sh
    target/debug/tedge-p11-server ./my-socket.sock --module-path /usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so --pin 821370
    ```

3. In the container, set `device.cryptoki.socket_path`
    ```sh
    tedge config set device.cryptoki.socket_path /path/to/my-socket.sock
    ```

4. Connect to c8y
    ```sh
    tedge reconnect c8y
    ```
