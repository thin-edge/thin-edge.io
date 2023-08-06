#!/bin/sh
set -e

TYPE=full
TMPDIR=/tmp/tedge
LOGFILE=/tmp/tedge/install.log
# REPO_CHANNEL="${REPO_CHANNEL:-release}"
REPO_CHANNEL="${REPO_CHANNEL:-dev}"
INSTALL_PREDEPENDS="${INSTALL_PREDEPENDS:-1}"
PREDEPENDS_PACKAGES=

# TODO
# * Support installing a specific version - check if this is really required?
#

# Set shell used by the script (can be overwritten during dry run mode)
sh_c='sh -c'

usage() {
    cat <<EOF
USAGE:
    $0 [<VERSION>] [--minimal] [--package-manager <apt|apk|dnf|microdnf|zypper|tarball>]

ARGUMENTS:
    <VERSION>     Install specific version of thin-edge.io - if not provided installs latest minor release

OPTIONS:
    --minimal                     Install only basic set of components - tedge cli and tedge mappers
    --channel string              Repository channel to use, e.g. official 'release', or 'main' etc.
                                  Available: release, main
    -p, --package-manager string  Package manager to use to install thin-edge.io. Defaults to auto detection
                                  Available: apt, apk, dnf, microdnf, zypper, tarball
    --dry-run                     Don't install anything, just let me know what it does
    --help                        Show this help

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
    echo "Architecture:   $(dpkg --print-architecture 2>/dev/null || true)"
    if command_exists "lsb_release"; then
        DISTRIBUTION=$(lsb_release -a 2>/dev/null | grep "Description" | cut -d: -f2- | xargs)
        echo "Distribution:   $DISTRIBUTION"
    elif [ -f /etc/os-release ]; then
        echo "os-release details:"
        head -4 /etc/os-release
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

should_install_predepends() {
    [ "$INSTALL_PREDEPENDS" = 1 ]
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

install_via_tarball() {
    #
    # Install tarballs 
    #
    url_arch=""
    ARCH=$(uname -m)
    case "$ARCH" in
        *86_64*|*amd64*)
            url_arch="amd64"
            ;;
        *aarch64*|*arm64*)
            url_arch="arm64"
            ;;
        *armv7*)
            url_arch="arm64"
            ;;
        *armv6*)
            url_arch="armv6"
            ;;
        *)
            fail 1 "Unsupported architecture: $ARCH. Supported architectures are: amd64, arm64, armv7, armv6"
            ;;
    esac

    # Download tarball
    download_file "https://dl.cloudsmith.io/public/thinedge/${PACKAGE_REPO}/raw/names/tedge-${url_arch}/versions/latest/tedge.tar.gz"
    DOWNLOADED_TARBALL="$TMPDIR/tedge.tar.gz"

    # Prefer gtar over tar, as gtar is guranteed to be GNU tar
    # where tar could also be bsdtar which has different options
    # and some systems only have gtar (e.g. rockylinux 9 minimal)
    tar_cmd="tar"
    if command_exists gtar; then
        tar_cmd="gtar"
    fi

    log "Expanding tar: $DOWNLOADED_TARBALL $*"
    $sh_c "$tar_cmd xzf '$DOWNLOADED_TARBALL' -C /usr/bin/ $*"

    # Run manual initializations
    log "Running post installation tasks"
    tarball_postinstall
}

download_file() {
    #
    # Download a file either using curl or wget (whatever is available)
    # The file is downloaded to the temp directory
    #
    # Usage
    #   download_file <url>
    #
    url="$1"

    echo
    printf 'Downloading %s...' "$url"

    if [ ! -d "$TMPDIR" ]; then
        mkdir -p "$TMPDIR"
    fi

    # Prefer curl over wget as docs instruct the user to download this script using curl
    if command_exists curl; then
        if ! (cd "$TMPDIR" && $sh_c "curl -1fsSLO '$url'" >> "$LOGFILE" 2>&1 ); then
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
}

is_root() {
    user="$(id -un 2>/dev/null || true)"
    [ "$user" = "root" ]
}

