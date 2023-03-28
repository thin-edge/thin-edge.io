#!/bin/sh
set -e

TYPE=full
TMPDIR=/tmp/tedge
LOGFILE=/tmp/tedge/install.log

# Packages names were changed to confirm to debian naming conventions
# But we should still care about installing older versions. It will still cause
# issues when downgrading as there will be a package name clash as we can't re-release
# older packages
TEDGE=tedge
TEDGE_MAPPER=tedge-mapper
TEDGE_AGENT=tedge-agent
TEDGE_WATCHDOG=tedge-watchdog
TEDGE_APT_PLUGIN=tedge-apt-plugin
C8Y_CONFIGURATION_PLUGIN=c8y-configuration-plugin
C8Y_LOG_PLUGIN=c8y-log-plugin
C8Y_FIRMWARE_PLUGIN=c8y-firmware-plugin
C8Y_REMOTE_ACCESS_PLUGIN=c8y-remote-access-plugin

PURGE_OLD_PACKAGES=

# Set shell used by the script (can be overwritten during dry run mode)
sh_c='sh -c'

usage() {
    cat <<EOF
USAGE:
    get-thin-edge_io [<VERSION>] [--minimal]

ARGUMENTS:
    <VERSION>     Install specific version of thin-edge.io - if not provided installs latest minor release

OPTIONS:
    --minimal   Install only basic set of components - tedge cli and tedge mappers
    --dry-run   Don't install anything, just let me know what it does

EOF
}

log() {
    echo "$@" | tee -a "$LOGFILE"
}

debug() {
    echo "$@" >> "$LOGFILE" 2>&1
}

print_debug() {
    echo
    echo "--------------- machine details ---------------------"
    echo "date:           $(date || true)"
    echo "tedge:          $VERSION"
    echo "Machine:        $(uname -a || true)"
    echo "Architecture:   $(dpkg --print-architecture || true)"
    if command_exists "lsb_release"; then
        DISTRIBUTION=$(lsb_release -a 2>/dev/null | grep "Description" | cut -d: -f2- | xargs)
        echo "Distribution:   $DISTRIBUTION"
    fi
    echo
    echo "--------------- error details ------------------------"

    if [ -f "$LOGFILE" ]; then
        cat "$LOGFILE"
    fi

    echo "------------------------------------------------------"
    echo
}

# Enable print of info if something unexpected happens
trap print_debug EXIT

fail() {
    exit_code="$1"
    shift

    log "Failed to install thin-edge.io"
    echo
    log "Reason: $*"
    log "Please create a ticket using the following link and include the console output"
    log "    https://github.com/thin-edge/thin-edge.io/issues/new?assignees=&labels=bug&template=bug_report.md"

    exit "$exit_code"
}

command_exists() {
	command -v "$@" > /dev/null 2>&1
}

is_dry_run() {
	if [ -z "$DRY_RUN" ]; then
		return 1
	else
		return 0
	fi
}

check_prerequisites() {
    if ! command_exists dpkg; then
        fail 1 "Missing prerequisite: dpkg"
    fi

    if ! command_exists curl && ! command_exists wget; then
        fail 1 "Missing prerequisite: wget or curl"
    fi
}

configure_shell() {
    # Check if has sudo rights or if it can be requested
    user="$(id -un 2>/dev/null || true)"
    sh_c='sh -c'
    if [ "$user" != 'root' ]; then
        if command_exists sudo; then
            sh_c='sudo -E sh -c'
        elif command_exists su; then
            sh_c='su -c'
        else
            cat >&2 <<-EOF
Error: this installer needs the ability to run commands as root.
We are unable to find either "sudo" or "su" available to make this happen.
EOF
            exit 1
        fi
    fi

    if is_dry_run; then
        sh_c="echo"
    fi
}

