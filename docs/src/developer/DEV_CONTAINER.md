# VSCode Dev Containers

VSCode Dev Containers are a great way to develop as they can contain all of the project's dependencies and in the end you have a read-to-use dockerFile which can also be used in your CI/CD pipeline.

It provides a normalized experience to all VSCode users across MacOS, Windows and Linux. All you need to get going is a the open source docker engine (not Docker Desktop!) and docker-compose.

More information about Dev Containers can be found from the [official documentation](https://code.visualstudio.com/docs/devcontainers/containers).

## Setting up Docker (OSS - Variant)

### MacOS (amd64/arm64)

There are different ways to install the docker engine on MacOS, however this guide will focus on using [colima](https://github.com/abiosoft/colima) which uses `limactl` under the hood to create a virtual machine and supporting functionality such as automatic port forwarding.

1. Install homebrew. You can also install homebrew without `sudo` rights using this [guide](https://docs.brew.sh/Installation#untar-anywhere-unsupported)

    ✨ **Tip** ✨

    Make sure you read the homebrew instructions which are printed out on the console 😉

2. Install [colima](https://github.com/abiosoft/colima) and the docker cli

    ```sh
    brew install colima
    ```

3. Install the docker cli tools

    ```sh
    brew install docker docker-compose
    ```

    Then configure `docker-compose` as a docker plugin so that you can use `docker compose` instead of the legacy `docker-compose` script.

    ```sh
    mkdir -p ~/.docker/cli-plugins
    ln -sfn $(brew --prefix)/opt/docker-compose/bin/docker-compose ~/.docker/cli-plugins/docker-compose
    ```

4. Configure and start `colima`

    ```sh
    colima start --cpu 4 --memory 16
    ```

    **Note**

    The extra arguments are only needed the first time you start it. You can change the settings to based on what you are need/have.

5. Check that everything is working by starting the `hello-world` container

    ```sh
    docker run -it --rm hello-world
    ```

#### References

* https://smallsharpsoftwaretools.com/tutorials/use-colima-to-run-docker-containers-on-macos/



## Getting started with Dev Containers

Now that you have `docker` and `docker compose` installed you are ready to start using.

1. Install VSCode and make sure it is up to date

2. Install the Remote Development extension

    ```
    ms-vscode-remote.vscode-remote-extensionpack
    ```

3. Currently the project does not have a `.devcontainer` folder so you will have to follow [these instructions](https://github.com/reubenmiller/vscode-dev-containers) first on how to add custom dev container templates for a project without having the files actually in the project. Don't worry this will not be required in the future.

4. In VSCode, open the command pallet (`Cmd + Shift + P` (MacOS) or `Ctrl + Shift + P` (Windows))

5. Enter `Clone Repository In Named Container Volume`

    ```
    Dev Containers: Clone Repository In Named Container Volume
    ```

    Then follow the prompts

6. Now you can sit back and wait for the docker image to build. VSCode will set everything up for you (including installing the project specific extensions automatically)
