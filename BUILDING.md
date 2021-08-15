# Building thin-edge.io

## Requirements

You can use any OS to build from source (below has been tested on Ubuntu, but we also use Debian, macOS, and FreeBSD successfully). 

Our recommended setup and required tools are:

* Ubuntu 20.04 or Debian 10.9 (Buster)
* git
* Rust toolchain

Following packages are required:

* build-essentials
* curl
* gcc

A list of our test platforms can be found [here](docs/src/supported-platforms.md).

## Get the code

thin-edge.io code is in git repository on github to acquire the code use following command:

* via SSH:

```shell
git clone git@github.com:thin-edge/thin-edge.io.git
```

* or via HTTPS:

```shell
git clone https://github.com/thin-edge/thin-edge.io.git
```

## Installing toolchain

### Rust toolchain

To install Rust follow [Official installation guide](https://www.rust-lang.org/tools/install).
To get started you need Cargo's bin directory (`$HOME/.cargo/bin`) in your `PATH` environment variable.

```shell
export PATH=$PATH:$HOME/.cargo/bin
```

And then you can run `rustc` to view current version:

```shell
$ rustc --version
rustc 1.54.0 (a178d0322 2021-07-26)
```

> Note: Above command will add rust to path only for existing session,
> after you restart the session you will have to add it again,
> to add rust to the path permanently it will depend on your shell but for Bash,
> you simply need to add the line from above, `export PATH=$PATH:$HOME/.cargo/bin` to your ~/.bashrc.

> For other shells, you'll want to find the appropriate place to set a configuration at start time,
> eg. zsh uses ~/.zshrc. Check your shell's documentation to find what file it uses.

thin-edge.io operates the `MSRV` (Minimum Supported Rust Version) and uses stable toolchain.

Current MSRV is `1.54.0`.

### Cross compilation toolchain (optional)

thin-edge.io can be compiled for target architecture on non-target device, this is called cross compilation.
Currently we support `Raspberry Pi 3B` for `armv7` architecture with Rust's cross compilation toolchain called [cargo cross](https://github.com/rust-embedded/cross).

To install [cargo cross](https://github.com/rust-embedded/cross):

```shell
cargo install cross
```

### Debian packaging (optional)

We use [cargo deb](https://github.com/mmstick/cargo-deb) to build our debian packages, the tool takes care of all the work to package thin-edge.io.

To install [cargo deb](https://github.com/mmstick/cargo-deb) use:

```shell
cargo install cargo-deb
```

## Compiling

To build thin-edge.io we are using `cargo`.

As we are using  `cargo workspace` for all our crates. All compiled files are put in `./target/` directory with target's name eg: `./target/debug` or `./target/release` for native builds and for cross compiled targets `./target/<architecture>/debug` or `./target/<architecture>/release` dependent on the target of the build.

### Compiling dev

To compile dev profile (with debug symbols) we use following command:

```shell
cargo build
```

Build artifacts can be found in `./target/debug` and will include executables:

```shell
$ ls ./target/debug/tedge*
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge_mapper
```

Binaries can be run eg: `./target/debug/tedge`.
Alternatively, you can use `cargo` to build and run executable in a single command:

```shell
cargo run --bin tedge
```

### Compiling release

To compile release profile we use following command:

```shell
cargo build --release
```

Build artifacts can be found in `./target/release` and will include executables:

```shell
$ ls ./target/release/tedge*
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge_mapper
```

Binaries can be run eg: `./target/release/tedge`.

## Building deb package

Currently thin-edge.io contains 2 binaries, `tedge` (cli) and `tedge_mapper` which are packaged as separate debian packages. To create following commands are to be issued:

```shell
cargo deb -p tedge
```

```shell
cargo deb -p tedge_mapper
```

All resulting packages are going to be in: `./target/debian/` directory:

```shell
$ ls ./target/debian -l
total 2948
-rw-rw-r-- 1 user user 11111 Jan 1 00:00 tedge_0.1.0_amd64.deb
-rw-rw-r-- 1 user user 11111 Jan 1 00:00 tedge_mapper_0.1.0_amd64.deb
```

## Cross compiling

To create binaries which can run on different platform than one you are currently on you can use [cargo cross](https://github.com/rust-embedded/cross):

```shell
cross build --target armv7-unknown-linux-gnueabihf
```

Build artifacts can be found in `./target/armv7-unknown-linux-gnueabihf/debug` and will include executables:

```shell
$ ls ./target/armv7-unknown-linux-gnueabihf/debug/tedge*
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge
-rwxrwxr-x   2 user user 11111 Jan 1 00:00 tedge_mapper
```

To cross compile release version of the binaries just add `--release` to the above command like so:

```shell
cross build --target armv7-unknown-linux-gnueabihf --release
```

## Running tests

When contributing to thin-edge.io we ask you to write tests for the code you have written. The tests will be run by build pipeline when you create pull request, but you can easily run all the tests when you are developing with following command:

```shell
cargo test
```

This will run all tests from the repository and sometime may take long time, `cargo` allows you to run specific test or set of tests for binary:

```shell
cargo test --bin tedge
```
