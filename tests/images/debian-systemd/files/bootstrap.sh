#!/bin/sh
set -e

show_usage() {
    echo "
DESCRIPTION
    Install and bootstrap thin-edge.io.

USAGE

    $0 [VERSION]

FLAGS
    WORKFLOW FLAGS
    --clean/--no-clean                      Clean the device of any existing tedge installations before installing/connecting. Default False
    --install/--no-install                  Install thin-edge.io. Default True
    --connect/--no-connect                  Connect the mapper. Provide the type of mapper via '--mapper <name>'. Default True
    --mapper <name>                         Name of the mapper to use when connecting (if user has specified the --connect option).
                                            Defaults to 'c8y'. Currently only c8y works.
    --bootstrap                             Force bootstrapping/re-bootstrapping of the device
    --secure/--no-secure                    Configure certificate-based broker and client authentication. Default True.

    DEVICE FLAGS
    --device-id <name>                      Use a specific device-id. A prefix will be added to the device id
    --random                                Use a random device-id. This will override the --device-id flag value
    --prefix <prefix>                       Device id prefix to add to the device-id or random device id. Defaults to 'tedge_'

    INSTALLATION FLAGS
    --version <version>                     Thin-edge.io version to install. Only applies for apt/script installation methods
    --channel <release|main>                Which channel, e.g. release or main to install thin-edge.io from. Defaults to main
    --install-method <apt|script|local>     Type of method to use to install thin.edge.io. Checkout the 'Install method' section for more info
    --install-sourcedir <path>              Path where to look for local deb files to install

    CUMULOCITY FLAGS
    --c8y-url <host>                        Cumulocity url, e.g. 'mydomain.c8y.example.io'
    --c8y-user <username>                   Cumulocity username (required when a new device certificate is created)

    MISC FLAGS
    --prompt/--no-prompt                    Set if the script should prompt the user for input or not. By default prompts
                                            will be disabled on non-interactive shells
    --help/-h                               Show this help

INSTALL METHODS
    local - Install the thin-edge.io .deb packages found locally on the disk under '--install-sourcedir <path>'
    apt - Install using public APT repository
    script - Install using the get-thin-edge_io.sh script from GitHub

EXAMPLES
    sudo -E $0
    # Install and bootstrap thin-edge.io using the default settings

    sudo -E $0 --device-id mydevice --bootstrap
    # Install latest version and force bootstrapping using a given device id

    sudo -E $0 --device-id mydevice --bootstrap --prefix ''
    # Install latest version, and bootstrap with a device name without the default prefix

    sudo -E $0 --random --bootstrap
    # Install latest version and force bootstrapping using a random device id
    
    sudo -E $0 --clean
    # Clean the device before installing, then install and bootstrap

    sudo -E $0 ./deb/
    # Install and bootstrap using locally found tedge debian files under ./deb/ folder

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

uses_old_package_name() {
    version="$1"
    echo "$version" | grep --silent "^0\.[0-8]\."
}

uses_new_package_name() {
    version="$1"
    echo "$version" | grep --silent -e "^0\.[9]\." -e "^0\.[1-9][0-9]" -e "^[1-9]"
}

parse_domain() {
    echo "$1" | sed -E 's|^.*://||g' | sed -E 's|/$||g'
}

banner() {
    echo
    echo "----------------------------------------------------------"
    echo "$1"
    echo "----------------------------------------------------------"
}

# Defaults
DEVICE_ID=${DEVICE_ID:-}
BOOTSTRAP=${BOOTSTRAP:-}
SECURE=${SECURE:-1}
CONNECT=${CONNECT:-1}
INSTALL=${INSTALL:-1}
CLEAN=${CLEAN:-0}
VERSION=${VERSION:-}
INSTALL_METHOD="${INSTALL_METHOD:-}"
INSTALL_SOURCEDIR=${INSTALL_SOURCEDIR:-.}
MAX_CONNECT_ATTEMPTS=${MAX_CONNECT_ATTEMPTS:-2}
TEDGE_MAPPER=${TEDGE_MAPPER:-c8y}
ARCH=${ARCH:-}
USE_RANDOM_ID=${USE_RANDOM_ID:-0}
SHOULD_PROMPT=${SHOULD_PROMPT:-1}
CAN_PROMPT=0
UPLOAD_CERT_WAIT=${UPLOAD_CERT_WAIT:-1}
CONFIGURE_TEST_SETUP=${CONFIGURE_TEST_SETUP:-1}
TEST_USER=${TEST_USER:-petertest}
PREFIX=${PREFIX:-tedge_}
REPO_CHANNEL=${REPO_CHANNEL:-main}
C8Y_BASEURL=${C8Y_BASEURL:-}


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

generate_device_id() {
    #
    # Generate a device id
    # Either use a raond device, or the device's hostname
    #
    if [ "$USE_RANDOM_ID" = "1" ]; then
        if [ -n "$DEVICE_ID" ]; then
            echo "Overriding the non-empty DEVICE_ID variable with a random name" >&2
        fi

        RANDOM_ID=
        if [ -e /dev/urandom ]; then
            RANDOM_ID=$(head -c 128 /dev/urandom | md5sum | head -c 10)
        elif [ -e /dev/random ]; then
            RANDOM_ID=$(head -c 128 /dev/random | md5sum | head -c 10)
        fi

        if [ -n "$RANDOM_ID" ]; then
            DEVICE_ID="${PREFIX}${RANDOM_ID}"
        else
            warning "Could not generate a random id. Check if /dev/random is available or not"
        fi
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

check_sudo() {
    if [ "$(id -u)" -ne 0 ]; then
        echo "Please run as root or using sudo"
        show_usage
        exit 1
    fi
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
        # Should the device be cleaned prior to installation/bootstrapping
        --clean)
            CLEAN=1
            BOOTSTRAP=1
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
        # Bootstrap
        # ----------------------
        # Device id options
        --device-id)
            DEVICE_ID="$2"
            shift
            ;;

        --prefix)
            PREFIX="$2"
            shift
            ;;
        --random)
            USE_RANDOM_ID=1
            ;;

        --bootstrap)
            BOOTSTRAP=1
            ;;

        --no-bootstrap)
            BOOTSTRAP=0
            ;;

        --secure)
            SECURE=1
            ;;
        --no-secure)
            SECURE=0
            ;;

        # ----------------------
        # Connect mapper
        # ----------------------
        --connect)
            CONNECT=1
            ;;
        --no-connect)
            CONNECT=0
            ;;

        # Preferred mapper
        --mapper)
            TEDGE_MAPPER="$2"
            shift
            ;;
        
        # Cumulocity settings
        --c8y-user)
            C8Y_USER="$2"
            shift
            ;;
        --c8y-url)
            C8Y_BASEURL="$2"
            shift
            ;;

        # ----------------------
        # Misc
        # ----------------------
        # Should prompt for use if information is missing?
        --prompt)
            SHOULD_PROMPT=1
            ;;
        --no-prompt)
            SHOULD_PROMPT=0
            ;;

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
        # Check if the user is requestion an official version or not
        if echo "$VERSION" | grep --silent "^[0-9]\+.[0-9]\+.[0-9]\+$"; then
            REPO_CHANNEL="release"
        else
            REPO_CHANNEL="main"
        fi
    fi
