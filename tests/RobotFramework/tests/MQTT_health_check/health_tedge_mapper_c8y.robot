*** Comments ***
# Command to execute:    robot -d \results --timestampoutputs --log health_tedge_mapper.html --report NONE --variable HOST:192.168.1.120 /thin-edge.io-fork/tests/RobotFramework/MQTT_health_check/health_tedge_mapper.robot


*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Logs

Test Tags           theme:monitoring    theme:c8y


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
    ${pid}=    Service Should Be Running    tedge-mapper-c8y
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of tedge-mapper
    ${pid1}=    Service Should Be Running    tedge-mapper-c8y
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '10d' /lib/systemd/system/tedge-mapper-c8y.service

Watchdog does not kill mapper if it responds
    # Set the watchdog interval low so we don't have to wait long
    Execute Command    sudo systemctl stop tedge-mapper-c8y.service
    Execute Command    sudo systemctl stop tedge-watchdog.service
    Execute Command    cmd=sudo sed -i '10iWatchdogSec=5' /lib/systemd/system/tedge-mapper-c8y.service
    Execute Command    sudo systemctl daemon-reload
    Execute Command    sudo systemctl start tedge-mapper-c8y.service
    Execute Command    sudo systemctl start tedge-watchdog.service

    ${pid_before_healthcheck}=    Service Should Be Running    tedge-mapper-c8y
    # The watchdog should send a health check command while we wait
    Sleep    10s
    ${pid_after_healthcheck}=    Service Should Be Running    tedge-mapper-c8y

    Should Have MQTT Messages    topic=te/device/main/service/tedge-mapper-c8y/cmd/health/check    minimum=1
    Should Be Equal    ${pid_before_healthcheck}    ${pid_after_healthcheck}
