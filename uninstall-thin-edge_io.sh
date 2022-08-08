#!/bin/sh

set -e

usage() {
    cat <<EOF
USAGE:
    delete-thin-edge_io [COMMAND]
    
COMMANDS:
    remove     Uninstall thin-edge.io with keeping configuration files
    purge      Uninstall thin-edge.io and also remove configuration files  

EOF
}

stop_a_service_if_running() {
    status=$(sudo systemctl is-active "$1") && returncode=$? || returncode=$?
    if [ "$status" = "active" ]; then
        sudo systemctl stop "$1"
    fi
}

stop_extension_services() {
    stop_a_service_if_running "tedge-watchdog.service"
    stop_a_service_if_running "tedge-mapper-collectd.service"
    stop_a_service_if_running "c8y-log-plugin.service"
    stop_a_service_if_running "c8y-configuration-plugin.service"
}

remove_or_purge_package_if_exists() {
    status=$(dpkg -s "$2" | grep -w installed) && returncode=$? || returncode=$?
    if [ "$status" = "Status: install ok installed" ]; then
        sudo apt --assume-yes "$1" "$2"
    fi
}

# Here don't need to remove the tedge_mapper, tedge_agent, and tedge_watchdog packages explicitly,
# as they will be removed by removing the tedge package.
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
    echo "remove thin-edge_io"
    disconnect_from_cloud    
    stop_extension_services  
    remove_packages  
}

purge_packages() {
    remove_or_purge_package_if_exists "purge" "tedge"
    remove_or_purge_package_if_exists "purge" "tedge_apt_plugin"
    remove_or_purge_package_if_exists "purge" "tedge_apama_plugin"
    remove_or_purge_package_if_exists "purge" "c8y_log_plugin"
    remove_or_purge_package_if_exists "purge" "c8y_configuration_plugin"
}

# Here don't need to purge the tedge_mapper, tedge_agent, and tedge_watchdog packages explicitly,
# as they will be removed by removing the tedge package.
purge_thin_edge_io() {
    echo "purge thin-edge_io"
    disconnect_from_cloud
    stop_extension_services
    purge_packages

    # if in case the configs are not removed then its better to remove.
    if [ -d "/etc/tedge" ]; then
        sudo rm -rf /etc/tedge
    fi

}

if [ $# -eq 1 ]; then
    DELETE_OR_PURGE=$1
    if [ "$DELETE_OR_PURGE" = 'remove' ]; then
        remove_thin_edge_io
    elif [ "$DELETE_OR_PURGE" = 'purge' ]; then
        purge_thin_edge_io
    fi
else
    usage
fi