fi

#
# Detect settings
if command_exists tedge; then
    if [ -z "$C8Y_BASEURL" ]; then
        C8Y_BASEURL=$( tedge config list | grep "^c8y.url=" | sed 's/^c8y.url=//' )    
    fi

    if [ -z "$DEVICE_ID" ]; then
        DEVICE_ID=$( tedge config list | grep "^device.id=" | sed 's/^device.id=//' )
    fi

    # Detect if bootstrapping is required or not?
    if [ -z "$BOOTSTRAP" ]; then
        # If connection already exists, then assume bootstrapping does not need to occur again
        # If already connected, then stick with using the same certificate
        if tedge connect "$TEDGE_MAPPER" --test >/dev/null 2>&1; then
            echo "No need for bootstrapping as $TEDGE_MAPPER mapper is already connected. You can force bootstrapping using either --bootstrap or --clean flags"
            BOOTSTRAP=0
        fi
    fi
fi

if [ -z "$DEVICE_ID" ] || [ "$CLEAN" = 1 ]; then
    DEVICE_ID=$(generate_device_id)
fi

if [ -z "$BOOTSTRAP" ]; then
    BOOTSTRAP=1
fi

#
# Detect if the shell is running in interactive mode or not
if [ -t 0 ]; then
    CAN_PROMPT=1
