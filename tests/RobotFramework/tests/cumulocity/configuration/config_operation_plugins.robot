*** Settings ***
Resource            ../../../resources/common.resource
Library             DateTime
Library             OperatingSystem
Library             String
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log


*** Test Cases ***
Config operation journald plugin
    ${operation}=    Cumulocity.Get Configuration    hello.conf::hello
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=10

    ${config_url}=    Cumulocity.Create Inventory Binary
    ...    hello.conf
    ...    hello.conf
    ...    file=${CURDIR}/plugins/hello-v2.conf
    ${operation}=    Cumulocity.Set Configuration    hello.conf::hello    url=${config_url}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=10


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist    ${DEVICE_SN}

    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/hello.sh
    ...    /usr/local/bin/hello.sh
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/hello.conf
    ...    /etc/hello.conf
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/hello.service
    ...    /etc/systemd/system/hello.service
    ThinEdgeIO.Transfer To Device
    ...    ${CURDIR}/plugins/hello
    ...    /etc/tedge/config-plugins/hello

    Execute Command    chmod +x /usr/local/bin/hello.sh
    Execute Command    systemctl daemon-reload
    Execute Command    systemctl enable --now hello.service

    Execute Command    chmod +x /etc/tedge/config-plugins/hello
    Execute Command
    ...    cmd=echo "tedge ALL = (ALL) NOPASSWD:SETENV: /etc/tedge/config-plugins/[a-zA-Z0-9]**" >> /etc/sudoers.d/tedge

    Restart Service    tedge-agent
    ThinEdgeIO.Service Health Status Should Be Up    tedge-agent
