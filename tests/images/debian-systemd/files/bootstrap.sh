#!/bin/bash
set -e

show_usage() {
    echo "
DESCRIPTION
    Install thin-edge.io and prepare the device for testing

USAGE

    $0 [VERSION]

FLAGS
    WORKFLOW FLAGS
    --clean/--no-clean                      Clean the device of any existing tedge installations before installing/connecting. Default False
    --install/--no-install                  Install thin-edge.io. Default True
    --secure/--no-secure                    Configure certificate-based broker and client authentication. Default True.

    INSTALLATION FLAGS
    --version <version>                     Thin-edge.io version to install. Only applies for apt/script installation methods
    --channel <release|main>                Which channel, e.g. release or main to install thin-edge.io from. Defaults to main
    --install-method <apt|script|local>     Type of method to use to install thin.edge.io. Checkout the 'Install method' section for more info
    --install-sourcedir <path>              Path where to look for local deb files to install

    --help/-h                               Show this help

INSTALL METHODS
    local - Install the thin-edge.io .deb packages found locally on the disk under '--install-sourcedir <path>'
    apt - Install using public APT repository
    script - Install using the https://thin-edge.io/install.sh script

EXAMPLES
    sudo -E $0
    # Install thin-edge.io using the default settings

    sudo -E $0 --clean
    # Clean the device before installing, then install thin-edge.io

    sudo -E $0 ./packages/
    # Install using locally found tedge debian files under ./packages/ folder

    sudo -E $0 --channel main
    # Install the latest available version from the main repository. It will includes the latest version built from main

    sudo -E $0 --channel main --version 0.9.0-304-gfd2ed977
    # Install a specific version from the main repository

    sudo -E $0 --channel release
    # Install the latest available version from the release repository

    sudo -E $0 --install-method script
    # Install the latest version using the GitHub install script
    "
}

fail () { echo "$1" >&2; exit 1; }
warning () { echo "$1" >&2; }
command_exists() { command -v "$1" >/dev/null 2>&1; }

banner() {
    echo
    echo "----------------------------------------------------------"
    echo "$1"
    echo "----------------------------------------------------------"
}

# Defaults
SECURE=${SECURE:-1}
CLEAN=${CLEAN:-0}
INSTALL=${INSTALL:-1}
VERSION=${VERSION:-}
INSTALL_METHOD="${INSTALL_METHOD:-}"
INSTALL_SOURCEDIR=${INSTALL_SOURCEDIR:-.}
REPO_CHANNEL=${REPO_CHANNEL:-main}
ARCH=${ARCH:-}
CONFIGURE_TEST_SETUP=${CONFIGURE_TEST_SETUP:-1}
TEST_USER=${TEST_USER:-petertest}

get_debian_arch() {
    arch=
    if command_exists dpkg; then
        arch=$(dpkg --print-architecture)
    else
        arch=$(uname -m)
        case "$arch" in
            armv7*|armv6*)
                arch="armhf"
                ;;

            aarch64|arm64)
                arch="arm64"
                ;;

            x86_64|amd64)
                arch="amd64"
                ;;

            *)
                fail "Unsupported architecture. arch=$arch. This script only supports: [armv6l, armv7l, aarch64, x86_64]"
                ;;
        esac
    fi

    echo "$arch"
}

# ---------------------------------------
# Argument parsing
# ---------------------------------------
while [ $# -gt 0 ]
do
    case "$1" in
        # ----------------------
        # Clean
        # ----------------------
        # Should the device be cleaned prior to installation
        --clean)
            CLEAN=1
            ;;
        --no-clean)
            CLEAN=0
            ;;

        # ----------------------
        # Install thin-edge.io
        # ----------------------
        --install)
            INSTALL=1
            ;;
        --no-install)
            INSTALL=0
            ;;

        # Which channel, e.g. release or main to install thin-edge.io from
        --channel)
            REPO_CHANNEL="$2"
            shift
            ;;
        
        # Tedge install options
        --install-method)
            # Either "apt", "script" or "local". Unknown options will use "script"
            INSTALL_METHOD="$2"
            shift
            ;;

        --install-sourcedir)
            # Source install directory if install method "local" is used. Location of the .deb files
            INSTALL_SOURCEDIR="$2"
            shift
            ;;
        
        # ----------------------
        # Additional configuration
        # ----------------------
        --secure)
            SECURE=1
            ;;
        --no-secure)
            SECURE=0
            ;;

        # ----------------------
        # Misc
        # ----------------------
        --help|-h)
            show_usage
            exit 0
            ;;
        
        *)
            POSITIONAL_ARGS="$1"
            ;;
    esac
    shift
