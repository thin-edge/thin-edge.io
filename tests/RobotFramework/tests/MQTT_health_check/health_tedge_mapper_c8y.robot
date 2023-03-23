#Command to execute:    robot -d \results --timestampoutputs --log health_tedge_mapper.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io-fork/tests/RobotFramework/MQTT_health_check/health_tedge_mapper.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:monitoring    theme:c8y
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Stop tedge-mapper
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service

Update the service file
    Execute Command    cmd=sudo sed -i '10iWatchdogSec=30' /lib/systemd/system/tedge-mapper-c8y.service

Reload systemd files
    Execute Command    sudo systemctl daemon-reload

Start tedge-mapper
    Execute Command    sudo systemctl start tedge-mapper-c8y.service

Start watchdog service
    Execute Command    sudo systemctl start tedge-watchdog.service

    Sleep    10s
Check PID of tedge-mapper
    ${pid}=    Execute Command    pgrep -f 'tedge-mapper c8y'        strip=True
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of tedge-mapper
    ${pid1}=    Execute Command    pgrep -f 'tedge-mapper c8y'        strip=True
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '10d' /lib/systemd/system/tedge-mapper-c8y.service
