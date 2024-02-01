---
title: Build
tags: [Contribute, Build]
description: Build %%te%% from the source code
---

This page details how to build %%te%% from the source code.

## Requirements

You can use any OS to build from source (below has been tested on Ubuntu, but we also use Debian, macOS, and FreeBSD successfully).

Our recommended setup and required tools are:

* Ubuntu 20.04 or Debian 12 (Bookworm)
* git
* Rust toolchain

Following packages are required:

* build-essentials
* curl
* gcc

A list of our test platforms can be found [here](../references/supported-platforms.md).

## Get the code

%%te%% code is in git repository on github to acquire the code use following command:

* via SSH:

```sh
git clone git@github.com:thin-edge/thin-edge.io.git
```

* or via HTTPS:

```sh
git clone https://github.com/thin-edge/thin-edge.io.git
```

## Installing toolchain

### Rust toolchain

To install Rust follow [Official installation guide](https://www.rust-lang.org/tools/install).
To get started you need Cargo's bin directory (`$HOME/.cargo/bin`) in your `PATH` environment variable.

```sh
export PATH=$PATH:$HOME/.cargo/bin
```

And then you can run `rustc` to view current version:

```sh
rustc --version
```

```text title="Output"
rustc 1.65.0 (897e37553 2022-11-02)
```

:::note
Above command will add rust to path only for existing session,
after you restart the session you will have to add it again,
to add rust to the path permanently it will depend on your shell but for Bash,
you simply need to add the line from above, `export PATH=$PATH:$HOME/.cargo/bin` to your `~/.bashrc`.

For other shells, you'll want to find the appropriate place to set a configuration at start time,
eg. zsh uses `~/.zshrc`. Check your shell's documentation to find what file it uses.
:::

%%te%% operates the `MSRV` (Minimum Supported Rust Version) and uses stable toolchain.

Current MSRV is `1.65`.

### Cross compilation toolchain (optional)

%%te%% can be compiled for target architecture on non-target device, this is called cross compilation.
Currently we support `Raspberry Pi 3B` for `armv7` architecture with Rust's cross compilation toolchain called [cargo cross](https://github.com/rust-embedded/cross).

To install [cargo cross](https://github.com/rust-embedded/cross):

```sh
cargo install cross
```

### Linux packaging (optional)

We use [nfpm](https://github.com/goreleaser/nfpm) to build our linux packages (deb, rpm and apk).

Follow the [nfpm install instructions](https://nfpm.goreleaser.com/install/) to install the dependency. The linux packages will automatically be built when running `just release`.


```sh
just release
```

The virtual packages (e.g. `tedge-full` and `tedge-minimal`) can be built using the following command:

```sh
just release-linux-virtual
```

## Compiling

To build %%te%% we are using `cargo`.

As we are using  `cargo workspace` for all our crates. All compiled files are put in `./target/` directory with target's name eg: `./target/debug` or `./target/release` for native builds and for cross compiled targets `./target/<architecture>/debug` or `./target/<architecture>/release` dependent on the target of the build.

### Compiling dev

To compile dev profile (with debug symbols) we use following command:

```sh
cargo build
```

Build artifacts can be found in `./target/debug` and will include executables:

```sh
ls -l ./target/debug/tedge*
```

```text title="Output"
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge-mapper
```

Binaries can be run eg: `./target/debug/tedge`.
Alternatively, you can use `cargo` to build and run executable in a single command:

```sh
cargo run --bin tedge
```

### Compiling release

To compile release profile we use following command:

```sh
cargo build --release
```

Build artifacts can be found in `./target/release` and will include executables:

```sh
ls -l ./target/release/tedge*
```

```text title="Output"
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge-mapper
```

Binaries can be run eg: `./target/release/tedge`.

## Building deb package

Currently %%te%% contains 2 binaries, `tedge` (cli) and `tedge-mapper` which are packaged as separate debian packages. To create following commands are to be issued:

```sh
cargo deb -p tedge
cargo deb -p tedge-mapper
```

All resulting packages are going to be in: `./target/debian/` directory:

```sh
ls -l ./target/debian
```

```text title="Output"
total 2948
-rw-rw-r-- 1 user user 11111 Jan 1 00:00 tedge_0.9.0_amd64.deb
-rw-rw-r-- 1 user user 11111 Jan 1 00:00 tedge-mapper_0.9.0_amd64.deb
```

## Cross compiling

To create binaries which can run on different platform than one you are currently on you can use [cargo cross](https://github.com/rust-embedded/cross):

```sh
cross build --target armv7-unknown-linux-gnueabihf
```

Build artifacts can be found in `./target/armv7-unknown-linux-gnueabihf/debug` and will include executables:

```sh
ls -l ./target/armv7-unknown-linux-gnueabihf/debug/tedge*
```

```text title="Output"
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge-mapper
```

To cross compile release version of the binaries just add `--release` to the above command like so:

```sh
cross build --target armv7-unknown-linux-gnueabihf --release
```

## Running tests

When contributing to %%te%% we ask you to write tests for the code you have written. The tests will be run by build pipeline when you create pull request, but you can easily run all the tests when you are developing with following command:

```sh
cargo test
```

This will run all tests from the repository and sometime may take long time, `cargo` allows you to run specific test or set of tests for binary:

```sh
cargo test --bin tedge
```