else
    CAN_PROMPT=0
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
            | distro=raspbian version=11 codename=bullseye sudo -E bash
        else
            echo "Repo (channel=${REPO_CHANNEL}) is already configured"
        fi
    else
        # TODO: Support non-bash environments (but the cloudsmith script only supports bash)
        fail "Bash is missing. Currently this script requires bash to setup the apt repos"
        # deb [signed-by=/usr/share/keyrings/thinedge-tedge-release-archive-keyring.gpg] https://dl.cloudsmith.io/public/thinedge/tedge-release/deb/raspbian bullseye main
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
            c8y-configuration-plugin="$VERSION" \
            c8y-log-plugin="$VERSION" \
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
            c8y-configuration-plugin \
            c8y-log-plugin \
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
        curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
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

    # Install tedge packages in same order as the get-thin-edge_io.sh script
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge_[0-9]*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]mapper_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]agent_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]apt[_-]plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y[_-]configuration[_-]plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y[_-]log[_-]plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y-firmware-plugin_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "tedge[_-]watchdog_*_$ARCH.deb"
    find_then_install_deb "$INSTALL_SOURCEDIR" "c8y-remote-access-plugin*_$ARCH.deb"
}

clean_files() {
    echo "Cleaning up tedge files"
    sudo rm -f /etc/tedge/tedge.toml
    sudo rm -f /etc/tedge/system.toml
    sudo rm -f /var/log/tedge/agent/*.log
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
    configure_repos

    # Check if any packages are incompatible, if so remove the previous version first
    # Use new and old packages names, and check each of them one by one
    packages="tedge tedge-mapper tedge-agent c8y-log-plugin c8y-configuration-plugin c8y-remote-access-plugin"
    packages="$packages tedge tedge_mapper tedge_agent c8y_log_plugin c8y_configuration_plugin"
    REMOVE_BEFORE_INSTALL=0

    for package in $packages; do
        if command_exists "$package"; then
            EXISTING_VERSION=$("$package" --version | tail -1 | cut -d' ' -f2 || true)

            if [ -n "$EXISTING_VERSION" ]; then
                if uses_old_package_name "$VERSION" && uses_new_package_name "$EXISTING_VERSION"; then
                    REMOVE_BEFORE_INSTALL=1
                    break
                fi
            fi
        fi
    done

    if [ "$REMOVE_BEFORE_INSTALL" = 1 ]; then
        echo "Uninstalling tedge before downgrading because the package names changed"
        # TODO: This does not work as the script requires bash and not sh
        curl -sSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh | sudo bash -s remove

        # Preserve c8y settings
        if [ -n "$C8Y_BASEURL" ]; then
            c8y_url=$(parse_domain "$C8Y_BASEURL")
            sudo sh -c "
            echo '[c8y]' > /etc/tedge/tedge.toml;
            echo 'url = \"${c8y_url}\"' >> /etc/tedge/tedge.toml
            "
        else
            sudo rm -f /etc/tedge/tedge.toml
        fi
        sudo rm -f /etc/tedge/system.toml
        # Refresh path, to ensure old binaries are still not being detected
        hash -r
    fi

    if [ "$INSTALL_METHOD" = "apt" ] && uses_old_package_name "$VERSION"; then
        echo "Installing versions older than 0.9.0 is not supported via apt. Using the 'script' install method"
        INSTALL_METHOD="script"
    fi

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
            echo "Installing thin-edge.io using the 'get-thin-edge_io.sh' script"
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
    subjectAltName=DNS:$(hostname), DNS:localhost
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

    openssl x509 -req \
        -in client.csr \
        -CA ca.crt \
        -CAkey ca.key \
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

    systemctl restart mosquitto
}

prompt_value() {
    user_text="$1"
    value="$2"

    if [ "$SHOULD_PROMPT" = 1 ] && [ "$CAN_PROMPT" = 1 ]; then
        printf "\n%s (%s): " "$user_text" "${value:-not set}" >&2
        read -r user_input
        if [ -n "$user_input" ]; then
            value="$user_input"
        fi
    fi
    echo "$value"
}

bootstrap_c8y() {
    # If bootstrapping is called, then it assumes the full bootstrapping
    # needs to be done.

    # Force disconnection of mapper before setting url
    sudo tedge disconnect "$TEDGE_MAPPER" >/dev/null 2>&1 || true

    DEVICE_ID=$(prompt_value "Enter the device.id" "$DEVICE_ID")

    # Remove existing certificate if it does not match
    if tedge cert show >/dev/null 2>&1; then
        echo "Removing existing device certificate"
        sudo tedge cert remove
    fi

    echo "Creating certificate: $DEVICE_ID"
    sudo tedge cert create --device-id "$DEVICE_ID"

    # Cumulocity URL
    C8Y_BASEURL=$(prompt_value "Enter the Cumulocity IoT url" "$C8Y_BASEURL")

    # Normalize url, by stripping url schema
    if [ -n "$C8Y_BASEURL" ]; then
        C8Y_BASEURL=$(parse_domain "$C8Y_BASEURL")
    fi

    echo "Setting c8y.url to $C8Y_BASEURL"
    sudo tedge config set c8y.url "$C8Y_BASEURL"

    C8Y_USER=$(prompt_value "Enter your Cumulocity user" "$C8Y_USER")

    if [ -n "$C8Y_USER" ]; then
        echo "Uploading certificate to Cumulocity using tedge"
        if [ -n "$C8Y_PASSWORD" ]; then
            C8YPASS="$C8Y_PASSWORD" tedge cert upload c8y --user "$C8Y_USER"
        else
            echo ""
            tedge cert upload c8y --user "$C8Y_USER"
        fi
    else
        fail "When manually bootstrapping you have to upload the certificate again as the device certificate is recreated"
    fi

    # Grace period for the server to process the certificate
    # but it is not critical for the connection, as the connection
    # supports automatic retries, but it can improve the first connection success rate
    sleep "$UPLOAD_CERT_WAIT"
}

connect_mappers() {
    # retry connection attempts
    sudo tedge disconnect "$TEDGE_MAPPER" || true

    CONNECT_ATTEMPT=0
    while true; do
        CONNECT_ATTEMPT=$((CONNECT_ATTEMPT + 1))
        if sudo tedge connect "$TEDGE_MAPPER"; then
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
}

display_banner_c8y() {
    echo
    echo "----------------------------------------------------------"
    echo "Device information"
    echo "----------------------------------------------------------"
    echo ""
    echo "tedge.version:   $(tedge --version 2>/dev/null | tail -1 | cut -d' ' -f2)"
    echo "device.id:       ${DEVICE_ID}"
    DEVICE_SEARCH=$(echo "$DEVICE_ID" | sed 's/-/*/g')
    echo "Cumulocity IoT:  https://${C8Y_BASEURL}/apps/devicemanagement/index.html#/assetsearch?filter=*${DEVICE_SEARCH}*"
    echo ""
    echo "----------------------------------------------------------"    
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
    sudo sh -c "echo '%sudo ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/all"
    sudo sh -c "echo 'tedge  ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown' > /etc/sudoers.d/tedge"
}

main() {
    # ---------------------------------------
    # Preparation (clean and disconnect)
    # ---------------------------------------
    # Cleanup device as sometimes the existing state can affect tests
    if [ "$CLEAN" = 1 ]; then
        banner "Preparing device"
        purge_tedge
        clean_files
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
    # Bootstrap
    # ---------------------------------------
    if [ "$BOOTSTRAP" = 1 ]; then
        banner "Bootstrapping device"
        # Check if tedge is installed before trying to bootstrap
        if ! command_exists tedge; then
            fail "Can not bootstrap as tedge is not installed"
        fi

        bootstrap_c8y
    fi

    # ---------------------------------------
    # Connect
    # ---------------------------------------
    if [ "$CONNECT" = 1 ]; then
        banner "Connecting mapper"
        connect_mappers
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

    if [ "$BOOTSTRAP" = 1 ] || [ "$CONNECT" = 1 ]; then
        display_banner_c8y
    fi
}

main