install_artifact() {
    #
    # Download and install a package using either wget or curl (whatever is available)
    # Usage
    #   install_artifact <name>
    #
    name="$1"
    package_type="deb"

    echo
    printf 'Downloading %s...' "$name"
    filename="${name}_${VERSION}_${ARCH}.${package_type}"
    url="https://github.com/thin-edge/thin-edge.io/releases/download/${VERSION}/${filename}"

    if [ ! -d "$TMPDIR" ]; then
        mkdir -p "$TMPDIR"
    fi

    # Prefer curl over wget as docs instruct the user to download this script using curl
    if command_exists curl; then
        if ! (cd "$TMPDIR" && $sh_c "curl -fsSLO '$url'" >> "$LOGFILE" 2>&1 ); then
            fail 2 "Could not download package from url: $url"
        fi
    elif command_exists wget; then
        if ! $sh_c "wget --quiet '$url' -P '$TMPDIR'" >> "$LOGFILE" 2>&1; then
            fail 2 "Could not download package from url: $url"
        fi
    else
        # This should not happen due to the pre-requisite check
        echo "FAILED"
        fail 1 "Could not download file because neither wget or curl is installed. Please install 'wget' or 'curl' and try again"
    fi
    if is_dry_run; then
        echo "OK (DRY-RUN)"
    else
        echo "OK"
    fi

    printf 'Installing %s...' "$name"
    if $sh_c "dpkg -i '${TMPDIR}/${filename}'" >> "$LOGFILE" 2>&1; then
        if is_dry_run; then
            echo "OK (DRY-RUN)"
        else
            echo "OK"
        fi
    else
        echo "FAILED"
        fail 2 "Failed to install package '$name'"
    fi
}

remove_package() {
    #
    # Remove/purge a package. This should only be
    # used to remove packages which are no longer needed
    #
    name="$1"
    
    if dpkg -s >/dev/null 2>&1; then
        printf 'Purging renamed package %s...' "$name"
        if $sh_c "dpkg --purge $name" >> "$LOGFILE" 2>&1; then
            if is_dry_run; then
                echo "OK (DRY-RUN)"
            else
                echo "OK"
            fi
        else
            echo "FAILED"
            fail 3 "Failed to purge old renamed package '$name'"
        fi
    fi
}

install_basic_components() {
    install_artifact "$TEDGE"
    install_artifact "$TEDGE_MAPPER"

    if [ -n "$PURGE_OLD_PACKAGES" ]; then
        remove_package "tedge_mapper"
    fi
}

install_tedge_agent() {
    install_artifact "$TEDGE_AGENT"

    if [ -n "$PURGE_OLD_PACKAGES" ]; then
        remove_package "tedge_agent"
    fi
}

install_tedge_plugins() {
    install_artifact "$TEDGE_APT_PLUGIN"
    install_artifact "$C8Y_CONFIGURATION_PLUGIN"
    install_artifact "$C8Y_LOG_PLUGIN"
    install_artifact "$TEDGE_WATCHDOG"

    if [ -n "$C8Y_FIRMWARE_PLUGIN" ]; then
        install_artifact "$C8Y_FIRMWARE_PLUGIN"
    fi

    if [ -n "$C8Y_REMOTE_ACCESS_PLUGIN" ]; then
        install_artifact "$C8Y_REMOTE_ACCESS_PLUGIN"
    fi

    if [ -n "$PURGE_OLD_PACKAGES" ]; then
        remove_package "tedge_apt_plugin"
        remove_package "tedge_apama_plugin"
        remove_package "c8y_configuration_plugin"
        remove_package "c8y_log_plugin"
        remove_package "tedge_watchdog"
    fi
}

