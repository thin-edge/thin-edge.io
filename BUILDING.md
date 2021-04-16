# Building thin-edge.io

## Installing tools

### Rust toolchain

To install Rust please follow [Official installation guide](https://www.rust-lang.org/tools/install).
To get started you need Cargo's bin directory (`$HOME/.cargo/bin`) in your PATH
environment variable.

```shell
export PATH=$PATH:$HOME/.cargo/bin
```

And then you can run `rustc` to view current version:

```shell
$ rustc --version
rustc 1.51.0 (2fd73fabe 2021-03-23)
```

> Note: Above command will add rust to path only for existing session,
> after you restart the session you will have to add it again,
> to add rust to the path permanently it will depend on your shell but for Bash,
> you simply need to add the line from above, `export PATH=$PATH:$HOME/.cargo/bin` to your ~/.bashrc.

> For other shells, you'll want to find the appropriate place to set a configuration at start time,
> eg. zsh uses ~/.zshrc. Check your shell's documentation to find what file it uses.

thin-edge.io operates the `MSRV` (Minimum Supported Rust Version) and uses stable toolchain.

Current MSRV is `1.46.0`.

## Compiling

To build thin-edge.io we are using `cargo`.

As we are using a cargo workspace for all our crates all binaries are put in `./target/` directory with target's name eg: `./target/debug` or `./target/armv7-unknown-linux-gnueabihf` dependent on the type of build.

### Compiling dev

To compile in dev mode we use following command:

```shell
cargo build
```

Build artifacts can be found in `./target/debug` and will include executables:

```shell
$ ls ./target/debug/ted*
-rwxrwxr-x   2 user user 108549968 Jan 1 00:00 tedge
-rw-rw-r--   1 user user      3442 Jan 1 00:00 tedge.d
-rwxrwxr-x   2 user user  62168456 Jan 1 00:00 tedge_mapper
-rw-rw-r--   1 user user       411 Jan 1 00:00 tedge_mapper.d
```

Binaries can be run eg: `./target/debug/tedge`.

### Compiling release

To compile in dev mode we use following command:

```shell
cargo build --release
```

Build artifacts can be found in `./target/release` and will include executables:

```shell
$ ls ./target/debug/ted*
-rwxrwxr-x   2 user user 108549968 Jan 1 00:00 tedge
-rw-rw-r--   1 user user      3442 Jan 1 00:00 tedge.d
-rwxrwxr-x   2 user user  62168456 Jan 1 00:00 tedge_mapper
-rw-rw-r--   1 user user       411 Jan 1 00:00 tedge_mapper.d
```

Binaries can be run eg: `./target/release/tedge`.

## Running tests

When contributing to thin-edge.io we ask you to write tests for the code you have written. The tests will be run by build pipeline when you create pull request, but you can easily run all the tests when you are developing with following command:

```shell
cargo test
```

This will run all tests from the repository and sometime may take long time, `cargo` allows you to run specific test or set of tests for binary:

```shell
cargo test --bin tedge
```
