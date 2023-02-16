#!/bin/bash

set -e

# SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

CONNECT=${CONNECT:-1}
INSTALL=${INSTALL:-1}
PRE_CLEAN=${PRE_CLEAN:-0}
INSTALL_METHOD="${INSTALL_METHOD:-}"
INSTALL_SOURCEDIR=${INSTALL_SOURCEDIR:-.}
MAX_CONNECT_ATTEMPTS=${MAX_CONNECT_ATTEMPTS:-2}
TEDGE_MAPPER=${TEDGE_MAPPER:-c8y}
ARCH=${ARCH:-}
USE_RANDOM_ID=${USE_RANDOM_ID:-0}

get_debian_arch() {
    local arch=
    if command -v dpkg &> /dev/null; then
        arch=$(dpkg --print-architecture)
    else
        arch=$(uname -m)
        case "$arch" in
            armv7l|armv6l)
                arch="armhf"
                ;;

            aarch64)
                arch="armhf"
                ;;

            x86_64)
                arch="amd64"
                ;;
        esac
    fi

    echo "$arch"
}

ARCH=$(get_debian_arch)

while [ $# -gt 0 ]
do
    case "$1" in
        --no-install)
            INSTALL=0
            ;;

        --no-connect)
            CONNECT=0
            ;;

        --use-random-id)
            USE_RANDOM_ID=1
            ;;

        --mapper)
            TEDGE_MAPPER="$2"
            shift
            ;;

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
    esac
    shift
done

#
# Auto detect the install method by checking the local install folder
#
if [ -z "$INSTALL_METHOD" ]; then
    if [[ -n $(find "$INSTALL_SOURCEDIR" -type f -name "tedge_*.deb") ]]; then
        echo "Found local .deb files in folder [$INSTALL_SOURCEDIR], so using local dpkg install method"
        INSTALL_METHOD=local
    else
        echo "No local .deb files found in folder [$INSTALL_SOURCEDIR], so using apt install method"
        INSTALL_METHOD=apt
    fi
fi

# ---------------------------------------
# Install helpers
# ---------------------------------------
configure_repos() {
    LINUX_ARCH=$(uname -m)
    REPOS=()

    case "$LINUX_ARCH" in
        armv6l)
            # armv6 need their own repo as the debian arch (armhf) collides with that of armv7 (which is also armhf)
            REPOS=(
                # tedge-release-armv6
                tedge-main-armv6
            )
            ;;
        
        *)
            REPOS=(
                # tedge-release
                tedge-main
            )
            ;;
    esac

    for repo_name in "${REPOS[@]}"; do
        # Use a fixed distribution string to avoid guess work, and it does not really matter anyway
        curl -1sLf \
            "https://dl.cloudsmith.io/public/thinedge/${repo_name}/setup.deb.sh" \
            | distro=raspbian version=11 codename=bullseye sudo -E bash
    done
}

install_via_apt() {
    apt-get update

    if ! command -v mosquitto &>/dev/null; then
        apt-get install -y mosquitto
    fi

    apt-get install -y \
        tedge \
        tedge-mapper \
        tedge-agent \
        tedge-apt-plugin \
        c8y-configuration-plugin \
        c8y-log-plugin \
        tedge-watchdog
}

install_via_script() {
    apt-get update
    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
}

find_then_install_deb() {
    SOURCE_DIR="$1"
    PATTERN="$2"

    find "$SOURCE_DIR" -type f -name "$PATTERN" -print0 \
    | sort -r -V \
    | head -z -n 1 \
    | xargs -r0 dpkg -i
}

install_via_local_files() {
    if ! command -v mosquitto &>/dev/null; then
        apt-get update
        apt-get install -y mosquitto
    fi

    # Install tedge packages in same order as the get-thin-edge_io.sh script
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge_[0-9]*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]mapper_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]agent_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]apt[_-]plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y[_-]configuration[_-]plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y[_-]log[_-]plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]watchdog_*_$ARCH.deb"
}

