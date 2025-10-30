*** Comments ***
# Command to execute:    robot -d \results --timestampoutputs --log inotify_crate.html --report NONE --variable HOST:192.168.1.130 /thin-edge.io/tests/RobotFramework/MQTT_health_check/MQTT_health_endpoints.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Suite Logs
Test Timeout        5 minutes

Test Tags           theme:monitoring    theme:mqtt


*** Test Cases ***
tedge-agent health status
    ${pid}=    Service Should Be Running    tedge-agent
    Execute Command    sudo tedge mqtt pub 'te/device/main/service/tedge-agent/cmd/health/check' ''
    ${messages}=    Should Have MQTT Messages
    ...    te/device/main/service/tedge-agent/status/health
    ...    minimum=1
    ...    maximum=2
    Should Contain    ${messages[0]}    "pid":${pid}
    Should Contain    ${messages[0]}    "status":"up"
