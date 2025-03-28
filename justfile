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

# Publish the dev container to provide more reproducible dev environments
#
# docker login ghcr.io
publish-dev-container TAG="latest" IMAGE="ghcr.io/thin-edge/devcontainer" VARIANT="bookworm" OUTPUT_TYPE="registry":
    docker buildx install
    cd .devcontainer && docker buildx build \
        --platform linux/amd64,linux/arm64 \
        --build-arg "VARIANT={{VARIANT}}" \
        --label "org.opencontainers.image.version={{VERSION}}-{{VARIANT}}" \
        --label "org.opencontainers.image.source=https://github.com/thin-edge/thin-edge.io" \
        -t "{{IMAGE}}:{{TAG}}-{{VARIANT}}" \
        -t "{{IMAGE}}:latest-{{VARIANT}}" \
        -f Dockerfile \
        --output=type="{{OUTPUT_TYPE}}",oci-mediatypes=false \
        --provenance=false \
        .

# Default recipe
[private]
default:
    @just --list

# Prepare developer settings like enabling git --signoff by default
prepare-dev:
    ./ci/dev/add-git-hook-prepare-commit.sh

# Install necessary tools
install-tools:
    rustup component add rust-analyzer rust-analysis rust-src rustfmt clippy
    rustup toolchain install nightly
    rustup component add rustfmt --toolchain nightly
    cargo +stable install taplo-cli cargo-nextest
    cargo +stable install cargo-deny

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

    if ! cargo deny --help &> /dev/null; then
        echo "cargo-deny is not installed, use just install-tools or install it manually"
        exit 1
    fi

# Format code and tests
format: check-tools
    #!/usr/bin/env bash
    set -e

    cargo +nightly fmt
    taplo fmt

    if [ ! -d tests/RobotFramework/.venv ]; then
        just -f {{justfile()}} setup-integration-test
    fi
    cd tests/RobotFramework
    source .venv/bin/activate
    invoke format-tests

# Check code formatting
format-check: check-tools
    #!/usr/bin/env bash
    set -e
    cargo +nightly fmt -- --check
    taplo fmt --check

    if [ ! -d tests/RobotFramework/.venv ]; then
        just -f {{justfile()}} setup-integration-test
    fi
    cd tests/RobotFramework
    source .venv/bin/activate
    invoke lint-tests

# Check code
check TARGET=DEFAULT_TARGET:
    #!/usr/bin/env bash
    set -e

    {{CARGO}} check --target {{TARGET}}
    {{CARGO}} clippy --all-targets --all-features --target {{TARGET}}
    just -f {{justfile()}} check-dependencies
    if [ ! -d tests/RobotFramework/.venv ]; then
        just -f {{justfile()}} setup-integration-test
    fi
    cd tests/RobotFramework
    source .venv/bin/activate
    echo Checking tests...
    invoke lint-tests

# Check dependencies using cargo-deny
check-dependencies:
    cargo deny fetch
    cargo deny --all-features check --allow duplicate --allow unmaintained

# Release, building all binaries and debian packages
release *ARGS:
    ci/build_scripts/build.sh {{ARGS}}

# Run unit and doc tests
test *ARGS:
    just -f {{justfile()}} test-unit {{ARGS}}
    just -f {{justfile()}} test-docs

# Run unit tests
test-unit *ARGS:
    cargo nextest run --status-level fail --no-fail-fast --all-features --all-targets {{ARGS}}

# Run doc tests
test-docs *ARGS:
    cargo test --doc --no-fail-fast --all-features {{ARGS}}

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

# Compile WASM Components
build-wasm: wasm_deps
    #!/usr/bin/env bash
    set -e
    cd crates/extensions/tedge_wasm_mapper/components
    cargo +nightly fmt
    cargo build --target wasm32-wasip2 --release

wasm_deps:
    rustup target add wasm32-wasip2