get_latest_version() {
    # Detect latest version from github api to avoid having a default version in the script
    if command_exists curl; then
        response=$(curl -s https://api.github.com/repos/thin-edge/thin-edge.io/releases/latest)
    elif command_exists wget; then
        response=$(wget -q --output-document - https://api.github.com/repos/thin-edge/thin-edge.io/releases/latest)
    else
        fail 1 "Detecting latest version requires either curl or wget to be installed"
    fi

    # use the same url pattern as expected when downloading the artifacts (so as not to rely on github api response fields)
    version=$(
        echo "$response" \
            | grep -o "https://github.com/thin-edge/thin-edge.io/releases/download/[0-9]\+\.[0-9]\+\.[0-9]\+/.*\.deb" \
            | grep -o "/[0-9]\+.[0-9]\+.[0-9]\+/" \
            | cut -d/ -f2 \
            | head -1
    )

    if [ -z "$version" ]; then
        fail 1 "Failed to detect latest version. You can try specifying an explicit version. Check the help for more details"
    fi
    echo "$version"
}

main() {
    if [ -d "$TMPDIR" ]; then
        rm -Rf "$TMPDIR"
    fi
    mkdir -p "$TMPDIR"

    check_prerequisites
    configure_shell

    ARCH=$(dpkg --print-architecture)

    if [ -z "$VERSION" ]; then
        VERSION="$(get_latest_version)"

        log "Version argument has not been provided, installing latest: $VERSION"
        log "To install a particular version use this script with the version as an argument."
        log "For example: sudo ./get-thin-edge_io.sh $VERSION"
    fi

    if dpkg --compare-versions "$VERSION" le "0.8.1"; then
        # Use older style packages names (with underscore)
        TEDGE=tedge
        TEDGE_MAPPER=tedge_mapper
        TEDGE_AGENT=tedge_agent
        TEDGE_WATCHDOG=tedge_watchdog
        TEDGE_APT_PLUGIN=tedge_apt_plugin
        C8Y_CONFIGURATION_PLUGIN=c8y_configuration_plugin
        C8Y_LOG_PLUGIN=c8y_log_plugin
    else
        # New package names will be installed so activate
        # flag to remove the old packages after the installation
        # Note: No configuration files will be removed as the legacy postrm
        # are removed by the renamed packages
        PURGE_OLD_PACKAGES=1
    fi

    # Ignore plugins for older versions
    if dpkg --compare-versions "$VERSION" lt "0.10.0"; then
        log "ignore c8y-firmware-plugin and c8y-remote-access-plugin as they are not supported in <= 0.10.0"
        C8Y_FIRMWARE_PLUGIN=
        C8Y_REMOTE_ACCESS_PLUGIN=
    fi

    echo "Thank you for trying thin-edge.io!"
    echo

    if [ "$ARCH" = "aarch64" ] || [ "$ARCH" = "arm64" ] || [ "$ARCH" = "armhf" ] || [ "$ARCH" = "amd64" ]; then
        # Some OSes may read architecture type as `aarch64`, `aarch64` and `arm64` are the same architectures types.
        if [ "$ARCH" = "aarch64" ]; then
            ARCH='arm64'
        fi

        # For arm64, only the versions above 0.3.0 are available.
        if [ "$ARCH" = "arm64" ] && ! dpkg --compare-versions "$VERSION" ge "0.3.0"; then
            log "aarch64/arm64 compatible packages are only available for version 0.3.0 or above."
            exit 1
        fi

        log "Installing for architecture $ARCH"
    else
        log "$ARCH is currently not supported. Currently supported are aarch64/arm64, armhf and amd64."
        exit 0
    fi

    if ! command_exists mosquitto; then
        log "Installing mosquitto as prerequisite for thin-edge.io"

        # Lazily check apt-get dependency
        if command_exists apt-get; then
            export DEBIAN_FRONTEND=noninteractive
            $sh_c "apt-get -y install mosquitto" >> "$LOGFILE"
        elif command_exists apk; then
            $sh_c "apk add mosquitto"
        else
            fail 1 "Missing prerequisite: 'apt-get' or 'apk'"
        fi
    fi

    case "$TYPE" in
    minimal) install_basic_components ;;
    full)
        install_basic_components
        install_tedge_agent
        install_tedge_plugins
        ;;
    *)
        log "Unsupported argument type."
        exit 1
        ;;
    esac


    if is_dry_run; then
        echo
        echo "Dry run complete"
    # Test if tedge command is there and working
    elif tedge help >/dev/null; then
        # remove error handler
        trap - EXIT

        # Only delete when everything was ok to help with debugging
        rm -Rf "$TMPDIR"

        echo
        echo "thin-edge.io is now installed on your system!"
        echo
        echo "You can go to our documentation to find next steps: https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/howto-guides/003_registration.md"
    else
        echo "Something went wrong in the installation process please try the manual installation steps instead:"
        echo "https://github.com/thin-edge/thin-edge.io/blob/main/docs/src/howto-guides/002_installation.md"
    fi
}

DRY_RUN=${DRY_RUN:-}
VERSION=

if [ $# -lt 4 ]; then
    while :; do
        case $1 in
        --minimal)
            TYPE="minimal"
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        *)
            if [ -z "$1" ]; then
                break
            fi

            if [ -n "$VERSION" ]; then
                break
            fi            

            VERSION="$1"
            
            shift $(( $# > 0 ? 1 : 0 ))
            break
            ;;
        esac
    done
else
    usage
    exit 0
fi

# wrapped up in a function so that we have some protection against only getting
# half the file during "curl | sh"
main
