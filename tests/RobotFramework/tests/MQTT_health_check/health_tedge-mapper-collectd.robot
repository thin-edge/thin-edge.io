#Command to execute:    robot -d \results --timestampoutputs --log health_tedge-mapper-collectd.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io-fork/tests/RobotFramework/MQTT_health_check/health_tedge-mapper-collectd.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:monitoring
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Stop tedge-mapper-collectd
    Execute Command    sudo systemctl stop tedge-mapper-collectd.service

Update the service file
    Execute Command    cmd=sudo sed -i '10iWatchdogSec=30' /lib/systemd/system/tedge-mapper-collectd.service

Reload systemd files
    Execute Command    sudo systemctl daemon-reload

Start tedge-mapper-collectd
    Execute Command    sudo systemctl start tedge-mapper-collectd.service

Start watchdog service
    Execute Command    sudo systemctl start tedge-watchdog.service
    Sleep    10s

Check PID of tedge-mapper-collectd
    ${pid}=    Execute Command    pgrep -f 'tedge-mapper collectd'
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of tedge-mapper-collectd
    ${pid1}=    Execute Command    pgrep -f 'tedge-mapper collectd'    strip=True
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '10d' /lib/systemd/system/tedge-mapper-collectd.service

tedge-collectd-mapper health status
    Execute Command    sudo systemctl start tedge-mapper-collectd.service

    Sleep    5s     reason=It fails without this! It needs a better way of queuing requests
    ${pid}=    Execute Command    pgrep -f "tedge-mapper collectd"    strip=True
    Execute Command    sudo tedge mqtt pub 'tedge/health-check/tedge-mapper-collectd' ''
    ${messages}=    Should Have MQTT Messages    tedge/health/tedge-mapper-collectd    minimum=1    maximum=2
    Should Contain    ${messages[0]}    "pid":${pid}
    Should Contain    ${messages[0]}    "status":"up"
