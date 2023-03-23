#Command to execute:    robot -d \results --timestampoutputs --log health_tedge-agent.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io-fork/tests/RobotFramework/MQTT_health_check/health_tedge-agent.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:monitoring
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Stop tedge-agent
    Execute Command    sudo systemctl stop tedge-agent.service

Update the service file
    Execute Command    cmd=sudo sed -i '11iWatchdogSec=30' /lib/systemd/system/tedge-agent.service

Reload systemd files
    Execute Command    sudo systemctl daemon-reload

Start tedge-agent
    Execute Command    sudo systemctl start tedge-agent.service

Start watchdog service
    Execute Command    sudo systemctl start tedge-watchdog.service
    Sleep    10s

Check PID of tedge-mapper
    ${pid}=    Execute Command    pgrep -f tedge-agent    strip=True
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of tedge-agent
    ${pid1}=    Execute Command    pgrep -f tedge-agent    strip=True
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '11d' /lib/systemd/system/tedge-agent.service