cleanup() {
    echo "Cleaning up tedge files"
    # Remove existing config files as it can cause issues 
    sudo apt-get purge -y "tedge*" "c8y*"
    sudo rm -f /etc/tedge/tedge.toml
    sudo rm -f /etc/tedge/system.toml
    sudo rm -f /var/log/tedge/agent/*.log
}

# Cleanup device as sometimes the existing state can affect tests
if [ "$PRE_CLEAN" == 1 ]; then
    cleanup
fi

# Try disconnect mapper before installing (in case if left over from last installation)
if command -v tedge &>/dev/null; then
    # disconnect mapper (don't both testing)
    sudo tedge disconnect "$TEDGE_MAPPER" || true
fi

# ---------------------------------------
# Install
# ---------------------------------------
if [ "$INSTALL" == 1 ]; then
    echo ----------------------------------------------------------
    echo Installing thin-edge.io
    echo ----------------------------------------------------------
    echo
    configure_repos

    INSTALL_METHOD=${INSTALL_METHOD:-script}

    case "$INSTALL_METHOD" in
        apt)
            echo "Installing thin-edge.io using apt"
            install_via_apt
            ;;

        local)
            if [ $# -gt 1 ]; then
                INSTALL_SOURCEDIR="$2"
            fi
            echo "Installing thin-edge.io using local files (from path=$INSTALL_SOURCEDIR)"
            install_via_local_files
            ;;

        *)
            echo "Installing thin-edge.io using the 'get-thin-edge_io.sh' script"
            # Remove system.toml as the latest official release does not support custom reboot command
            rm -f /etc/tedge/system.toml
            install_via_script
            ;;
    esac
fi

echo ----------------------------------------------------------
echo Bootstrapping device
echo ----------------------------------------------------------
echo

PREFIX=${PREFIX:-tedge}

if [ -n "$PREFIX" ]; then
    PREFIX="${PREFIX}_"
fi

get_device_id() {
    if [ "$USE_RANDOM_ID" == "1" ]; then
        if [ -n "$DEVICE_ID" ]; then
            echo "Overriding the non-empty DEVICE_ID variable with a random name" >&2
        fi
        DEVICE_ID="${PREFIX}$(echo "$RANDOM" | md5sum | head -c 10)"
    fi

    if [ -n "$DEVICE_ID" ]; then
        echo "$DEVICE_ID"
        return
    fi

    if [ -n "$HOSTNAME" ]; then
        echo "${PREFIX}${HOSTNAME}"
        return
    fi
    if [ -n "$HOST" ]; then
        echo "${PREFIX}${HOST}"
        return
    fi
    echo "${PREFIX}unknown-device"
}

if [ -n "$C8Y_BASEURL" ]; then
    C8Y_HOST="$C8Y_BASEURL"
fi

if [ -z "$C8Y_HOST" ]; then
    echo "Missing Cumulocity Host url: C8Y_HOST" >&2
    exit 1
fi

# Strip any http/s prefixes
C8Y_HOST=$(echo "$C8Y_HOST" | sed -E 's|^.*://||g' | sed -E 's|/$||g')

# Check if tedge is installed before trying to bootstrap
if ! command -v tedge &>/dev/null; then
    echo "Skipping bootstrapping as tedge is not installed"
    exit 0
fi

echo "Setting c8y.url to $C8Y_HOST"
tedge config set c8y.url "$C8Y_HOST"

CURRENT_DEVICE_ID=$( tedge config get device.id | grep -v "tedge_config::")
EXPECTED_CERT_COMMON_NAME=$(get_device_id)
DEVICE_ID="$EXPECTED_CERT_COMMON_NAME"

# Remove existing certificate if it does not match
if tedge cert show >/dev/null 2>&1; then
    if [ "$CURRENT_DEVICE_ID" != "$EXPECTED_CERT_COMMON_NAME" ]; then
        echo "Device does not match expected. got=$CURRENT_DEVICE_ID, want=$EXPECTED_CERT_COMMON_NAME. Removing existing certificate"
        sudo tedge cert remove
    fi
fi

if ! tedge cert show >/dev/null 2>&1; then
    echo "Creating certificate: $EXPECTED_CERT_COMMON_NAME"
    tedge cert create --device-id "$EXPECTED_CERT_COMMON_NAME"

    if [ -n "$C8Y_PASSWORD" ]; then
        echo "Uploading certificate to Cumulocity using tedge"
        C8YPASS="$C8Y_PASSWORD" tedge cert upload c8y --user "$C8Y_USER"

        # Grace period for the server to process the certificate
        sleep 1
    fi
else
    echo "Certificate already exists"
fi

if [[ "$CONNECT" == 1 ]]; then
    # retry connection attempts
    CONNECT_ATTEMPT=0
    while true; do
        CONNECT_ATTEMPT=$((CONNECT_ATTEMPT + 1))
        if tedge connect "$TEDGE_MAPPER"; then
            break
        else
            if [ "$CONNECT_ATTEMPT" -ge "$MAX_CONNECT_ATTEMPTS" ]; then
                echo "Failed after $CONNECT_ATTEMPT connection attempts. Giving up"
                exit 2
            fi
        fi

        echo "WARNING: Connection attempt failed ($CONNECT_ATTEMPT of $MAX_CONNECT_ATTEMPTS)! Retrying to connect in 2s"
        sleep 2
    done
fi

# Add additional tools
systemctl start ssh

if ! id -u peter >/dev/null 2>&1; then
    useradd -ms /bin/bash peter && echo "peter:peter" | chpasswd && adduser peter sudo
fi

echo "Setting sudoers.d config"
echo '%sudo ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/all
echo 'tedge  ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown' > /etc/sudoers.d/tedge


echo
echo "----------------------------------------------------------"
echo "Device information"
echo "----------------------------------------------------------"
echo ""
echo "device.id:       ${DEVICE_ID}"
echo "Cumulocity IoT:  https://${C8Y_HOST}/apps/devicemanagement/index.html#/assetsearch?filter=*${DEVICE_ID//-/*}*"
echo ""
echo "----------------------------------------------------------"
