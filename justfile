set ignore-comments

# Default recipe
[private]
default:
    @just --list

# Install necessary tools
install-tools:
    rustup component add rustfmt --toolchain nightly
    cargo install cargo-sort

# Check if necessary tools are installed
[private]
check-tools:
    #!/usr/bin/env bash
    if ! cargo +nightly fmt --help &> /dev/null; then
        echo "cargo +nightly fmt is not installed, use just install-tools or install it manually"
        exit 1
    fi

    if ! cargo sort --help &> /dev/null; then
        echo "cargo sort is not installed, use just install-tools or install it manually"
        exit 1
    fi

# Format code
format: check-tools
    cargo +nightly fmt
    cargo sort -w .

# Check code formatting
format-check: check-tools
    cargo +nightly fmt -- --check
    cargo sort -w . --check

# Check code
check:
    cargo check
    cargo clippy --all-targets --all-features

# Release, building all binaries and debian packages
release *ARGS:
    ci/build_scripts/build.sh {{ARGS}}

# Run unit tests
test:
    cargo test --no-fail-fast --all-features --all-targets

integration-test:
    #!/usr/bin/env bash
    ci/build_scripts/build.sh x86_64-unknown-linux-musl
    cd tests/RobotFramework
    source .venv/bin/activate
    invoke build --local
    invoke tests

# Generate docs and start web server
docs:
    cd docs && mdbook serve

# Install doc dependencies
docs-install:
    cargo install mdbook mdbook-linkcheck mdbook-mermaid mdbook-admonish mdbook-cmdrun
