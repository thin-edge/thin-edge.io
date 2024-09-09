#!/bin/sh

set -e

# Package names for version <= 0.8.1
packages="tedge tedge_apt_plugin tedge_apama_plugin c8y_log_plugin c8y_configuration_plugin tedge_mapper tedge_agent tedge_watchdog"

# Package names for version > 0.8.1
packages="$packages tedge-apt-plugin tedge-apama-plugin c8y-log-plugin tedge-log-plugin c8y-configuration-plugin tedge-configuration-plugin c8y-remote-access-plugin c8y-firmware-plugin tedge-watchdog tedge-agent tedge-mapper"

extension_services="tedge-watchdog.service tedge-mapper-collectd.service tedge-log-plugin.service c8y-log-plugin.service c8y-configuration-plugin.service tedge-configuration-plugin.service c8y-firmware-plugin.service"

clouds="c8y az aws"

installation_paths="/usr/local/bin /usr/bin /opt/tedge"

usage() {
    cat <<EOF
USAGE:
    uninstall-thin-edge_io.sh [COMMAND]
    
COMMANDS:
    remove     Uninstall thin-edge.io with keeping configuration files
    purge      Uninstall thin-edge.io and also remove configuration files  

EOF
}

disconnect_from_cloud() {
    for cloud in $clouds; do
        if [ -f "/etc/tedge/mosquitto-conf/$cloud-bridge.conf" ]; then
            sudo tedge disconnect "$cloud"
        fi
    done
}

stop_extension_services() {
    for service in $extension_services; do
        status=$(sudo systemctl is-active "$service") || true
        if [ "$status" = "active" ]; then
            sudo systemctl stop "$service"
        fi
    done
}

terminate_tedge_processes() {
    pkill -u tedge || true
}

remove_or_purge_package_if_exists() {
    disconnect_from_cloud
    stop_extension_services
    terminate_tedge_processes
    for package in $packages; do
        if dpkg -s "$package" >/dev/null 2>&1; then
            sudo apt --assume-yes "$1" "$package"
        fi
    done
}

remove_installed_files() {
    for path in $installation_paths; do
        if [ -d "$path" ]; then
            sudo rm -rf "$path/tedge"
            sudo rm -rf "$path/tedge_*"
        fi
    done
    sudo rm -rf /etc/tedge
    sudo rm -rf /var/log/tedge
    sudo rm -rf /var/lib/tedge
    sudo rm -rf /opt/tedge
}

if [ $# -eq 1 ]; then
    DELETE_OR_PURGE=$1
    if [ "$DELETE_OR_PURGE" = 'remove' ]; then
        remove_or_purge_package_if_exists "remove"
    elif [ "$DELETE_OR_PURGE" = 'purge' ]; then
        remove_or_purge_package_if_exists "purge"
        remove_installed_files
    fi
else
    usage
fi
