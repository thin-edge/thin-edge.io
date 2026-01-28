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
    Set Cumulocity URLs
    Execute Command
    ...    tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 30s
    Execute Command    tedge connect c8y

Register Device Using Cumulocity CA with url flag
    [Setup]    Custom Setup
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    ${DOMAIN}=    Cumulocity.Get Domain
    Execute Command
    ...    tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --url ${DOMAIN} --retry-every 5s --max-timeout 30s
    Set Cumulocity URLs
    Execute Command    tedge connect c8y

Reuse existing Device ID whilst downloading
    [Documentation]    Re-registering the device shouldn't require providing the device-id again
    [Setup]    Custom Setup
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    ${DOMAIN}=    Cumulocity.Get Domain
    Execute Command
    ...    tedge cert download c8y --device-id "${DEVICE_SN}" --one-time-password '${credentials.one_time_password}' --url ${DOMAIN} --retry-every 5s --max-timeout 30s
    Set Cumulocity URLs
    Execute Command    tedge connect c8y

    # re-register the device (ensure that the DEVICE_ID env is also not set)
    ${credentials}=    Bulk Register Device With Cumulocity CA    ${DEVICE_SN}
    Execute Command
    ...    cmd=env DEVICE_ID= tedge cert download c8y --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 30s
    Execute Command    tedge reconnect c8y

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

Certificate Renewal with Cloud Profiles
    [Documentation]    Check if the certificate renewal service is automatically created when using Cumulocity with
    ...    cloud profiles. In the test, a device is connected twice to the same tenant (using different device ids).
    ...    Normally the device would be connected to two different tenants, however this would require more testing
    ...    infrastructure
    [Setup]    Setup With Cumulocity CA Certificate
    ${DEVICE_SN_2}=    Set Variable    ${DEVICE_SN}_2
    ${credentials}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN_2}

    Set Cumulocity URLs    profile=customer
    Execute Command
    ...    tedge config set c8y.device.cert_path /etc/tedge/device-certs/tedge-certificate-customer.pem --profile customer
    Execute Command    tedge config set c8y.bridge.topic_prefix c8y-customer --profile customer
    Execute Command    tedge config set c8y.proxy.bind.port 8002 --profile customer
    Execute Command
    ...    cmd=tedge cert download c8y --device-id "${DEVICE_SN_2}" --one-time-password '${credentials.one_time_password}' --retry-every 5s --max-timeout 30s --profile customer
    Execute Command    cmd=tedge config set c8y.enable.log_upload false --profile customer
    Execute Command    cmd=tedge config set c8y.enable.config_snapshot true --profile customer
    Execute Command    cmd=tedge config set c8y.enable.config_update false --profile customer
    Execute Command    cmd=tedge config set c8y.enable.firmware_update false --profile customer
    Execute Command    cmd=tedge config set c8y.enable.device_profile false --profile customer

    Execute Command    tedge connect c8y --profile customer

    # check if the cert renewal c8y timer is enabled by default
    Execute Command    systemctl is-active tedge-cert-renewer-c8y@customer.timer

    # Enforce a renewal using the service
    ${cert_before}=    Execute Command    tedge cert show c8y --profile customer | grep Thumbprint
    Execute Command    tedge config set certificate.validity.minimum_duration 365d
    Execute Command    systemctl start tedge-cert-renewer-c8y@customer.service

    ${cert_after}=    Execute Command    tedge cert show c8y --profile customer | grep Thumbprint
    Should Not Be Equal    ${cert_before}    ${cert_after}
    [Teardown]    Disconnect And Delete Device From Cumulocity    profile=customer    external_id=${DEVICE_SN_2}


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Test Variable    $DEVICE_SN

Setup With Self-Signed Certificate
    ${DEVICE_SN}=    Setup    register_using=self-signed
    Set Test Variable    $DEVICE_SN

Setup With Cumulocity CA Certificate
    ${DEVICE_SN}=    Setup    register_using=c8y-ca
    Set Test Variable    $DEVICE_SN

Disconnect And Delete Device From Cumulocity
    [Arguments]    ${profile}    ${external_id}
    Execute Command    tedge disconnect c8y --profile ${profile}    ignore_exit_code=${True}
    Cumulocity.Delete Managed Object And Device User    ${external_id}
