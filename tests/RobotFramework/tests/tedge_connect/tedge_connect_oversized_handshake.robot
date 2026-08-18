*** Settings ***
Documentation       `tedge connect c8y` should explain a TLS handshake that is too large for rustls
...                 to buffer, and say that it is Cumulocity's to fix rather than the device's.
...                 A local socket stands in for the broker: rustls enforces the limit while
...                 buffering, so no certificate or ServerHello is needed to provoke the failure.

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Suite Logs

Test Tags           theme:c8y    theme:tls    adapter:docker


*** Variables ***
${SERVER_PORT}      18883


*** Test Cases ***
Explains a handshake that fills the buffer before completing
    ${stderr}=    Connect To Oversized Handshake Server    buffer-full
    Should Contain    ${stderr}    larger than the 64 KB thin-edge.io can accept
    Should Contain    ${stderr}    certificate_authorities
    Should Contain    ${stderr}    Cumulocity support
    Should Contain    ${stderr}    localhost

Explains a handshake message that announces more than the limit
    ${stderr}=    Connect To Oversized Handshake Server    too-large
    Should Contain    ${stderr}    larger than the 64 KB thin-edge.io can accept
    Should Contain    ${stderr}    Cumulocity support

Does not suggest the certificates or the configuration are at fault
    ${stderr}=    Connect To Oversized Handshake Server    buffer-full
    Should Not Contain    ${stderr}    tedge cert upload
    Should Not Contain    ${stderr}    device.key_path
    Should Not Contain    ${stderr}    c8y.root_cert_path


*** Keywords ***
Custom Setup
    # The device needs a certificate to offer, but must not be connected: the address is
    # about to be pointed at the local socket instead of the tenant
    ${DEVICE_SN}=    Setup    connect=${False}
    Set Suite Variable    $DEVICE_SN

    ThinEdgeIO.Transfer To Device    ${CURDIR}/oversized_handshake_server.sh    /usr/bin/
    Execute Command    sudo chmod +x /usr/bin/oversized_handshake_server.sh
    Execute Command    sudo tedge config set c8y.mqtt localhost:${SERVER_PORT}

Connect To Oversized Handshake Server
    [Arguments]    ${mode}
    Start Oversized Handshake Server    ${mode}
    ${stderr}=    Execute Command    sudo tedge connect c8y
    ...    exp_exit_code=!0    stdout=${False}    stderr=${True}    timeout=120
    [Teardown]    Stop Oversized Handshake Server
    RETURN    ${stderr}

Start Oversized Handshake Server
    [Arguments]    ${mode}
    Execute Command
    ...    nohup oversized_handshake_server.sh ${mode} ${SERVER_PORT} >/tmp/handshake-server.log 2>&1 &
    Wait Until Keyword Succeeds    10x    1s    Server Should Be Listening

Server Should Be Listening
    Execute Command    netstat -ltn | grep ':${SERVER_PORT} '

Stop Oversized Handshake Server
    Execute Command    pkill -f oversized_handshake_server.sh    ignore_exit_code=${True}
    Execute Command    pkill -f 'nc -l ${SERVER_PORT}'    ignore_exit_code=${True}
