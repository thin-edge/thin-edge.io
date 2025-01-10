*** Settings ***
Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Custom Setup
Suite Teardown      Get Logs

Test Tags           theme:cli


*** Variables ***
${DEVICE_SN}            ${EMPTY}
${SECOND_DEVICE_SN}     ${EMPTY}


*** Test Cases ***
Run tedge cert create
    Execute Command    tedge cert create --device-id ${DEVICE_SN}
    File Should Exist    /etc/tedge/device-certs/tedge-certificate.pem
    File Should Exist    /etc/tedge/device-certs/tedge-private-key.pem

    ${subject}=    Execute Command    openssl x509 -noout -subject -in /etc/tedge/device-certs/tedge-certificate.pem
    Should Match Regexp    ${subject}    pattern=^subject=CN = ${DEVICE_SN},.+$

    ${config_output}=    Execute Command    tedge config get device.id 2>/dev/null    strip=True
    Should Be Equal    ${config_output}    ${DEVICE_SN}
    ${toml_output}=    Execute Command    cmd=grep -B 1 ${DEVICE_SN} /etc/tedge/tedge.toml    strip=True
    Should Be Equal    ${toml_output}    second=[device]\nid = "${DEVICE_SN}"

Run tedge cert create with cloud profile
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

    ${config_output}=    Execute Command    tedge config get c8y.device.id --profile second 2>/dev/null    strip=True
    Should Be Equal    ${config_output}    ${SECOND_DEVICE_SN}
    ${toml_output}=    Execute Command    cmd=grep -B 1 ${SECOND_DEVICE_SN} /etc/tedge/tedge.toml    strip=True
    Should Be Equal    ${toml_output}    second=[c8y.profiles.second.device]\nid = "${SECOND_DEVICE_SN}"


*** Keywords ***
Custom Setup
    ${device_sn}=    Setup    skip_bootstrap=${True}
    Execute Command    ./bootstrap.sh --no-bootstrap --no-connect

    Set Suite Variable    $DEVICE_SN    ${device_sn}
    Set Suite Variable    $SECOND_DEVICE_SN    ${device_sn}-second