run_repo_setup() {
    filename="$1"

    # Use generic distribution version and codement to be compatable with different OSs
    if is_root; then
        env version=any-version codename="" bash "$filename"
    elif command_exists sudo; then
        sudo -E env version=any-version codename="" bash "$filename"
    fi
}

install_via_apk() {
    if should_install_predepends && [ -n "$PREDEPENDS_PACKAGES" ]; then
        $sh_c "apk add --no-cache $PREDEPENDS_PACKAGES"
    fi
    download_file "https://dl.cloudsmith.io/public/thinedge/${PACKAGE_REPO}/setup.apk.sh"
    run_repo_setup "$TMPDIR/setup.apk.sh"
    $sh_c "apk add --no-cache $*"
}

install_via_apt() {
    if should_install_predepends && [ -n "$PREDEPENDS_PACKAGES" ]; then
        $sh_c "apt-get update && apt-get install -y $PREDEPENDS_PACKAGES"
    fi
    download_file "https://dl.cloudsmith.io/public/thinedge/${PACKAGE_REPO}/setup.deb.sh"
    run_repo_setup "$TMPDIR/setup.deb.sh"

    $sh_c "DEBIAN_FRONTEND=noninteractive apt-get install --no-install-recommends -y $*"
}

install_via_dnf() {
    if should_install_predepends && [ -n "$(dnf repoquery epel-release --refresh 2>/dev/null)" ]; then
        log "Adding epel-release"
        $sh_c "dnf --setopt=install_weak_deps=0 install -y epel-release"
    fi

    if should_install_predepends && [ -n "$PREDEPENDS_PACKAGES" ]; then
        $sh_c "dnf install -y $PREDEPENDS_PACKAGES"
    fi
    download_file "https://dl.cloudsmith.io/public/thinedge/${PACKAGE_REPO}/setup.rpm.sh"
    run_repo_setup "$TMPDIR/setup.rpm.sh"

    $sh_c "dnf install --best --refresh -y $*"
}

install_via_microdnf() {
    if should_install_predepends && [ -n "$(microdnf repoquery epel-release --refresh 2>/dev/null)" ]; then
        log "Adding epel-release"
        $sh_c "microdnf --setopt=install_weak_deps=0 install -y epel-release"
    fi

    if should_install_predepends && [ -n "$PREDEPENDS_PACKAGES" ]; then
        $sh_c "microdnf install -y $PREDEPENDS_PACKAGES"
    fi
    download_file "https://dl.cloudsmith.io/public/thinedge/${PACKAGE_REPO}/setup.rpm.sh"
    run_repo_setup "$TMPDIR/setup.rpm.sh"

    $sh_c "microdnf install --best --refresh -y $*"
}

install_via_zypper() {
    if should_install_predepends && [ -n "$PREDEPENDS_PACKAGES" ]; then
        $sh_c "zypper install -y $PREDEPENDS_PACKAGES"
    fi
    download_file "https://dl.cloudsmith.io/public/thinedge/${PACKAGE_REPO}/setup.rpm.sh"
    run_repo_setup "$TMPDIR/setup.rpm.sh"

    $sh_c "zypper install -y $*"
}

group_exists() {
    name="$1"
    if command_exists getent; then
        getent group "$name" >/dev/null
    else
        # Fallback to plain grep, as busybox does not have getent
        grep -q "^${name}:" /etc/group
    fi
}

user_exists() {
    name="$1"
    if command_exists getent; then
        getent passwd "$name" >/dev/null
    else
        # Fallback to plain grep, as busybox does not have getent
        grep -q "^${name}:" /etc/passwd
    fi
}

