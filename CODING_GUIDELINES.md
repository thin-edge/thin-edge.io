# Coding Guidelines

To ease development, thin-edge.io uses [just](https://github.com/casey/just) to define common project tasks such as running the formatter, checking linting etc. These guidelines assume that you have already installed just and the additional tools to support the daily development.

Once you have installed `just`, you can install the additional tools using the following command:

```sh
just install-tools
```

## Code Style
Follow [Rust coding guidelines](https://doc.rust-lang.org/style-guide/index.html).
When adding new feature or adding an API try to adhere to [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/about.html).
Avoid using unsafe code.

## Code formatting
Always use rustfmt before you commit as your code won't pass CI pipeline if rustfmt is not applied. We adhere to default settings.

```sh
just format
```


## Code analysis
Clippy is used to catch common mistakes and we run it as part of our CI pipeline.

```sh
just check
```


## Code testing 
Ideally, all code should be unit tested. Unit tests should be in module file or if the file or tests are very long in separate file `tests.rs` in the same directory as `mod.rs`.

## Error handling 
Error handling suggestions follow the [Rust book guidance](https://doc.rust-lang.org/book/ch09-00-error-handling.html). Recoverable errors should be handled with [Result](https://doc.rust-lang.org/std/result/). Our suggestions on unrecoverable errors are listed below:
* Panic (the code should not panic)
* `unwrap()` - Unwrap should only be used for mutexes (e.g. `lock().unwrap()`) and test code. For all other use cases, prefer `expect()`. The only exception is if the error message is custom-generated, in which case use `.unwrap_or_else(|| panic!("error: {}", foo))`
* `expect()` - Expect should be invoked when a system invariant is expected to be preserved. `expect()` is preferred over `unwrap()` and should contain a detailed error message on failure in most cases.
* `assert!()` - This macro is kept in both debug/release and should be used to protect invariants of the system as necessary.
