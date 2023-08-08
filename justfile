set ignore-comments

VERSION := `./ci/build_scripts/build.sh --version 2>/dev/null || exit 0`
# Detect the default target based on the user's CPU arch.
# For MacOS m1 users, it will return a linux host but
# match the appropriate CPU architecture,
# e.g. MacOS m1 => "aarch64-unknown-linux-musl"
DEFAULT_TARGET := `./ci/build_scripts/detect_target.sh`

# Print project and host machine info
info:
    @echo "OS:             {{os()}}"
    @echo "OS_FAMILY:      {{os_family()}}"
    @echo "HOST_ARCH:      {{arch()}}"
    @echo "VERSION:        {{VERSION}}"
    @echo "DEFAULT_TARGET: {{DEFAULT_TARGET}}"

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

# Install integration test dependencies
setup-integration-test *ARGS:
    tests/RobotFramework/bin/setup.sh {{ARGS}}

# Run integration tests (using local build)
integration-test *ARGS: release
    cd tests/RobotFramework && .venv/bin/python3 -m invoke build --local tests {{ARGS}}

# Generate docs and start web server
docs:
    cd docs && mdbook serve

# Install doc dependencies
docs-install:
    cargo install mdbook mdbook-linkcheck mdbook-mermaid mdbook-admonish mdbook-cmdrun

# Generate linux package scripts from templates
generate-linux-package-scripts:
    ./configuration/package_scripts/generate.py

# Build linux virtual packages
release-linux-virtual:
    ./ci/build_scripts/package.sh build_virtual "all" --version "{{VERSION}}" --output target/virtual-packages

# Publish linux virtual packages
publish-linux-virtual *ARGS='':
    ./ci/build_scripts/publish_packages.sh --path target/virtual-packages {{ARGS}}

# Publish linux packages for a specific target
publish-linux-target TARGET=DEFAULT_TARGET *ARGS='':
    ./ci/build_scripts/publish_packages.sh --path "target/{{TARGET}}/packages" {{ARGS}}