done

set -- "$POSITIONAL_ARGS"

# ---------------------------------------
# Initializing
# ---------------------------------------
banner "Initializing"

# Try guessing the positional arguments
# If it looks like a directory, then use the as the install directory
# else use it has a version. But in both ca
while [ $# -gt 0 ]; do
    if [ -d "$1" ] && [ -z "$INSTALL_SOURCEDIR" ]; then
        echo "Detected install-sourcedir from positional argument. install-sourcedir=$1"
        INSTALL_SOURCEDIR="$1"
    elif [ -z "$VERSION" ]; then
        echo "Detected version from positional argument. version=$1"
        VERSION="$1"
    else
        fail "Unexpected positional arguments. Check the usage by provide '--help' for examples"
    fi
    shift
done

if [ -z "$REPO_CHANNEL" ]; then
    if [ -z "$VERSION" ]; then
        REPO_CHANNEL="release"
    else
        # Check if the user has requested an official version or not
        if echo "$VERSION" | grep --silent "^[0-9]\+.[0-9]\+.[0-9]\+$"; then
            REPO_CHANNEL="release"
        else
            REPO_CHANNEL="main"
        fi
    fi
fi

#
# Auto detect the install method by checking the local install folder
#
# Only the script install method supports installing from older versions
if [ -z "$INSTALL_METHOD" ]; then
    if [ -n "$(find "$INSTALL_SOURCEDIR" -type f -name "tedge_[0-9]*.deb")" ]; then
        echo "Using local dpkg install method as local .deb files were found in folder: $INSTALL_SOURCEDIR"
        INSTALL_METHOD=local
    else
        echo "Using apt install method as no local .deb files found in folder: $INSTALL_SOURCEDIR"
        INSTALL_METHOD=apt
    fi
else
    if [ "$INSTALL_METHOD" != "apt" ] && [ "$INSTALL_METHOD" != "local" ] && [ "$INSTALL_METHOD" != "script" ]; then
        fail "Invalid install method [$INSTALL_METHOD]. Only 'apt', 'local' or 'script' values are supported"
    fi
fi


# ---------------------------------------
# Install helpers
# ---------------------------------------
configure_repos() {
    LINUX_ARCH=$(uname -m)
    REPO=""
    REPO_SUFFIX=

    case "$LINUX_ARCH" in
        armv6l)
            # armv6 need their own repo as the debian arch (armhf) collides with that of armv7 (which is also armhf)
            REPO_SUFFIX="-armv6"
            ;;
    esac

    case "$REPO_CHANNEL" in
        main|release)
            REPO="tedge-${REPO_CHANNEL}${REPO_SUFFIX}"
            ;;

        *)
            fail "Invalid channel"
            ;;
    esac

    # Remove any other repos
    DELETED_REPOS=$(sudo find /etc/apt/sources.list.d/ -type f \( -name "thinedge-*.list" -a ! -name "thinedge-${REPO}.list" -a ! -name "thinedge-community.list" \) -delete -print0)
    if [ -n "$DELETED_REPOS" ]; then
        echo "Cleaning other repos. $DELETED_REPOS"
        sudo apt-get clean -y

        # When changing repository channel the package needs to be removed but keep files
        remove_tedge
    fi

    if command_exists bash; then
        if [ ! -f "/etc/apt/sources.list.d/thinedge-${REPO}.list" ]; then
            # Use a fixed distribution string to avoid guess work, and it does not really matter anyway
            curl -1sLf \
            "https://dl.cloudsmith.io/public/thinedge/${REPO}/setup.deb.sh" \
            | distro=raspbian version=11 codename=bookworm sudo -E bash
        else
            echo "Repo (channel=${REPO_CHANNEL}) is already configured"
        fi
    else
        # TODO: Support non-bash environments (but the cloudsmith script only supports bash)
        fail "Bash is missing. Currently this script requires bash to setup the apt repos"
        # deb [signed-by=/usr/share/keyrings/thinedge-tedge-release-archive-keyring.gpg] https://dl.cloudsmith.io/public/thinedge/tedge-release/deb/raspbian bookworm main
    fi
}

