*** Settings ***
Resource        ../../../../resources/common.resource
Library         String
Library         Cumulocity
Library         ThinEdgeIO

Test Tags       theme:c8y


*** Test Cases ***
Register Device Using Cumulocity CA
    [Setup]    Custom Setup
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    ${DOMAIN}=    Cumulocity.Get Domain
    Execute Command    tedge config set c8y.url "${DOMAIN}"
    Execute Command
    ...    tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 30s
    Execute Command    tedge connect c8y

Register Device Using Cumulocity CA with url flag
    [Setup]    Custom Setup
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    ${DOMAIN}=    Cumulocity.Get Domain
    Execute Command
    ...    tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --url ${DOMAIN} --retry-every 5s --max-timeout 30s
    Execute Command    tedge config set c8y.url "${DOMAIN}"
    Execute Command    tedge connect c8y

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
Custom Setup
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --no-bootstrap --no-connect || true

    Set Test Variable    $DEVICE_SN

Setup With Self-Signed Certificate
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Execute Command    test -f ./bootstrap.sh && ./bootstrap.sh --cert-method selfsigned
    Set Test Variable    $DEVICE_SN
    Register Certificate For Cleanup
