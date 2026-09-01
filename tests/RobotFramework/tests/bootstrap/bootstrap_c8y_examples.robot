*** Settings ***

Resource            ../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Test Setup          Custom Setup
Test Teardown       Get Logs

Test Tags           theme:bootstrap    theme:c8y


*** Variables ***
${EXAMPLES}     ${CURDIR}/examples
${HOOKS}        /usr/share/tedge/bootstrap.d


*** Test Cases ***
QR Code Is Rendered For The Pending Registration
    [Documentation]    Print a QR code with that users can scan to register the device
    Transfer To Device    ${EXAMPLES}/qr-registration/configure.d/90_qr_code    ${HOOKS}/configure.d/
    Execute Command    cmd=chmod +x ${HOOKS}/configure.d/*
    ${domain}=    Cumulocity.Get Domain
    ${output}=    Execute Command
    ...    cmd=timeout 30 tedge bootstrap c8y --url "${domain}" --device-id "${DEVICE_SN}" 2>&1
    ...    exp_exit_code=!0
    Should Contain    ${output}    Scan to register the device:

Full Bootstrap Runs The Event And Device Link Finalize Hooks
    Transfer To Device    ${EXAMPLES}/bootstrap-event/finalize.d/50_bootstrap_event    ${HOOKS}/finalize.d/
    Transfer To Device    ${EXAMPLES}/device-link/finalize.d/60_device_link    ${HOOKS}/finalize.d/
    Execute Command    cmd=chmod +x ${HOOKS}/finalize.d/*
    ${domain}=    Cumulocity.Get Domain
    ${output}=    Execute Command
    ...    cmd=sudo env C8Y_USER='${C8Y_CONFIG.username}' C8Y_PASSWORD='${C8Y_CONFIG.password}' tedge bootstrap c8y --url "${domain}" --device-id "${DEVICE_SN}" --register self-signed 2>&1
    Device Should Exist    ${DEVICE_SN}
    # the device-link hook prints a click-through to the onboarded device
    Should Contain    ${output}    /apps/devicemanagement/index.html#/assetsearch?filter=*${DEVICE_SN}*
    # the bootstrap-event hook announces the completed bootstrap locally
    Should Have MQTT Messages    te/device/main///e/device_bootstrap

Bootstrap pre-registered basic auth device
    ${domain}=    Cumulocity.Get Domain
    ${CREDENTIALS}=    Cumulocity.Bulk Register Device With Basic Auth    external_id=${DEVICE_SN}
    ${output}=    Execute Command
    ...    cmd=sudo env C8Y_DEVICE_USER='${CREDENTIALS.username}' C8Y_DEVICE_PASSWORD='${CREDENTIALS.password}' tedge bootstrap c8y --url "${domain}" --device-id "${DEVICE_SN}" --register basic-preregistered 2>&1

Bootstrap using a non-interactive service
    [Documentation]    The offline-firstboot composition, end to end:
    ...    an offline bench run captures the invocation with --save,
    ...    the per-device secret is staged in a mode-600 environment file,
    ...    and the packaged oneshot service completes the bootstrap
    ...    non-interactively on the "first networked boot" -
    ...    shredding the secret after success
    ${domain}=    Cumulocity.Get Domain
    ${CREDENTIALS}=    Cumulocity.Bulk Register Device With Cumulocity CA    external_id=${DEVICE_SN}

    # save bootstrap config so it can be used by the bootstrap service
    Execute Command
    ...    cmd=tedge bootstrap c8y --url "${domain}" --offline --save /etc/tedge/bootstrap.json --dry-run

    # Add required env variables for the device specific values that will be read by the service
    Execute Command
    ...    cmd=jq '.[0].env = ["TEDGE_DEVICE_ID", "DEVICE_ONE_TIME_PASSWORD"]' /etc/tedge/bootstrap.json > /etc/tedge/bootstrap.json.tmp && mv /etc/tedge/bootstrap.json.tmp /etc/tedge/bootstrap.json

    # flash phase: the per-device values live only in the env file (mode 600)
    Execute Command    cmd=mkdir -p /boot/firstboot
    Execute Command
    ...    cmd=umask 077 && printf 'TEDGE_DEVICE_ID=%s\\nDEVICE_ONE_TIME_PASSWORD=%s\\n' '${DEVICE_SN}' '${CREDENTIALS.one_time_password}' > /boot/firstboot/tedge-bootstrap.env
    ${saved}=    Execute Command    cmd=cat /etc/tedge/bootstrap.json
    Should Contain    ${saved}    TEDGE_DEVICE_ID
    Should Contain    ${saved}    DEVICE_ONE_TIME_PASSWORD
    Should Not Contain    ${saved}    device_id
    Should Not Contain    ${saved}    ${CREDENTIALS.one_time_password}

    # first networked boot: the oneshot completion service
    Transfer To Device
    ...    ${EXAMPLES}/offline-firstboot/tedge-bootstrap-complete.service
    ...    /etc/systemd/system/
    Execute Command    cmd=systemctl daemon-reload
    Execute Command    cmd=systemctl start tedge-bootstrap-complete

    Device Should Exist    ${DEVICE_SN}
    # the secret was shredded after success
    Execute Command    cmd=test ! -f /boot/firstboot/tedge-bootstrap.env


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup    register=${False}
    Set Suite Variable    $DEVICE_SN
    Execute Command
    ...    cmd=mkdir -p ${HOOKS}/prepare.d ${HOOKS}/configure.d ${HOOKS}/register.d ${HOOKS}/finalize.d ${HOOKS}/clouds.d
