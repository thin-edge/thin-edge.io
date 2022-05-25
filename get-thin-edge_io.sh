#!/bin/sh
set -e

TYPE=${2:-full}

usage() {
    cat <<EOF
USAGE:
    get-thin-edge_io [<VERSION>] [--minimal]

ARGUMENTS:
    <VERSION>     Install specific version of thin-edge.io - if not provided installs latest minor release

OPTIONS:
    --minimal   Install only basic set of components - tedge cli and tedge mappers

EOF
}

install_basic_components() {
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/tedge_${VERSION}_${ARCH}.deb -P /tmp/tedge
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/tedge_mapper_${VERSION}_${ARCH}.deb -P /tmp/tedge

    dpkg -i /tmp/tedge/tedge_${VERSION}_${ARCH}.deb
    dpkg -i /tmp/tedge/tedge_mapper_${VERSION}_${ARCH}.deb

}

install_tedge_agent() {
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/tedge_agent_${VERSION}_${ARCH}.deb -P /tmp/tedge

    dpkg -i /tmp/tedge/tedge_agent_${VERSION}_${ARCH}.deb
}

install_tedge_plugins() {
    # Download and install apt plugin
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/tedge_apt_plugin_${VERSION}_${ARCH}.deb -P /tmp/tedge
    dpkg -i /tmp/tedge/tedge_apt_plugin_${VERSION}_${ARCH}.deb

    # Download and install apama plugin
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/tedge_apama_plugin_${VERSION}_${ARCH}.deb -P /tmp/tedge
    dpkg -i /tmp/tedge/tedge_apama_plugin_${VERSION}_${ARCH}.deb
    
    # Download and install configuration plugin
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/c8y_configuration_plugin_${VERSION}_${ARCH}.deb -P /tmp/tedge
    dpkg -i /tmp/tedge/c8y_configuration_plugin_${VERSION}_${ARCH}.deb

    # Download and install c8y log plugin
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/c8y_log_plugin_${VERSION}_${ARCH}.deb -P /tmp/tedge
    dpkg -i /tmp/tedge/c8y_log_plugin_${VERSION}_${ARCH}.deb

    # Download and install tedge_watchdog
    wget https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/tedge_watchdog_${VERSION}_${ARCH}.deb -P /tmp/tedge
    dpkg -i /tmp/tedge/tedge_watchdog_${VERSION}_${ARCH}.deb
}

if [ $# -lt 3 ]; then
    while :; do
        case $1 in
        --minimal)
            TYPE="minimal"
            shift
            ;;
        *) break ;;
        esac
    done
else
    usage
    exit 0
fi

VERSION=$1
ARCH=$(dpkg --print-architecture)

BLUE=''
COLORRESET=''
if [ -t 1 ]; then
    if [ "$(tput colors)" -ge 8 ]; then
        BLUE='\033[1;34m'
        COLORRESET='\033[0m'
    fi
fi

echo "${BLUE}Thank you for trying thin-edge.io! ${COLORRESET}\n"

if [ -z "$VERSION" ]; then
    VERSION=0.7.0

    echo "Version argument has not been provided, installing latest: ${BLUE}$VERSION${COLORRESET}"
    echo "To install a particular version use this script with the version as an argument."
    echo "For example: ${BLUE}sudo ./get-thin-edge_io.sh $VERSION${COLORRESET}"
fi

if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ] || [ "$ARCH" = "armhf" ] || [ "$ARCH" = "amd64" ]; then
    # Some OSes may read architecture type as `aarch64`, `aarch64` and `arm64` are the same architectures types.
    if [ "$ARCH" = "aarch64" ]; then
        ARCH='arm64'
    fi

    # For arm64, only the versions above 0.3.0 are available.
    if [ "$ARCH" = "arm64" ] && ! dpkg --compare-versions "$VERSION" ge "0.3.0"; then
        echo "aarch64/arm64 compatible packages are only available for version 0.3.0 or above."
        exit 1
    fi

    echo "${BLUE}Installing for architecture $ARCH ${COLORRESET}"
else
    echo "$ARCH is currently not supported. Currently supported are aarch64/arm64, armhf and amd64."
    exit 0
fi

if [ -d "/tmp/tedge" ]; then
    rm -R /tmp/tedge
fi

echo "${BLUE}Installing mosquitto as prerequirement for thin-edge.io${COLORRESET}"
apt install mosquitto -y

case $TYPE in
minimal) install_basic_components ;;
full)
    install_basic_components
    install_tedge_agent
    if apt -v &>/dev/null; then
        install_tedge_plugins
    fi
    ;;
*)
    echo "Unsupported argument type."
    exit 1
    ;;
esac

rm -R /tmp/tedge

# Test if tedge command is there and working
tedge help >/dev/null
if [ $? -eq 0 ]; then
    echo "\n${BLUE}thin-edge.io is now installed on your system!${COLORRESET}"
    echo ""
    echo "You can go to our documentation to find next steps:${BLUE} https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/howto-guides/003_registration.md ${COLORRESET}"
else
    echo "Something went wrong in the installation process please try the manual installation steps instead:\n https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/howto-guides/002_installation.md"
fi
