---
title: Operate Devices
tags: [Operate]
sidebar_position: 3
---

# How-to Guides

## Installation
- [How to install thin-edge.io?](installation/install.md)
- [How to install thin-edge.io on any Linux OS (no deb support)?](installation/installation_without_deb_support.md)
- [How to install thin-edge manually with openrc?](installation/how_to_install_thin_edge_manually.md)
- [How to install and enable software management?](installation/install_and_enable_software_management.md)

## Configuration
- [How to configure thin-edge.io?](configuration/config.md)
- [How to configure the local mqtt bind address and port?](configuration/config_local_mqtt_bind_address_and_port.md)
- [How to add self-signed certificate root to trusted certificates list?](security/add_self_signed_trusted.md)
- [How to use thin-edge.io with your preferred init system](configuration/how_to_use_preferred_init_system.md)
- [How to enable watchdog using systemd?](monitoring/enable_tedge_watchdog_using_systemd.md)
- [How to change temp path?](configuration/update_config_paths.md)
- [How to set up client/server authentication for the local MQTT broker](security/mqtt_local_broker_authentication.md)

## Cloud connection
- [How to create a test certificate?](security/registration.md)
- [How to connect a cloud end-point?](connection/connect.md)
- [How to test the cloud connection?](troubleshooting/test_connection.md)

## Operating the device
- [How to access the logs on the device?](troubleshooting/thin_edge_logs.md)
- [How to restart your thin-edge.io device?](operations/restart_device_operation.md)

## Telemetry
- [How to use `tedge mqtt` module?](telemetry/pub_sub.md)
- [How to connect an external device?](monitoring/connect_external_device.md)

## Monitoring
- [How to monitor health of tedge daemons?](troubleshooting/monitor_tedge_health.md)
- [How to trouble shoot device monitoring?](troubleshooting/trouble_shooting_monitoring.md)

## Device Management with Cumulocity

- [How to add custom fragments to Cumulocity?](c8y/c8y_fragments.md)
- [How to retrieve logs with the log plugin?](c8y/c8y_log_plugin.md)
- [How to add C8Y SmartRest Templates?](c8y/smartrest_templates.md)
- [How to manage configuration files with Cumulocity?](c8y/config_management_plugin.md)
- [How to enable configuration management on child devices?](c8y/child_device_config_management_agent.md)
- [How to enable firmware management on child devices](c8y/child_device_firmware_management.md)
- [How to remotely connect to your thin-edge.io device via SSH/VNC/Telnet with Cumulocity remote access?](c8y/remote_access_with_cumulocity.md)
- [How to monitor a service from Cumulocity?](c8y/c8y_service_monitoring.md)
- [How to manage apama software artifacts with apama plugin?](c8y/apama_software_management_plugin.md)
- [How to retrieve JWT token from Cumulocity?](c8y/retrieve_jwt_token_from_cumulocity.md)
