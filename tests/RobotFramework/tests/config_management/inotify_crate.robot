#Command to execute:    robot -d \results --timestampoutputs --log inotify_crate.html --report NONE --variable HOST:192.168.1.130 /thin-edge.io/tests/RobotFramework/config_management/inotify_crate.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO
Library    Cumulocity
Library    Collections

Test Tags    theme:configuration    theme:plugins
Test Teardown    Get Logs

*** Variables ***
${toml}    SEPARATOR=\n
...       files = [
...           { path = '/etc/tedge/tedge.toml', type = 'tedge.toml'},
...           { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },
...           { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },
...           { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' },
...           { path = '/etc/tedge/c8y/example.txt', type = 'example', user = 'tedge', group = 'tedge', mode = 0o444 }
...       ]

*** Test Cases ***

Configuration types should be detected on file change (without restarting service)
    ${DEVICE_SN}=    Setup    skip_bootstrap=True
    Execute Command    /setup/bootstrap.sh 2>&1
    Execute Command    sudo systemctl restart c8y-configuration-plugin.service
    Device Should Exist    ${DEVICE_SN}

    ${supported_configs}=    Should Support Configurations    c8y-configuration-plugin    includes=True
    Should Not Contain    ${supported_configs}    example
    
    Execute Command    sudo rm -f /etc/tedge/c8y/c8y-configuration-plugin.toml
    Execute Command    sudo printf '%s' "${toml}" > c8y-configuration-plugin.toml
    Execute Command    sudo mv c8y-configuration-plugin.toml /etc/tedge/c8y/

    ${supported_configs}=    Should Support Configurations    c8y-bridge.conf    c8y-configuration-plugin    mosquitto.conf    tedge-mosquitto.conf    tedge.toml    example