tarball_postinstall() {
    ### Create groups
    if ! group_exists tedge; then
        if command_exists groupadd; then
            groupadd --system tedge
        elif command_exists addgroup; then
            addgroup -S tedge
        else
            echo "WARNING: Could not create group: tedge" >&2
        fi
    fi

    ### Create users
    # Create user tedge with no home(--no-create-home), no login(--shell) and in group tedge(--gid)
    if ! user_exists tedge; then
        if command_exists useradd; then
            useradd --system --no-create-home --shell /sbin/nologin --gid tedge tedge
        elif command_exists adduser; then
            adduser -g "" -H -D tedge -G tedge
        else
            echo "WARNING: Could not create user: tedge" >&2
        fi
    fi

    ### Create file in /etc/sudoers.d directory. With this configuration, the tedge user have the right to call the tedge command with sudo rights, which is required for system-wide configuration in "/etc/tedge"
    if [ -d /etc/sudoers.d ]; then
        echo "tedge  ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init" >/etc/sudoers.d/tedge

        if [ -f "/etc/sudoers.d/010_pi-nopasswd" ]; then
            echo "tedge   ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init" >/etc/sudoers.d/tedge-nopasswd
        fi
    fi

    ### Add include to mosquitto.conf so tedge specific conf will be loaded
    if [ -f /etc/mosquitto/mosquitto.conf ]; then
        if ! grep -q "include_dir /etc/tedge/mosquitto-conf" "/etc/mosquitto/mosquitto.conf"; then
            # Insert `include_dir /etc/tedge/mosquitto-conf` before any `include_dir`
            # directive so that all other partial conf files inherit the
            # `per_listener_settings` defined in /etc/tedge/mosquitto-conf.
            # `per_listener_settings` has to be defined once, before other listener
            # settings or else it causes the following error:
            #
            # Error: per_listener_settings must be set before any other security
            # settings.
            # Match any included_dir directive as different distributions have different default settings:
            #  On Fedora: `#include_dir`. mosquitto does not use a /etc/mosquitto/conf.d folder
            #  On Debian: `include_dir /etc/mosquitto/conf.d`. Uses a conf.d folder, so the tedge setting must be before this

            # TODO: Check if the conf.d style should be normalized on other non-debian systems...
            # it might be better as it allows users to add and remove stuff by just using a symlink.

            # Check if `include_dir` or `#include_dir` (as the latter could be a future problem if the user uncomments it)
            if grep -qE '^#?include_dir' /etc/mosquitto/mosquitto.conf; then
                # insert tedge include_dir before the first `included_dir` (but only the first!)
                mosquitto_conf=$(awk '!found && /^#?include_dir/ \
                { print "include_dir /etc/tedge/mosquitto-conf"; found=1 }1' \
                /etc/mosquitto/mosquitto.conf)
                echo "$mosquitto_conf" > /etc/mosquitto/mosquitto.conf
            else
                # config does not contain any include_dir directive, so we can safely append it
                echo "include_dir /etc/tedge/mosquitto-conf" >> /etc/mosquitto/mosquitto.conf
            fi
        fi
    fi

    # Initialize the tedge
    tedge init ||:

    if command_exists c8y-remote-access-plugin; then
        c8y-remote-access-plugin --init ||:
    fi

    # TODO: Remove this as tedge should work out of the box without a /run/lock directory
    if [ -d /run ] && [ ! -d /run/lock ]; then
        $sh_c "mkdir --mode 1777 /run/lock"
    fi
}

get_package_manager() {
    package_manager=
    if command_exists apt-get; then
        package_manager="apt"
    elif command_exists apk; then
        package_manager="apk"
    elif command_exists dnf; then
        package_manager="dnf"
    elif command_exists microdnf; then
        package_manager="microdnf"
    elif command_exists zypper; then
        package_manager="zypper"
    fi

    echo "$package_manager"
}

try_install_dependencies() {
    case "$(get_package_manager)" in
        apk)
            $sh_c "apk add --no-cache $*"
            ;;
        apt)
            $sh_c "apt-get update"
            $sh_c "apt-get install --no-install-recommends -y $*"
            ;;
        dnf)
            $sh_c "dnf install -y $*"
            ;;
        microdnf)
            $sh_c "microdnf install -y $*"
            ;;
        zypper)
            $sh_c "zypper install -y $*"
            ;;
        *)
            debug "Package manager ($package_manager) does not support installing extra packages. Trying to continue anyway"
            ;;
    esac
}

configure_pre_depends() {
    # Some distributions have curl-minimal installed, which then causes
    # problems when trying to install the "curl" package. So only install it if the command is not available
    PREDEPENDS_PACKAGES="ca-certificates"
    if ! command_exists curl && ! command_exists wget; then
        PREDEPENDS_PACKAGES="$PREDEPENDS_PACKAGES curl"
    fi
}