install_via_apt() {
    sudo apt-get update

    if ! command -v mosquitto >/dev/null 2>&1; then
        sudo apt-get install -y mosquitto
    fi

    if [ -n "$VERSION" ]; then
        echo "Installing specific version: $VERSION"
        sudo apt-get install -y --allow-downgrades \
            tedge="$VERSION" \
            tedge-mapper="$VERSION" \
            tedge-agent="$VERSION" \
            tedge-apt-plugin="$VERSION" \
            c8y-firmware-plugin="$VERSION" \
            c8y-remote-access-plugin="$VERSION" \
            tedge-watchdog="$VERSION"
    else
        echo "Installing latest available version"
        sudo apt-get install -y --allow-downgrades \
            tedge \
            tedge-mapper \
            tedge-agent \
            tedge-apt-plugin \
            c8y-firmware-plugin \
            c8y-remote-access-plugin \
            tedge-watchdog
    fi
}

install_via_script() {
    sudo apt-get update
    if [ -n "$VERSION" ]; then
        echo "Installing specific version: $VERSION"
        curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s "$VERSION"
    else
        echo "Installing latest official version"
        curl -fsSL https://thin-edge.io/install.sh | sh -s
    fi
}

find_then_install_deb() {
    SOURCE_DIR="$1"
    PATTERN="$2"

    find "$SOURCE_DIR" -type f -name "$PATTERN" -print0 \
    | sort -r -V \
    | head -z -n 1 \
    | xargs -r0 sudo dpkg -i
}

install_via_local_files() {
    if ! command_exists mosquitto; then
        sudo apt-get update
        sudo apt-get install -y mosquitto
    fi

    ARCH=$(get_debian_arch)

    # Install tedge packages
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge_[0-9]*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]mapper_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]agent_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]apt[_-]plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y-firmware-plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]watchdog_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y-remote-access-plugin*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge-p11-server*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge-flows*_$ARCH.deb"
}

stop_services() {
    if command_exists systemctl; then
        sudo systemctl stop tedge-agent >/dev/null 2>&1 || true
        sudo systemctl stop tedge-mapper-c8y >/dev/null 2>&1 || true
    fi
}

