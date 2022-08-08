#!/bin/bash

set -euo pipefail

usage() {
    cat <<EOF
USAGE:
    delete_thin_edge [remove/purge]
EOF
}

stop_a_service_if_running() {
    local_status=$(systemctl show -p ActiveState --value "$1")
    if [ "$local_status" == "active" ]; then
        sudo systemctl stop "$1"
    fi
}

stop_services() {
    stop_a_service_if_running "tedge-watchdog.service"
    stop_a_service_if_running "tedge-mapper-collectd.service"
    stop_a_service_if_running "c8y_log_plugin"
    stop_a_service_if_running "c8y_configuration_plugin"
    stop_a_service_if_running "apama"
}

remove_or_purge_package_if_exists() {
    local_status=$(dpkg -s "$2" | grep -w installed) && returncode=$? || returncode=$?
    if [ "$local_status" == "Status: install ok installed" ]; then
        sudo apt --assume-yes "$1" "$2"
    fi
}

remove_packages() {
    remove_or_purge_package_if_exists "remove" "tedge"
    remove_or_purge_package_if_exists "remove" "tedge_apt_plugin"
    remove_or_purge_package_if_exists "remove" "tedge_apama_plugin"
    remove_or_purge_package_if_exists "remove" "c8y_log_plugin"
    remove_or_purge_package_if_exists "remove" "c8y_configuration_plugin"
}

disconnect_if_connected_to_cloud() {
    if [ -f "/etc/tedge/mosquitto-conf/$1-bridge.conf" ]; then
        sudo tedge disconnect "$1"
    fi
}

disconnect_from_cloud() {
    disconnect_if_connected_to_cloud "c8y"
    disconnect_if_connected_to_cloud "az"
}

remove_thin_edge_io() {
    echo "remove thin_edge_io"
    disconnect_from_cloud
    stop_services
    remove_packages
}

purge_thin_edge_io() {
    echo "purge thin-edge-io"
    remove_or_purge_package_if_exists "purge" "tedge"
    remove_or_purge_package_if_exists "purge" "tedge_apt_plugin"
    remove_or_purge_package_if_exists "purge" "tedge_apama_plugin"
    remove_or_purge_package_if_exists "purge" "c8y_log_plugin"
    remove_or_purge_package_if_exists "purge" "c8y_configuration_plugin"
    sudo DEBIAN_FRONTEND=noninteractive apt --assume-yes purge mosquitto-clients mosquitto libmosquitto1 collectd-core collectd

    # if in case the configs are not removed then its better to remove.
    if [ -d "/etc/tedge" ]; then
        sudo rm -rf /etc/tedge
    fi
}

if [ $# -eq 1 ]; then
    DELETE_OR_PURGE=$1
    if [ "$DELETE_OR_PURGE" == 'remove' ]; then
        remove_thin_edge_io
    elif [ "$DELETE_OR_PURGE" == 'purge' ]; then
        purge_thin_edge_io
    fi
else
    usage
fi