main() {
    if [ -d "$TMPDIR" ]; then
        rm -Rf "$TMPDIR"
    fi
    mkdir -p "$TMPDIR"

    configure_pre_depends
    configure_shell

    echo "Thank you for trying thin-edge.io!"
    echo

    ARCH=$(uname -m)
    case "$ARCH" in
        *armv6*)
            PACKAGE_REPO="tedge-${REPO_CHANNEL}-armv6"
            ;;
        *)
            PACKAGE_REPO="tedge-${REPO_CHANNEL}"
            ;;
    esac

    # Detect package manager
    if [ -z "$PACKAGE_MANAGER" ]; then
        package_manager=$(get_package_manager)
        PACKAGE_MANAGER="$package_manager"
        if [ -z "$package_manager" ]; then
            package_manager="tarball"
        fi
    fi

    # Fallback to tarball if bash is not available
    if [ "$PACKAGE_MANAGER" != "tarball" ] && ! command_exists bash; then
        log "Fallback to installing from tarball as bash was not found. bash is required to install thin-edge.io using a package manager"
        PACKAGE_MANAGER="tarball"
    fi

    ONLY_INCLUDE_BINARIES=
    case "$TYPE" in
        minimal)
            PACKAGES="tedge-minimal"
            ONLY_INCLUDE_BINARIES="tedge tedge-mapper"
            ;;
        full)
            PACKAGES="tedge-full"
            # An empty value will include all binaries
            ONLY_INCLUDE_BINARIES=""
            ;;
        *)
            log "Unsupported argument type."
            exit 1
            ;;
    esac

    log "Detected package manager: $PACKAGE_MANAGER"

    case "$PACKAGE_MANAGER" in
        tarball)
            # Note: If binaries are empty, then all included binaries are extracted
            # Also install mosquitto if possible
            # shellcheck disable=SC2086
            try_install_dependencies mosquitto $PREDEPENDS_PACKAGES
            # shellcheck disable=SC2086
            install_via_tarball $ONLY_INCLUDE_BINARIES
            ;;
        apk)
            install_via_apk $PACKAGES
            ;;
        apt)
            install_via_apt $PACKAGES
            ;;
        dnf)
            install_via_dnf $PACKAGES
            ;;
        microdnf)
            install_via_microdnf $PACKAGES
            ;;
        zypper)
            install_via_zypper $PACKAGES
            ;;
        *)
            # Should only happen if there is a bug in the script
            fail 1 "Unknown package manager: $PACKAGE_MANAGER"
            ;;
    esac

    if is_dry_run; then
        echo
        echo "Dry run complete"
    # Test if tedge command is there and working
    elif tedge help >/dev/null 2>&1; then
        # remove error handler
        trap - EXIT

        # Only delete when everything was ok to help with debugging
        rm -Rf "$TMPDIR"

        echo
        echo "thin-edge.io is now installed on your system!"
        echo
        echo "You can go to our documentation to find next steps: https://thin-edge.github.io/thin-edge.io/start/getting-started"
    else
        echo "Something went wrong in the installation process please try the manual installation steps instead:"
        echo "https://thin-edge.github.io/thin-edge.io/install/"
    fi
}

# Support reading setting from environment variables
DRY_RUN=${DRY_RUN:-}
VERSION=${VERSION:-}
PACKAGE_MANAGER="${PACKAGE_MANAGER:-}"

while [ $# -gt 0 ]; do
    case $1 in
        --minimal)
            TYPE="minimal"
            ;;
        # Allow user to specify which packages manager they would like
        # e.g. they might opt for the 'tarball' option even if they are on debian
        --package-manager|-p)
            PACKAGE_MANAGER="$2"
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            ;;
        --channel)
            REPO_CHANNEL="$2"
            shift
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        --*|-*)
            log "Unknown option: $1"
            usage
            exit 1
            ;;
        *)
            VERSION="$1"
            ;;
    esac
    shift $(( $# > 0 ? 1 : 0 ))
done

# wrapped up in a function so that we have some protection against only getting
# half the file during "curl | sh"
main
