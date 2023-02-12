# Integration testing

## Overview

The testing framework is written to support running on multiple targets either using docker container or using an SSH enabled device. The framework offers a simple way to setup a test using the desired adapter (e.g. `docker` or `ssh`). These adapters is called a `device adapter`. They are written to be device agnostic so they allow for maximum flexibility to write your tests to adapter to your needs.

The default device adapter is `docker`, and the primary support will be for `docker` at first, and the agnostic testing interface will be extended gradually. The `docker` device adapter was chosen as the primary adapter as for the following reasons:

* No hardware dependencies (you can spawn a container pretty much anywhere even in CI/CD)
* Supports complex testing of network interruptions as you can disconnect the device from the network but still control it via the docker API
* Have a fresh container each time (reduce side-effects from unrelated tests)

There are obvious drawbacks to the points above, however once the other adapters (`ssh`) are ready it will also supporting running 90% of the same tests on a different target with zero code changes.

## Device adapters

### Docker adapter

The docker adapter will spawn a new container on demand which can be used to run the tests on. The container provides a simple/clean device to run all of the tests, and can be destroyed afterwards. This makes it very convenient to run your tests as it does not require an external hardware.

The general test suite flow is as follows:

1. Create a device
2. Bootstrap the device (register it with the cloud)
3. Run tests (including assertions)
4. Collect device diagnostics (logs, cloud digital twin etc. for post analysis)
5. Cleanup device artifacts (e.g. delete uploaded certificate, cloud digital twin etc.)
6. Destroy the device

### SSH adapter

The ssh adapter uses an existing device and uses a SSH connect to run the test suite against it. In this setup, you are responsible for providing a device, container, server before the test can start.

The general test suit flow is very similar to the above [Docker adapter](./SETUP.md#docker-adapter) flow, however the device creation and destroy steps are skipped.

The core thin-edge.io team uses some physical devices setup in a test lab to facilitate testing on real hardware. These devices are not available for public use, however make up part of the automated and exploratory testing.

The list of test hardware devices can be found [here](./TEST_DEVICES.md).

# Setup

## Pre-requisites

Before you can run the tests you need to install the pre-requisites:

* docker
* python3 (>=3.10)
* pip3

It is assumed that you are running on either MacOS or Linux. If you are a Windows users then use WSL 2 and follow the **Debian/Ubuntu** instructions, or just use the dev container option (which requires docker which again can be run under WSL 2).

### Option 1: Installing the dependencies yourself

1. Install python3 (>= 3.8)
    
    Follow the [python instructions](https://www.python.org/downloads/), or

    **MacOS (using homebrew)**

    ```sh
    brew install python@3.10
    ```

    **Debian/Ubuntu**

    ```sh
    sudo apt-get install python3 python3-pip
    ```

3. Install docker and docker-compose using [this guide](../../docs/src/developer/INSTALLING_DOCKER.md)

### Option 2: Using the project's dev container

Checkout the [dev container instructions](../../docs/src/developer/DEV_CONTAINER.md) for more details.

## Running the tests

1. Navigate to the Robot Framework folder

    ```sh
    cd tests/RobotFramework
    ```

2. Run the setup script which will create the python virtual environment and install the dependencies. This only needs to be run once.

    ```sh
    ./bin/setup.sh
    ```

	Or if you only want to install the dependencies for a specific adapter than a list of adapter can be provided.

	```sh
	# only local adapter
	./bin/setup.sh "local"

	# multiple adapters
	./bin/setup.sh "local" "ssh"
	```

3. Follow the console instructions and edit the `.env` file which was created by the `./bin/setup.sh` script

4. Switch to the new python interpreter (the one with `.venv` in the name)

    **Note: VSCode users**
    
    Open the `tasks.py` file, then select the python interpreter in the bottom right hand corner. Then enter the following location of python:

    ```sh
    tests/RobotFramework/.venv/bin/python3
    ```

    If you are not using a devcontainer then add the following to your workspace settings `.vscode/settings.json` file.

    ```json
    {
        "python.defaultInterpreterPath": "${workspaceFolder}/tests/RobotFramework/.venv/bin/python3",
        "robot.language-server.python": "${workspaceFolder}/tests/RobotFramework/.venv/bin/python3",
        "robot.python.executable": "${workspaceFolder}/tests/RobotFramework/.venv/bin/python3",
        "python.envFile": "${workspaceFolder}/.env"
    }
    ```

    Afterwards it is worthwhile reloading some of the VSCode extension via the Command Pallet

    * `Python: Restart Language Server`
    * `Robot Framework: Clear caches and restart Robot Framework`

5. On the console, activate the environment (if it is not already activated)

    ```sh
    source .venv/bin/activate
    ```

6. Run the tests

    ```sh
    invoke test
    ```

    Or you can run robot directly

    ```sh
    robot --outputdir output ./tests
    ```

### Using custom built binaries for the tests

If you would like to run the tests using some custom built tedge packages, then run the following steps:

1. Open the terminal and navigate to the project root folder (not the RobotFramework root folder)

2. Create a symlink to the folder containing the built debian packages (this assumes you have already built tedge components)

    ```sh
    ln -s "$(pwd)/target/debian" "$(pwd)tests/images/deb/custom"
    ```

    You don't have to create a symlink if you don't want to, you can also just place the `*.deb` packages into the following folder.

    ```sh
    tests/images/deb/
    ```

3. Rebuild the docker image

    ```sh
    cd tests/RobotFramework
    source .venv/bin/activate
    invoke build
    ```

4. Run the tests

    ```sh
    invoke test
    ```

    Or if you are using VSCode you can navigate to a `*.robot` file and run an individual test case/suite in the text editor.


**What is happening?**

When building the docker image it will automatically add all the files/folders under the `tests/images/deb` path to the built image. These files are then used by the test bootstrapping script (`bootstrap.sh`) which is also part of the docker image. The bootstrapping process is smart enough to detect if there are debian packages there, and if will install the tedge related packages. If no packages are found then it will revert to installing the latest official tedge version via the `get-thin-edge_io.sh` script.

## Viewing the test reports and logs

The reports and logs are best viewed using a web browser. This can be easily done setting up a quick local web server using the following instructions.

1. Change to the robot framework directory (if you have not already done so)

    ```sh
    cd tests/RobotFramework
    ```

2. Open a console from the root folder of the project, then execute

    ```sh
    python -m http.server 9000 --directory output
    ```

    Or using the task

    ```sh
    invoke reports
    ```

3. Then open up [http://localhost:9000/tests/RobotFramework/output/log.html](http://localhost:9000/tests/RobotFramework/output/log.html) in your browser
