*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Teardown      Get Logs
Test Setup          Custom Setup

Test Tags           theme:cli


*** Test Cases ***
Run tedge cert create
    # device.id doesn't exist yet
    Execute Command    tedge config get device.id    exp_exit_code=1

    Execute Command    tedge cert create --device-id ${DEVICE_SN}
    File Should Exist    /etc/tedge/device-certs/tedge-certificate.pem
    File Should Exist    /etc/tedge/device-certs/tedge-private-key.pem

    ${subject}=    Execute Command    openssl x509 -noout -subject -in /etc/tedge/device-certs/tedge-certificate.pem
    Should Match Regexp    ${subject}    pattern=^subject=CN = ${DEVICE_SN},.+$

    # device.id is read from the cert's CN
    ${device_id}=    Execute Command    tedge config get device.id    strip=${True}
    Should Be Equal    ${device_id}    ${DEVICE_SN}

    # Remove the cert and key
    Execute Command    tedge cert remove
    File Should Not Exist    /etc/tedge/device-certs/tedge-certificate.pem
    File Should Not Exist    /etc/tedge/device-certs/tedge-private-key.pem
    Execute Command    tedge config get device.id    exp_exit_code=1

    # New cert/key can be also created without --device-id option if device.id is set in config
    Execute Command    tedge config set device.id ${DEVICE_SN}-two
    Execute Command    tedge cert create
    File Should Exist    /etc/tedge/device-certs/tedge-certificate.pem
    File Should Exist    /etc/tedge/device-certs/tedge-private-key.pem
    ${subject_two}=    Execute Command
    ...    openssl x509 -noout -subject -in /etc/tedge/device-certs/tedge-certificate.pem
    Should Match Regexp    ${subject_two}    pattern=^subject=CN = ${DEVICE_SN}-two,.+$

Run tedge cert create with cloud profile
    Set Test Variable    $SECOND_DEVICE_SN    ${device_sn}-second
    Set Test Variable    $THIRD_DEVICE_SN    ${device_sn}-third

    Execute Command
    ...    tedge config set c8y.device.cert_path --profile second /etc/tedge/device-certs/tedge-certificate@second.pem
    Execute Command
    ...    tedge config set c8y.device.key_path --profile second /etc/tedge/device-certs/tedge-private-key@second.pem

    Execute Command    tedge cert create --device-id ${SECOND_DEVICE_SN} c8y --profile second
    File Should Exist    /etc/tedge/device-certs/tedge-certificate@second.pem
    File Should Exist    /etc/tedge/device-certs/tedge-private-key@second.pem

    ${subject}=    Execute Command
    ...    openssl x509 -noout -subject -in /etc/tedge/device-certs/tedge-certificate@second.pem
    Should Match Regexp    ${subject}    pattern=^subject=CN = ${SECOND_DEVICE_SN},.+$

    # c8y.profiles.second.device.id is read from the cert's CN of the cloud profile "second"
    ${config_output}=    Execute Command
    ...    tedge config get c8y.device.id --profile second
    ...    strip=${True}
    Should Be Equal    ${config_output}    ${SECOND_DEVICE_SN}

    # Using another cloud profile. c8y.profiles.third.device.id is set by tedge config set
    Execute Command    tedge config set c8y.device.id --profile third ${THIRD_DEVICE_SN}
    Execute Command
    ...    tedge config set c8y.device.cert_path --profile third /etc/tedge/device-certs/tedge-certificate@third.pem
    Execute Command
    ...    tedge config set c8y.device.key_path --profile third /etc/tedge/device-certs/tedge-private-key@third.pem

    Execute Command    tedge cert create c8y --profile third

    File Should Exist    /etc/tedge/device-certs/tedge-certificate@third.pem
    File Should Exist    /etc/tedge/device-certs/tedge-private-key@third.pem
    ${subject}=    Execute Command
    ...    openssl x509 -noout -subject -in /etc/tedge/device-certs/tedge-certificate@third.pem
    Should Match Regexp    ${subject}    pattern=^subject=CN = ${THIRD_DEVICE_SN},.+$


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup    skip_bootstrap=${True}
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect

    Set Test Variable    $DEVICE_SN    ${device_sn}
