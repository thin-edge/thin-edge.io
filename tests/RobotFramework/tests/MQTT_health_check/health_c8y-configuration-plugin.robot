#Command to execute:    robot -d \results --timestampoutputs --log health_c8y-configuration-plugin.html --report NONE --variable HOST:192.168.1.120 /Users/glis/thin-edge.io-fork/tests/RobotFramework/MQTT_health_check/health_c8y-configuration-plugin.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:configuration    theme:monitoring    theme:c8y
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Stop c8y-configuration-plugin
    Execute Command    sudo systemctl stop c8y-configuration-plugin.service

Update the service file
    Execute Command    cmd=sudo sed -i '9iWatchdogSec=30' /lib/systemd/system/c8y-configuration-plugin.service

Reload systemd files
    Execute Command    sudo systemctl daemon-reload

Start c8y-configuration-plugin
    Execute Command    sudo systemctl start c8y-configuration-plugin.service

Start watchdog service
    Execute Command    sudo systemctl start tedge-watchdog.service
    Sleep    10s

Check PID of c8y-configuration-plugin
    ${pid}=    Execute Command    pgrep -f 'c8y-configuration-plugin'    strip=True
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of c8y-configuration-plugin
    ${pid1}=    Execute Command    pgrep -f 'c8y-configuration-plugin'    strip=True
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '9d' /lib/systemd/system/c8y-configuration-plugin.service
