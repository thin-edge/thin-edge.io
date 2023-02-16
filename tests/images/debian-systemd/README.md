# debian-systemd container

This folder contains the container definition which is used within the integration tests (when using the `docker` device test adapter). The image provides a `systemd` enabled multi-process container so that it can simulate a real device as much as possible.

To enable `systemd` inside the container, the container needs to be launched with elevated privileges, something that is not so "normal" in the container world. For this reason, it is recommended that this image only be used for development and testing purposes and to run on a machine that is not publicly accessible (or running in a sufficiently sand-boxed environment).


## Creating a test device

The image can be easily built by using the following steps

1. Open a terminal and browse to the project root folder, then change directory to `tests/images/debian-systemd`

    ```sh
    cd tests/images/debian-systemd
    ```

2. Create a `.env` file (in the `tests/images/debian-systemd` directory) with the following contents

    ```
    touch .env
    ```

    **Contents**

    ```sh
    DEVICE_ID=my-tedge-01
    C8Y_BASEURL=mytenant.cumulocity.com
    C8Y_USER=myusername@something.com
    C8Y_PASSWORD="mypassword"
    ```

3. Start the container using docker compose

    ```sh
    docker compose up -d --build
    ```

    **Note**

    This example uses the newer docker compose plugin (v2) and not the older python `docker-compose` variant.

4. Bootstrap the device (there is a special `bootstrap.sh` script inside the image which looks after most things for you)

    ```sh
    docker compose exec -it tedge ./bootstrap.sh
    ```

    This will install the latest official thin-edge.io version, and connect your device to Cumulocity IoT (using the credentials provided in the .env file)

    **Note**

    If you would like to use a randomly generated device id, then you can use:

    ```sh
    docker compose exec -it tedge ./bootstrap.sh --use-random-id
    ```

5. If you want to open a shell inside the container run:

    ```
    docker compose exec -it tedge bash
    ```

    Then check if the `tedge-mapper-c8y` is working properly

    ```
    systemctl status tedge-mapper-c8y
    ```

## Stopping the test device

The test device can be stopped using:

```sh
docker compose down
```

If you would also like to remove the volumes (where the device certificate is persisted), then use:

```sh
docker compose down --volumes
```

