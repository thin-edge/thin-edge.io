---
title: Build
tags: [Contribute, Build]
description: Build %%te%% from the source code
---

This page details how to build %%te%% from the source code.

## Requirements

Whilst you should be able to build %%te%% on different Operating Systems and CPU architectures, it is best to stick with one of the following setups if you would like support when something goes wrong.

* One of the following Operating Systems
    * Linux (we recommend using either Debian or Ubuntu)
    * WSL 2 (Windows Subsystem for Linux)
    * macOS
* git
* Rust toolchain, the Minimum Supported Rust Version (MSRV) is 1.78
* [just](https://github.com/casey/just)

A list of our test platforms can be found [here](../references/supported-platforms.md).

### Initial setup

The instructions below walk you through the process of installing the required tools and checking out the project for the first time.

1. Install the dependencies

    ```sh tab={"label":"macOS"}
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    cargo install just
    ```

    ```sh tab={"label":"Linux"}
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    cargo install just
    ```

    ```sh tab={"label":"WSL2"}
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    cargo install just
    ```

    :::note
    
    * If you have any problems installing Rust then consult the [official Rust installation guide](https://www.rust-lang.org/tools/install).
    * [just](https://just.systems/) is also written in Rust, so it can also be installed directly using Rust's package manager, Cargo.
    :::


2. Checkout the project

    :::tip
    If you plan on contributing to the project, then please [fork the project](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/fork-a-repo) first, then clone your fork instead of the main project.
    :::

    The %%te%% source is hosted in a git repository by GitHub, so you can get the project by using the one of the following commands:

    ```sh tab={"label":"HTTPS"}
    git clone https://github.com/thin-edge/thin-edge.io.git
    cd thin-edge.io
    ```

    ```sh tab={"label":"SSH"}
    git clone git@github.com:thin-edge/thin-edge.io.git
    cd thin-edge.io
    ```

    ```sh tab={"label":"GitHub CLI"}
    gh repo clone thin-edge/thin-edge.io
    cd thin-edge.io
    ```

    

## Building

### Build Packages

By default, if no target is provided, then the target architecture will be automatically detected, and the linux variant will be chosen.

```sh
just release
```

```text title="Output"
target/aarch64-unknown-linux-gnu/packages
```

:::note
For macOS users, `just release` will use chose the default target based on your machine's CPU architecture. The table below shows the default targets based on the type of CPU architecture of your machine.

|Processor|Default Target|
|---|--------------|
|Apple Silicon|aarch64-unknown-linux-musl|
|Intel (x86_64) processor|x86_64-unknown-linux-musl|
:::

You can build for other targets (e.g. cross compiling), by simply providing the Rust target as an additional argument:

```sh
# Intel/AMD (64 bit)
just release x86_64-unknown-linux-musl

# Intel/AMD (32 bit)
just release i686-unknown-linux-musl

# arm64 (64 bit), e.g. Raspberry Pi 3, 4, 5
just release aarch64-unknown-linux-musl

# armv7 (32 bit), e.g. Raspberry Pi 2, 3
just release armv7-unknown-linux-musleabihf

# armv6 (32 bit, hard-float), e.g. Raspberry Pi 1
just release arm-unknown-linux-musleabihf
```

:::tip
By default, [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) is used for cross compilation as it has minimal dependencies and works on different host machines (e.g.  aarch64 or x86_64) without requiring docker. You are free to use other cross compilation tools, however we might not be able to give advice if it does not work.

If you're looking to build other targets, then have a look at our [build-workflow](https://github.com/thin-edge/thin-edge.io/blob/main/.github/workflows/build-workflow.yml) as it provides another way to build the project but it requires a host machine with an x86_64 CPU.
:::

The `release` task will also build the linux packages, e.g. dep, rpm, apk and plain tarballs. Under the hood, we use [nfpm](https://github.com/goreleaser/nfpm) for the packaging. The task will attempt to installing it for you, however if that fails, you can manually install it by following the [nfpm install instructions](https://nfpm.goreleaser.com/install/).

### Build Linux virtual packages

%%te%% is composed of multiple packages (e.g. tedge, tedge-agent, tedge-mapper), so installing all of them can be complicated for new users, so to make this easier, we also create two virtual packages which allow you to install different combinations of the packages from a single package name. The virtual packages don't include any code themselves, they just have specific packages listed as dependencies so that the package manager will automatically install all of the dependencies when installing the virtual package.

The Linux virtual packages (e.g. `tedge-full` and `tedge-minimal`) can be built using the following command:

```sh
just release-linux-virtual
```

```text title="Output"
-----------------------------------------------------
thin-edge.io packager: build_virtual
-----------------------------------------------------
Parameters

  packages: 
  version: 
  types: deb,apk,rpm,tarball
  output_dir: target/virtual-packages

Cleaning output directory: target/virtual-packages
using deb packager...
created package: target/virtual-packages/tedge-full_1.4.3~230+gcfaf55d_all.deb
using rpm packager...
created package: target/virtual-packages/tedge-full-1.4.3~230+gcfaf55d-1.noarch.rpm
using apk packager...
created package: target/virtual-packages/tedge-full_1.4.3_rc230+gcfaf55d_pr0_noarch.apk
using deb packager...
created package: target/virtual-packages/tedge-minimal_1.4.3~230+gcfaf55d_all.deb
using rpm packager...
created package: target/virtual-packages/tedge-minimal-1.4.3~230+gcfaf55d-1.noarch.rpm
using apk packager...
created package: target/virtual-packages/tedge-minimal_1.4.3_rc230+gcfaf55d_pr0_noarch.apk

Successfully created packages
```

## Development

### Build (debug)

To build a non-optimized binary with debugging information, use the following command:

```sh
cargo build
```

Build artifacts can be found in `./target/debug` and will include the executable:

```sh
ls -l ./target/debug/tedge
```

```text title="Output"
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
```

:::note
The `tedge` is a multi-call binaries, which means that the single binary includes the core components of %%te%%, e.g. tedge, tedge-agent, tedge-mapper etc.

The easiest way to run a specific component manually is to use the `tedge run <component>` command, for example:

```sh
tedge run tedge-agent
```

You can run the same component by creating a symlink called `tedge-agent` which links to the `tedge` binary, and call the symlink instead.
:::

Alternatively, you can use `cargo` to build and run executable in a single command:

```sh
cargo run
```

If you need to pass arguments to the %%te%% component, then you use the `--` syntax, and everything afterwards will be passed to the binary being run and not to the `cargo run` command.

```sh
cargo run -- mqtt sub '#'
```

### Running tests

When contributing to %%te%%, we ask you to write tests for the code you have written. The tests will be run by the build pipeline when you create a pull request, but you can easily run all the tests whilst you are developing with following command:

```sh
just test
```

This will run all tests from the repository and may take some time to complete. Alternatively, you can run a specific test or set of tests for a given binary:

```sh
just test --bin tedge
```
