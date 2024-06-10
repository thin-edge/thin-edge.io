set ignore-comments
set dotenv-load

VERSION := `./ci/build_scripts/version.sh 2>/dev/null || exit 0`
# Detect the default target based on the user's CPU arch.
# For MacOS m1 users, it will return a linux host but
# match the appropriate CPU architecture,
# e.g. MacOS m1 => "aarch64-unknown-linux-musl"
DEFAULT_TARGET := `./ci/build_scripts/detect_target.sh`

CARGO := `command -V cargo-zigbuild >/dev/null 2>&1 && printf "cargo-zigbuild" || printf "cargo"`

# Print project and host machine info
info:
    @echo "OS:             {{os()}}"
    @echo "OS_FAMILY:      {{os_family()}}"
    @echo "HOST_ARCH:      {{arch()}}"
    @echo "VERSION:        {{VERSION}}"
    @echo "DEFAULT_TARGET: {{DEFAULT_TARGET}}"

# Print the current git generated version
version TYPE="all":
    @./ci/build_scripts/version.sh {{TYPE}} 2>/dev/null || exit 0

# Default recipe
[private]
default:
    @just --list

# Install necessary tools
install-tools:
    rustup component add rustfmt --toolchain nightly
    cargo install taplo-cli cargo-nextest

# Check if necessary tools are installed
[private]
check-tools:
    #!/usr/bin/env bash
    if ! cargo +nightly fmt --help &> /dev/null; then
        echo "cargo +nightly fmt is not installed, use just install-tools or install it manually"
        exit 1
    fi

    if ! taplo fmt --help &> /dev/null; then
        echo "taplo is not installed, use just install-tools or install it manually"
        exit 1
    fi

# Format code
format: check-tools
    cargo +nightly fmt
    taplo fmt

# Check code formatting
format-check: check-tools
    cargo +nightly fmt -- --check
    taplo fmt --check

# Check code
check TARGET=DEFAULT_TARGET:
    {{CARGO}} check --target {{TARGET}}
    {{CARGO}} clippy --all-targets --all-features --target {{TARGET}}

# Release, building all binaries and debian packages
release *ARGS:
    ci/build_scripts/build.sh {{ARGS}}

# Run unit tests
test:
    cargo nextest run --no-fail-fast --all-features --all-targets
    cargo test --doc --no-fail-fast --all-features

# Install integration test dependencies
setup-integration-test *ARGS:
    tests/RobotFramework/bin/setup.sh {{ARGS}}

# Run integration tests (using local build)
integration-test *ARGS: release
    #!/usr/bin/env bash
    set -e
    if [ ! -d tests/RobotFramework/.venv ]; then
        just -f {{justfile()}} setup-integration-test
    fi
    cd tests/RobotFramework
    source .venv/bin/activate
    invoke build --local
    invoke tests {{ARGS}}

# Generate linux package scripts from templates
generate-linux-package-scripts:
    ./configuration/package_scripts/generate.py

# Build linux virtual packages
release-linux-virtual:
    ./ci/build_scripts/package.sh build_virtual "all" --output target/virtual-packages

# Publish linux virtual packages
publish-linux-virtual *ARGS='':
    ./ci/build_scripts/publish_packages.sh --path target/virtual-packages {{ARGS}}

# Publish linux packages for a specific target
publish-linux-target TARGET=DEFAULT_TARGET *ARGS='':
    ./ci/build_scripts/publish_packages.sh --path "target/{{TARGET}}/packages" {{ARGS}}


# Generate changelog for a release
generate-changelog *ARGS:
    ./ci/changelog/changelog.sh {{ARGS}}
