#Command to execute:    robot -d \results --timestampoutputs --log health_c8y-configuration-plugin.html --report NONE --variable HOST:192.168.1.120 /Users/glis/thin-edge.io-fork/tests/RobotFramework/MQTT_health_check/health_c8y-configuration-plugin.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:configuration    theme:monitoring    theme:c8y
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Stop tedge-configuration-plugin
    Execute Command    sudo systemctl stop tedge-configuration-plugin.service

Update the service file
    Execute Command    cmd=sudo sed -i '9iWatchdogSec=30' /lib/systemd/system/tedge-configuration-plugin.service

Reload systemd files
    Execute Command    sudo systemctl daemon-reload

Start tedge-configuration-plugin
    Execute Command    sudo systemctl start tedge-configuration-plugin.service

Start watchdog service
    Execute Command    sudo systemctl start tedge-watchdog.service
    Sleep    10s

Check PID of tedge-configuration-plugin
    ${pid}=    Execute Command    pgrep -f '^/usr/bin/tedge-configuration-plugin'    strip=${True}
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of tedge-configuration-plugin
    ${pid1}=    Execute Command    pgrep -f '^/usr/bin/tedge-configuration-plugin'    strip=${True}
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '9d' /lib/systemd/system/tedge-configuration-plugin.service
