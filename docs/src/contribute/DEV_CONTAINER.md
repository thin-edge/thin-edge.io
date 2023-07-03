---
title: VSCode Dev Containers
tags: [Contribute]
sidebar_position: 2
---

# VSCode Dev Containers

VSCode Dev Containers are great for developing as they contain all of the project's dependencies and in the end you have a ready-to-use dockerFile which can also be used in your CI/CD pipeline.

It provides a normalized experience to all VSCode users across MacOS, Windows and Linux. All you need to get going is a the open source docker engine (not Docker Desktop!) and docker-compose.

More information about Dev Containers can be found from the [official documentation](https://code.visualstudio.com/docs/devcontainers/containers).

## Pre-requisites

Before VSCode Dev container can be used, you need to install a few things.

1. Install `docker` and `docker-compose` if you do not already have these

    Checkout the [INSTALLING_DOCKER](./INSTALLING_DOCKER.md) instructions to help guide you

1. Install VSCode and make sure it is up to date

2. Install the [Remote Development extension](https://marketplace.visualstudio.com/items?itemName=ms-vscode-remote.remote-containers)

    ```
    ms-vscode-remote.vscode-remote-extensionpack
    ```

## Getting started with Dev Containers

Once the pre-requisites have been installed, you are ready to start using the dev container. You only need to run this instructions once. If you have already setup your dev container and just want to know how to re-open it, please read the next section.

There are two main methods when setting up the dev container, we recommend using the `Method 1`. `Method 1` uses the `Clone in Named Container Volume` strategy as  it is more performant on most Operating Systems. For more information about it please refer to the following [Dev Container documentation](https://code.visualstudio.com/remote/advancedcontainers/improve-performance). The only downside to this method is that the code will be stored in a docker volume. This means you can not directly navigate to the project from the host system's file explorer, however you can still `download` files from it from VSCode file explorer.

But in both methods it is recommended to fork the project first, then clone from your fork. You can fork from the [GitHub](https://github.com/thin-edge/thin-edge.io) website, and then copy the git url from your fork.

### Method 1: Cloning in named container volume (Recommended)

1. In VSCode, open the command pallet (`Cmd + Shift + P` (MacOS) or `Ctrl + Shift + P` (Windows))

2. Enter `Clone Repository In Named Container Volume`

    ```
    Dev Containers: Clone Repository In Named Container Volume
    ```

    Enter your forks git url, for example the https style url will look something like this:

    ```
    https://github.com/<your_github_username>/thin-edge.io.git
    ```

    Then follow the remaining prompts (usually you can just accept the default values).

3. Now you can sit back and wait for the dev container to be built. VSCode will set everything up for you (including installing the project specific extensions automatically). Don't worry it just takes a while to build the first time around. Reopening the container again later on is quick

### Method 2: Cloning to the host filesystem

1. Open a terminal

2. Clone the forked thin-edge.io project

    **Example**

    ```sh
    git clone https://github.com/<your_github_username>/thin-edge.io.git
    ```

3. Change directory into the project folder

    ```sh
    cd thin-edge.io
    ```

4. Open the project folder inside VSCode

    Easiest way is to use the `code` helper which is hopefully installed already by VSCode. If it isn't then you can install it from VSCode via the command pallet under `Shell Command: Install 'code' command in PATH`.

    ```sh
    code .
    ```

    Alternatively you can open VSCode, then select `File > Open Folder...` from the menu, and select where the project folder was cloned to.

5. VSCode should ask you if you want to re-open the project in the Dev Container (in the bottom right-hand corner).

6. Now you can sit back and wait for the dev container to be built. VSCode will set everything up for you (including installing the project specific extensions automatically). Don't worry it just takes a while to build the first time around. Reopening the container again later on is quick

## Re-opening an already setup dev container

1. Open VSCode

2. Click on the `Remote Explorer` Icon on the left hand navigation menu (the icon looks like a computer monitor)

3. From the `Dev Containers` section, right-click on the thin-edge.io dev container, and select `Open Folder in Container`


## Rebuilding the dev container

Rebuilding the dev container is sometimes required/useful. Some common reason to rebuild it are:

* Some dependencies were added/changed in the `.devcontainer` folder and you would like to use the changes
* You want a fresh dev container as you have modified some of the container's OS system and you suspect you broke something

But don't worry, rebuilding is easy, you just need to follow these short instructions:

1. In VSCode, open the command pallet (`Cmd + Shift + P` (MacOS) or `Ctrl + Shift + P` (Windows))

2. Enter the following command

    ```
    Dev Containers: Rebuild Container
    ```
