*** Comments ***
# Command to execute:    robot -d \results --timestampoutputs --log inotify_crate.html --report NONE --variable HOST:192.168.1.130 /thin-edge.io/tests/RobotFramework/config_management/inotify_crate.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             Collections
Library             ThinEdgeIO
Library             Cumulocity

Test Teardown       Get Logs
Test Timeout        5 minutes

Test Tags           theme:configuration    theme:plugins


*** Variables ***
${toml}     SEPARATOR=\n
...         files = [
...         { path = '/etc/tedge/tedge.toml', type = 'tedge.toml'},
...         { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },
...         { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },
...         { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' },
...         { path = '/etc/tedge/c8y/example.txt', type = 'example', user = 'tedge', group = 'tedge', mode = 0o444 }
...         ]


*** Test Cases ***
Configuration types should be detected on file change (without restarting service)
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}

    ${supported_configs}=    Should Contain Supported Configuration Types    tedge-configuration-plugin
    Should Not Contain    ${supported_configs}    example

    Execute Command    sudo rm -f /etc/tedge/plugins/tedge-configuration-plugin.toml
    Execute Command    sudo printf '%s' "${toml}" > tedge-configuration-plugin.toml
    Execute Command    sudo mv tedge-configuration-plugin.toml /etc/tedge/plugins/

    ${supported_configs}=    Should Have Exact Supported Configuration Types
    ...    c8y-bridge.conf
    ...    tedge-configuration-plugin
    ...    mosquitto.conf
    ...    tedge-mosquitto.conf
    ...    tedge.toml
    ...    example
