*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Suite Logs
Test Timeout        5 minutes

Test Tags           theme:monitoring    theme:az


*** Test Cases ***
Stop tedge-mapper-az
    Execute Command    sudo systemctl stop tedge-mapper-az.service

Update the service file
    Execute Command    cmd=sudo sed -i '10iWatchdogSec=30' /lib/systemd/system/tedge-mapper-az.service
    Execute Command
    ...    cmd=sudo sed -i "s/\\\\[Service\\\\]/\\\\0\\\\nEnvironment=\"TEDGE_MQTT_BRIDGE_BUILT_IN=false\"/" /lib/systemd/system/tedge-mapper-az.service

Reload systemd files
    Execute Command    sudo systemctl daemon-reload

Start tedge-mapper-az
    Execute Command    sudo systemctl start tedge-mapper-az.service

Start watchdog service
    Execute Command    sudo systemctl start tedge-watchdog.service

    Sleep    10s

Check PID of tedge-mapper-az
    ${pid}=    Service Should Be Running    tedge-mapper-az
    Set Suite Variable    ${pid}

Kill the PID
    Kill Process    ${pid}

Recheck PID of tedge-mapper-az
    ${pid1}=    Service Should Be Running    tedge-mapper-az
    Set Suite Variable    ${pid1}

Compare PID change
    Should Not Be Equal    ${pid}    ${pid1}

Stop watchdog service
    Execute Command    sudo systemctl stop tedge-watchdog.service

Remove entry from service file
    Execute Command    sudo sed -i '10d' /lib/systemd/system/tedge-mapper-az.service

Watchdog does not kill mapper if it responds
    # Set the watchdog interval low so we don't have to wait long
    Execute Command    sudo systemctl stop tedge-mapper-az.service
    Execute Command    sudo systemctl stop tedge-watchdog.service
    Execute Command    cmd=sudo sed -i '10iWatchdogSec=5' /lib/systemd/system/tedge-mapper-az.service
    Execute Command    sudo systemctl daemon-reload
    Execute Command    sudo systemctl start tedge-mapper-az.service
    Execute Command    sudo systemctl start tedge-watchdog.service

    ${pid_before_healthcheck}=    Service Should Be Running    tedge-mapper-az
    # The watchdog should send a health check command while we wait
    Sleep    10s
    ${pid_after_healthcheck}=    Service Should Be Running    tedge-mapper-az

    Should Have MQTT Messages    topic=te/device/main/service/tedge-mapper-az/cmd/health/check    minimum=1
    Should Be Equal    ${pid_before_healthcheck}    ${pid_after_healthcheck}
