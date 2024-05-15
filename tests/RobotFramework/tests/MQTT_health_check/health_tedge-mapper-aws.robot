*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:monitoring    theme:az
Suite Setup       Setup
Suite Teardown    Get Logs


*** Test Cases ***

Watchdog does not kill mapper if it responds
    # Set the watchdog interval low so we don't have to wait long
    Execute Command    sudo systemctl stop tedge-mapper-aws.service
    Execute Command    sudo systemctl stop tedge-watchdog.service
    Execute Command    cmd=sudo sed -i '10iWatchdogSec=5' /lib/systemd/system/tedge-mapper-aws.service
    Execute Command    cmd=sudo sed -i "s/\\\\[Service\\\\]/\\\\0\\\\nEnvironment=\"TEDGE_MQTT_BRIDGE_BUILT_IN=false\"/" /lib/systemd/system/tedge-mapper-aws.service
    Execute Command    sudo systemctl daemon-reload
    Execute Command    sudo systemctl start tedge-mapper-aws.service
    Execute Command    sudo systemctl start tedge-watchdog.service

    ${pid_before_healthcheck}=    Service Should Be Running    tedge-mapper-aws
    # The watchdog should send a health check command while we wait
    Sleep    10s
    ${pid_after_healthcheck}=    Service Should Be Running    tedge-mapper-aws

    Should Have MQTT Messages     topic=te/device/main/service/tedge-mapper-aws/cmd/health/check    minimum=1
    Should Be Equal               ${pid_before_healthcheck}    ${pid_after_healthcheck}


