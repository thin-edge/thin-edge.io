#Command to execute:    robot -d \results --timestampoutputs --log inotify_crate.html --report NONE --variable HOST:192.168.1.130 /thin-edge.io/tests/RobotFramework/MQTT_health_check/MQTT_health_endpoints.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:monitoring    theme:mqtt
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

tedge-log-plugin health status
    Execute Command    sudo systemctl start tedge-log-plugin.service

    Sleep    5s    reason=It fails without this! It needs a better way of queuing requests
    ${pid}=    Execute Command    pgrep -f '^/usr/bin/tedge-log-plugin'    strip=${True}
    Execute Command    sudo tedge mqtt pub 'te/device/main/service/tedge-log-plugin/cmd/health/check' ''
    ${messages}=    Should Have MQTT Messages    te/device/main/service/tedge-log-plugin/status/health    minimum=1    maximum=2
    Should Contain    ${messages[0]}    "pid":${pid}
    Should Contain    ${messages[0]}    "status":"up"

c8y-configuration-plugin health status
    Execute Command    sudo systemctl start c8y-configuration-plugin.service

    Sleep             5s                 reason=It fails without this! It needs a better way of queuing requests
    ${pid}=           Execute Command    pgrep -f '^/usr/bin/c8y[_-]configuration[_-]plugin'    strip=${True}
    Execute Command   sudo tedge mqtt pub 'te/device/main/service/c8y-configuration-plugin/cmd/health/check' ''
    ${messages}=      Should Have MQTT Messages    te/device/main/service/c8y-configuration-plugin/status/health    minimum=1    maximum=2
    Should Contain    ${messages[0]}    "pid":${pid}
    Should Contain    ${messages[0]}    "status":"up"
