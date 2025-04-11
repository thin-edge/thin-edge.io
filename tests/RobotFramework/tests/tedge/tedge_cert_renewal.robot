*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Test Teardown       Get Logs

Test Tags           theme:cli    theme:mqtt    theme:c8y


*** Test Cases ***
Certificate Renewal Service Using Cumulocity Certificate Authority
    [Setup]    Setup With Self-Signed Certificate
    ${cert_before}=    Execute Command    tedge cert show | grep -v Status:
    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge cert upload c8y
    ...    log_output=${False}
    Execute Command    tedge reconnect c8y

    # Enforce a renewal using the service
    Execute Command    sudo tedge config set certificate.validity.minimum_duration 365d
    Execute Command    sudo systemctl start tedge-cert-renewer@c8y.service

    # Wait for service to stop
    Service Should Be Stopped    tedge-cert-renewer@c8y.service
    ${cert_after}=    Execute Command    tedge cert show c8y | grep -v Status:
    Should Not Be Equal    ${cert_before}    ${cert_after}


*** Keywords ***
Setup With Self-Signed Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Test Variable    $DEVICE_SN
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --cert-method selfsigned