purge_tedge() {
    echo "Purging tedge"

    # try stopping agent (as downgrading can have problems)
    stop_services

    # Remove existing config files as it can cause issues
    sudo apt-get purge -y "tedge*" "c8y*"

    # Refresh path, to ensure old binaries are still not being detected
    hash -r

    echo "Cleaning up tedge files"
    sudo rm -f /etc/tedge/tedge.toml
    sudo rm -f /etc/tedge/system.toml
    sudo rm -f /var/log/tedge/agent/*.log
}

remove_tedge() {
    echo "Removing tedge"

    # try stopping agent (as downgrading can have problems)
    stop_services

    # Remove packages but keep configuration files
    sudo apt-get remove -y "tedge*" "c8y*"

    # Refresh path, to ensure old binaries are still not being detected
    hash -r
}

install_tedge() {
    case "$INSTALL_METHOD" in
        local)
            echo "Skipping repo configuration as thin-edge.io is being installed using local packages" >&2
            ;;
        *)
            configure_repos
            ;;
    esac

    case "$INSTALL_METHOD" in
        apt)
            echo "Installing thin-edge.io using apt"
            if ! install_via_apt; then
                echo "Installing via apt failed, installing via script"
                install_via_script
            fi
            ;;

        local)
            echo "Installing thin-edge.io using local files (from path=$INSTALL_SOURCEDIR)"
            install_via_local_files
            ;;

        *)
            echo "Installing thin-edge.io using the install script"
            # Remove system.toml as the latest official release does not support custom reboot command
            rm -f /etc/tedge/system.toml
            install_via_script
            ;;
    esac
}

gen_certs() {
    openssl req \
        -new \
        -x509 \
        -days 365 \
        -extensions v3_ca \
        -nodes \
        -subj "/C=US/ST=Denial/L=Springfield/O=Dis/CN=ca" \
        -keyout ca.key \
        -out ca.crt

    openssl genrsa -out server.key 2048

    openssl req -out server.csr -key server.key -new \
        -subj "/C=US/ST=Denial/L=Springfield/O=Dis/CN=$(hostname)"

    cat > v3.ext << EOF
    authorityKeyIdentifier=keyid
    basicConstraints=CA:FALSE
    keyUsage = digitalSignature, keyAgreement
    subjectAltName=DNS:$(hostname), DNS:localhost, IP:127.0.0.1
EOF

    openssl x509 -req \
        -in server.csr \
        -CA ca.crt \
        -CAkey ca.key \
        -extfile v3.ext \
        -CAcreateserial \
        -out server.crt \
        -days 365

    openssl genrsa -out client.key 2048

    openssl req -out client.csr \
        -key client.key \
        -subj "/C=US/ST=Denial/L=Springfield/O=Dis/CN=client1" \
        -new

    cat > client-v3.ext << EOF
basicConstraints=CA:FALSE
extendedKeyUsage = clientAuth
EOF

    openssl x509 -req \
        -in client.csr \
        -CA ca.crt \
        -CAkey ca.key \
        -extfile client-v3.ext \
        -CAcreateserial \
        -out client.crt \
        -days 365

    mv ca* /etc/mosquitto/ca_certificates
    mv server* /etc/mosquitto/ca_certificates

    cp secure-listener.conf /etc/mosquitto/conf.d/

    chown -R mosquitto:mosquitto /etc/mosquitto/ca_certificates
    chown tedge:tedge /setup/client.*

    tedge config set mqtt.client.port 8883
    tedge config set mqtt.client.auth.ca_file /etc/mosquitto/ca_certificates/ca.crt
    tedge config set mqtt.client.auth.cert_file /setup/client.crt
    tedge config set mqtt.client.auth.key_file /setup/client.key

    if ! sudo systemctl restart mosquitto.service; then
        echo "Failed to restart mosquitto"
        exit 1
    fi

    echo "Generated certificates successfully"
}

check_systemd_cgroup_compat() {
    #
    # Check the systemd/cgroup version compatibility
    # Fail if an invalid combination is detected
    #
    CGROUPS_VERSION=1
    if [ -e /sys/fs/cgroup/cgroup.controllers ]; then
        CGROUPS_VERSION=2
    fi

    # systemd version 256 drops cgroup v1 support, which means on older operating systems like
    # ubuntu 20.04, systemd may not behave as expected
    # ee systemd release notes https://github.com/systemd/systemd/blob/main/NEWS
    SYSTEMD_VERSION=
    if command_exists systemctl; then
        SYSTEMD_VERSION=$(systemctl --version | head -n1 | cut -d' ' -f2)
    fi
    if [[ "$SYSTEMD_VERSION" =~ ^\d+$ ]]; then
        if [ "$CGROUPS_VERSION" -lt 2 ] && [ "$SYSTEMD_VERSION" -ge 256 ]; then
            echo "ERROR!!!!: incompatible cgroup version detected. systemd >=256 drops support for cgroupsv1. systemd_version=$SYSTEMD_VERSION, cgroup_version=$CGROUPS_VERSION"
            exit 1
        fi
    fi
}

configure_test_user() {
    if [ -n "$TEST_USER" ]; then
        if ! id -u "$TEST_USER" >/dev/null 2>&1; then
            sudo useradd -ms /bin/sh "${TEST_USER}" && echo "${TEST_USER}:${TEST_USER}" | sudo chpasswd && sudo adduser "${TEST_USER}" sudo
        fi
    fi
}

post_configure() {
    echo "Setting sudoers.d config"
    sudo sh -c "echo '%sudo ALL=(ALL) NOPASSWD:SETENV:ALL' > /etc/sudoers.d/all"
    sudo sh -c "echo 'tedge  ALL = (ALL) NOPASSWD:SETENV: /sbin/shutdown' > /etc/sudoers.d/shutdown"
}

main() {
    check_systemd_cgroup_compat

    # ---------------------------------------
    # Preparation (clean and disconnect)
    # ---------------------------------------
    # Cleanup device as sometimes the existing state can affect tests
    if [ "$CLEAN" = 1 ]; then
        banner "Preparing device"
        purge_tedge
    fi

    # ---------------------------------------
    # Install
    # ---------------------------------------
    if [ "$INSTALL" = 1 ]; then
        banner "Installing thin-edge.io"
        install_tedge
    fi

    # ---------------------------------------
    # Set up authentication
    # ---------------------------------------
    if [ "$SECURE" = 1 ]; then
        banner "Setting up certificates"
        gen_certs
    fi

    # ---------------------------------------
    # Post setup
    # ---------------------------------------
    if [ "$CONFIGURE_TEST_SETUP" = 1 ]; then
        configure_test_user
        post_configure

        # Add additional tools
        if command_exists systemctl; then
            sudo systemctl start ssh
        fi
    fi
}

main
